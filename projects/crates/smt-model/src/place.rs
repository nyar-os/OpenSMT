//! Placement sequencing and feeder allocation.
//!
//! Both reports are deterministic. Feeders are allocated in id order over
//! components sorted by reference designator, and the placement head visits the
//! nearest remaining component, breaking ties by reference designator. The top
//! side is processed before the bottom side, and each side starts at the machine
//! origin.
//!
//! An optional 2-opt pass improves each side's nearest-neighbour route until no
//! improving reversal remains or the fixed pass limit is reached.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    SmtError,
    model::{Component, Feeder, Side, SmtDocument},
    validate::{package_counts, validate},
};

/// Fixed pick-and-place overhead per component, in seconds.
pub const PLACE_SECONDS: f64 = 0.6;

/// Travelling head speed used for the cycle-time estimate, in millimetres per second.
pub const HEAD_SPEED: f64 = 200.0;

/// Maximum number of improving reversals a 2-opt run applies per side.
const MAX_OPTIMIZATION_PASSES: usize = 64;

/// Distance differences at or below this value count as ties.
const OPTIMIZATION_EPSILON: f64 = 1e-9;

/// One step of the placement program.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementEntry {
    /// Zero-based program index.
    pub index: u32,
    /// Reference designator.
    pub refdes: String,
    /// Package identifier.
    pub package: String,
    /// Board side.
    pub side: Side,
    /// Placement X coordinate.
    pub x: f64,
    /// Placement Y coordinate.
    pub y: f64,
    /// Rotation in degrees.
    pub rotation: f64,
    /// Feeder that supplies the component; `null` when unallocated.
    pub feeder: Option<String>,
}

/// Usage of one package.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageUsage {
    /// Package identifier.
    pub package: String,
    /// Components of this package.
    pub components: u32,
    /// Feeders that supply this package.
    pub feeders: u32,
}

/// Allocation of one feeder.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeederUsage {
    /// Feeder identifier.
    pub id: String,
    /// Package supplied by the feeder.
    pub package: String,
    /// Components loaded on the lane.
    pub loaded: u32,
    /// Maximum lane capacity.
    pub capacity: u32,
    /// Components assigned to the lane.
    pub assigned: u32,
    /// Components still available after allocation.
    pub remaining: u32,
    /// Reference designators assigned to the lane, in program order.
    pub refdes: Vec<String>,
}

/// Feeder setup report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeederReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// One entry per used package, in package order.
    pub packages: Vec<PackageUsage>,
    /// One entry per feeder, in id order.
    pub feeders: Vec<FeederUsage>,
    /// Reference designators that could not be allocated; empty for a valid document.
    pub unassigned: Vec<String>,
}

/// Placement program report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Number of boards on the panel.
    pub board_count: u32,
    /// Panel width including column gaps.
    pub panel_width: f64,
    /// Panel depth including row gaps.
    pub panel_depth: f64,
    /// Head travel of the program, in millimetres.
    pub travel_length: f64,
    /// Estimated placement time in seconds.
    pub placement_seconds: f64,
    /// Whether the sequence was produced with the 2-opt pass.
    #[serde(default, skip_serializing_if = "is_false")]
    pub optimized: bool,
    /// Millimetres saved by 2-opt relative to nearest neighbour; present only
    /// when `optimize` was requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub travel_saved: Option<f64>,
    /// Program steps in execution order.
    pub entries: Vec<PlacementEntry>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Allocates every component to a feeder lane.
///
/// The document is validated first, so a missing or undersized feeder is
/// reported as an error instead of producing a partial setup.
pub fn allocate(document: &SmtDocument) -> Result<FeederReport, SmtError> {
    let report = validate(document);
    if let Some(error) = crate::validate::first_error(&report) {
        return Err(error);
    }

    let components: Vec<&Component> = document.components.iter().collect();
    let (feeders, unassigned) = allocate_lanes(&document.feeders, &components);

    let counts = package_counts(document);
    let packages = counts
        .into_iter()
        .map(|(package, components)| PackageUsage {
            feeders: document.feeders.iter().filter(|feeder| feeder.package == package).count() as u32,
            package,
            components,
        })
        .collect();

    Ok(FeederReport {
        format: "open-smt-feeders".to_string(),
        version: 1,
        units: document.units.clone(),
        packages,
        feeders,
        unassigned,
    })
}

/// Allocates `components` to `feeders` and returns the lane usage in id order
/// together with the reference designators that had no remaining lane.
///
/// Components are visited in reference designator order and each one takes the
/// lane of the same package with the smallest id that still has capacity.
pub(crate) fn allocate_lanes(feeders: &[Feeder], components: &[&Component]) -> (Vec<FeederUsage>, Vec<String>) {
    allocate_lanes_with(feeders, components, |feeder| feeder.loaded)
}

