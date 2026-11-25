//! Deterministic feeder setup drawing.
//!
//! The sheet turns the feeder allocation into a tape-bank layout: one slot per
//! lane in feeder id order, a load bar for the loaded share of the capacity and
//! the reference designators the allocation assigns to each lane. The drawing
//! reads the same [`crate::place::allocate`] report as the `feeders` export, so
//! a missing or undersized lane fails with the same validation codes. The SVG
//! counts millimetres at [`crate::drawing::SVG_PX_PER_MM`] pixels per
//! millimetre, reuses the placement-drawing colour palette and never carries a
//! timestamp, so two calls on the same document are byte-identical.

use serde::Serialize;

use crate::{
    SmtError,
    drawing::{self, DrawingLoss, TITLE_HEIGHT_MM},
    model::SmtDocument,
    place,
};

/// Width of the sheet content beside the margins, in millimetres.
const CONTENT_MM: f64 = 170.0;
/// Height of one lane row without mapping lines, in millimetres.
const ROW_MM: f64 = 11.0;
/// Height of one wrapped reference designator line, in millimetres.
const LINE_MM: f64 = 3.2;
/// Width of the load bar, in millimetres.
const BAR_MM: f64 = 30.0;
/// Longest mapping line before the reference list wraps, in characters.
const MAP_LINE_CHARS: usize = 64;

/// Counts of a rendered feeder setup drawing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeederDrawingSummary {
    /// Unit system taken from the document.
    pub units: String,
    /// Number of feeder lanes drawn.
    pub lanes: u32,
    /// Number of components the allocation assigned to a lane.
    pub mapped_components: u32,
    /// Number of components the allocation could not assign.
    pub unassigned_components: u32,
    /// Number of non-ASCII characters written as `?`.
    pub dropped_characters: u32,
}

impl FeederDrawingSummary {
    /// Returns the aggregated text losses of this drawing.
    pub fn losses(&self) -> Vec<DrawingLoss> {
        if self.dropped_characters == 0 {
            Vec::new()
        }
        else {
            vec![DrawingLoss { code: "feeder/non-ascii-text".to_string(), count: self.dropped_characters }]
        }
    }
}

/// One rendered feeder setup drawing and its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct FeederDrawing {
    /// Complete file contents.
    pub text: String,
    /// Counts and losses of the rendered drawing.
    pub summary: FeederDrawingSummary,
}

struct Lane {
    id: String,
    package: String,
    pitch: f64,
    loaded: u32,
    capacity: u32,
    assigned: u32,
    remaining: u32,
    refdes: Vec<String>,
}

struct Plan {
    id: String,
    units: String,
    revision: u32,
    boards: u32,
    lanes: Vec<Lane>,
    unassigned: Vec<String>,
    summary: FeederDrawingSummary,
}

/// Renders the feeder setup as a millimetre SVG sheet.
///
/// The lanes come from [`crate::place::allocate`], so anything the `feeders`
/// report rejects — an unknown unit, a missing or undersized lane — fails with
/// the same code. A document without feeders or components still produces a
/// framed sheet. The result is deterministic.
pub fn render_svg(document: &SmtDocument, id: &str) -> Result<FeederDrawing, SmtError> {
    let plan = build_plan(document, id)?;
    let layout = Layout::new(&plan);
    let mut svg = String::new();
    drawing::svg_begin(&mut svg, layout.width, layout.height);
    push_svg_header(&mut svg, &plan, &layout);
    push_svg_lanes(&mut svg, &plan, &layout);
    push_svg_title(&mut svg, &plan, &layout);
    drawing::svg_end(&mut svg);
    Ok(FeederDrawing { text: svg, summary: plan.summary })
}

