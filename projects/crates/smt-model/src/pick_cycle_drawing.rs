//! Pick-cycle matrix drawing.
//!
//! One row per placement cycle, one column per head slot. A used slot carries
//! its package and the reference designators of the picks it serves; an unused
//! slot is greyed out. The right column repeats the cycle detail so a row can be
//! read without decoding the block colours, and a summary band reports the head
//! utilisation. The sheet has no timestamp, so two calls on the same document
//! are byte-identical.

use serde::Serialize;

use crate::{
    SmtError,
    drawing::{self, DrawingLoss},
    model::SmtDocument,
    pick_cycle::{self, PickCycleReport},
};

/// Head count used by the export path when the document does not override it.
pub const HEADS: u32 = pick_cycle::DEFAULT_HEADS;

/// Counts of a rendered pick-cycle drawing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickCycleDrawingSummary {
    /// Unit system taken from the document.
    pub units: String,
    /// Head slots per cycle.
    pub heads: u32,
    /// Number of cycles drawn.
    pub cycles: u32,
    /// Number of picks in the program.
    pub picks: u32,
    /// Number of head slots across every cycle.
    pub head_slots: u32,
    /// Share of head slots that carry a pick, in `0.0..=1.0`.
    pub utilization: f64,
    /// Number of head slots left empty.
    pub wasted_slots: u32,
    /// Number of cycles a single-head machine would add.
    pub cycles_saved: u32,
}

impl PickCycleDrawingSummary {
    /// Returns the aggregated text losses of this drawing.
    pub fn losses(&self) -> Vec<DrawingLoss> {
        Vec::new()
    }
}

/// One rendered pick-cycle drawing and its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct PickCycleDrawing {
    /// Complete file contents.
    pub text: String,
    /// Counts of the rendered drawing.
    pub summary: PickCycleDrawingSummary,
}

const MARGIN: f64 = drawing::DRAW_MARGIN_MM;
const ROW_LABEL_W: f64 = 16.0;
const ROW_H: f64 = 6.4;
const SLOT_H: f64 = 4.8;
const DETAIL_ROW_H: f64 = 4.6;
const BAND_H: f64 = 22.0;
const TARGET_W: f64 = 232.0;

/// Renders the pick-cycle matrix of `document` as an SVG sheet.
pub fn render_svg(document: &SmtDocument, id: &str) -> Result<PickCycleDrawing, SmtError> {
    let report = pick_cycle::plan(document, HEADS)?;
    let summary = PickCycleDrawingSummary {
        units: report.units.clone(),
        heads: report.heads,
        cycles: report.cycles,
        picks: report.picks,
        head_slots: report.head_slots,
        utilization: report.utilization,
        wasted_slots: report.wasted_slots,
        cycles_saved: report.cycles_saved,
    };
    let cell_w = ((TARGET_W - ROW_LABEL_W) / f64::from(report.heads.max(1))).clamp(24.0, 38.0);
    let matrix_w = ROW_LABEL_W + cell_w * f64::from(report.heads.max(1));
    let detail_x = MARGIN + matrix_w + drawing::COLUMN_GAP_MM;
    let legend_note = "utilisation = picks / (cycles x heads); cycles merge consecutive same-package picks";
    let detail_w = report
        .entries
        .iter()
        .map(|cycle| drawing::text_width(&detail_line(cycle), 2.3))
        .fold(drawing::text_width("CYCLE DETAIL", 2.4), f64::max)
        .max(drawing::text_width(legend_note, 2.3))
        .clamp(58.0, 150.0);
    let width = detail_x + detail_w + MARGIN;
    let matrix_y = MARGIN + 18.0 + BAND_H + 6.0;
    let matrix_h = f64::from(report.cycles.max(1)) * ROW_H;
    let detail_h = f64::from(report.cycles.max(1)) * DETAIL_ROW_H + 6.0;
    // The legend sits under the detail list, so a tall matrix is not pushed up
    // by a full-width legend band.
    let legend_y = matrix_y + detail_h + 4.0;
    let content_bottom = (matrix_y + matrix_h).max(legend_y + 34.0);

    let mut svg = String::new();
    svg.push_str("  <g id=\"pick-cycle\">\n");
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" font-weight=\"bold\" fill=\"#{}\">PICK CYCLE MATRIX</text>\n",
        drawing::number(MARGIN),
        drawing::number(MARGIN + 5.0),
        drawing::number(drawing::FONT_TITLE),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{} cycle(s) x {} head slot(s); used slots carry the package, unused slots are greyed</text>\n",
        drawing::number(MARGIN),
        drawing::number(MARGIN + 11.0),
        drawing::number(drawing::FONT_SUBTITLE),
        drawing::hex(drawing::MUTED),
        report.cycles,
        report.heads
    ));
    push_band(&mut svg, &report, MARGIN, MARGIN + 15.0, width - 2.0 * MARGIN);
    push_matrix(&mut svg, &report, MARGIN, matrix_y, cell_w, detail_x, detail_w);
    push_legend(&mut svg, detail_x, legend_y, detail_w);
    svg.push_str("  </g>\n");
    let text = drawing::render_sheet((0.0, 0.0, width, content_bottom), &svg, &title_text(&report, id, "A4 landscape"));
    Ok(PickCycleDrawing { text, summary })
}

