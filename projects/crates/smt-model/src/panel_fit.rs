//! Read-only panel fit report.
//!
//! The report answers how many copies of the document's board fit into a
//! maximum panel size. It never reads or writes the stored panel array, so the
//! document is unchanged. Gaps default to the stored panel gaps and may be
//! overridden per call.

use serde::Serialize;

use crate::{SmtError, model::SmtDocument};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-panel-fit";

/// How many boards fit into a maximum panel size.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelFitReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Maximum panel width that was tested, in millimetres.
    pub max_width: f64,
    /// Maximum panel depth that was tested, in millimetres.
    pub max_depth: f64,
    /// Column gap used by the fit, in millimetres.
    pub gap_x: f64,
    /// Row gap used by the fit, in millimetres.
    pub gap_y: f64,
    /// Board width, in millimetres.
    pub width: f64,
    /// Board depth, in millimetres.
    pub depth: f64,
    /// Number of board rows that fit.
    pub rows: u32,
    /// Number of board columns that fit.
    pub columns: u32,
    /// Number of boards on the fitted panel.
    pub board_count: u32,
    /// Width of the fitted panel including column gaps.
    pub panel_width: f64,
    /// Depth of the fitted panel including row gaps.
    pub panel_depth: f64,
    /// Whether at least one board fits.
    pub fits: bool,
    /// Unused width when a board fits; zero otherwise.
    pub wasted_width: f64,
    /// Unused depth when a board fits; zero otherwise.
    pub wasted_depth: f64,
    /// `panelWidth × panelDepth / (maxWidth × maxDepth)` when a board fits;
    /// zero otherwise.
    pub utilization: f64,
}

/// Computes how many boards fit into `max_width × max_depth`.
///
/// `gap_x` and `gap_y` default to the stored panel gaps and must be finite and
/// non-negative when supplied. Both maximum sizes must be finite and positive.
/// The arithmetic mirrors the stored panel:
/// `columns = floor((maxWidth + gapX) / (width + gapX))`,
/// `rows = floor((maxDepth + gapY) / (depth + gapY))`, and each count is
/// floored at zero. The document is not modified.
pub fn fit(
    document: &SmtDocument,
    max_width: f64,
    max_depth: f64,
    gap_x: Option<f64>,
    gap_y: Option<f64>,
) -> Result<PanelFitReport, SmtError> {
    if !positive(max_width) {
        return Err(SmtError::new("panel/invalid", "maximum panel width must be finite and greater than zero"));
    }
    if !positive(max_depth) {
        return Err(SmtError::new("panel/invalid", "maximum panel depth must be finite and greater than zero"));
    }
    let gap_x = gap_x.unwrap_or(document.panel.gap_x);
    let gap_y = gap_y.unwrap_or(document.panel.gap_y);
    if !non_negative(gap_x) {
        return Err(SmtError::new("panel/invalid", "panel gap X must be finite and not negative"));
    }
    if !non_negative(gap_y) {
        return Err(SmtError::new("panel/invalid", "panel gap Y must be finite and not negative"));
    }

    let width = document.board.width;
    let depth = document.board.depth;
    let columns = count(max_width, width, gap_x);
    let rows = count(max_depth, depth, gap_y);
    let board_count = rows.saturating_mul(columns);
    let fits = board_count > 0;

    let panel_width = if columns == 0 { 0.0 } else { f64::from(columns) * width + f64::from(columns - 1) * gap_x };
    let panel_depth = if rows == 0 { 0.0 } else { f64::from(rows) * depth + f64::from(rows - 1) * gap_y };
    let wasted_width = if fits { (max_width - panel_width).max(0.0) } else { 0.0 };
    let wasted_depth = if fits { (max_depth - panel_depth).max(0.0) } else { 0.0 };
    let area = max_width * max_depth;
    let utilization = if fits && area > 0.0 { panel_width * panel_depth / area } else { 0.0 };

    Ok(PanelFitReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        max_width,
        max_depth,
        gap_x,
        gap_y,
        width,
        depth,
        rows,
        columns,
        board_count,
        panel_width,
        panel_depth,
        fits,
        wasted_width,
        wasted_depth,
        utilization,
    })
}

fn count(maximum: f64, size: f64, gap: f64) -> u32 {
    let step = size + gap;
    if !(step.is_finite() && step > 0.0) {
        return 0;
    }
    let raw = ((maximum + gap) / step).floor();
    if !raw.is_finite() || raw <= 0.0 { 0 } else { raw.min(f64::from(u32::MAX)) as u32 }
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn non_negative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}