/// Allocates `components` to `feeders` using lane capacity instead of the
/// loaded quantity as the limit.
///
/// The balance report compares the resulting `assigned` demand with `loaded`,
/// so a lane must be able to take components that are not loaded yet.
pub(crate) fn allocate_lanes_by_capacity(feeders: &[Feeder], components: &[&Component]) -> (Vec<FeederUsage>, Vec<String>) {
    allocate_lanes_with(feeders, components, |feeder| feeder.capacity)
}

fn allocate_lanes_with(
    feeders: &[Feeder],
    components: &[&Component],
    limit: impl Fn(&Feeder) -> u32,
) -> (Vec<FeederUsage>, Vec<String>) {
    let mut lanes: BTreeMap<String, FeederUsage> = BTreeMap::new();
    for feeder in feeders {
        lanes.insert(
            feeder.id.clone(),
            FeederUsage {
                id: feeder.id.clone(),
                package: feeder.package.clone(),
                loaded: feeder.loaded,
                capacity: feeder.capacity,
                assigned: 0,
                remaining: limit(feeder),
                refdes: Vec::new(),
            },
        );
    }

    let mut order = components.to_vec();
    order.sort_by(|left, right| left.refdes.cmp(&right.refdes));

    let mut unassigned = Vec::new();
    for component in order {
        let lane = lanes
            .values_mut()
            .filter(|lane| lane.package == component.package && lane.remaining > 0)
            .min_by(|left, right| left.id.cmp(&right.id));
        match lane {
            Some(lane) => {
                lane.assigned += 1;
                lane.remaining -= 1;
                lane.refdes.push(component.refdes.clone());
            }
            None => unassigned.push(component.refdes.clone()),
        }
    }

    (lanes.into_values().collect(), unassigned)
}

/// Orders `components` for one machine and returns them with the total travel.
///
/// The top side is processed before the bottom side and every side starts at
/// the machine origin `(0, 0)`, exactly like a single-machine program.
pub(crate) fn order_with_travel<'a>(components: &[&'a Component]) -> (Vec<&'a Component>, f64) {
    let (ordered, travel, _) = order_with_travel_optimized(components, false);
    (ordered, travel)
}

/// Orders `components` for one machine and returns the sequence, the total
/// travel and the millimetres saved by 2-opt.
///
/// With `optimize` false the result is the plain nearest-neighbour program and
/// the saving is zero.
pub(crate) fn order_with_travel_optimized<'a>(components: &[&'a Component], optimize: bool) -> (Vec<&'a Component>, f64, f64) {
    let mut ordered = Vec::with_capacity(components.len());
    let mut travel = 0.0;
    let mut saved = 0.0;
    for side in [Side::Top, Side::Bottom] {
        let side_components: Vec<&Component> = components.iter().copied().filter(|component| component.side == side).collect();
        let nearest = nearest_neighbor(&side_components);
        let (sequenced, side_saved) = if optimize { two_opt(nearest) } else { (nearest, 0.0) };
        saved += side_saved;
        let mut x = 0.0;
        let mut y = 0.0;
        for component in sequenced {
            travel += distance(x, y, component.x, component.y);
            x = component.x;
            y = component.y;
            ordered.push(component);
        }
    }
    (ordered, travel, saved)
}

/// Estimated placement time of `count` components with `travel` millimetres.
pub(crate) fn estimated_seconds(count: usize, travel: f64) -> f64 {
    count as f64 * PLACE_SECONDS + travel / HEAD_SPEED
}

/// Sequences the placement program using an existing feeder allocation.
pub fn sequence(document: &SmtDocument, feeders: &FeederReport) -> Result<PlacementReport, SmtError> {
    sequence_with(document, feeders, false)
}