fn push_band(svg: &mut String, report: &PickCycleReport, x: f64, y: f64, width: f64) {
    let width = width.max(120.0);
    svg.push_str(&format!(
        "  <g id=\"utilization\">\n    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        drawing::number(x),
        drawing::number(y),
        drawing::number(width),
        drawing::number(BAND_H),
        drawing::hex(drawing::BOARD_FILL),
        drawing::hex(drawing::PANEL_EDGE)
    ));
    let metrics = [
        ("UTILIZATION", format!("{:.1}%", report.utilization * 100.0)),
        ("HEAD SLOTS", report.head_slots.to_string()),
        ("PICKS", report.picks.to_string()),
        ("WASTED SLOTS", report.wasted_slots.to_string()),
        ("CYCLES SAVED", report.cycles_saved.to_string()),
    ];
    let column = width / metrics.len() as f64;
    for (index, (label, value)) in metrics.iter().enumerate() {
        let left = x + index as f64 * column;
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">{}</text>\n",
            drawing::number(left + 3.0),
            drawing::number(y + 6.0),
            drawing::hex(drawing::MUTED),
            label
        ));
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"4.2\" font-weight=\"bold\" fill=\"#{}\">{}</text>\n",
            drawing::number(left + 3.0),
            drawing::number(y + 13.6),
            drawing::hex(drawing::INK),
            value
        ));
    }
    let bar_w = width - 6.0;
    svg.push_str(&format!(
        "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"3.0\" fill=\"#{}\"/>\n",
        drawing::number(x + 3.0),
        drawing::number(y + 16.5),
        drawing::number(bar_w),
        drawing::hex(drawing::BOARD_EDGE)
    ));
    svg.push_str(&format!(
        "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"3.0\" fill=\"#{}\"/>\n",
        drawing::number(x + 3.0),
        drawing::number(y + 16.5),
        drawing::number((bar_w * report.utilization).max(0.3)),
        drawing::hex(drawing::TOP_EDGE)
    ));
    svg.push_str("  </g>\n");
}

fn push_matrix(svg: &mut String, report: &PickCycleReport, x: f64, y: f64, cell_w: f64, detail_x: f64, detail_w: f64) {
    let heads = report.heads.max(1) as usize;
    svg.push_str("  <g id=\"cycles\">\n");
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">CYCLE</text>\n",
        drawing::number(x),
        drawing::number(y - 2.5),
        drawing::hex(drawing::MUTED)
    ));
    for head in 0..heads {
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">H{}</text>\n",
            drawing::number(x + ROW_LABEL_W + head as f64 * cell_w + 1.0),
            drawing::number(y - 2.5),
            drawing::hex(drawing::MUTED),
            head + 1
        ));
    }
    for (index, cycle) in report.entries.iter().enumerate() {
        let top = y + index as f64 * ROW_H;
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.6\" font-weight=\"bold\" fill=\"#{}\">C{}</text>\n",
            drawing::number(x),
            drawing::number(top + 4.2),
            drawing::hex(drawing::INK),
            cycle.cycle
        ));
        for slot in 0..heads {
            let left = x + ROW_LABEL_W + slot as f64 * cell_w;
            let used = slot < cycle.refdes.len();
            let (fill, edge) = if used {
                match cycle.side {
                    crate::model::Side::Top => (drawing::TOP_FILL, drawing::TOP_EDGE),
                    crate::model::Side::Bottom => (drawing::BOTTOM_FILL, drawing::BOTTOM_EDGE),
                }
            }
            else {
                (drawing::UNUSED_FILL, drawing::UNUSED_EDGE)
            };
            svg.push_str(&format!(
                "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"0.8\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.25\"/>\n",
                drawing::number(left + 0.8),
                drawing::number(top + 0.8),
                drawing::number(cell_w - 1.6),
                drawing::number(SLOT_H),
                drawing::hex(fill),
                drawing::hex(edge)
            ));
            if used {
                let label = format!("{} {}", cycle.package, cycle.refdes[slot]);
                let text = shrink_to_fit(&label, cell_w - 2.2, 2.1);
                svg.push_str(&format!(
                    "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.1\" fill=\"#{}\">{}</text>\n",
                    drawing::number(left + 1.6),
                    drawing::number(top + 4.1),
                    drawing::hex(drawing::INK),
                    drawing::escape_xml(&text)
                ));
            }
        }
    }
    let detail_y = y + 6.0;
    let side_x = detail_x;
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">CYCLE DETAIL</text>\n",
        drawing::number(side_x),
        drawing::number(y - 2.5),
        drawing::hex(drawing::MUTED)
    ));
    for (index, cycle) in report.entries.iter().enumerate() {
        let row = detail_y + index as f64 * DETAIL_ROW_H;
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.3\" fill=\"#{}\">{}</text>\n",
            drawing::number(side_x),
            drawing::number(row),
            drawing::hex(drawing::INK),
            drawing::escape_xml(&shrink_to_fit(&detail_line(cycle), detail_w, 2.3))
        ));
    }
    svg.push_str("  </g>\n");
}

