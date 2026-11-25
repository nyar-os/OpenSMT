//! Read-only component height and clearance checks.
//!
//! The model stores no body dimensions, so each component's body outline comes
//! from the shared package rules table (`packages`), the same table the stencil
//! generator uses. The check reports three kinds of warnings: a component whose
//! stored height exceeds the maximum height, a pair of same-side bodies that
//! overlap, and a pair whose body-to-body gap is below the minimum gap. Bodies
//! are compared as axis-aligned bounding boxes of the rotated package body, so
//! any rotation is handled conservatively. Components with non-finite
//! coordinates are skipped; the document is never modified.

use serde::{Deserialize, Serialize};

use crate::{
    SmtError,
    model::{Side, SmtDocument},
    packages,
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-clearance";

/// Report schema version.
pub const VERSION: u32 = 1;

/// Default minimum body-to-body gap in millimetres.
pub const DEFAULT_MIN_GAP_MM: f64 = 0.5;

/// Default maximum component height in millimetres.
pub const DEFAULT_MAX_HEIGHT_MM: f64 = 12.0;

/// Caller-supplied clearance limits.
///
/// Both limits are optional; an omitted limit keeps its default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClearanceLimits {
    /// Minimum acceptable body-to-body gap, in millimetres.
    pub min_gap: Option<f64>,
    /// Maximum acceptable component height, in millimetres.
    pub max_height: Option<f64>,
}

impl ClearanceLimits {
    fn resolved(&self) -> Result<(f64, f64), SmtError> {
        let min_gap = self.min_gap.unwrap_or(DEFAULT_MIN_GAP_MM);
        let max_height = self.max_height.unwrap_or(DEFAULT_MAX_HEIGHT_MM);
        if !min_gap.is_finite() || min_gap < 0.0 {
            return Err(SmtError::new("clearance/invalid", "minimum gap must be finite and non-negative"));
        }
        if !max_height.is_finite() || max_height <= 0.0 {
            return Err(SmtError::new("clearance/invalid", "maximum height must be finite and positive"));
        }
        Ok((min_gap, max_height))
    }
}

/// One warning of the clearance check.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearanceFinding {
    /// Always `warning`; the check never fails the document.
    pub severity: String,
    /// `clearance/height`, `clearance/overlap` or `clearance/gap`.
    pub code: String,
    /// Human-readable description of the finding.
    pub message: String,
    /// Reference designator of the first component.
    pub refdes: String,
    /// Reference designator of the second component, for pair findings.
    pub other: Option<String>,
    /// Board side of the pair, for pair findings.
    pub side: Option<Side>,
    /// Body-to-body gap in millimetres, `0` for an overlap.
    pub gap: Option<f64>,
    /// Measured height in millimetres, for height findings.
    pub height: Option<f64>,
}

/// Clearance check report of a document.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearanceReport {
    /// Report format identifier.
    pub format: String,
    /// Report schema version.
    pub version: u32,
    /// Unit system taken from the document.
    pub units: String,
    /// Minimum gap the check applied in millimetres.
    pub min_gap: f64,
    /// Maximum height the check applied in millimetres.
    pub max_height: f64,
    /// Number of components inspected.
    pub components: u32,
    /// Number of same-side component pairs compared.
    pub pairs_checked: u32,
    /// Findings in component order, then in pair order.
    pub findings: Vec<ClearanceFinding>,
    /// Whether the check found nothing to warn about.
    pub ok: bool,
}

/// Axis-aligned bounding box of one component body.
struct Body {
    refdes: String,
    side: Side,
    x: f64,
    y: f64,
    half_x: f64,
    half_y: f64,
}

/// Checks the document for oversized, overlapping and too-close components.
///
/// Invalid limits report `clearance/invalid`.
pub fn check(document: &SmtDocument, limits: &ClearanceLimits) -> Result<ClearanceReport, SmtError> {
    let (min_gap, max_height) = limits.resolved()?;
    let mut findings: Vec<ClearanceFinding> = Vec::new();
    let mut bodies: Vec<Body> = Vec::new();
    for component in &document.components {
        if let Some(height) = component.height {
            if height.is_finite() && height > max_height {
                findings.push(ClearanceFinding {
                    severity: "warning".to_string(),
                    code: "clearance/height".to_string(),
                    message: format!(
                        "{} is {} mm tall; the maximum height is {} mm",
                        component.refdes,
                        trim(height),
                        trim(max_height)
                    ),
                    refdes: component.refdes.clone(),
                    other: None,
                    side: Some(component.side),
                    gap: None,
                    height: Some(height),
                });
            }
        }
        if !component.x.is_finite() || !component.y.is_finite() || !component.rotation.is_finite() {
            continue;
        }
        let geometry = packages::geometry(&component.package);
        let radians = component.rotation.to_radians();
        let (cosine, sine) = (radians.cos().abs(), radians.sin().abs());
        bodies.push(Body {
            refdes: component.refdes.clone(),
            side: component.side,
            x: component.x,
            y: component.y,
            half_x: (cosine * geometry.body_length + sine * geometry.body_width) / 2.0,
            half_y: (sine * geometry.body_length + cosine * geometry.body_width) / 2.0,
        });
    }

    let mut pairs_checked = 0;
    for first in 0..bodies.len() {
        for second in first + 1..bodies.len() {
            if bodies[first].side != bodies[second].side {
                continue;
            }
            pairs_checked += 1;
            let gap_x = (bodies[first].x - bodies[second].x).abs() - (bodies[first].half_x + bodies[second].half_x);
            let gap_y = (bodies[first].y - bodies[second].y).abs() - (bodies[first].half_y + bodies[second].half_y);
            let side = bodies[first].side;
            if gap_x < 0.0 && gap_y < 0.0 {
                let depth = (-gap_x).min(-gap_y);
                findings.push(ClearanceFinding {
                    severity: "warning".to_string(),
                    code: "clearance/overlap".to_string(),
                    message: format!(
                        "{} overlaps {} by {} mm on the {} side",
                        bodies[first].refdes,
                        bodies[second].refdes,
                        trim(round3(depth)),
                        side.as_str()
                    ),
                    refdes: bodies[first].refdes.clone(),
                    other: Some(bodies[second].refdes.clone()),
                    side: Some(side),
                    gap: Some(0.0),
                    height: None,
                });
                continue;
            }
            let gap = if gap_x > 0.0 && gap_y > 0.0 { gap_x.hypot(gap_y) } else { gap_x.max(gap_y) };
            if gap < min_gap {
                findings.push(ClearanceFinding {
                    severity: "warning".to_string(),
                    code: "clearance/gap".to_string(),
                    message: format!(
                        "{} is {} mm from {} on the {} side; the minimum gap is {} mm",
                        bodies[first].refdes,
                        trim(round3(gap)),
                        bodies[second].refdes,
                        side.as_str(),
                        trim(min_gap)
                    ),
                    refdes: bodies[first].refdes.clone(),
                    other: Some(bodies[second].refdes.clone()),
                    side: Some(side),
                    gap: Some(round3(gap)),
                    height: None,
                });
            }
        }
    }

    let ok = findings.is_empty();
    Ok(ClearanceReport {
        format: FORMAT.to_string(),
        version: VERSION,
        units: document.units.clone(),
        min_gap,
        max_height,
        components: document.components.len() as u32,
        pairs_checked,
        findings,
        ok,
    })
}

/// Rounds to three decimals, the report precision.
fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Formats a length without trailing zeros.
fn trim(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}
