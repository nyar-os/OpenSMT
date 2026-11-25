//! Deterministic pick-and-place sequence drawing.
//!
//! The sheet turns the default nearest-neighbour placement program into a
//! route map: one polyline per board side starting at the machine origin
//! `(0, 0)`, one numbered marker per step and a legend with the travel length.
//! The routes come from [`crate::place::sequence`], so a missing or undersized
//! lane fails with the same validation codes as the `program` export. The SVG
//! counts millimetres at [`crate::drawing::SVG_PX_PER_MM`] pixels per
//! millimetre and the PDF is one A4 landscape page fitted from the same
//! layout; neither carries a timestamp, so two calls on the same document are
//! byte-identical.

use serde::Serialize;

use crate::{
    SmtError,
    drawing::{
        self, BOARD_EDGE, BOARD_FILL, BOTTOM_EDGE, BOTTOM_FILL, DRAW_MARGIN_MM, DrawingLoss, INK, MUTED, PANEL_EDGE,
        PDF_MARGIN, PDF_PAGE_HEIGHT, PDF_PAGE_WIDTH, PT_PER_MM, Pdf, RULE, SVG_PX_PER_MM, TITLE_HEIGHT_MM, TOP_EDGE, TOP_FILL,
        hex, number, pdf_page,
    },
    model::{Side, SmtDocument},
    place,
};

/// Width of the legend column beside the board, in millimetres.
const SIDE_WIDTH_MM: f64 = 46.0;
/// Radius of one step marker, in millimetres.
const STEP_RADIUS_MM: f64 = 0.9;

/// Counts of a rendered pick-and-place sequence drawing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceDrawingSummary {
    /// Unit system taken from the document.
    pub units: String,
    /// Board width in document units.
    pub board_width: f64,
    /// Board depth in document units.
    pub board_depth: f64,
    /// Number of placement steps on the sheet.
    pub steps: u32,
    /// Number of top-side steps.
    pub top_steps: u32,
    /// Number of bottom-side steps.
    pub bottom_steps: u32,
    /// Total head travel of the drawn routes, in millimetres.
    pub travel_length: f64,
    /// Number of non-ASCII characters written as `?`.
    pub dropped_characters: u32,
}

impl SequenceDrawingSummary {
    /// Returns the aggregated text losses of this drawing.
    pub fn losses(&self) -> Vec<DrawingLoss> {
        if self.dropped_characters == 0 {
            Vec::new()
        }
        else {
            vec![DrawingLoss { code: "sequence/non-ascii-text".to_string(), count: self.dropped_characters }]
        }
    }
}

/// One rendered pick-and-place sequence drawing and its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct SequenceDrawing {
    /// Complete file contents.
    pub text: String,
    /// Counts and losses of the rendered drawing.
    pub summary: SequenceDrawingSummary,
}

struct Step {
    index: u32,
    refdes: String,
    x: f64,
    y: f64,
    side: Side,
}

struct Plan {
    id: String,
    units: String,
    revision: u32,
    board_width: f64,
    board_depth: f64,
    steps: Vec<Step>,
    summary: SequenceDrawingSummary,
}

struct Layout {
    width: f64,
    height: f64,
    frame_x: f64,
    frame_y: f64,
    side_x: f64,
    title_y: f64,
}

/// Renders the placement routes as a millimetre SVG sheet.
///
/// The program is the default nearest-neighbour sequence over the allocated
/// lanes, so anything the `program` export rejects fails with the same code.
/// A document without components still produces a framed sheet. The result is
/// deterministic.
pub fn render_svg(document: &SmtDocument, id: &str) -> Result<SequenceDrawing, SmtError> {
    let plan = build_plan(document, id)?;
    let layout = Layout::new(&plan);
    let width = (layout.width * SVG_PX_PER_MM).round();
    let height = (layout.height * SVG_PX_PER_MM).round();
    let mut svg = String::new();
    svg.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{width:.0}\" height=\"{height:.0}\">\n",
        number(layout.width),
        number(layout.height)
    ));
    svg.push_str(&format!(
        "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>\n",
        number(layout.width),
        number(layout.height)
    ));
    push_svg_header(&mut svg);
    push_svg_frame(&mut svg, &plan, &layout);
    push_svg_routes(&mut svg, &plan, &layout);
    push_svg_side(&mut svg, &plan, &layout);
    push_svg_title(&mut svg, &plan, &layout, &format!("scale 1:1 in millimetres; 1 mm = {SVG_PX_PER_MM} px"));
    svg.push_str("</svg>\n");
    Ok(SequenceDrawing { text: svg, summary: plan.summary })
}

