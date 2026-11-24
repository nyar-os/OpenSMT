//! Document validation.
//!
//! Validation is the support claim of the engine: a document that reports
//! `ok: true` can be sequenced, allocated to feeders and evaluated against its
//! reflow profile. Every finding carries a stable code.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::{
    SmtError,
    model::{FORMAT, SmtDocument, UNITS, VERSION},
};

/// Severity of one finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// The document cannot be assembled as written.
    Error,
    /// The document can be assembled but the value is suspicious.
    Warning,
    /// Informational finding.
    Notice,
}

/// One validation finding.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// Stable finding code.
    pub code: String,
    /// Severity of the finding.
    pub severity: Severity,
    /// Human readable explanation.
    pub message: String,
}

impl Issue {
    fn error(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), severity: Severity::Error, message: message.into() }
    }

    fn warning(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), severity: Severity::Warning, message: message.into() }
    }
}

/// Result of validating a document.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    /// `true` when no finding has [`Severity::Error`].
    pub ok: bool,
    /// Every finding in the order the validator produced them.
    pub issues: Vec<Issue>,
}

/// Validates a document and returns every finding.
pub fn validate(document: &SmtDocument) -> ValidationReport {
    let mut issues = Vec::new();

    if document.format != FORMAT {
        issues.push(Issue::error("document/format", format!("format is `{}` but `{FORMAT}` is required", document.format)));
    }
    if document.version != VERSION {
        issues.push(Issue::error("document/version", format!("version is {} but {VERSION} is required", document.version)));
    }
    if document.units != UNITS {
        issues.push(Issue::error("document/units", format!("units are `{}` but only `{UNITS}` is supported", document.units)));
    }
    if !positive(document.board.width) || !positive(document.board.depth) || !positive(document.board.thickness) {
        issues.push(Issue::error("board/invalid", "board width, depth and thickness must be finite and greater than zero"));
    }
    if document.panel.rows == 0 || document.panel.columns == 0 {
        issues.push(Issue::error("panel/invalid", "a panel needs at least one row and one column"));
    }
    if !non_negative(document.panel.gap_x) || !non_negative(document.panel.gap_y) {
        issues.push(Issue::error("panel/invalid", "panel gaps must be finite and not negative"));
    }

    let mut refdes = HashSet::new();
    for component in &document.components {
        if component.refdes.trim().is_empty() {
            issues.push(Issue::error("component/invalid", "a component reference designator must be non-empty"));
            continue;
        }
        if !refdes.insert(component.refdes.as_str()) {
            issues.push(Issue::error(
                "component/duplicate-refdes",
                format!("component `{}` is declared twice", component.refdes),
            ));
        }
        if component.package.trim().is_empty() {
            issues.push(Issue::error("component/invalid", format!("component `{}` needs a package", component.refdes)));
        }
        let finite = component.x.is_finite() && component.y.is_finite();
        if !finite {
            issues.push(Issue::error(
                "component/invalid",
                format!("component `{}` has non-finite coordinates", component.refdes),
            ));
        } else if component.x < 0.0
            || component.y < 0.0
            || component.x > document.board.width
            || component.y > document.board.depth
        {
            issues.push(Issue::error(
                "component/outside-board",
                format!(
                    "component `{}` at ({}, {}) is outside the {} × {} board",
                    component.refdes, component.x, component.y, document.board.width, document.board.depth
                ),
            ));
        }
        if !component.rotation.is_finite() || !(0.0..=360.0).contains(&component.rotation) {
            issues.push(Issue::error(
                "component/invalid",
                format!("component `{}` rotation must be between 0 and 360 degrees", component.refdes),
            ));
        } else if !is_right_angle(component.rotation) {
            issues.push(Issue::warning(
                "component/rotation",
                format!("component `{}` rotation {} is not a multiple of 90 degrees", component.refdes, component.rotation),
            ));
        }
        if let Some(height) = component.height {
            if !positive(height) {
                issues.push(Issue::error(
                    "component/invalid",
                    format!("component `{}` height must be greater than zero", component.refdes),
                ));
            }
        }
    }
    if document.components.is_empty() {
        issues.push(Issue::warning("component/none", "the document has no components"));
    }

    let mut feeder_ids = HashSet::new();
    for feeder in &document.feeders {
        if feeder.id.trim().is_empty() {
            issues.push(Issue::error("feeder/invalid", "a feeder id must be non-empty"));
        } else if !feeder_ids.insert(feeder.id.as_str()) {
            issues.push(Issue::error("feeder/duplicate-id", format!("feeder `{}` is declared twice", feeder.id)));
        }
        if feeder.package.trim().is_empty() {
            issues.push(Issue::error("feeder/invalid", format!("feeder `{}` needs a package", feeder.id)));
        }
        if !positive(feeder.pitch) {
            issues.push(Issue::error("feeder/invalid", format!("feeder `{}` needs a positive pitch", feeder.id)));
        }
        if feeder.capacity == 0 {
            issues.push(Issue::error("feeder/invalid", format!("feeder `{}` needs a positive capacity", feeder.id)));
        }
        if feeder.loaded > feeder.capacity {
            issues.push(Issue::error(
                "feeder/invalid",
                format!("feeder `{}` is loaded with {} but its capacity is {}", feeder.id, feeder.loaded, feeder.capacity),
            ));
        }
    }
    if document.feeders.is_empty() {
        issues.push(Issue::warning("feeder/none", "the document has no feeders"));
    }

    let mut splice_keys = HashSet::new();
    for splice in &document.splices {
        if !feeder_ids.contains(splice.feeder.as_str()) {
            issues.push(Issue::error("feeder/unknown-id", format!("splice references unknown feeder `{}`", splice.feeder)));
        }
        if splice.sequence == 0 || splice.sequence as usize > document.components.len() {
            issues.push(Issue::error(
                "feeder/splice-invalid",
                format!(
                    "splice on feeder `{}` names placement step {} but the program has {} steps",
                    splice.feeder,
                    splice.sequence,
                    document.components.len()
                ),
            ));
        }
        if !splice_keys.insert((splice.feeder.as_str(), splice.sequence)) {
            issues.push(Issue::error(
                "feeder/splice-invalid",
                format!("feeder `{}` declares more than one splice at placement step {}", splice.feeder, splice.sequence),
            ));
        }
    }

    let counts = package_counts(document);
    let mut packages_with_feeders = HashSet::new();
    for feeder in &document.feeders {
        if !counts.contains_key(&feeder.package) && !feeder.package.trim().is_empty() {
            issues.push(Issue::warning(
                "feeder/unused",
                format!("feeder `{}` supplies unused package `{}`", feeder.id, feeder.package),
            ));
        }
        packages_with_feeders.insert(feeder.package.as_str());
    }
    for (package, count) in &counts {
        let suppliers: Vec<&crate::model::Feeder> =
            document.feeders.iter().filter(|feeder| feeder.package == *package).collect();
        if suppliers.is_empty() {
            issues.push(Issue::error(
                "feeder/missing",
                format!("package `{package}` is used by {count} components but has no feeder"),
            ));
            continue;
        }
        let loaded: u32 = suppliers.iter().map(|feeder| feeder.loaded).sum();
        if loaded < *count {
            issues.push(Issue::error(
                "feeder/capacity",
                format!("package `{package}` needs {count} placements but only {loaded} are loaded"),
            ));
        }
    }

    validate_profile(document, &mut issues);

    let ok = !issues.iter().any(|issue| issue.severity == Severity::Error);
    ValidationReport { ok, issues }
}

