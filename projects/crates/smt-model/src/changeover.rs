//! Read-only feeder changeover sequence between two documents.
//!
//! The plan compares the feeder setups of a current and a next document by
//! feeder id. Lanes that only the current document declares are removed,
//! lanes that only the next document declares are added, lanes present in
//! both but changed are swapped, and lanes that match exactly stay untouched.
//! The sequence runs removals first, then swaps, then additions, each group
//! in ascending feeder id order, so the machine is empty of stale reels
//! before new ones enter. The downtime estimate multiplies one changeover
//! action by a fixed time per feeder change; a swap counts as two changes.
//! Neither document is modified.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    SmtError,
    model::{Feeder, SmtDocument},
    validate::{self, Severity},
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-changeover";

/// Default time to change one feeder reel, in seconds. Matches the takt
/// machine model's feeder-change default.
pub const DEFAULT_FEEDER_CHANGE_SECONDS: f64 = 30.0;

/// One planned changeover action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeoverKind {
    /// Unload a reel the next document does not declare.
    Remove,
    /// Unload and reload the same lane because its package or pitch changed.
    Swap,
    /// Load a reel the current document does not declare.
    Add,
}

impl ChangeoverKind {
    /// Short identifier used by the CLI and by reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Remove => "remove",
            Self::Swap => "swap",
            Self::Add => "add",
        }
    }
}

/// One step of the changeover sequence.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeoverAction {
    /// One-based position in the sequence.
    pub step: u32,
    /// What happens at this step.
    pub kind: ChangeoverKind,
    /// Feeder lane the action touches.
    pub id: String,
    /// Package of the removed lane; present only for `remove`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Package before the swap; present only for `swap`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_package: Option<String>,
    /// Package after the swap; present only for `swap`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_package: Option<String>,
    /// Tape pitch before the swap; present only for `swap`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_pitch: Option<f64>,
    /// Tape pitch after the swap; present only for `swap`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_pitch: Option<f64>,
    /// Lane capacity to load; present only for `add`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<u32>,
    /// Loaded quantity to load; present only for `add`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loaded: Option<u32>,
}

/// Changeover sequence report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeoverReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Number of lanes in the current document.
    pub current_feeders: u32,
    /// Number of lanes in the next document.
    pub next_feeders: u32,
    /// Actions in execution order: removals, swaps, additions.
    pub actions: Vec<ChangeoverAction>,
    /// Lane ids that match by package and exact pitch, in ascending id order.
    pub unchanged: Vec<String>,
    /// Number of physical reel changes; a swap counts twice.
    pub changes: u32,
    /// Time assumed for one reel change, in seconds.
    pub feeder_change_seconds: f64,
    /// `changes × feederChangeSeconds`.
    pub estimated_seconds: f64,
}

/// Plans the feeder changeover from `current` to `next`.
///
/// Both documents must pass the structural validation; feeder coverage errors
/// are ignored because the plan compares setups, not builds. A non-finite or
/// non-positive `feeder_change_seconds` reports `changeover/invalid`. Lanes
/// are matched by id; a lane present in both counts as unchanged only when
/// its package and pitch match exactly. Neither document is modified.
pub fn plan(current: &SmtDocument, next: &SmtDocument, feeder_change_seconds: f64) -> Result<ChangeoverReport, SmtError> {
    if !feeder_change_seconds.is_finite() || feeder_change_seconds <= 0.0 {
        return Err(SmtError::new("changeover/invalid", "feeder change seconds must be finite and greater than zero"));
    }
    for (label, document) in [("current", current), ("next", next)] {
        let report = validate::validate(document);
        if let Some(issue) = report.issues.iter().find(|issue| {
            issue.severity == Severity::Error && issue.code != "feeder/missing" && issue.code != "feeder/capacity"
        }) {
            return Err(SmtError::new(issue.code.clone(), format!("the {label} document is invalid: {}", issue.message)));
        }
    }

    let current_lanes: BTreeMap<&str, &Feeder> = current.feeders.iter().map(|feeder| (feeder.id.as_str(), feeder)).collect();
    let next_lanes: BTreeMap<&str, &Feeder> = next.feeders.iter().map(|feeder| (feeder.id.as_str(), feeder)).collect();

    let mut actions = Vec::new();
    for (id, feeder) in &current_lanes {
        if !next_lanes.contains_key(id) {
            actions.push(ChangeoverAction {
                step: 0,
                kind: ChangeoverKind::Remove,
                id: (*id).to_string(),
                package: Some(feeder.package.clone()),
                from_package: None,
                to_package: None,
                from_pitch: None,
                to_pitch: None,
                capacity: None,
                loaded: None,
            });
        }
    }
    let mut unchanged = Vec::new();
    for (id, feeder) in &current_lanes {
        if let Some(target) = next_lanes.get(id) {
            if feeder.package == target.package && feeder.pitch == target.pitch {
                unchanged.push((*id).to_string());
            }
            else {
                actions.push(ChangeoverAction {
                    step: 0,
                    kind: ChangeoverKind::Swap,
                    id: (*id).to_string(),
                    package: None,
                    from_package: Some(feeder.package.clone()),
                    to_package: Some(target.package.clone()),
                    from_pitch: Some(feeder.pitch),
                    to_pitch: Some(target.pitch),
                    capacity: None,
                    loaded: None,
                });
            }
        }
    }
    for (id, feeder) in &next_lanes {
        if !current_lanes.contains_key(id) {
            actions.push(ChangeoverAction {
                step: 0,
                kind: ChangeoverKind::Add,
                id: (*id).to_string(),
                package: Some(feeder.package.clone()),
                from_package: None,
                to_package: None,
                from_pitch: None,
                to_pitch: None,
                capacity: Some(feeder.capacity),
                loaded: Some(feeder.loaded),
            });
        }
    }

    for (index, action) in actions.iter_mut().enumerate() {
        action.step = index as u32 + 1;
    }
    let changes = actions
        .iter()
        .map(|action| match action.kind {
            ChangeoverKind::Swap => 2,
            _ => 1,
        })
        .sum::<u32>();

    Ok(ChangeoverReport {
        format: FORMAT.to_string(),
        version: 1,
        current_feeders: current.feeders.len() as u32,
        next_feeders: next.feeders.len() as u32,
        actions,
        unchanged,
        changes,
        feeder_change_seconds,
        estimated_seconds: f64::from(changes) * feeder_change_seconds,
    })
}
