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
    drawing::{self, DrawingLoss},
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

/// Counts of a rendered panel map drawing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelMapDrawingSummary {
    /// Unit system taken from the document.
    pub units: String,
    /// Number of panel rows.
    pub rows: u32,
    /// Number of panel columns.
    pub columns: u32,
    /// Number of boards on the panel.
    pub board_count: u32,
    /// Panel width including gaps, in millimetres.
    pub panel_width: f64,
    /// Panel depth including gaps, in millimetres.
    pub panel_depth: f64,
    /// Number of mapped components.
    pub components: u32,
    /// Components whose coordinates fall outside the single board.
    pub outside_board: u32,
    /// Number of non-ASCII characters written as `?`.
    pub dropped_characters: u32,
}

impl PanelMapDrawingSummary {
    /// Returns the aggregated text losses of this drawing.
    pub fn losses(&self) -> Vec<DrawingLoss> {
        if self.dropped_characters == 0 {
            Vec::new()
        }
        else {
            vec![DrawingLoss { code: "panel-map/non-ascii-text".to_string(), count: self.dropped_characters }]
        }
    }
}

/// One rendered panel map drawing and its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelMapDrawing {
    /// Complete file contents.
    pub text: String,
    /// Counts and losses of the rendered drawing.
    pub summary: PanelMapDrawingSummary,
}

/// Draws the panel array with every board instance and its component dots.
///
/// The panel is scaled to fit a fixed plot box, board origins are numbered in
/// row-major order, and each component is drawn as a side-coloured dot in panel
/// coordinates. A coordinate origin cross marks the panel `(0, 0)`. The result
/// is deterministic and the document is never modified.
pub fn render_svg(document: &SmtDocument, id: &str) -> Result<PanelMapDrawing, SmtError> {
    let report = map(document)?;
    let (id, mut dropped) = drawing::ascii(id);
    let (units, count) = drawing::ascii(&document.units);
    dropped += count;

    let plot_x = drawing::DRAW_MARGIN_MM + 18.0;
    let plot_y = drawing::DRAW_MARGIN_MM + 18.0;
    let plot_w = 150.0;
    let scale = (plot_w / report.panel_width).min(110.0 / report.panel_depth);
    let draw_w = report.panel_width * scale;
    let draw_h = report.panel_depth * scale;
    let width = plot_x + plot_w + drawing::DRAW_MARGIN_MM;
    let title_y = plot_y + draw_h + 14.0;
    let height = title_y + drawing::TITLE_HEIGHT_MM + drawing::DRAW_MARGIN_MM;
    let px = |x: f64| plot_x + x * scale;
    let py = |y: f64| plot_y + y * scale;

    let mut svg = String::new();
    drawing::svg_begin(&mut svg, width, height);

    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"3.2\" font-weight=\"bold\" fill=\"#{}\">PANEL MAP</text>\n",
        drawing::number(drawing::DRAW_MARGIN_MM),
        drawing::number(drawing::DRAW_MARGIN_MM + 4.0),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" fill=\"#{}\">{} x {} array | {} board(s) | {} component(s){}</text>\n",
        drawing::number(drawing::DRAW_MARGIN_MM),
        drawing::number(drawing::DRAW_MARGIN_MM + 8.6),
        drawing::hex(drawing::MUTED),
        report.rows,
        report.columns,
        report.board_count,
        report.components.len(),
        if report.ok { String::new() } else { format!(" | {} outside board", report.outside_board) }
    ));

    // Panel outline.
    svg.push_str(&format!(
        "  <rect data-role=\"panel\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.5\"/>\n",
        drawing::number(plot_x),
        drawing::number(plot_y),
        drawing::number(draw_w),
        drawing::number(draw_h),
        drawing::hex(drawing::BOARD_FILL),
        drawing::hex(drawing::PANEL_EDGE)
    ));

    // Board instances with their row-major number.
    for instance in &report.instances {
        let board_w = report.width * scale;
        let board_h = report.depth * scale;
        svg.push_str(&format!(
            "  <rect data-board=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
            instance.board,
            drawing::number(px(instance.offset_x)),
            drawing::number(py(instance.offset_y)),
            drawing::number(board_w),
            drawing::number(board_h),
            drawing::hex(drawing::BOARD_EDGE)
        ));
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" fill=\"#{}\">B{}</text>\n",
            drawing::number(px(instance.offset_x) + 1.4),
            drawing::number(py(instance.offset_y) + 3.4),
            drawing::hex(drawing::MUTED),
            instance.board
        ));
    }

    // Component dots in panel coordinates, coloured by side.
    for component in &report.components {
        let (fill, edge) = drawing::colors(component.side);
        for placement in &component.instances {
            svg.push_str(&format!(
                "  <circle data-refdes=\"{}\" cx=\"{}\" cy=\"{}\" r=\"0.9\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.2\"/>\n",
                drawing::escape_xml(&component.refdes),
                drawing::number(px(placement.x)),
                drawing::number(py(placement.y)),
                drawing::hex(fill),
                drawing::hex(edge)
            ));
        }
    }

    // Coordinate origin cross at the panel (0, 0).
    svg.push_str(&format!(
        "  <path d=\"M {} {} l 2.4 2.4 M {} {} l 2.4 -2.4\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        drawing::number(plot_x - 1.2),
        drawing::number(plot_y - 1.2),
        drawing::number(plot_x - 1.2),
        drawing::number(plot_y + 1.2),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"1.8\" fill=\"#{}\">ORIGIN (0,0)</text>\n",
        drawing::number(plot_x + 1.6),
        drawing::number(plot_y - 1.4),
        drawing::hex(drawing::MUTED)
    ));

    // Title block.
    svg.push_str(&format!(
        "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#{}\" stroke-width=\"0.45\"/>\n",
        drawing::number(drawing::DRAW_MARGIN_MM),
        drawing::number(title_y),
        drawing::number(width - 2.0 * drawing::DRAW_MARGIN_MM),
        drawing::number(drawing::TITLE_HEIGHT_MM),
        drawing::hex(drawing::PANEL_EDGE)
    ));
    let lines = [
        format!("OpenSMT panel map drawing | drawing OSMT-PM-01 | revision {}", document.revision),
        drawing::title_stamp("panel map", "OSMT-PM-01", &id, &units, "fit to frame", "top and bottom"),
        format!(
            "board {} x {} mm | gaps {} x {} mm | panel {} x {} mm",
            drawing::number(report.width),
            drawing::number(report.depth),
            drawing::number(report.gap_x),
            drawing::number(report.gap_y),
            drawing::number(report.panel_width),
            drawing::number(report.panel_depth)
        ),
        "boards are numbered row-major from the panel origin".to_string(),
    ];
    for (index, line) in lines.iter().enumerate() {
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{}</text>\n",
            drawing::number(drawing::DRAW_MARGIN_MM + 2.5),
            drawing::number(title_y + 5.6 + index as f64 * 5.6),
            if index == 0 { "2.4" } else { "2.0" },
            drawing::hex(if index == 0 { drawing::INK } else { drawing::MUTED }),
            drawing::escape_xml(line)
        ));
    }

    drawing::svg_end(&mut svg);
    let summary = PanelMapDrawingSummary {
        units,
        rows: report.rows,
        columns: report.columns,
        board_count: report.board_count,
        panel_width: report.panel_width,
        panel_depth: report.panel_depth,
        components: report.components.len() as u32,
        outside_board: report.outside_board,
        dropped_characters: dropped as u32,
    };
    Ok(PanelMapDrawing { text: svg, summary })
}
