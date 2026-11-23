//! Read-only machine program interchange and round-trip check.
//!
//! A machine program is a neutral CSV that a placement machine or an offline
//! tool can read: one row per placement step in program order with the
//! coordinates, rotation, feeder lane, tape-bank station, tape pitch, nozzle and
//! head speed. It is not a vendor dialect; Fuji, Yamaha, Juki and ASM remain
//! unimplemented.
//!
//! [`render`] and [`parse`] are inverse over the canonical three-decimal
//! values, so [`verify`] can compare an exported file with the program the
//! document produces, step by step. The document is never modified.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    SmtError,
    csv_import::{Record, read_records},
    export::{escape, number as csv_number},
    model::{Side, SmtDocument},
    place::{self, HEAD_SPEED},
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-machine-program";

/// Exact CSV header of the interchange format.
pub const HEADER: &str = "sequence,side,refdes,package,x_mm,y_mm,rotation_deg,feeder,station,pitch_mm,nozzle,speed_mm_s";

/// Column names in header order.
const COLUMNS: [&str; 12] = [
    "sequence",
    "side",
    "refdes",
    "package",
    "x_mm",
    "y_mm",
    "rotation_deg",
    "feeder",
    "station",
    "pitch_mm",
    "nozzle",
    "speed_mm_s",
];

/// One machine-readable placement step.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineStep {
    /// Placement step number, starting at 1 in program order.
    pub sequence: u32,
    /// Board side.
    pub side: Side,
    /// Reference designator.
    pub refdes: String,
    /// Package identifier.
    pub package: String,
    /// Placement X coordinate on one board.
    pub x: f64,
    /// Placement Y coordinate on one board.
    pub y: f64,
    /// Rotation in degrees.
    pub rotation: f64,
    /// Feeder lane that supplies the component.
    pub feeder: String,
    /// One-based position of the lane in the document feeder setup.
    pub station: u32,
    /// Tape pitch of the lane, in millimetres.
    pub pitch: f64,
    /// Nozzle assigned to the package by the caller; `null` when unassigned.
    pub nozzle: Option<String>,
    /// Head speed of the machine model, in millimetres per second.
    pub speed: f64,
}

/// Neutral machine program of one board.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineProgram {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Head speed of the machine model, in millimetres per second.
    pub head_speed: f64,
    /// Number of placement steps.
    pub components: u32,
    /// Number of feeder lanes in the document.
    pub station_count: u32,
    /// Steps in program order.
    pub steps: Vec<MachineStep>,
}

/// Result of writing a machine program and reading it back.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineProgramExport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Target path exactly as the caller supplied it.
    pub path: String,
    /// Number of placement steps written.
    pub steps: u32,
    /// Number of bytes written.
    pub bytes: usize,
    /// Whether the written file parsed back to the same program.
    pub verified: bool,
}

/// One field where a read-back program differs from the document program.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramMismatch {
    /// Step the mismatch belongs to; zero for a whole-program difference.
    pub sequence: u32,
    /// Field that differs.
    pub field: String,
    /// Value the document produces.
    pub expected: String,
    /// Value the file contains.
    pub actual: String,
}

/// Result of checking a machine program file against a document.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineProgramVerify {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Program path exactly as the caller supplied it.
    pub path: String,
    /// Number of steps the document produces.
    pub steps: u32,
    /// Every field that differs, in step then field order.
    pub mismatches: Vec<ProgramMismatch>,
    /// Whether the file matches the document program field by field.
    pub ok: bool,
}