fn build_plan(document: &SmtDocument, id: &str) -> Result<Plan, SmtError> {
    let allocation = place::allocate(document)?;
    let (id, mut dropped) = drawing::ascii(id);
    let (units, count) = drawing::ascii(&document.units);
    dropped += count;

    let mut lanes = Vec::with_capacity(allocation.feeders.len());
    for lane in &allocation.feeders {
        let (lane_id, count) = drawing::ascii(&lane.id);
        dropped += count;
        let (package, count) = drawing::ascii(&lane.package);
        dropped += count;
        let pitch = document.feeders.iter().find(|feeder| feeder.id == lane.id).map_or(0.0, |feeder| feeder.pitch);
        let mut refdes = Vec::with_capacity(lane.refdes.len());
        for reference in &lane.refdes {
            let (text, count) = drawing::ascii(reference);
            dropped += count;
            refdes.push(text);
        }
        lanes.push(Lane {
            id: lane_id,
            package,
            pitch,
            loaded: lane.loaded,
            capacity: lane.capacity,
            assigned: lane.assigned,
            remaining: lane.remaining,
            refdes,
        });
    }

    let mut unassigned = Vec::with_capacity(allocation.unassigned.len());
    for reference in &allocation.unassigned {
        let (text, count) = drawing::ascii(reference);
        dropped += count;
        unassigned.push(text);
    }

    let mapped_components = lanes.iter().map(|lane| lane.refdes.len()).sum::<usize>();
    let summary = FeederDrawingSummary {
        units: units.clone(),
        lanes: lanes.len() as u32,
        mapped_components: mapped_components as u32,
        unassigned_components: unassigned.len() as u32,
        dropped_characters: dropped as u32,
    };
    Ok(Plan { id, units, revision: document.revision, boards: document.panel.board_count(), lanes, unassigned, summary })
}

struct Layout {
    width: f64,
    height: f64,
    rows_y: f64,
    title_y: f64,
}

impl Layout {
    fn new(plan: &Plan) -> Self {
        let rows_y = drawing::DRAW_MARGIN_MM + 18.0;
        let rows_height = plan.lanes.iter().map(|lane| ROW_MM + wrapped_lines(&lane.refdes) as f64 * LINE_MM).sum::<f64>();
        let notes_y = if plan.lanes.is_empty() { rows_y + 8.0 } else { rows_y + rows_height + 6.0 };
        let title_y = notes_y + 10.0;
        Self {
            width: 2.0 * drawing::DRAW_MARGIN_MM + CONTENT_MM,
            height: title_y + TITLE_HEIGHT_MM + drawing::DRAW_MARGIN_MM,
            rows_y,
            title_y,
        }
    }
}

/// Wraps the reference list into comma-separated lines of at most
/// [`MAP_LINE_CHARS`] characters.
fn wrapped_lines(refdes: &[String]) -> usize {
    if refdes.is_empty() {
        return 1;
    }
    let mut lines = 1;
    let mut length = 0;
    for reference in refdes {
        let addition = reference.len() + if length == 0 { 0 } else { 2 };
        if length + addition > MAP_LINE_CHARS {
            lines += 1;
            length = reference.len();
        }
        else {
            length += addition;
        }
    }
    lines
}

