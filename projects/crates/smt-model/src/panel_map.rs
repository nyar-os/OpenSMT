//! Read-only single-board to panel coordinate map.
//!
//! The report maps every component of the stored single board into every board
//! instance of the stored panel array. Instances run row-major from board 1 at
//! the panel origin, so a caller can check a panel program without redoing the
//! offset arithmetic. Components keep their document order and every component
//! lists its instances in board order. The map also counts components whose
//! stored coordinates fall outside the single board. Nothing is written and the
//! document is unchanged.

use serde::Serialize;

use crate::{
    SmtError,
    model::{Side, SmtDocument},
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-panel-map";

/// One board instance of the panel array.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelInstance {
    /// Board number, starting at 1 in row-major order.
    pub board: u32,
    /// Panel row, starting at 1.
    pub row: u32,
    /// Panel column, starting at 1.
    pub column: u32,
    /// X offset of the board origin, `(column − 1) × (width + gapX)`.
    pub offset_x: f64,
    /// Y offset of the board origin, `(row − 1) × (depth + gapY)`.
    pub offset_y: f64,
}

/// One component placement in panel coordinates.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelPlacement {
    /// Board number the placement belongs to, starting at 1.
    pub board: u32,
    /// Panel X coordinate, `offsetX + component x`.
    pub x: f64,
    /// Panel Y coordinate, `offsetY + component y`.
    pub y: f64,
}

/// One component with its placement on every board instance.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MappedComponent {
    /// Reference designator.
    pub refdes: String,
    /// Package identifier.
    pub package: String,
    /// Board side.
    pub side: Side,
    /// X coordinate on the single board.
    pub x: f64,
    /// Y coordinate on the single board.
    pub y: f64,
    /// Rotation in degrees.
    pub rotation: f64,
    /// Placements in board order; empty when the coordinates are not finite.
    pub instances: Vec<PanelPlacement>,
}

/// Single-board to panel coordinate map.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelMapReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Board width, in millimetres.
    pub width: f64,
    /// Board depth, in millimetres.
    pub depth: f64,
    /// Board thickness, in millimetres.
    pub thickness: f64,
    /// Number of panel rows.
    pub rows: u32,
    /// Number of panel columns.
    pub columns: u32,
    /// Gap between columns, in millimetres.
    pub gap_x: f64,
    /// Gap between rows, in millimetres.
    pub gap_y: f64,
    /// Number of boards on the panel.
    pub board_count: u32,
    /// Panel width including column gaps, in millimetres.
    pub panel_width: f64,
    /// Panel depth including row gaps, in millimetres.
    pub panel_depth: f64,
    /// Board origins in row-major order, one per board.
    pub instances: Vec<PanelInstance>,
    /// Components in document order, each with its board placements.
    pub components: Vec<MappedComponent>,
    /// Components whose coordinates fall outside the single board.
    pub outside_board: u32,
    /// Whether every component lies inside the single board.
    pub ok: bool,
}

/// Maps every component into every board instance of the stored panel.
///
/// A board that is not finite and positive reports `board/invalid`; a panel
/// with zero rows or columns, or with a negative or non-finite gap, reports
/// `panel/invalid`. Instances are numbered row-major from 1 at the panel
/// origin: `board = (row − 1) × columns + column`,
/// `offsetX = (column − 1) × (width + gapX)` and
/// `offsetY = (row − 1) × (depth + gapY)`. A component placement is
/// `offsetX + x` and `offsetY + y`. The document is not modified.
pub fn map(document: &SmtDocument) -> Result<PanelMapReport, SmtError> {
    let width = document.board.width;
    let depth = document.board.depth;
    if !positive(width) || !positive(depth) || !positive(document.board.thickness) {
        return Err(SmtError::new("board/invalid", "board width, depth and thickness must be finite and greater than zero"));
    }
    if document.panel.rows == 0 || document.panel.columns == 0 {
        return Err(SmtError::new("panel/invalid", "a panel needs at least one row and one column"));
    }
    if !non_negative(document.panel.gap_x) || !non_negative(document.panel.gap_y) {
        return Err(SmtError::new("panel/invalid", "panel gaps must be finite and not negative"));
    }

    let rows = document.panel.rows;
    let columns = document.panel.columns;
    let gap_x = document.panel.gap_x;
    let gap_y = document.panel.gap_y;
    let board_count = rows.saturating_mul(columns);

    let mut instances = Vec::with_capacity(usize::try_from(board_count).unwrap_or(0));
    for row in 1..=rows {
        for column in 1..=columns {
            let offset_x = f64::from(column - 1) * (width + gap_x);
            let offset_y = f64::from(row - 1) * (depth + gap_y);
            instances.push(PanelInstance {
                board: (row - 1).saturating_mul(columns).saturating_add(column),
                row,
                column,
                offset_x,
                offset_y,
            });
        }
    }

    let mut outside_board = 0;
    let mut components = Vec::with_capacity(document.components.len());
    for component in &document.components {
        let finite = component.x.is_finite() && component.y.is_finite();
        if !finite || component.x < 0.0 || component.y < 0.0 || component.x > width || component.y > depth {
            outside_board += 1;
        }
        let placements = if finite {
            instances
                .iter()
                .map(|instance| PanelPlacement {
                    board: instance.board,
                    x: instance.offset_x + component.x,
                    y: instance.offset_y + component.y,
                })
                .collect()
        }
        else {
            Vec::new()
        };
        components.push(MappedComponent {
            refdes: component.refdes.clone(),
            package: component.package.clone(),
            side: component.side,
            x: component.x,
            y: component.y,
            rotation: component.rotation,
            instances: placements,
        });
    }

    let panel_width = f64::from(columns) * width + f64::from(columns - 1) * gap_x;
    let panel_depth = f64::from(rows) * depth + f64::from(rows - 1) * gap_y;
    let ok = outside_board == 0;
    Ok(PanelMapReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        width,
        depth,
        thickness: document.board.thickness,
        rows,
        columns,
        gap_x,
        gap_y,
        board_count,
        panel_width,
        panel_depth,
        instances,
        components,
        outside_board,
        ok,
    })
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn non_negative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}
