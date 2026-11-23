//! Read-only feeder station assignment along the tape bank.
//!
//! The report runs the same feeder allocation and placement sequence as the
//! `placement` report, then orders the feeder lanes so that every lane sits at
//! the slot closest to the mean placement X of the components it supplies.
//! The head travel modelled here is the tape-bank axis only: the sum of the
//! absolute slot-to-slot moves between consecutive picks of the same side,
//! starting from slot position zero at the start of every side. Both the
//! stored feeder order (the baseline) and the optimized order are measured, so
//! `travelSaved` names the gain. Nothing is written and the document is
//! unchanged.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    SmtError,
    model::{Side, SmtDocument},
    place::{self, PlacementEntry},
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-station";

/// Default distance between neighbouring feeder slots, in millimetres.
pub const DEFAULT_SLOT_PITCH: f64 = 16.0;

/// Distance differences at or below this value count as ties.
const MEAN_TIE_EPSILON: f64 = 1e-9;

/// One feeder placed at one slot.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    /// Slot number, starting at 1 in assignment order.
    pub slot: u32,
    /// Slot position along the tape bank, `(slot - 1) × slotPitch`.
    pub position_mm: f64,
    /// Feeder lane sitting at this slot.
    pub feeder_id: String,
    /// Package supplied by the lane.
    pub package: String,
    /// Picks the lane supplies in the placement program.
    pub picks: u32,
    /// Mean placement X of the picks; `null` for a lane the program never uses.
    pub mean_x: Option<f64>,
}

/// Feeder station assignment report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Slot pitch used for the positions, in millimetres.
    pub slot_pitch: f64,
    /// Number of assigned slots; one per feeder lane.
    pub slot_count: u32,
    /// Tape-bank travel of the stored feeder order, in millimetres.
    pub baseline_travel_mm: f64,
    /// Tape-bank travel of the optimized assignment, in millimetres.
    pub travel_mm: f64,
    /// `baselineTravelMm - travelMm`, floored at zero.
    pub travel_saved: f64,
    /// One entry per lane, in slot order.
    pub stations: Vec<Station>,
}

/// Assigns every feeder lane a tape-bank slot and measures the travel.
///
/// Lanes are ordered by the mean placement X of the picks they supply, ties
/// broken by feeder id, and lanes the program never uses are appended in id
/// order after the used ones. A non-finite or non-positive `slot_pitch`
/// reports `station/invalid`; anything the allocation or the sequence rejects
/// fails with its own code. The document is not modified.
pub fn assign(document: &SmtDocument, slot_pitch: f64) -> Result<StationReport, SmtError> {
    if !slot_pitch.is_finite() || slot_pitch <= 0.0 {
        return Err(SmtError::new("station/invalid", "slot pitch must be finite and greater than zero"));
    }

    let feeders = place::allocate(document)?;
    let placement = place::sequence(document, &feeders)?;

    let mut picks: BTreeMap<&str, (u32, f64)> = BTreeMap::new();
    for entry in &placement.entries {
        if let Some(id) = entry.feeder.as_deref() {
            let lane = picks.entry(id).or_insert((0, 0.0));
            lane.0 += 1;
            lane.1 += entry.x;
        }
    }

    let mut ordered: Vec<&crate::model::Feeder> = document.feeders.iter().collect();
    ordered.sort_by(|left, right| {
        let left_used = picks.contains_key(left.id.as_str());
        let right_used = picks.contains_key(right.id.as_str());
        if left_used != right_used {
            return right_used.cmp(&left_used);
        }
        if left_used {
            let left_mean = picks[left.id.as_str()].1 / f64::from(picks[left.id.as_str()].0);
            let right_mean = picks[right.id.as_str()].1 / f64::from(picks[right.id.as_str()].0);
            if (left_mean - right_mean).abs() > MEAN_TIE_EPSILON {
                return left_mean.partial_cmp(&right_mean).unwrap_or(std::cmp::Ordering::Equal);
            }
        }
        left.id.cmp(&right.id)
    });

    let mut stations = Vec::with_capacity(ordered.len());
    let mut optimized_positions: BTreeMap<&str, f64> = BTreeMap::new();
    for (index, feeder) in ordered.iter().enumerate() {
        let slot = index as u32 + 1;
        let position = f64::from(slot - 1) * slot_pitch;
        optimized_positions.insert(feeder.id.as_str(), position);
        let usage = picks.get(feeder.id.as_str());
        stations.push(Station {
            slot,
            position_mm: position,
            feeder_id: feeder.id.clone(),
            package: feeder.package.clone(),
            picks: usage.map_or(0, |usage| usage.0),
            mean_x: usage.map(|(count, sum)| sum / f64::from(*count)),
        });
    }

    let baseline_positions: BTreeMap<&str, f64> = document
        .feeders
        .iter()
        .enumerate()
        .map(|(index, feeder)| (feeder.id.as_str(), f64::from(index as u32) * slot_pitch))
        .collect();

    let baseline_travel_mm = tape_travel(&placement.entries, &baseline_positions);
    let travel_mm = tape_travel(&placement.entries, &optimized_positions);

    Ok(StationReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        slot_pitch,
        slot_count: stations.len() as u32,
        baseline_travel_mm,
        travel_mm,
        travel_saved: (baseline_travel_mm - travel_mm).max(0.0),
        stations,
    })
}

/// Tape-bank travel of the program for a slot map, in millimetres.
///
/// The bank axis starts at position zero for every side, exactly like the
/// placement origin restarts per side.
fn tape_travel(entries: &[PlacementEntry], positions: &BTreeMap<&str, f64>) -> f64 {
    let mut travel = 0.0;
    let mut current_side = Side::Top;
    let mut last_position = 0.0;
    let mut started = false;
    for entry in entries {
        if !started || entry.side != current_side {
            last_position = 0.0;
            current_side = entry.side;
            started = true;
        }
        if let Some(id) = entry.feeder.as_deref() {
            let position = positions[id];
            travel += (position - last_position).abs();
            last_position = position;
        }
    }
    travel
}