/// Renders the same routes as one A4 landscape PDF page.
pub fn render_pdf(document: &SmtDocument, id: &str) -> Result<SequenceDrawing, SmtError> {
    let plan = build_plan(document, id)?;
    let layout = Layout::new(&plan);
    let fit = ((PDF_PAGE_WIDTH - 2.0 * PDF_MARGIN) / (layout.width * PT_PER_MM))
        .min((PDF_PAGE_HEIGHT - 2.0 * PDF_MARGIN) / (layout.height * PT_PER_MM))
        .max(0.001);
    let scale = PT_PER_MM * fit;
    let origin_x = (PDF_PAGE_WIDTH - layout.width * scale) / 2.0;
    let origin_y = (PDF_PAGE_HEIGHT - layout.height * scale) / 2.0;
    let mut pdf = Pdf { content: String::new(), height: layout.height, scale, origin_x, origin_y };
    pdf.text(DRAW_MARGIN_MM, DRAW_MARGIN_MM + 4.0, 3.2, INK, "PICK AND PLACE SEQUENCE");
    pdf.text(DRAW_MARGIN_MM, DRAW_MARGIN_MM + 8.6, 2.0, MUTED, "nozzle routes in program order, one route per side");
    push_pdf_frame(&mut pdf, &plan, &layout);
    push_pdf_routes(&mut pdf, &plan, &layout);
    push_pdf_side(&mut pdf, &plan, &layout);
    let scale_note = format!("A4 landscape fit; 1 mm = {PT_PER_MM} pt x fit {fit:.3}");
    push_pdf_title(&mut pdf, &plan, &layout, &scale_note);
    Ok(SequenceDrawing { text: pdf_page(pdf.content), summary: plan.summary })
}

fn build_plan(document: &SmtDocument, id: &str) -> Result<Plan, SmtError> {
    let allocation = place::allocate(document)?;
    let program = place::sequence(document, &allocation)?;
    let (id, mut dropped) = drawing::ascii(id);
    let (units, count) = drawing::ascii(&document.units);
    dropped += count;

    let mut steps = Vec::with_capacity(program.entries.len());
    for entry in &program.entries {
        let (refdes, count) = drawing::ascii(&entry.refdes);
        dropped += count;
        steps.push(Step { index: entry.index + 1, refdes, x: entry.x, y: entry.y, side: entry.side });
    }
    let top_steps = steps.iter().filter(|step| step.side == Side::Top).count() as u32;
    let bottom_steps = steps.iter().filter(|step| step.side == Side::Bottom).count() as u32;
    let summary = SequenceDrawingSummary {
        units: units.clone(),
        board_width: document.board.width,
        board_depth: document.board.depth,
        steps: steps.len() as u32,
        top_steps,
        bottom_steps,
        travel_length: program.travel_length,
        dropped_characters: dropped as u32,
    };
    Ok(Plan {
        id,
        units,
        revision: document.revision,
        board_width: document.board.width,
        board_depth: document.board.depth,
        steps,
        summary,
    })
}

impl Layout {
    fn new(plan: &Plan) -> Self {
        let frame_x = DRAW_MARGIN_MM;
        let frame_y = DRAW_MARGIN_MM + HEADER_HEIGHT_MM;
        let side_x = DRAW_MARGIN_MM + plan.board_width + drawing::COLUMN_GAP_MM;
        let content_height = plan.board_depth.max(SIDE_HEIGHT_MM);
        let title_y = frame_y + content_height + 8.0;
        Self {
            width: DRAW_MARGIN_MM * 2.0 + plan.board_width + drawing::COLUMN_GAP_MM + SIDE_WIDTH_MM,
            height: title_y + TITLE_HEIGHT_MM + DRAW_MARGIN_MM,
            frame_x,
            frame_y,
            side_x,
            title_y,
        }
    }
}