/// Splits the reference list into the same lines [`wrapped_lines`] counts.
fn wrap(refdes: &[String]) -> Vec<String> {
    if refdes.is_empty() {
        return vec!["no components assigned".to_string()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for reference in refdes {
        let addition = if current.is_empty() { reference.len() } else { reference.len() + 2 };
        if current.len() + addition > MAP_LINE_CHARS {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str(", ");
        }
        current.push_str(reference);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn push_svg_header(svg: &mut String, plan: &Plan, layout: &Layout) {
    let x = drawing::DRAW_MARGIN_MM;
    let y = drawing::DRAW_MARGIN_MM;
    svg.push_str(&format!("  <g id=\"header\" transform=\"translate({} {})\">\n", drawing::number(x), drawing::number(y)));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"4\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"3.2\" font-weight=\"bold\" fill=\"#{}\">FEEDER SETUP</text>\n",
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"8.6\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" fill=\"#{}\">tape bank, lanes in feeder id order</text>\n",
        drawing::hex(drawing::MUTED)
    ));
    svg.push_str(&format!(
        "    <line x1=\"0\" y1=\"12.4\" x2=\"{}\" y2=\"12.4\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        drawing::number(CONTENT_MM),
        drawing::hex(drawing::RULE)
    ));
    for (column, label) in ["SLOT", "PACKAGE", "PITCH", "LOADED", "LOAD BAR", "ASSIGNED", "REMAINING"].iter().enumerate() {
        let offset = [0.0, 20.0, 55.0, 80.0, 104.0, 140.0, 158.0][column];
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"16.6\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"1.8\" fill=\"#{}\">{label}</text>\n",
            drawing::number(offset),
            drawing::hex(drawing::MUTED)
        ));
    }
    if plan.lanes.is_empty() {
        svg.push_str(&format!(
            "    <text x=\"0\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.1\" fill=\"#{}\">no feeder lanes declared</text>\n",
            drawing::number(layout.rows_y - y + 4.0),
            drawing::hex(drawing::MUTED)
        ));
    }
    svg.push_str("  </g>\n");
}

fn push_svg_lanes(svg: &mut String, plan: &Plan, layout: &Layout) {
    if plan.lanes.is_empty() {
        return;
    }
    let x = drawing::DRAW_MARGIN_MM;
    svg.push_str(&format!(
        "  <g id=\"lanes\" transform=\"translate({} {})\">\n",
        drawing::number(x),
        drawing::number(layout.rows_y)
    ));
    let mut row_y = 0.0;
    for lane in &plan.lanes {
        svg.push_str(&format!("    <g data-lane=\"{}\">\n", drawing::escape_xml(&lane.id)));
        svg.push_str(&format!(
            "      <rect x=\"0\" y=\"{}\" width=\"14\" height=\"9\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
            drawing::number(row_y + 1.0),
            drawing::hex(drawing::BOARD_FILL),
            drawing::hex(drawing::BOARD_EDGE)
        ));
        svg.push_str(&format!(
            "      <text x=\"7\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" text-anchor=\"middle\" fill=\"#{}\">{}</text>\n",
            drawing::number(row_y + 6.4),
            drawing::hex(drawing::INK),
            drawing::escape_xml(&lane.id)
        ));
        svg.push_str(&format!(
            "      <text x=\"20\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" fill=\"#{}\">{}</text>\n",
            drawing::number(row_y + 6.4),
            drawing::hex(drawing::INK),
            drawing::escape_xml(&lane.package)
        ));
        svg.push_str(&format!(
            "      <text x=\"55\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" fill=\"#{}\">{}</text>\n",
            drawing::number(row_y + 6.4),
            drawing::hex(drawing::INK),
            drawing::number(lane.pitch)
        ));
        svg.push_str(&format!(
            "      <text x=\"80\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" fill=\"#{}\">{}/{}</text>\n",
            drawing::number(row_y + 6.4),
            drawing::hex(drawing::INK),
            lane.loaded,
            lane.capacity
        ));
        svg.push_str(&format!(
            "      <rect data-load-bar=\"true\" x=\"104\" y=\"{}\" width=\"{}\" height=\"3\" fill=\"none\" stroke=\"#{}\" stroke-width=\"0.2\"/>\n",
            drawing::number(row_y + 4.0),
            drawing::number(BAR_MM),
            drawing::hex(drawing::RULE)
        ));
        let ratio = if lane.capacity == 0 { 0.0 } else { (lane.loaded as f64 / lane.capacity as f64).min(1.0) };
        if ratio > 0.0 {
            svg.push_str(&format!(
                "      <rect x=\"104\" y=\"{}\" width=\"{}\" height=\"3\" fill=\"#{}\" stroke=\"none\"/>\n",
                drawing::number(row_y + 4.0),
                drawing::number(BAR_MM * ratio),
                drawing::hex(drawing::TOP_EDGE)
            ));
        }
        svg.push_str(&format!(
            "      <text x=\"140\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" fill=\"#{}\">{}</text>\n",
            drawing::number(row_y + 6.4),
            drawing::hex(drawing::INK),
            lane.assigned
        ));
        svg.push_str(&format!(
            "      <text x=\"158\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" fill=\"#{}\">{}</text>\n",
            drawing::number(row_y + 6.4),
            drawing::hex(drawing::INK),
            lane.remaining
        ));
        for (index, line) in wrap(&lane.refdes).iter().enumerate() {
            svg.push_str(&format!(
                "      <text x=\"20\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"1.8\" fill=\"#{}\">{}</text>\n",
                drawing::number(row_y + 12.0 + index as f64 * LINE_MM),
                drawing::hex(drawing::MUTED),
                drawing::escape_xml(line)
            ));
        }
        svg.push_str("    </g>\n");
        row_y += ROW_MM + wrapped_lines(&lane.refdes) as f64 * LINE_MM;
    }
    svg.push_str("  </g>\n");
}

