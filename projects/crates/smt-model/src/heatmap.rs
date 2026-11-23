//! Read-only takt bottleneck heat map data.
//!
//! The report runs the same feeder allocation and nearest-neighbour sequence as
//! the default `takt` estimate and attributes the time of every program step to
//! the step itself, its package and its side. A step's time is the fixed
//! placement overhead plus the inbound travel of the same side divided by the
//! head speed, so the step times sum to the `takt` board time of the default
//! machine model with no change overhead. Shares are fractions of that total,
//! and the cumulative package share lets a caller draw a Pareto heat map.
//! Nothing is written and the document is unchanged.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    SmtError,
    model::{Side, SmtDocument},
    place::{self, HEAD_SPEED, PLACE_SECONDS},
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-heatmap";

/// One program step with its attributed time.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatStep {
    /// Placement step number, starting at 1 in program order.
    pub sequence: u32,
    /// Reference designator.
    pub refdes: String,
    /// Package identifier.
    pub package: String,
    /// Board side.
    pub side: Side,
    /// Feeder lane that supplies the component.
    pub feeder: String,
    /// Placement X coordinate on one board.
    pub x: f64,
    /// Placement Y coordinate on one board.
    pub y: f64,
    /// Inbound travel from the previous step of the same side, in millimetres.
    pub travel_mm: f64,
    /// Fixed placement overhead of the step, in seconds.
    pub place_seconds: f64,
    /// Travel segment of the step, in seconds.
    pub travel_seconds: f64,
    /// `placeSeconds + travelSeconds`.
    pub seconds: f64,
    /// Fraction of the board total, in `[0, 1]`.
    pub share: f64,
}

/// Time aggregated by package, ordered from the largest share downwards.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatPackage {
    /// Package identifier.
    pub package: String,
    /// Number of program steps of this package.
    pub steps: u32,
    /// Summed placement overhead, in seconds.
    pub place_seconds: f64,
    /// Summed travel segment, in seconds.
    pub travel_seconds: f64,
    /// `placeSeconds + travelSeconds`.
    pub seconds: f64,
    /// Fraction of the board total, in `[0, 1]`.
    pub share: f64,
    /// Running share of this package and every package before it.
    pub cumulative_share: f64,
}

/// Time aggregated by board side.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatSide {
    /// Board side.
    pub side: Side,
    /// Number of program steps on the side.
    pub steps: u32,
    /// Summed step time, in seconds.
    pub seconds: f64,
    /// Fraction of the board total, in `[0, 1]`.
    pub share: f64,
}

/// Takt bottleneck heat map of one board.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Number of placement steps on one board.
    pub components: u32,
    /// Summed placement overhead, in seconds.
    pub place_seconds: f64,
    /// Summed travel segment, in seconds.
    pub travel_seconds: f64,
    /// `placeSeconds + travelSeconds`, the board time of the default model.
    pub total_seconds: f64,
    /// Package with the largest step time; `null` for an empty document.
    pub bottleneck: Option<String>,
    /// Packages from the largest share downwards.
    pub packages: Vec<HeatPackage>,
    /// Sides that carry at least one step, `top` before `bottom`.
    pub sides: Vec<HeatSide>,
    /// Steps in program order.
    pub steps: Vec<HeatStep>,
}

/// Builds the takt bottleneck heat map of a document.
///
/// Every side restarts at the machine origin, exactly like the placement
/// program, so the inbound travel of a side's first step is measured from
/// `(0, 0)`. Step times sum to the default `takt` board time. Packages are
/// ordered by descending time, ties by package name; sides keep `top` before
/// `bottom`. The allocation and the sequence run like the placement program, so
/// feeder coverage errors propagate. The document is not modified.
pub fn heatmap(document: &SmtDocument) -> Result<HeatmapReport, SmtError> {
    let allocation = place::allocate(document)?;
    let placement = place::sequence(document, &allocation)?;

    let mut steps = Vec::with_capacity(placement.entries.len());
    let mut side: Option<Side> = None;
    let mut x = 0.0;
    let mut y = 0.0;
    for entry in &placement.entries {
        if side != Some(entry.side) {
            x = 0.0;
            y = 0.0;
            side = Some(entry.side);
        }
        let travel_mm = place::distance(x, y, entry.x, entry.y);
        x = entry.x;
        y = entry.y;
        let travel_seconds = travel_mm / HEAD_SPEED;
        steps.push(HeatStep {
            sequence: entry.index.saturating_add(1),
            refdes: entry.refdes.clone(),
            package: entry.package.clone(),
            side: entry.side,
            feeder: entry.feeder.clone().unwrap_or_default(),
            x: entry.x,
            y: entry.y,
            travel_mm,
            place_seconds: PLACE_SECONDS,
            travel_seconds,
            seconds: PLACE_SECONDS + travel_seconds,
            share: 0.0,
        });
    }

    let total_seconds: f64 = steps.iter().map(|step| step.seconds).sum();
    let place_seconds = steps.len() as f64 * PLACE_SECONDS;
    let travel_seconds = placement.travel_length / HEAD_SPEED;
    for step in &mut steps {
        step.share = share(step.seconds, total_seconds);
    }

    let mut by_package: BTreeMap<&str, (u32, f64, f64)> = BTreeMap::new();
    let mut by_side: BTreeMap<&str, (Side, u32, f64)> = BTreeMap::new();
    for step in &steps {
        let package = by_package.entry(step.package.as_str()).or_insert((0, 0.0, 0.0));
        package.0 += 1;
        package.1 += step.place_seconds;
        package.2 += step.travel_seconds;
        let side = by_side.entry(step.side.as_str()).or_insert((step.side, 0, 0.0));
        side.1 += 1;
        side.2 += step.seconds;
    }

    let mut packages: Vec<HeatPackage> = by_package
        .into_iter()
        .map(|(package, (steps, place_seconds, travel_seconds))| {
            let seconds = place_seconds + travel_seconds;
            HeatPackage {
                package: package.to_string(),
                steps,
                place_seconds,
                travel_seconds,
                seconds,
                share: share(seconds, total_seconds),
                cumulative_share: 0.0,
            }
        })
        .collect();
    packages.sort_by(|left, right| {
        right
            .seconds
            .partial_cmp(&left.seconds)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.package.cmp(&right.package))
    });
    let mut cumulative = 0.0;
    for package in &mut packages {
        cumulative += package.share;
        package.cumulative_share = cumulative;
    }

    let sides = [Side::Top, Side::Bottom]
        .into_iter()
        .filter_map(|side| by_side.remove(side.as_str()))
        .map(|(side, steps, seconds)| HeatSide { side, steps, seconds, share: share(seconds, total_seconds) })
        .collect();

    let bottleneck = packages.first().map(|package| package.package.clone());
    Ok(HeatmapReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        components: steps.len() as u32,
        place_seconds,
        travel_seconds,
        total_seconds,
        bottleneck,
        packages,
        sides,
        steps,
    })
}

/// Fraction of a total, zero for an empty total.
fn share(value: f64, total: f64) -> f64 {
    if total > 0.0 { value / total } else { 0.0 }
}