/// Builds the machine program of a document.
///
/// The allocation and the sequence run exactly like the placement program, so
/// a package without a lane reports `feeder/missing` and an undersized setup
/// reports `feeder/capacity`. The station is the one-based position of the
/// lane in the document feeder setup, the nozzle comes from `nozzles` keyed by
/// package, and the speed is [`HEAD_SPEED`]. Coordinates, rotation, pitch and
/// speed are rounded to three decimals so the CSV round trip is exact. An
/// empty package or nozzle in the map reports `program/invalid`; the document
/// is not modified.
pub fn build(document: &SmtDocument, nozzles: &BTreeMap<String, String>) -> Result<MachineProgram, SmtError> {
    for (package, nozzle) in nozzles {
        if package.trim().is_empty() {
            return Err(SmtError::new("program/invalid", "nozzle map package must be non-empty"));
        }
        if nozzle.trim().is_empty() {
            return Err(SmtError::new("program/invalid", format!("nozzle for package `{package}` must be non-empty")));
        }
    }

    let allocation = place::allocate(document)?;
    let placement = place::sequence(document, &allocation)?;

    let station_of: BTreeMap<&str, u32> =
        document.feeders.iter().enumerate().map(|(index, feeder)| (feeder.id.as_str(), index as u32 + 1)).collect();
    let pitch_of: BTreeMap<&str, f64> = document.feeders.iter().map(|feeder| (feeder.id.as_str(), feeder.pitch)).collect();

    let steps = placement
        .entries
        .iter()
        .map(|entry| {
            let feeder = entry.feeder.clone().unwrap_or_default();
            MachineStep {
                sequence: entry.index.saturating_add(1),
                side: entry.side,
                refdes: entry.refdes.clone(),
                package: entry.package.clone(),
                x: canonical(entry.x),
                y: canonical(entry.y),
                rotation: canonical(entry.rotation),
                station: station_of.get(feeder.as_str()).copied().unwrap_or(0),
                pitch: canonical(pitch_of.get(feeder.as_str()).copied().unwrap_or(0.0)),
                nozzle: nozzles.get(&entry.package).cloned(),
                feeder,
                speed: canonical(HEAD_SPEED),
            }
        })
        .collect();

    Ok(MachineProgram {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        head_speed: HEAD_SPEED,
        components: placement.entries.len() as u32,
        station_count: document.feeders.len() as u32,
        steps,
    })
}

/// Renders a machine program as CSV, one row per step.
pub fn render(program: &MachineProgram) -> String {
    let mut text = String::from(HEADER);
    text.push('\n');
    for step in &program.steps {
        text.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            step.sequence,
            step.side.as_str(),
            escape(&step.refdes),
            escape(&step.package),
            csv_number(step.x),
            csv_number(step.y),
            csv_number(step.rotation),
            escape(&step.feeder),
            step.station,
            csv_number(step.pitch),
            escape(step.nozzle.as_deref().unwrap_or("")),
            csv_number(step.speed)
        ));
    }
    text
}

/// Parses a machine program CSV.
///
/// The header must name the twelve columns of [`HEADER`] in order; names are
/// case-insensitive, fields may be double-quoted and empty lines are skipped. A
/// missing or unknown column, a wrong field count, an unknown side, an empty
/// reference, package or feeder, a non-finite number, a zero sequence or
/// station and a non-positive speed report `program/invalid` and name the
/// physical line.
pub fn parse(source: &str) -> Result<MachineProgram, SmtError> {
    let records = read_records(source).map_err(|error| SmtError::new("program/invalid", error.message))?;
    let Some(header) = records.first()
    else {
        return Err(SmtError::new("program/invalid", "the machine program file is empty"));
    };
    columns(header)?;

    let mut steps = Vec::with_capacity(records.len().saturating_sub(1));
    for record in records.iter().skip(1) {
        steps.push(row(record)?);
    }

    let components = steps.len() as u32;
    let station_count = steps.iter().map(|step| step.station).max().unwrap_or(0);
    Ok(MachineProgram {
        format: FORMAT.to_string(),
        version: 1,
        units: "mm".to_string(),
        head_speed: steps.first().map_or(HEAD_SPEED, |step| step.speed),
        components,
        station_count,
        steps,
    })
}

/// Compares the fields of two programs and lists every difference.
///
/// Steps are compared by position, so a reordered file is reported step by
/// step. A different step count adds one program-level mismatch with sequence
/// `0`.
pub fn verify(expected: &MachineProgram, actual: &MachineProgram) -> Vec<ProgramMismatch> {
    let mut mismatches = Vec::new();
    if expected.steps.len() != actual.steps.len() {
        mismatches.push(ProgramMismatch {
            sequence: 0,
            field: "steps".to_string(),
            expected: expected.steps.len().to_string(),
            actual: actual.steps.len().to_string(),
        });
    }
    let mut push = |sequence: u32, field: &str, expected: String, actual: String| {
        mismatches.push(ProgramMismatch { sequence, field: field.to_string(), expected, actual });
    };
    for (left, right) in expected.steps.iter().zip(actual.steps.iter()) {
        let sequence = left.sequence;
        if left.sequence != right.sequence {
            push(sequence, "sequence", left.sequence.to_string(), right.sequence.to_string());
        }
        if left.side != right.side {
            push(sequence, "side", left.side.as_str().to_string(), right.side.as_str().to_string());
        }
        if left.refdes != right.refdes {
            push(sequence, "refdes", left.refdes.clone(), right.refdes.clone());
        }
        if left.package != right.package {
            push(sequence, "package", left.package.clone(), right.package.clone());
        }
        if left.x != right.x {
            push(sequence, "x", csv_number(left.x), csv_number(right.x));
        }
        if left.y != right.y {
            push(sequence, "y", csv_number(left.y), csv_number(right.y));
        }
        if left.rotation != right.rotation {
            push(sequence, "rotation", csv_number(left.rotation), csv_number(right.rotation));
        }
        if left.feeder != right.feeder {
            push(sequence, "feeder", left.feeder.clone(), right.feeder.clone());
        }
        if left.station != right.station {
            push(sequence, "station", left.station.to_string(), right.station.to_string());
        }
        if left.pitch != right.pitch {
            push(sequence, "pitch", csv_number(left.pitch), csv_number(right.pitch));
        }
        if left.nozzle != right.nozzle {
            let current = left.nozzle.clone().unwrap_or_default();
            let file = right.nozzle.clone().unwrap_or_default();
            push(sequence, "nozzle", current, file);
        }
        if left.speed != right.speed {
            push(sequence, "speed", csv_number(left.speed), csv_number(right.speed));
        }
    }
    mismatches
}

