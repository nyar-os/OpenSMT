//! Reflow profile evaluation.
//!
//! The model assumes each zone is entered at the previous zone's set-point,
//! starting from [`AMBIENT`] degrees Celsius, and ramps linearly to its own
//! set-point over `length / conveyor_speed`. Time above liquidus is integrated
//! over those linear ramps, so a profile that crosses the liquidus inside a zone
//! is still measured exactly.

use serde::Serialize;

use crate::{SmtError, model::SmtDocument, validate};

/// Temperature assumed at the conveyor entrance, in degrees Celsius.
pub const AMBIENT: f64 = 25.0;

/// One zone of the evaluated profile.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneReport {
    /// Zone name.
    pub name: String,
    /// Zone length along the conveyor.
    pub length: f64,
    /// Temperature entering the zone.
    pub entry_temperature: f64,
    /// Set-point temperature leaving the zone.
    pub exit_temperature: f64,
    /// Time spent in the zone, in seconds.
    pub seconds: f64,
    /// Time above the liquidus spent in this zone, in seconds.
    pub seconds_above_liquidus: f64,
}

/// Evaluated reflow profile.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflowReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Unit of every temperature field.
    pub temperature_unit: String,
    /// Conveyor speed in millimetres per minute.
    pub conveyor_speed: f64,
    /// Alloy liquidus in degrees Celsius.
    pub liquidus: f64,
    /// Highest set-point temperature.
    pub peak_temperature: f64,
    /// Total time above liquidus, in seconds.
    pub time_above_liquidus: f64,
    /// Total time from entrance to exit, in seconds.
    pub total_seconds: f64,
    /// One entry per zone, in conveyor order.
    pub zones: Vec<ZoneReport>,
}

/// Evaluates the reflow profile of a document.
///
/// The document is validated first, so an inconsistent profile returns an error
/// instead of a partial evaluation.
pub fn evaluate(document: &SmtDocument) -> Result<ReflowReport, SmtError> {
    let report = validate::validate(document);
    if let Some(error) = validate::first_error(&report) {
        return Err(error);
    }

    let profile = &document.profile;
    let mut temperature = AMBIENT;
    let mut time_above_liquidus = 0.0;
    let mut total_seconds = 0.0;
    let mut peak_temperature = AMBIENT;
    let mut zones = Vec::with_capacity(profile.zones.len());

    for zone in &profile.zones {
        let seconds = zone.length / profile.conveyor_speed * 60.0;
        let above = seconds_above(temperature, zone.temperature, seconds, profile.liquidus);
        zones.push(ZoneReport {
            name: zone.name.clone(),
            length: zone.length,
            entry_temperature: temperature,
            exit_temperature: zone.temperature,
            seconds,
            seconds_above_liquidus: above,
        });
        temperature = zone.temperature;
        peak_temperature = peak_temperature.max(zone.temperature);
        time_above_liquidus += above;
        total_seconds += seconds;
    }

    Ok(ReflowReport {
        format: "open-smt-reflow".to_string(),
        version: 1,
        units: document.units.clone(),
        temperature_unit: "celsius".to_string(),
        conveyor_speed: profile.conveyor_speed,
        liquidus: profile.liquidus,
        peak_temperature,
        time_above_liquidus,
        total_seconds,
        zones,
    })
}

fn seconds_above(entry: f64, exit: f64, seconds: f64, liquidus: f64) -> f64 {
    if entry <= liquidus && exit <= liquidus {
        return 0.0;
    }
    if entry >= liquidus && exit >= liquidus {
        return seconds;
    }
    let difference = (exit - entry).abs();
    if difference <= 1e-12 {
        return 0.0;
    }
    if exit > entry { seconds * (exit - liquidus) / difference } else { seconds * (entry - liquidus) / difference }
}