fn detail_line(cycle: &pick_cycle::PickCycle) -> String {
    format!(
        "C{}  {} {}  |  {} pick(s)  |  {}",
        cycle.cycle,
        cycle.package,
        match cycle.side {
            crate::model::Side::Top => "top",
            crate::model::Side::Bottom => "bottom",
        },
        cycle.picks,
        cycle.refdes.join(" ")
    )
}

fn push_legend(svg: &mut String, x: f64, y: f64, detail_width: f64) {
    svg.push_str(&format!("  <g id=\"legend\" transform=\"translate({} {})\">\n", drawing::number(x), drawing::number(y)));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"0\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" font-weight=\"bold\" fill=\"#{}\">LEGEND</text>\n",
        drawing::number(drawing::FONT_SECTION),
        drawing::hex(drawing::INK)
    ));
    let rows = [
        (drawing::TOP_FILL, drawing::TOP_EDGE, "top-side pick"),
        (drawing::BOTTOM_FILL, drawing::BOTTOM_EDGE, "bottom-side pick"),
        (drawing::UNUSED_FILL, drawing::UNUSED_EDGE, "unused head slot"),
    ];
    for (index, (fill, edge, text)) in rows.iter().enumerate() {
        let top = 5.0 + index as f64 * 7.0;
        svg.push_str(&format!(
            "    <rect x=\"0\" y=\"{}\" width=\"9\" height=\"4\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
            drawing::number(top),
            drawing::hex(fill),
            drawing::hex(edge)
        ));
        svg.push_str(&format!(
            "    <text x=\"13\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.5\" fill=\"#{}\">{}</text>\n",
            drawing::number(top + 3.2),
            drawing::hex(drawing::INK),
            text
        ));
    }
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"31\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.3\" fill=\"#{}\">{}</text>\n",
        drawing::hex(drawing::MUTED),
        drawing::escape_xml(&shrink_to_fit(
            "utilisation = picks / (cycles x heads); cycles merge consecutive same-package picks",
            detail_width,
            2.3
        ))
    ));
    svg.push_str("  </g>\n");
}

fn title_text(report: &PickCycleReport, id: &str, scale_note: &str) -> Vec<drawing::TextLine> {
    let title = |text: String| drawing::TextLine { text, size: drawing::FONT_TITLE, emphasis: true };
    let body = |text: String| drawing::TextLine { text, size: drawing::FONT_BODY, emphasis: false };
    vec![
        title("OpenSMT pick-cycle drawing | drawing OSMT-PC-01 | revision 1".to_string()),
        body(drawing::title_stamp("pick-cycle", "OSMT-PC-01", id, &report.units, "not to scale", "cycle x head slot")),
        body(format!(
            "{} cycle(s) | {} pick(s) | {} of {} head slot(s) used | {:.1}% utilisation | {} wasted slot(s)",
            report.cycles,
            report.picks,
            report.picks,
            report.head_slots,
            report.utilization * 100.0,
            report.wasted_slots
        )),
        body(format!(
            "one row per cycle; used slots show package and reference, unused slots are grey; {} cycle(s) saved against a single-head machine",
            report.cycles_saved
        )),
        body(format!("millimetres; {scale_note}")),
    ]
}

fn shrink_to_fit(text: &str, width: f64, size: f64) -> String {
    if drawing::text_width(text, size) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for character in text.chars() {
        if drawing::text_width(&format!("{out}{character}..."), size) > width {
            break;
        }
        out.push(character);
    }
    out.push_str("...");
    out
}
