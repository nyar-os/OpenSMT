//! Read-only feeder replenishment schedule across a production run.
//!
//! The schedule runs the same feeder allocation as the consumption plan, so a
//! lane's picks per board equal its `assigned` count. Boards are simulated in
//! order from the stored `loaded` quantity. Whenever a lane cannot cover the
//! next board it installs one full reel and records the change after the board
//! that consumed its stock. A splice joins one new reel at its placement step on
//! the first board, and per-reel leader/trailer waste and manual scrap are
//! charged without resetting the cumulative used quantity. Splice downtime,
//! partial reels and moisture exposure are not modelled. The report is read-only
//! and deterministic.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    SmtError,
    model::SmtDocument,
    place::{self, FeederUsage, PlacementEntry},
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-feeder-schedule";

/// Reason recorded when a lane installs a full replacement reel.
pub const REASON_REPLENISH: &str = "replenish";

/// Reason recorded when a splice joins a new reel.
pub const REASON_SPLICE: &str = "splice";

/// One reel change of a feeder lane.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReelChange {
    /// One-based board after which the new reel is installed, so the reel is
    /// available before the next board.
    pub board: u32,
    /// Cumulative reels on the lane after the change, the initial reel counted.
    pub reels: u64,
    /// Reason of the change: [`REASON_REPLENISH`] or [`REASON_SPLICE`].
    pub reason: String,
    /// Placement step of a splice, the 1-based program step at which the new
    /// reel is joined; `null` for a replenishment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u32>,
}

/// One splice resolved against the placement program.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneSplice {
    /// 1-based placement program step at which the new reel is joined.
    pub sequence: u32,
    /// Lane picks in the program before this step.
    pub picks_before: u32,
    /// Additional leader waste charged at the joint.
    pub leader_waste: u32,
    /// Additional trailer waste charged at the joint.
    pub trailer_waste: u32,
    /// Manual scrap charged at the joint.
    pub scrap: u32,
}

/// Waste profile of one lane, copied from its feeder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LaneWaste {
    /// Leader tape components wasted when a reel is installed.
    pub leader_waste: u32,
    /// Trailer tape components wasted when a reel is exhausted.
    pub trailer_waste: u32,
    /// Extra manual scrap for the lane.
    pub scrap: u32,
}

/// Replenishment schedule of one feeder lane.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleLane {
    /// Feeder identifier.
    pub id: String,
    /// Package supplied by the lane.
    pub package: String,
    /// Components the lane supplies on one board.
    pub picks_per_board: u32,
    /// Maximum parts on one reel of this lane.
    pub capacity: u32,
    /// Parts loaded on the lane before the run.
    pub initial_loaded: u32,
    /// Net parts the lane places over the whole run.
    pub picks: u64,
    /// Gross parts drawn from reels: `picks` plus every scrap charge.
    pub gross: u64,
    /// Leader/trailer waste and manual scrap over the run.
    pub scrap: u64,
    /// Splice events applied to this lane.
    pub splices: u32,
    /// Reels the run uses, the initial reel plus every splice and replenishment.
    pub reels: u64,
    /// Reel changes in board order.
    pub changes: Vec<ReelChange>,
    /// Whether the stock cannot cover a board at any point of the run.
    pub short: bool,
    /// Parts left on the last reel after the final board.
    pub remaining_after_run: u64,
}

/// Read-only feeder replenishment schedule.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeederScheduleReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Boards the schedule covers.
    pub boards: u32,
    /// Number of reel changes across every lane.
    pub change_count: u32,
    /// Number of reels across every lane, the initial reels included.
    pub reel_count: u64,
    /// Net parts across every lane.
    pub total_picks: u64,
    /// Gross parts across every lane.
    pub total_gross: u64,
    /// Scrap across every lane.
    pub total_scrap: u64,
    /// Splice events across every lane.
    pub splices: u32,
    /// Whether every lane covers the run.
    pub ok: bool,
    /// One entry per feeder lane, in id order.
    pub lanes: Vec<ScheduleLane>,
}