/// Height of the legend column, in millimetres.
const SIDE_HEIGHT_MM: f64 = 46.0;
/// Height of the sheet header above the board frame, in millimetres.
const HEADER_HEIGHT_MM: f64 = 12.0;

fn push_svg_header(svg: &mut String) {
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"3.2\" font-weight=\"bold\" fill=\"#{}\">PICK AND PLACE SEQUENCE</text>\n",
        number(DRAW_MARGIN_MM),
        number(DRAW_MARGIN_MM + 4.0),
        hex(INK)
    ));
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" fill=\"#{}\">nozzle routes in program order, one route per side</text>\n",
        number(DRAW_MARGIN_MM),
        number(DRAW_MARGIN_MM + 8.6),
        hex(MUTED)
    ));
}

fn push_svg_frame(svg: &mut String, plan: &Plan, layout: &Layout) {
    svg.push_str(&format!(
        "  <rect data-role=\"board\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.4\"/>\n",
        number(layout.frame_x),
        number(layout.frame_y),
        number(plan.board_width),
        number(plan.board_depth),
        hex(BOARD_FILL),
        hex(BOARD_EDGE)
    ));
    if plan.steps.is_empty() {
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">no placements in the program</text>\n",
            number(layout.frame_x + 4.0),
            number(layout.frame_y + plan.board_depth / 2.0),
            hex(MUTED)
        ));
    }
}

fn push_svg_routes(svg: &mut String, plan: &Plan, layout: &Layout) {
    for side in [Side::Top, Side::Bottom] {
        let steps: Vec<&Step> = plan.steps.iter().filter(|step| step.side == side).collect();
        if steps.is_empty() {
            continue;
        }
        let (fill, edge) =
            (if side == Side::Top { TOP_FILL } else { BOTTOM_FILL }, if side == Side::Top { TOP_EDGE } else { BOTTOM_EDGE });
        let name = if side == Side::Top { "top" } else { "bottom" };
        let mut points = vec![format!("{},{}", number(layout.frame_x), number(layout.frame_y))];
        points.extend(
            steps.iter().map(|step| format!("{},{}", number(layout.frame_x + step.x), number(layout.frame_y + step.y))),
        );
        svg.push_str(&format!(
            "  <polyline data-route=\"{name}\" points=\"{}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"0.35\" stroke-linejoin=\"round\"/>\n",
            points.join(" "),
            hex(edge)
        ));
        for step in steps {
            let x = layout.frame_x + step.x;
            let y = layout.frame_y + step.y;
            svg.push_str(&format!(
                "  <circle data-step=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.25\"/>\n",
                step.index,
                number(x),
                number(y),
                number(STEP_RADIUS_MM),
                hex(fill),
                hex(edge)
            ));
            svg.push_str(&format!(
                "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"1.7\" fill=\"#{}\">{} {}</text>\n",
                number(x + 1.6),
                number(y - 1.4),
                hex(INK),
                step.index,
                drawing::escape_xml(&step.refdes)
            ));
        }
    }
    svg.push_str(&format!(
        "  <path d=\"M {} {} l 2.4 2.4 M {} {} l 2.4 -2.4\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        number(layout.frame_x - 1.2),
        number(layout.frame_y - 1.2),
        number(layout.frame_x - 1.2),
        number(layout.frame_y + 1.2),
        hex(INK)
    ));
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"1.8\" fill=\"#{}\">ORIGIN (0,0)</text>\n",
        number(layout.frame_x + 2.0),
        number(layout.frame_y + 3.6),
        hex(MUTED)
    ));
}