/// Maps every header name to its column index.
fn columns(header: &Record) -> Result<(), SmtError> {
    if header.fields.len() != COLUMNS.len() {
        return Err(SmtError::new(
            "program/invalid",
            format!("line {}: the header needs {} columns, found {}", header.line, COLUMNS.len(), header.fields.len()),
        ));
    }
    for (index, raw) in header.fields.iter().enumerate() {
        let name = raw.trim().to_ascii_lowercase();
        if name != COLUMNS[index] {
            return Err(SmtError::new(
                "program/invalid",
                format!("line {}: column {} is `{name}`, expected `{}`", header.line, index + 1, COLUMNS[index]),
            ));
        }
    }
    Ok(())
}

/// Reads one machine program row.
fn row(record: &Record) -> Result<MachineStep, SmtError> {
    if record.fields.len() != COLUMNS.len() {
        return Err(SmtError::new(
            "program/invalid",
            format!("line {}: found {} fields but the header declares {}", record.line, record.fields.len(), COLUMNS.len()),
        ));
    }

    let sequence = record.fields[0].trim();
    let sequence = sequence.parse::<u32>().ok().filter(|value| *value > 0).ok_or_else(|| {
        SmtError::new("program/invalid", format!("line {}: `sequence` must be a positive integer: `{sequence}`", record.line))
    })?;

    let raw_side = record.fields[1].trim();
    let side = raw_side.parse::<Side>().map_err(|_| {
        SmtError::new(
            "program/invalid",
            format!("line {}: unknown side `{raw_side}`; only top or bottom is supported", record.line),
        )
    })?;

    let refdes = record.fields[2].trim();
    if refdes.is_empty() {
        return Err(SmtError::new("program/invalid", format!("line {}: `refdes` must be non-empty", record.line)));
    }
    let package = record.fields[3].trim();
    if package.is_empty() {
        return Err(SmtError::new(
            "program/invalid",
            format!("line {}: `package` must be non-empty for `{refdes}`", record.line),
        ));
    }
    let feeder = record.fields[7].trim();
    if feeder.is_empty() {
        return Err(SmtError::new(
            "program/invalid",
            format!("line {}: `feeder` must be non-empty for `{refdes}`", record.line),
        ));
    }

    let station = record.fields[8].trim();
    let station = station.parse::<u32>().ok().filter(|value| *value > 0).ok_or_else(|| {
        SmtError::new("program/invalid", format!("line {}: `station` must be a positive integer: `{station}`", record.line))
    })?;

    let speed = number(record, "speed_mm_s", &record.fields[11])?;
    if speed <= 0.0 {
        return Err(SmtError::new("program/invalid", format!("line {}: `speed_mm_s` must be greater than zero", record.line)));
    }

    let nozzle = match record.fields[10].trim() {
        "" => None,
        value => Some(value.to_string()),
    };

    Ok(MachineStep {
        sequence,
        side,
        refdes: refdes.to_string(),
        package: package.to_string(),
        x: number(record, "x_mm", &record.fields[4])?,
        y: number(record, "y_mm", &record.fields[5])?,
        rotation: number(record, "rotation_deg", &record.fields[6])?,
        feeder: feeder.to_string(),
        station,
        pitch: number(record, "pitch_mm", &record.fields[9])?,
        nozzle,
        speed,
    })
}

/// Reads one finite numeric cell, naming the physical line on failure.
fn number(record: &Record, column: &str, raw: &str) -> Result<f64, SmtError> {
    match raw.trim().parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        _ => Err(SmtError::new(
            "program/invalid",
            format!("line {}: `{column}` is not a finite number: `{}`", record.line, raw.trim()),
        )),
    }
}

/// Rounds to the three decimals the interchange format carries.
fn canonical(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
