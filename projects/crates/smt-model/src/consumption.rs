//! Read-only feeder consumption plan for a build quantity.
//!
//! The plan runs the same feeder allocation as the placement program, then
//! multiplies every lane's per-board picks by the requested board count. Reels
//! are derived from lane capacity, splices and waste, so net picks, gross
//! consumption and leader/trailer or manual scrap are reported separately.
//! Nothing is written and the document is unchanged.

use serde::Serialize;

use crate::{
    SmtError,
    model::SmtDocument,
    place::{self, FeederUsage},
    schedule::{self, LaneSplice, LaneWaste},
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
    /// Net parts the lane places over the whole run.
    pub picks: u64,
    /// Gross parts drawn from reels: `picks` plus every scrap charge.
    pub gross: u64,
    /// Leader/trailer waste and manual scrap over the run.
    pub scrap: u64,
    /// Splice events applied to this lane.
    pub splices: u32,
    /// Maximum parts on one reel of this lane.
    pub capacity: u32,
    /// Parts currently loaded on the lane.
    pub loaded: u32,
    /// Reels needed for the run; zero when the lane supplies nothing.
    pub reels: u64,
    /// Net parts placed from the last reel; zero when no reel is needed.
    pub remainder: u32,
    /// Whether the currently loaded quantity and splice reels cover gross
    /// consumption.
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
    /// Net parts of this package placed over the run.
    pub total_picks: u64,
    /// Gross parts of this package drawn from reels.
    pub total_gross: u64,
    /// Leader/trailer waste and manual scrap of this package.
    pub scrap: u64,
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
    /// Net parts the run places across every lane.
    pub total_picks: u64,
    /// Gross parts the run draws from reels across every lane.
    pub total_gross: u64,
    /// Leader/trailer waste and manual scrap across every lane.
    pub total_scrap: u64,
    /// Splice events across every lane.
    pub splices: u32,
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
    let placement = place::sequence(document, &allocation)?;
    let wastes = schedule::lane_waste(document);
    let splices = schedule::lane_splices(document, &placement.entries);
    let feeders: Vec<FeederConsumption> = allocation
        .feeders
        .iter()
        .map(|lane| {
            let waste = wastes.get(&lane.id).copied().unwrap_or_default();
            let lane_splices = splices.get(&lane.id).map(Vec::as_slice).unwrap_or(&[]);
            consumption_of(lane, boards, waste, lane_splices)
        })
        .collect();
    let run = u64::from(boards);

    let packages = package_counts(document)
        .into_iter()
        .map(|(package, components_per_board)| {
            let lanes: Vec<&FeederConsumption> = feeders.iter().filter(|lane| lane.package == package).collect();
            PackageConsumption {
                package,
                components_per_board,
                total_picks: u64::from(components_per_board) * run,
                total_gross: lanes.iter().map(|lane| lane.gross).sum(),
                scrap: lanes.iter().map(|lane| lane.scrap).sum(),
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
        total_gross: feeders.iter().map(|lane| lane.gross).sum(),
        total_scrap: feeders.iter().map(|lane| lane.scrap).sum(),
        splices: feeders.iter().map(|lane| lane.splices).sum(),
        packages,
        feeders,
        ok,
    })
}

fn consumption_of(lane: &FeederUsage, boards: u32, waste: LaneWaste, splices: &[LaneSplice]) -> FeederConsumption {
    let picks = u64::from(lane.assigned) * u64::from(boards);
    let per_reel = u64::from(waste.leader_waste.saturating_add(waste.trailer_waste));
    let usable = u64::from(lane.capacity).saturating_sub(per_reel);
    let splice_count = splices.len() as u64;
    let splice_extra: u64 =
        splices.iter().map(|splice| u64::from(splice.leader_waste.saturating_add(splice.trailer_waste))).sum();
    let manual: u64 = u64::from(waste.scrap) + splices.iter().map(|splice| u64::from(splice.scrap)).sum::<u64>();
    let initial_reels = if picks > 0 || !splices.is_empty() { 1 + splice_count } else { 0 };
    let demand = picks + manual + splice_extra;
    let supply = initial_reels * usable;
    let extra_reels = if usable == 0 { 0 } else { demand.saturating_sub(supply).div_ceil(usable) };
    let reels = initial_reels + extra_reels;
    let scrap = reels * per_reel + splice_extra + manual;
    let gross = picks + scrap;
    let remainder = if reels == 0 { 0 } else { picks.saturating_sub((reels - 1) * usable).min(u64::from(u32::MAX)) as u32 };
    let available = u64::from(lane.loaded) + splice_count * usable;
    FeederConsumption {
        id: lane.id.clone(),
        package: lane.package.clone(),
        picks_per_board: lane.assigned,
        picks,
        gross,
        scrap,
        splices: splices.len() as u32,
        capacity: lane.capacity,
        loaded: lane.loaded,
        reels,
        remainder,
        short: gross > available,
    }
}