/// Plans the feeder replenishment schedule of `boards` boards.
///
/// A board count of zero reports `schedule/invalid`. The document is allocated
/// exactly like the `feeders` report, so a missing or undersized feeder fails
/// with `feeder/missing` or `feeder/capacity`. Spliced reels, per-reel
/// leader/trailer waste and manual scrap are applied as plan data. A short lane
/// is still simulated: it consumes what it has (never below zero) and
/// replenishes when it can, and the report marks it `short` instead of failing.
/// The schedule is read-only and deterministic.
pub fn plan(document: &SmtDocument, boards: u32) -> Result<FeederScheduleReport, SmtError> {
    if boards == 0 {
        return Err(SmtError::new("schedule/invalid", "board count must be at least 1"));
    }
    let allocation = place::allocate(document)?;
    let placement = place::sequence(document, &allocation)?;
    Ok(report_full(
        &document.units,
        boards,
        &allocation.feeders,
        &lane_waste(document),
        &lane_splices(document, &placement.entries),
    ))
}

/// Builds the schedule report from an allocation and a board count.
///
/// `units` is copied into the report and `boards` must be at least 1; this is
/// the pure report builder behind [`plan`], which validates and allocates the
/// document first. Lanes keep the order of `feeders`. A lane whose stock cannot
/// cover a board, or whose zero capacity leaves demand uncovered, is reported as
/// short and still simulated deterministically.
pub fn report(units: &str, boards: u32, feeders: &[FeederUsage]) -> FeederScheduleReport {
    report_full(units, boards, feeders, &BTreeMap::new(), &BTreeMap::new())
}

fn report_full(
    units: &str,
    boards: u32,
    feeders: &[FeederUsage],
    wastes: &BTreeMap<String, LaneWaste>,
    splices: &BTreeMap<String, Vec<LaneSplice>>,
) -> FeederScheduleReport {
    let lanes: Vec<ScheduleLane> = feeders
        .iter()
        .map(|lane| {
            let waste = wastes.get(&lane.id).copied().unwrap_or_default();
            let lane_splices = splices.get(&lane.id).map(Vec::as_slice).unwrap_or(&[]);
            simulate(lane, boards, waste, lane_splices)
        })
        .collect();
    let change_count = lanes.iter().map(|lane| lane.changes.len() as u32).sum();
    let reel_count = lanes.iter().map(|lane| lane.reels).sum();
    let total_picks = lanes.iter().map(|lane| lane.picks).sum();
    let total_gross = lanes.iter().map(|lane| lane.gross).sum();
    let total_scrap = lanes.iter().map(|lane| lane.scrap).sum();
    let splices = lanes.iter().map(|lane| lane.splices).sum();
    FeederScheduleReport {
        format: FORMAT.to_string(),
        version: 1,
        units: units.to_string(),
        boards,
        change_count,
        reel_count,
        total_picks,
        total_gross,
        total_scrap,
        splices,
        ok: !lanes.iter().any(|lane| lane.short),
        lanes,
    }
}

/// Builds the replenishment schedule of one allocated lane with no waste.
pub fn plan_lane(lane: &FeederUsage, boards: u32) -> ScheduleLane {
    simulate(lane, boards, LaneWaste::default(), &[])
}