/// Sequences the placement program, optionally improving each side with 2-opt.
///
/// `optimize` false reproduces [`sequence`] exactly.
pub fn sequence_with(document: &SmtDocument, feeders: &FeederReport, optimize: bool) -> Result<PlacementReport, SmtError> {
    if !feeders.unassigned.is_empty() {
        return Err(SmtError::new("feeder/capacity", format!("{} components have no feeder lane", feeders.unassigned.len())));
    }
    let mut lane_of = BTreeMap::new();
    for feeder in &feeders.feeders {
        for refdes in &feeder.refdes {
            lane_of.insert(refdes.clone(), feeder.id.clone());
        }
    }

    let components: Vec<&Component> = document.components.iter().collect();
    let (ordered, travel_length, travel_saved) = order_with_travel_optimized(&components, optimize);
    let mut entries = Vec::with_capacity(ordered.len());
    for component in ordered {
        entries.push(PlacementEntry {
            index: entries.len() as u32,
            refdes: component.refdes.clone(),
            package: component.package.clone(),
            side: component.side,
            x: component.x,
            y: component.y,
            rotation: component.rotation,
            feeder: lane_of.get(&component.refdes).cloned(),
        });
    }

    let board_count = document.panel.board_count();
    let columns = f64::from(document.panel.columns);
    let rows = f64::from(document.panel.rows);
    let panel_width = columns * document.board.width + (columns - 1.0) * document.panel.gap_x;
    let panel_depth = rows * document.board.depth + (rows - 1.0) * document.panel.gap_y;
    let placement_seconds = estimated_seconds(entries.len(), travel_length);

    Ok(PlacementReport {
        format: "open-smt-placement".to_string(),
        version: 1,
        units: document.units.clone(),
        board_count,
        panel_width,
        panel_depth,
        travel_length,
        placement_seconds,
        optimized: optimize,
        travel_saved: optimize.then_some(travel_saved),
        entries,
    })
}

fn nearest_neighbor<'a>(components: &[&'a Component]) -> Vec<&'a Component> {
    let mut remaining: Vec<&'a Component> = components.to_vec();
    remaining.sort_by(|left, right| left.refdes.cmp(&right.refdes));
    let mut x = 0.0;
    let mut y = 0.0;
    let mut ordered = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let mut best = 0;
        let mut best_distance = distance(x, y, remaining[0].x, remaining[0].y);
        for (index, component) in remaining.iter().enumerate().skip(1) {
            let candidate = distance(x, y, component.x, component.y);
            let tie = (candidate - best_distance).abs() <= 1e-9;
            if candidate < best_distance - 1e-9 || (tie && component.refdes < remaining[best].refdes) {
                best = index;
                best_distance = candidate;
            }
        }
        let chosen = remaining.remove(best);
        x = chosen.x;
        y = chosen.y;
        ordered.push(chosen);
    }
    ordered
}

/// Improves one side's route with 2-opt and returns it with the saved distance.
///
/// The origin stays first and the end is free. Each pass reverses the segment
/// that gives the largest strict improvement; ties keep the earliest segment, so
/// the result is deterministic. The run stops when no reversal improves by more
/// than [`OPTIMIZATION_EPSILON`] or after [`MAX_OPTIMIZATION_PASSES`] reversals.
fn two_opt(mut route: Vec<&Component>) -> (Vec<&Component>, f64) {
    let count = route.len();
    if count < 3 {
        return (route, 0.0);
    }
    let initial = route_travel(&route);
    let mut travel = initial;
    for _ in 0..MAX_OPTIMIZATION_PASSES {
        let mut best: Option<(usize, usize)> = None;
        let mut best_delta = -OPTIMIZATION_EPSILON;
        for first in 0..count - 1 {
            let (from_x, from_y) = if first == 0 { (0.0, 0.0) } else { (route[first - 1].x, route[first - 1].y) };
            for last in first + 1..count {
                let before_next = if last + 1 < count {
                    distance(route[last].x, route[last].y, route[last + 1].x, route[last + 1].y)
                }
                else {
                    0.0
                };
                let after_next = if last + 1 < count {
                    distance(route[first].x, route[first].y, route[last + 1].x, route[last + 1].y)
                }
                else {
                    0.0
                };
                let before = distance(from_x, from_y, route[first].x, route[first].y) + before_next;
                let after = distance(from_x, from_y, route[last].x, route[last].y) + after_next;
                let delta = after - before;
                if delta < best_delta {
                    best = Some((first, last));
                    best_delta = delta;
                }
            }
        }
        let Some((first, last)) = best
        else {
            break;
        };
        route[first..=last].reverse();
        travel += best_delta;
    }
    (route, (initial - travel).max(0.0))
}

/// Travel of `route` from the machine origin, in millimetres.
fn route_travel(route: &[&Component]) -> f64 {
    let mut x = 0.0;
    let mut y = 0.0;
    let mut travel = 0.0;
    for component in route {
        travel += distance(x, y, component.x, component.y);
        x = component.x;
        y = component.y;
    }
    travel
}

fn distance(from_x: f64, from_y: f64, to_x: f64, to_y: f64) -> f64 {
    let dx = to_x - from_x;
    let dy = to_y - from_y;
    (dx * dx + dy * dy).sqrt()
}