/// Returns the first error of a report as an [`SmtError`], if any.
pub fn first_error(report: &ValidationReport) -> Option<SmtError> {
    report
        .issues
        .iter()
        .find(|issue| issue.severity == Severity::Error)
        .map(|issue| SmtError::new(issue.code.clone(), issue.message.clone()))
}

/// Counts components per package, in package order.
pub fn package_counts(document: &SmtDocument) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for component in &document.components {
        *counts.entry(component.package.clone()).or_insert(0) += 1;
    }
    counts
}

fn validate_profile(document: &SmtDocument, issues: &mut Vec<Issue>) {
    if !positive(document.profile.conveyor_speed) {
        issues.push(Issue::error("profile/invalid", "the conveyor speed must be finite and greater than zero"));
    }
    if !positive(document.profile.liquidus) {
        issues.push(Issue::error("profile/invalid", "the liquidus must be finite and greater than zero"));
    }
    if document.profile.zones.is_empty() {
        issues.push(Issue::error("profile/invalid", "the reflow profile needs at least one zone"));
        return;
    }
    for zone in &document.profile.zones {
        if zone.name.trim().is_empty() {
            issues.push(Issue::error("profile/invalid", "a zone name must be non-empty"));
        }
        if !positive(zone.length) {
            issues.push(Issue::error("profile/invalid", format!("zone `{}` needs a positive length", zone.name)));
        }
        if !positive(zone.temperature) {
            issues.push(Issue::error("profile/invalid", format!("zone `{}` needs a positive temperature", zone.name)));
        }
    }
    let peak = document.profile.zones.iter().map(|zone| zone.temperature).fold(f64::MIN, f64::max);
    if peak < document.profile.liquidus {
        issues.push(Issue::warning(
            "profile/below-liquidus",
            format!("the peak zone temperature {peak} is below the liquidus {}", document.profile.liquidus),
        ));
    }
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn non_negative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

fn is_right_angle(rotation: f64) -> bool {
    let remainder = rotation % 90.0;
    remainder.abs() < 1e-9 || (90.0 - remainder.abs()).abs() < 1e-9
}