pub(crate) fn simulate(lane: &FeederUsage, boards: u32, waste: LaneWaste, splices: &[LaneSplice]) -> ScheduleLane {
    let picks_per_board = u64::from(lane.assigned);
    let capacity = u64::from(lane.capacity);
    let per_reel_waste = u64::from(waste.leader_waste.saturating_add(waste.trailer_waste));
    let usable = capacity.saturating_sub(per_reel_waste);
    let has_reel = picks_per_board > 0 || !splices.is_empty();
    let mut remaining =
        if has_reel { u64::from(lane.loaded).saturating_sub(per_reel_waste).saturating_sub(u64::from(waste.scrap)) } else { 0 };
    let mut reels = u64::from(has_reel);
    let mut scrap = if has_reel { per_reel_waste + u64::from(waste.scrap) } else { 0 };
    let mut short = false;
    let mut changes = Vec::new();
    let mut picks_done = 0u64;

    for splice in splices {
        let target = u64::from(splice.picks_before).min(picks_per_board);
        let demand = target.saturating_sub(picks_done);
        consume(&mut remaining, demand, &mut short);
        picks_done = target;
        let splice_usable = usable.saturating_sub(u64::from(splice.leader_waste.saturating_add(splice.trailer_waste)));
        remaining = remaining.saturating_add(splice_usable).saturating_sub(u64::from(splice.scrap));
        scrap += per_reel_waste + u64::from(splice.leader_waste) + u64::from(splice.trailer_waste) + u64::from(splice.scrap);
        reels += 1;
        changes.push(ReelChange { board: 1, reels, reason: REASON_SPLICE.to_string(), sequence: Some(splice.sequence) });
    }
    consume(&mut remaining, picks_per_board.saturating_sub(picks_done), &mut short);
    if boards > 1 && usable > 0 && remaining < picks_per_board {
        remaining += usable;
        scrap += per_reel_waste;
        reels += 1;
        changes.push(ReelChange { board: 1, reels, reason: REASON_REPLENISH.to_string(), sequence: None });
    }

    for board in 2..=boards {
        consume(&mut remaining, picks_per_board, &mut short);
        if board < boards && usable > 0 && remaining < picks_per_board {
            remaining += usable;
            scrap += per_reel_waste;
            reels += 1;
            changes.push(ReelChange { board, reels, reason: REASON_REPLENISH.to_string(), sequence: None });
        }
    }

    let picks = picks_per_board * u64::from(boards);
    ScheduleLane {
        id: lane.id.clone(),
        package: lane.package.clone(),
        picks_per_board: lane.assigned,
        capacity: lane.capacity,
        initial_loaded: lane.loaded,
        picks,
        gross: picks + scrap,
        scrap,
        splices: splices.len() as u32,
        reels,
        changes,
        short,
        remaining_after_run: remaining,
    }
}

fn consume(remaining: &mut u64, demand: u64, short: &mut bool) {
    if *remaining < demand {
        *short = true;
        *remaining = 0;
    }
    else {
        *remaining -= demand;
    }
}

/// Copies the waste profile of every feeder, keyed by feeder id.
pub(crate) fn lane_waste(document: &SmtDocument) -> BTreeMap<String, LaneWaste> {
    document
        .feeders
        .iter()
        .map(|feeder| {
            (
                feeder.id.clone(),
                LaneWaste { leader_waste: feeder.leader_waste, trailer_waste: feeder.trailer_waste, scrap: feeder.scrap },
            )
        })
        .collect()
}

/// Resolves every stored splice against the placement program, keyed by lane.
///
/// The lane's picks before the splice are counted from the 1-based program step,
/// so the schedule applies the new reel exactly when the head reaches it. Events
/// are sorted by step, then feeder id for a stable order.
pub(crate) fn lane_splices(document: &SmtDocument, entries: &[PlacementEntry]) -> BTreeMap<String, Vec<LaneSplice>> {
    let mut lanes: BTreeMap<String, Vec<LaneSplice>> = BTreeMap::new();
    for splice in &document.splices {
        let before = usize::try_from(splice.sequence.saturating_sub(1)).unwrap_or(usize::MAX).min(entries.len());
        let picks_before =
            entries[..before].iter().filter(|entry| entry.feeder.as_deref() == Some(splice.feeder.as_str())).count();
        lanes.entry(splice.feeder.clone()).or_default().push(LaneSplice {
            sequence: splice.sequence,
            picks_before: u32::try_from(picks_before).unwrap_or(u32::MAX),
            leader_waste: splice.leader_waste,
            trailer_waste: splice.trailer_waste,
            scrap: splice.scrap,
        });
    }
    for lane in lanes.values_mut() {
        lane.sort_by_key(|splice| (splice.picks_before, splice.sequence));
    }
    lanes
}
