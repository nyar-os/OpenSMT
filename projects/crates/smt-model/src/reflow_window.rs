//! Read-only reflow process-window margins.
//!
//! The report reuses [`crate::reflow::evaluate`] for the zone timing, then
//! derives the signed heating or cooling rate of every zone and compares the
//! profile metrics with the limits the caller supplies. Every comparison reports
//! the distance to the limit, so a profile that is inside the window still shows
//! how much margin is left. Nothing is written and the document is unchanged.

use serde::{Deserialize, Serialize};

use crate::{SmtError, model::SmtDocument, reflow};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-reflow-window";

/// Process-window limits of one reflow profile.
///
/// Every limit is optional, so a request names only the metrics it cares about.
/// An omitted limit produces no check for that metric.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReflowWindowLimits {
    /// Lowest acceptable peak temperature, in degrees Celsius.
    pub peak_min: Option<f64>,
    /// Highest acceptable peak temperature, in degrees Celsius.
    pub peak_max: Option<f64>,
    /// Shortest acceptable time above liquidus, in seconds.
    pub time_above_liquidus_min: Option<f64>,
    /// Longest acceptable time above liquidus, in seconds.
    pub time_above_liquidus_max: Option<f64>,
    /// Highest acceptable heating rate, in degrees Celsius per second.
    pub max_heating_rate: Option<f64>,
    /// Highest acceptable cooling rate, in degrees Celsius per second.
    pub max_cooling_rate: Option<f64>,
}

impl ReflowWindowLimits {
    fn validate(&self) -> Result<(), SmtError> {
        for (name, value) in [
            ("peak min", self.peak_min),
            ("peak max", self.peak_max),
            ("time above liquidus min", self.time_above_liquidus_min),
            ("time above liquidus max", self.time_above_liquidus_max),
            ("max heating rate", self.max_heating_rate),
            ("max cooling rate", self.max_cooling_rate),
        ] {
            if let Some(value) = value {
                if !non_negative(value) {
                    return Err(SmtError::new(
                        "reflow/invalid",
                        format!("{name} must be finite and not negative, got {value}"),
                    ));
                }
            }
        }
        if let (Some(min), Some(max)) = (self.peak_min, self.peak_max) {
            if min > max {
                return Err(SmtError::new("reflow/invalid", "peak min must not exceed peak max"));
            }
        }
        if let (Some(min), Some(max)) = (self.time_above_liquidus_min, self.time_above_liquidus_max) {
            if min > max {
                return Err(SmtError::new("reflow/invalid", "time above liquidus min must not exceed its max"));
            }
        }
        Ok(())
    }
}

/// One zone with its signed ramp rate.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowZone {
    /// Zone name.
    pub name: String,
    /// Time spent in the zone, in seconds.
    pub seconds: f64,
    /// Temperature entering the zone.
    pub entry_temperature: f64,
    /// Set-point temperature leaving the zone.
    pub exit_temperature: f64,
    /// Signed rate `(exit − entry) / seconds`; negative means cooling.
    pub ramp_rate: f64,
}

/// One metric compared with one limit.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowCheck {
    /// Metric compared: `peakTemperature`, `timeAboveLiquidus`,
    /// `maxHeatingRate` or `maxCoolingRate`.
    pub metric: String,
    /// Direction of the limit: `min` or `max`.
    pub kind: String,
    /// Profile value of the metric.
    pub value: f64,
    /// Limit the value was compared with.
    pub limit: f64,
    /// `value − limit` for a minimum, `limit − value` for a maximum.
    pub margin: f64,
    /// Whether the margin is not negative.
    pub pass: bool,
}

/// Read-only reflow process-window report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflowWindowReport {
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
    /// Highest positive zone ramp rate, or zero when no zone heats.
    pub max_heating_rate: f64,
    /// Highest cooling rate magnitude, or zero when no zone cools.
    pub max_cooling_rate: f64,
    /// One entry per zone, in conveyor order.
    pub zones: Vec<WindowZone>,
    /// One entry per supplied limit, in a fixed metric order.
    pub checks: Vec<WindowCheck>,
    /// Whether every supplied limit is met; true when no limit was supplied.
    pub ok: bool,
}

/// Evaluates the reflow profile against caller limits.
///
/// Invalid limits (non-finite, negative, or a minimum above its maximum) report
/// `reflow/invalid` before the document is read. The profile itself is evaluated
/// with [`crate::reflow::evaluate`], so an inconsistent document fails with the
/// same code as the `reflow` report. The document is not modified.
pub fn evaluate(document: &SmtDocument, limits: &ReflowWindowLimits) -> Result<ReflowWindowReport, SmtError> {
    limits.validate()?;
    let report = reflow::evaluate(document)?;

    let zones: Vec<WindowZone> = report
        .zones
        .iter()
        .map(|zone| WindowZone {
            name: zone.name.clone(),
            seconds: zone.seconds,
            entry_temperature: zone.entry_temperature,
            exit_temperature: zone.exit_temperature,
            ramp_rate: if zone.seconds > 0.0 { (zone.exit_temperature - zone.entry_temperature) / zone.seconds } else { 0.0 },
        })
        .collect();

    let max_heating_rate = zones.iter().map(|zone| zone.ramp_rate).filter(|rate| *rate > 0.0).fold(0.0, f64::max);
    let max_cooling_rate = zones.iter().map(|zone| -zone.ramp_rate).filter(|rate| *rate > 0.0).fold(0.0, f64::max);

    let mut checks = Vec::new();
    push_check(&mut checks, "peakTemperature", "min", report.peak_temperature, limits.peak_min);
    push_check(&mut checks, "peakTemperature", "max", report.peak_temperature, limits.peak_max);
    push_check(&mut checks, "timeAboveLiquidus", "min", report.time_above_liquidus, limits.time_above_liquidus_min);
    push_check(&mut checks, "timeAboveLiquidus", "max", report.time_above_liquidus, limits.time_above_liquidus_max);
    push_check(&mut checks, "maxHeatingRate", "max", max_heating_rate, limits.max_heating_rate);
    push_check(&mut checks, "maxCoolingRate", "max", max_cooling_rate, limits.max_cooling_rate);

    let ok = checks.iter().all(|check| check.pass);

    Ok(ReflowWindowReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        temperature_unit: "celsius".to_string(),
        conveyor_speed: report.conveyor_speed,
        liquidus: report.liquidus,
        peak_temperature: report.peak_temperature,
        time_above_liquidus: report.time_above_liquidus,
        total_seconds: report.total_seconds,
        max_heating_rate,
        max_cooling_rate,
        zones,
        checks,
        ok,
    })
}

fn push_check(checks: &mut Vec<WindowCheck>, metric: &str, kind: &str, value: f64, limit: Option<f64>) {
    let Some(limit) = limit
    else {
        return;
    };
    let margin = if kind == "min" { value - limit } else { limit - value };
    checks.push(WindowCheck { metric: metric.to_string(), kind: kind.to_string(), value, limit, margin, pass: margin >= 0.0 });
}

fn non_negative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}
