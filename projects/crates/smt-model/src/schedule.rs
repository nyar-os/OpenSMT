//! Read-only feeder replenishment schedule across a production run.
//!
//! The schedule runs the same feeder allocation as the consumption plan, so a
//! lane's picks per board equal its `assigned` count. Boards are simulated in
//! order from the stored `loaded` quantity. Whenever a lane cannot cover the
//! next board it installs one full reel and records the change after the board
//! that consumed its stock. The report is read-only and deterministic.

use serde::Serialize;

use crate::{
    SmtError,
    model::SmtDocument,
    place::{self, FeederUsage},
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-feeder-schedule";

/// Reason recorded for every reel change of this report.
pub const REASON_REPLENISH: &str = "replenish";

/// One reel change of a feeder lane.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReelChange {
    /// One-based board after which the new reel is installed, so the reel is
    /// available before the next board.
    pub board: u32,
    /// Cumulative reels on the lane after the change, the initial reel counted.
    pub reels: u64,
    /// Reason of the change; always [`REASON_REPLENISH`].
    pub reason: String,
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
    /// Parts the lane consumes over the whole run.
    pub picks: u64,
    /// Reels the run uses, the initial reel plus every replenishment.
    pub reels: u64,
    /// Reel changes in board order.
    pub changes: Vec<ReelChange>,
    /// Whether the initial stock cannot cover the first board, or a zero
    /// capacity leaves demand uncovered.
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
    /// Whether every lane covers the run.
    pub ok: bool,
    /// One entry per feeder lane, in id order.
    pub lanes: Vec<ScheduleLane>,
}

/// Plans the feeder replenishment schedule of `boards` boards.
///
/// A board count of zero reports `schedule/invalid`. The document is allocated
/// exactly like the `feeders` report, so a missing or undersized feeder fails
/// with `feeder/missing` or `feeder/capacity`. A short lane is still simulated:
/// it consumes what it has (never below zero) and replenishes when it can, and
/// the report marks it `short` instead of failing. The schedule is read-only and
/// deterministic.
pub fn plan(document: &SmtDocument, boards: u32) -> Result<FeederScheduleReport, SmtError> {
    if boards == 0 {
        return Err(SmtError::new("schedule/invalid", "board count must be at least 1"));
    }
    let allocation = place::allocate(document)?;
    Ok(report(&document.units, boards, &allocation.feeders))
}

/// Builds the schedule report from an allocation and a board count.
///
/// `units` is copied into the report and `boards` must be at least 1; this is
/// the pure report builder behind [`plan`], which validates and allocates the
/// document first. Lanes keep the order of `feeders`. A lane whose initial stock
/// cannot cover the first board, or whose zero capacity leaves demand
/// uncovered, is reported as short and still simulated deterministically.
pub fn report(units: &str, boards: u32, feeders: &[FeederUsage]) -> FeederScheduleReport {
    let lanes: Vec<ScheduleLane> = feeders.iter().map(|lane| plan_lane(lane, boards)).collect();
    let change_count = lanes.iter().map(|lane| lane.changes.len() as u32).sum();
    let reel_count = lanes.iter().map(|lane| lane.reels).sum();
    FeederScheduleReport {
        format: FORMAT.to_string(),
        version: 1,
        units: units.to_string(),
        boards,
        change_count,
        reel_count,
        ok: !lanes.iter().any(|lane| lane.short),
        lanes,
    }
}

/// Builds the replenishment schedule of one allocated lane.
///
/// The lane starts with its loaded quantity, consumes its per-board picks after
/// every board and installs one full reel after a board when the remaining stock
/// cannot cover the next one. A short lane still consumes from what it has
/// (never below zero) and replenishes whenever it can, so the result is always
/// deterministic.
pub fn plan_lane(lane: &FeederUsage, boards: u32) -> ScheduleLane {
    let picks_per_board = u64::from(lane.assigned);
    let capacity = u64::from(lane.capacity);
    let mut remaining = u64::from(lane.loaded);
    let mut reels = u64::from(picks_per_board > 0);
    let mut changes = Vec::new();
    for board in 1..=boards {
        remaining = remaining.saturating_sub(picks_per_board);
        // The new reel is installed after this board, before the next one.
        if board < boards && capacity > 0 && remaining < picks_per_board {
            remaining += capacity;
            reels += 1;
            changes.push(ReelChange { board, reels, reason: REASON_REPLENISH.to_string() });
        }
    }

    ScheduleLane {
        id: lane.id.clone(),
        package: lane.package.clone(),
        picks_per_board: lane.assigned,
        capacity: lane.capacity,
        initial_loaded: lane.loaded,
        picks: picks_per_board * u64::from(boards),
        reels,
        changes,
        short: u64::from(lane.loaded) < picks_per_board || (capacity == 0 && picks_per_board > 0),
        remaining_after_run: remaining,
    }
}