fn push_svg_title(svg: &mut String, plan: &Plan, layout: &Layout) {
    let x = drawing::DRAW_MARGIN_MM;
    let y = layout.title_y;
    svg.push_str(&format!(
        "  <g id=\"notes\" transform=\"translate({} {})\">\n",
        drawing::number(x),
        drawing::number(y - 10.0)
    ));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"0\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" fill=\"#{}\">load bar = loaded / capacity; mapping lines list the assigned reference designators</text>\n",
        drawing::hex(drawing::MUTED)
    ));
    let unassigned = if plan.unassigned.is_empty() { vec!["none".to_string()] } else { wrap(&plan.unassigned) };
    for (index, line) in unassigned.iter().enumerate() {
        svg.push_str(&format!(
            "    <text x=\"0\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" fill=\"#{}\">unassigned: {}</text>\n",
            drawing::number(4.2 + index as f64 * 2.4),
            drawing::hex(drawing::MUTED),
            drawing::escape_xml(line)
        ));
    }
    svg.push_str("  </g>\n");
    svg.push_str(&format!("  <g id=\"title-block\" transform=\"translate({} {})\">\n", drawing::number(x), drawing::number(y)));
    svg.push_str(&format!(
        "    <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#{}\" stroke-width=\"0.45\"/>\n",
        drawing::number(CONTENT_MM),
        drawing::number(TITLE_HEIGHT_MM),
        drawing::hex(drawing::PANEL_EDGE)
    ));
    // The allocation is for a single board, so a multi-board panel is annotated
    // with its multiplier instead of a millimetre scale note.
    let multiplier = if plan.boards > 1 { format!(" | per board (x{} on the panel)", plan.boards) } else { String::new() };
    let lines = [
        format!("OpenSMT feeder setup drawing | drawing OSMT-FD-01 | revision {}", plan.revision),
        drawing::title_stamp("feeder setup", "OSMT-FD-01", &plan.id, &plan.units, "1:1", "n/a (feeder bank)"),
        format!(
            "{} lane(s) | {} component(s) mapped | {} unassigned{}",
            plan.summary.lanes, plan.summary.mapped_components, plan.summary.unassigned_components, multiplier
        ),
        "lane order = feeder id order; the allocation caps every lane at its loaded quantity".to_string(),
    ];
    for (index, line) in lines.iter().enumerate() {
        svg.push_str(&format!(
            "    <text x=\"2.5\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{}</text>\n",
            drawing::number(5.6 + index as f64 * 5.6),
            if index == 0 { "2.9" } else { "2.2" },
            if index == 0 { drawing::hex(drawing::INK) } else { drawing::hex(drawing::MUTED) },
            drawing::escape_xml(line)
        ));
    }
    svg.push_str("  </g>\n");
}