fn push_svg_side(svg: &mut String, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = layout.frame_y;
    svg.push_str(&format!("  <g id=\"legend\" transform=\"translate({} {})\">\n", number(x), number(y)));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"3\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.6\" font-weight=\"bold\" fill=\"#{}\">ROUTES</text>\n",
        hex(INK)
    ));
    let rows = [
        ("TOP ROUTE", format!("{} step(s)", plan.summary.top_steps), TOP_EDGE, TOP_FILL),
        ("BOTTOM ROUTE", format!("{} step(s)", plan.summary.bottom_steps), BOTTOM_EDGE, BOTTOM_FILL),
    ];
    for (index, (label, value, edge, fill)) in rows.iter().enumerate() {
        let row = 9.0 + index as f64 * 6.0;
        svg.push_str(&format!(
            "    <line x1=\"0\" y1=\"{}\" x2=\"8\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.5\"/>\n",
            number(row - 0.8),
            number(row - 0.8),
            hex(edge)
        ));
        svg.push_str(&format!(
            "    <circle cx=\"4\" cy=\"{}\" r=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.25\"/>\n",
            number(row - 0.8),
            number(STEP_RADIUS_MM),
            hex(fill),
            hex(edge)
        ));
        svg.push_str(&format!(
            "    <text x=\"11\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.1\" fill=\"#{}\">{label}</text>\n",
            number(row),
            hex(INK)
        ));
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.1\" fill=\"#{}\">{value}</text>\n",
            number(SIDE_WIDTH_MM),
            number(row),
            hex(MUTED)
        ));
    }
    svg.push_str(&format!(
        "    <line x1=\"0\" y1=\"23.4\" x2=\"{}\" y2=\"23.4\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        number(SIDE_WIDTH_MM),
        hex(RULE)
    ));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"27.6\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.1\" fill=\"#{}\">steps</text>\n",
        hex(MUTED)
    ));
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"27.6\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.1\" fill=\"#{}\">{}</text>\n",
        number(SIDE_WIDTH_MM),
        hex(INK),
        plan.summary.steps
    ));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"32.2\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.1\" fill=\"#{}\">travel</text>\n",
        hex(MUTED)
    ));
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"32.2\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.1\" fill=\"#{}\">{} mm</text>\n",
        number(SIDE_WIDTH_MM),
        hex(INK),
        number(plan.summary.travel_length)
    ));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"36.8\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.1\" fill=\"#{}\">est. time</text>\n",
        hex(MUTED)
    ));
    let seconds = place::estimated_seconds(plan.steps.len(), plan.summary.travel_length);
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"36.8\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.1\" fill=\"#{}\">{} s</text>\n",
        number(SIDE_WIDTH_MM),
        hex(INK),
        number(seconds)
    ));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"43.4\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"1.8\" fill=\"#{}\">numbers mark the program step</text>\n",
        hex(MUTED)
    ));
    svg.push_str("  </g>\n");
}

fn title_lines(plan: &Plan, scale_note: &str) -> Vec<String> {
    vec![
        format!("OpenSMT pick-and-place sequence drawing | {} | revision {} | units {}", plan.id, plan.revision, plan.units),
        format!(
            "{} step(s) | top {} | bottom {} | travel {} mm",
            plan.summary.steps,
            plan.summary.top_steps,
            plan.summary.bottom_steps,
            number(plan.summary.travel_length)
        ),
        "routes follow the default nearest-neighbour program; each side starts at the machine origin".to_string(),
        scale_note.to_string(),
    ]
}

fn push_svg_title(svg: &mut String, plan: &Plan, layout: &Layout, scale_note: &str) {
    svg.push_str(&format!(
        "  <g id=\"title-block\" transform=\"translate({} {})\">\n",
        number(DRAW_MARGIN_MM),
        number(layout.title_y)
    ));
    svg.push_str(&format!(
        "    <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#{}\" stroke-width=\"0.45\"/>\n",
        number(layout.width - 2.0 * DRAW_MARGIN_MM),
        number(TITLE_HEIGHT_MM),
        hex(drawing::PANEL_EDGE)
    ));
    for (index, line) in title_lines(plan, scale_note).iter().enumerate() {
        svg.push_str(&format!(
            "    <text x=\"2.5\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{}</text>\n",
            number(5.6 + index as f64 * 5.6),
            if index == 0 { "2.9" } else { "2.2" },
            if index == 0 { hex(INK) } else { hex(MUTED) },
            drawing::escape_xml(line)
        ));
    }
    svg.push_str("  </g>\n");
}

fn push_pdf_frame(pdf: &mut Pdf, plan: &Plan, layout: &Layout) {
    pdf.rect(layout.frame_x, layout.frame_y, plan.board_width, plan.board_depth, Some(BOARD_FILL), Some((BOARD_EDGE, 0.4)));
    if plan.steps.is_empty() {
        pdf.text(layout.frame_x + 4.0, layout.frame_y + plan.board_depth / 2.0, 2.4, MUTED, "no placements in the program");
    }
}

