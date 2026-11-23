//! Read-only feeder consumption plan for a build quantity.
//!
//! The plan runs the same feeder allocation as the placement program, then
//! multiplies every lane's per-board picks by the requested board count. Reels
//! are derived from the lane capacity, so the report reconciles the packaging
//! quantity of each package with the parts the run consumes. Nothing is written
//! and the document is unchanged.

use serde::Serialize;

use crate::{
    SmtError,
    model::SmtDocument,
    place::{self, FeederUsage},
    validate::package_counts,
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-consumption";

/// Consumption of one feeder lane over the run.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeederConsumption {
    /// Feeder identifier.
    pub id: String,
    /// Package supplied by the lane.
    pub package: String,
    /// Components the lane supplies on one board.
    pub picks_per_board: u32,
    /// Parts the lane consumes over the whole run.
    pub picks: u64,
    /// Maximum parts on one reel of this lane.
    pub capacity: u32,
    /// Parts currently loaded on the lane.
    pub loaded: u32,
    /// Full reels needed for the run; zero when the lane supplies nothing.
    pub reels: u64,
    /// Parts on the last reel, between 1 and `capacity`; zero when no reel is
    /// needed.
    pub remainder: u32,
    /// Whether the currently loaded quantity covers the run.
    pub short: bool,
}

/// Consumption of one package over the run.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageConsumption {
    /// Package identifier.
    pub package: String,
    /// Components of this package on one board.
    pub components_per_board: u32,
    /// Parts of this package consumed over the run.
    pub total_picks: u64,
    /// Lanes that supply this package.
    pub feeders: u32,
    /// Reels needed across every lane of this package.
    pub reels: u64,
    /// Whether any lane of this package is short for the run.
    pub short: bool,
}

/// Read-only feeder consumption plan.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumptionReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Boards the plan covers.
    pub boards: u32,
    /// Components on one board.
    pub components_per_board: u32,
    /// Parts the run consumes across every lane.
    pub total_picks: u64,
    /// One entry per used package, in package order.
    pub packages: Vec<PackageConsumption>,
    /// One entry per feeder lane, in id order.
    pub feeders: Vec<FeederConsumption>,
    /// Whether the loaded quantities cover the run.
    pub ok: bool,
}

/// Plans the feeder consumption of `boards` boards.
///
/// A board count of zero reports `consumption/invalid`. The document is
/// allocated exactly like the `feeders` report, so a missing or undersized
/// feeder fails with `feeder/missing` or `feeder/capacity`. Reels use the lane
/// capacity, the last reel reports its `remainder`, and `short` marks a lane
/// whose currently loaded quantity does not cover the run. The plan is read-only
/// and deterministic.
pub fn plan(document: &SmtDocument, boards: u32) -> Result<ConsumptionReport, SmtError> {
    if boards == 0 {
        return Err(SmtError::new("consumption/invalid", "board count must be at least 1"));
    }
    let allocation = place::allocate(document)?;
    let run = u64::from(boards);
    let feeders: Vec<FeederConsumption> = allocation.feeders.iter().map(|lane| consumption_of(lane, run)).collect();

    let packages = package_counts(document)
        .into_iter()
        .map(|(package, components_per_board)| {
            let lanes: Vec<&FeederConsumption> = feeders.iter().filter(|lane| lane.package == package).collect();
            PackageConsumption {
                package,
                components_per_board,
                total_picks: u64::from(components_per_board) * run,
                feeders: lanes.len() as u32,
                reels: lanes.iter().map(|lane| lane.reels).sum(),
                short: lanes.iter().any(|lane| lane.short),
            }
        })
        .collect();

    let ok = !feeders.iter().any(|lane| lane.short);
    Ok(ConsumptionReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        boards,
        components_per_board: document.components.len() as u32,
        total_picks: (document.components.len() as u64) * run,
        packages,
        feeders,
        ok,
    })
}

fn consumption_of(lane: &FeederUsage, run: u64) -> FeederConsumption {
    let picks = u64::from(lane.assigned) * run;
    let capacity = u64::from(lane.capacity);
    let reels = if picks == 0 { 0 } else { picks.div_ceil(capacity) };
    let remainder = if reels == 0 { 0 } else { (picks - (reels - 1) * capacity) as u32 };
    FeederConsumption {
        id: lane.id.clone(),
        package: lane.package.clone(),
        picks_per_board: lane.assigned,
        picks,
        capacity: lane.capacity,
        loaded: lane.loaded,
        reels,
        remainder,
        short: picks > u64::from(lane.loaded),
    }
}
