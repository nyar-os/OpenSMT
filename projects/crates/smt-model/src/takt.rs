//! Read-only placement cycle time and throughput estimate.
//!
//! The estimate runs the same feeder allocation and placement sequence as the
//! `placement` report, then applies a small configurable machine model: a fixed
//! pick-and-place overhead per component, a constant head speed over the
//! program's travel and an optional feeder-change and nozzle-change overhead
//! per board. The result is split evenly over `machine_count` machines.
//! Nothing is written and the document is unchanged.

use serde::{Deserialize, Serialize};

use crate::{
    SmtError,
    balance::{MAX_MACHINES, MIN_MACHINES},
    model::SmtDocument,
    place::{self, HEAD_SPEED, PLACE_SECONDS},
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-takt";

/// Machine model of a takt estimate.
///
/// Every field has a serde default, so an empty request selects the defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaktOptions {
    /// Fixed pick-and-place overhead per component, in seconds. Defaults to
    /// [`PLACE_SECONDS`].
    pub place_seconds: f64,
    /// Head travel speed in millimetres per second. Defaults to [`HEAD_SPEED`].
    pub head_speed: f64,
    /// Time to change one feeder reel, in seconds. Defaults to `30`.
    pub feeder_change_seconds: f64,
    /// Feeder changes assumed per board. Defaults to `0`.
    pub feeder_changes: u32,
    /// Time to switch one nozzle, in seconds. Defaults to `5`.
    pub nozzle_change_seconds: f64,
    /// Nozzle changes assumed per board. The document carries no nozzle
    /// information, so this is a fixed caller-supplied count. Defaults to `0`.
    pub nozzle_changes: u32,
    /// Number of machines the work is split across evenly. Defaults to `1`.
    pub machine_count: u32,
    /// Whether the sequence is improved with the 2-opt pass first.
    pub optimize: bool,
}

impl Default for TaktOptions {
    fn default() -> Self {
        Self {
            place_seconds: PLACE_SECONDS,
            head_speed: HEAD_SPEED,
            feeder_change_seconds: 30.0,
            feeder_changes: 0,
            nozzle_change_seconds: 5.0,
            nozzle_changes: 0,
            machine_count: 1,
            optimize: false,
        }
    }
}

/// Cycle time and throughput report of one board.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaktReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Number of placement points on one board.
    pub components: u32,
    /// Nozzle pick-and-place cycles per board; one per component.
    pub picks: u32,
    /// Head travel of the program, in millimetres, matching the sequence.
    pub travel_mm: f64,
    /// Placement time of the `placeSeconds` model, in seconds.
    pub place_seconds: f64,
    /// Travel time of the `headSpeed` model, in seconds.
    pub travel_seconds: f64,
    /// Feeder and nozzle change overhead, in seconds.
    pub overhead_seconds: f64,
    /// Estimated time of one board on one machine, in seconds.
    pub board_seconds: f64,
    /// Number of machines the work is split across.
    pub machine_count: u32,
    /// `boardSeconds / machineCount`, the effective per-machine board time.
    pub machine_seconds: f64,
    /// Boards produced per hour; zero when a machine takes zero seconds.
    pub boards_per_hour: f64,
    /// Components placed per hour across every machine.
    pub components_per_hour: f64,
    /// Whether the sequence was produced with the 2-opt pass.
    #[serde(default, skip_serializing_if = "is_false")]
    pub optimized: bool,
    /// Millimetres saved by 2-opt relative to nearest neighbour; present only
    /// when `optimize` was requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub travel_saved: Option<f64>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Estimates the cycle time and throughput of one board.
///
/// The document is allocated and sequenced exactly like the `placement` report,
/// so [`TaktReport::travel_mm`] matches [`crate::PlacementReport::travel_length`].
/// A zero board time yields zero throughput instead of an infinite rate.
/// Invalid machine-model parameters report `takt/invalid`; anything the
/// allocation or the sequence rejects (a missing or undersized feeder, a broken
/// document) fails with its own code. The document is not modified.
pub fn estimate(document: &SmtDocument, options: &TaktOptions) -> Result<TaktReport, SmtError> {
    if !non_negative(options.place_seconds) {
        return Err(SmtError::new("takt/invalid", "place seconds must be finite and not negative"));
    }
    if !positive(options.head_speed) {
        return Err(SmtError::new("takt/invalid", "head speed must be finite and greater than zero"));
    }
    if !non_negative(options.feeder_change_seconds) {
        return Err(SmtError::new("takt/invalid", "feeder change seconds must be finite and not negative"));
    }
    if !non_negative(options.nozzle_change_seconds) {
        return Err(SmtError::new("takt/invalid", "nozzle change seconds must be finite and not negative"));
    }
    if !(MIN_MACHINES..=MAX_MACHINES).contains(&options.machine_count) {
        return Err(SmtError::new(
            "takt/invalid",
            format!("machine count must be between {MIN_MACHINES} and {MAX_MACHINES}, got {}", options.machine_count),
        ));
    }

    let feeders = place::allocate(document)?;
    let placement = place::sequence_with(document, &feeders, options.optimize)?;

    let components = placement.entries.len() as u32;
    let place_seconds = f64::from(components) * options.place_seconds;
    let travel_seconds = placement.travel_length / options.head_speed;
    let overhead_seconds = f64::from(options.feeder_changes) * options.feeder_change_seconds
        + f64::from(options.nozzle_changes) * options.nozzle_change_seconds;
    let board_seconds = place_seconds + travel_seconds + overhead_seconds;
    let machine_seconds = board_seconds / f64::from(options.machine_count);
    let boards_per_hour = if machine_seconds > 0.0 { 3600.0 / machine_seconds } else { 0.0 };
    let components_per_hour = f64::from(components) * boards_per_hour;

    Ok(TaktReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        components,
        picks: components,
        travel_mm: placement.travel_length,
        place_seconds,
        travel_seconds,
        overhead_seconds,
        board_seconds,
        machine_count: options.machine_count,
        machine_seconds,
        boards_per_hour,
        components_per_hour,
        optimized: placement.optimized,
        travel_saved: placement.travel_saved,
    })
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn non_negative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}