fn push_pdf_routes(pdf: &mut Pdf, plan: &Plan, layout: &Layout) {
    for side in [Side::Top, Side::Bottom] {
        let steps: Vec<&Step> = plan.steps.iter().filter(|step| step.side == side).collect();
        if steps.is_empty() {
            continue;
        }
        let (fill, edge) =
            (if side == Side::Top { TOP_FILL } else { BOTTOM_FILL }, if side == Side::Top { TOP_EDGE } else { BOTTOM_EDGE });
        let mut points = vec![(layout.frame_x, layout.frame_y)];
        points.extend(steps.iter().map(|step| (layout.frame_x + step.x, layout.frame_y + step.y)));
        pdf.route(&points, edge, 0.35);
        for step in steps {
            let x = layout.frame_x + step.x;
            let y = layout.frame_y + step.y;
            pdf.marker(x, y, STEP_RADIUS_MM, fill, edge);
            pdf.text(x + 1.6, y - 1.4, 1.7, INK, &format!("{} {}", step.index, step.refdes));
        }
    }
    pdf.text(layout.frame_x + 2.0, layout.frame_y + 3.6, 1.8, MUTED, "ORIGIN (0,0)");
}

fn push_pdf_side(pdf: &mut Pdf, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = layout.frame_y;
    pdf.text(x, y + 3.0, 2.6, INK, "ROUTES");
    let rows = [
        ("TOP ROUTE", format!("{} step(s)", plan.summary.top_steps), TOP_EDGE),
        ("BOTTOM ROUTE", format!("{} step(s)", plan.summary.bottom_steps), BOTTOM_EDGE),
    ];
    for (index, (label, value, edge)) in rows.iter().enumerate() {
        let row = 9.0 + index as f64 * 6.0;
        pdf.line(x, y + row - 1.6, x + 8.0, y + row - 1.6, edge, 0.5);
        pdf.marker(x + 4.0, y + row - 1.6, STEP_RADIUS_MM, if index == 0 { TOP_FILL } else { BOTTOM_FILL }, edge);
        pdf.text(x + 11.0, y + row, 2.1, INK, label);
        pdf.text(x + SIDE_WIDTH_MM - 12.0, y + row, 2.1, MUTED, value);
    }
    pdf.line(x, y + 23.4, x + SIDE_WIDTH_MM, y + 23.4, RULE, 0.3);
    pdf.text(x, y + 27.6, 2.1, MUTED, "steps");
    pdf.text(x + SIDE_WIDTH_MM - 8.0, y + 27.6, 2.1, INK, &plan.summary.steps.to_string());
    pdf.text(x, y + 32.2, 2.1, MUTED, "travel");
    pdf.text(x + SIDE_WIDTH_MM - 20.0, y + 32.2, 2.1, INK, &format!("{} mm", number(plan.summary.travel_length)));
    pdf.text(x, y + 36.8, 2.1, MUTED, "est. time");
    let seconds = place::estimated_seconds(plan.steps.len(), plan.summary.travel_length);
    pdf.text(x + SIDE_WIDTH_MM - 16.0, y + 36.8, 2.1, INK, &format!("{} s", number(seconds)));
    pdf.text(x, y + 43.4, 1.8, MUTED, "numbers mark the program step");
}

fn push_pdf_title(pdf: &mut Pdf, plan: &Plan, layout: &Layout, scale_note: &str) {
    let x = DRAW_MARGIN_MM;
    let y = layout.title_y;
    pdf.rect(x, y, layout.width - 2.0 * DRAW_MARGIN_MM, TITLE_HEIGHT_MM, Some("1 1 1"), Some((PANEL_EDGE, 0.45)));
    for (index, line) in title_lines(plan, scale_note).iter().enumerate() {
        pdf.text(
            x + 2.5,
            y + 5.6 + index as f64 * 5.6,
            if index == 0 { 2.9 } else { 2.2 },
            if index == 0 { INK } else { MUTED },
            line,
        );
    }
}
