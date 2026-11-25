//! Read-only stencil aperture generation.
//!
//! The stencil sheet turns the component list into paste apertures through the
//! shared package rules: every component receives one aperture per terminal,
//! sized from the family table and shrunk by the family's area reduction
//! (`aperture side = pad side × √reduction`). The report lists one rule per
//! package in first-seen order plus one entry per aperture with its absolute
//! centre on a single board. The SVG and PDF sheets reuse the placement
//! drawing infrastructure: a millimetre board outline with the apertures
//! coloured by side, a rule table, a legend and a title block. Everything is
//! deterministic and the document is never modified.

use serde::Serialize;

use crate::{
    SmtError,
    drawing::{self, DrawingLoss},
    export::{escape, number},
    model::{Side, SmtDocument},
    packages::{self, Family, Shape},
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-stencil";

/// Report schema version.
pub const VERSION: u32 = 1;

/// Stencil rule of one package.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StencilRule {
    /// Package identifier as written on the component.
    pub package: String,
    /// Recognised family: `chip`, `sot23`, `soic`, `qfp`, `qfn`, `lga` or `generic`.
    pub family: String,
    /// Aperture shape: `rect` or `circle`.
    pub shape: String,
    /// Nominal terminal width along the component long axis in millimetres.
    pub pad_width: f64,
    /// Nominal terminal height across the component short axis in millimetres.
    pub pad_height: f64,
    /// Aperture width after the area reduction in millimetres.
    pub aperture_width: f64,
    /// Aperture height after the area reduction in millimetres.
    pub aperture_height: f64,
    /// Paste area reduction factor; `0.9` means a 10 % area reduction.
    pub area_reduction: f64,
    /// Apertures generated per component of this package.
    pub apertures_per_component: u32,
}

/// One generated aperture on a single board.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StencilAperture {
    /// Reference designator of the owning component.
    pub refdes: String,
    /// Package identifier of the owning component.
    pub package: String,
    /// Board side the aperture belongs to.
    pub side: Side,
    /// One-based aperture number within the component.
    pub index: u32,
    /// Aperture shape: `rect` or `circle`.
    pub shape: String,
    /// Aperture centre X on a single board in millimetres.
    pub x: f64,
    /// Aperture centre Y on a single board in millimetres.
    pub y: f64,
    /// Aperture width in millimetres; diameter for a circle.
    pub width: f64,
    /// Aperture height in millimetres; diameter for a circle.
    pub height: f64,
    /// Rotation of the owning component in degrees; `0` for a circle.
    pub rotation: f64,
}

/// Stencil aperture report of a document.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StencilReport {
    /// Report format identifier.
    pub format: String,
    /// Report schema version.
    pub version: u32,
    /// Unit system taken from the document.
    pub units: String,
    /// Board width in millimetres.
    pub board_width: f64,
    /// Board depth in millimetres.
    pub board_depth: f64,
    /// Number of components on one board.
    pub components: u32,
    /// Number of generated apertures on one board.
    pub apertures: u32,
    /// Packages that fell back to the generic rule, in first-seen order.
    pub unmatched_packages: Vec<String>,
    /// One rule per distinct package, in first-seen order.
    pub rules: Vec<StencilRule>,
    /// One entry per aperture, in component order.
    pub entries: Vec<StencilAperture>,
}

/// Counts of a rendered stencil drawing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StencilDrawingSummary {
    /// Unit system taken from the document.
    pub units: String,
    /// Board width in millimetres.
    pub board_width: f64,
    /// Board depth in millimetres.
    pub board_depth: f64,
    /// Number of components on the board.
    pub components: u32,
    /// Number of apertures drawn.
    pub apertures: u32,
    /// Number of packages that used the generic rule.
    pub unmatched_packages: u32,
    /// Number of non-ASCII characters written as `?`.
    pub dropped_characters: u32,
}

impl StencilDrawingSummary {
    /// Returns the aggregated text losses of this drawing.
    pub fn losses(&self) -> Vec<DrawingLoss> {
        if self.dropped_characters == 0 {
            Vec::new()
        }
        else {
            vec![DrawingLoss { code: "stencil/non-ascii-text".to_string(), count: self.dropped_characters }]
        }
    }
}

/// One rendered stencil drawing and its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct StencilDrawing {
    /// Complete file contents.
    pub text: String,
    /// Counts and losses of the rendered drawing.
    pub summary: StencilDrawingSummary,
}

/// Returns the stencil aperture report of a document.
pub fn report(document: &SmtDocument) -> StencilReport {
    let mut rules: Vec<StencilRule> = Vec::new();
    let mut unmatched: Vec<String> = Vec::new();
    let mut entries: Vec<StencilAperture> = Vec::new();
    for component in &document.components {
        let geometry = packages::geometry(&component.package);
        let terminals = packages::terminals(&geometry);
        if !rules.iter().any(|rule| rule.package == component.package) {
            if geometry.family == Family::Generic {
                unmatched.push(component.package.clone());
            }
            rules.push(StencilRule {
                package: component.package.clone(),
                family: geometry.family.as_str().to_string(),
                shape: geometry.shape.as_str().to_string(),
                pad_width: geometry.aperture_width,
                pad_height: geometry.aperture_height,
                aperture_width: reduced(geometry.aperture_width, geometry.reduction),
                aperture_height: reduced(geometry.aperture_height, geometry.reduction),
                area_reduction: geometry.reduction,
                apertures_per_component: terminals.len() as u32,
            });
        }
        let radians = component.rotation.to_radians();
        let (cosine, sine) = (radians.cos(), radians.sin());
        for (index, terminal) in terminals.iter().enumerate() {
            entries.push(StencilAperture {
                refdes: component.refdes.clone(),
                package: component.package.clone(),
                side: component.side,
                index: (index + 1) as u32,
                shape: geometry.shape.as_str().to_string(),
                x: round3(component.x + terminal.dx * cosine - terminal.dy * sine),
                y: round3(component.y + terminal.dx * sine + terminal.dy * cosine),
                width: reduced(terminal.width, geometry.reduction),
                height: reduced(terminal.height, geometry.reduction),
                rotation: if geometry.shape == Shape::Circle { 0.0 } else { component.rotation },
            });
        }
    }
    StencilReport {
        format: FORMAT.to_string(),
        version: VERSION,
        units: document.units.clone(),
        board_width: document.board.width,
        board_depth: document.board.depth,
        components: document.components.len() as u32,
        apertures: entries.len() as u32,
        unmatched_packages: unmatched,
        rules,
        entries,
    }
}

/// Renders the stencil aperture entries as CSV, one row per aperture.
pub fn csv(report: &StencilReport) -> String {
    let mut text = String::from("refdes,package,side,index,shape,x,y,width,height,rotation\n");
    for entry in &report.entries {
        text.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            escape(&entry.refdes),
            escape(&entry.package),
            entry.side.as_str(),
            entry.index,
            entry.shape,
            number(entry.x),
            number(entry.y),
            number(entry.width),
            number(entry.height),
            number(entry.rotation)
        ));
    }
    text
}

/// Renders the stencil sheet as a millimetre SVG.
///
/// A non-finite or non-positive board reports `board/invalid`. The result is
/// deterministic.
pub fn render_svg(document: &SmtDocument, id: &str) -> Result<StencilDrawing, SmtError> {
    let plan = build_plan(document, id)?;
    let layout = StencilLayout::new(&plan);
    let mut body = String::new();
    push_svg_board(&mut body, &plan);
    push_svg_legend(&mut body, &layout);
    push_svg_rules(&mut body, &plan, &layout);
    let content =
        (drawing::DRAW_MARGIN_MM, drawing::DRAW_MARGIN_MM, layout.width - drawing::DRAW_MARGIN_MM, layout.content_bottom);
    let text = drawing::render_sheet(content, &body, &title_text(&plan, "A4 landscape"));
    Ok(StencilDrawing { text, summary: plan.summary })
}

/// Renders the stencil sheet as a one-page A4 landscape PDF.
pub fn render_pdf(document: &SmtDocument, id: &str) -> Result<StencilDrawing, SmtError> {
    let plan = build_plan(document, id)?;
    let layout = StencilLayout::new(&plan);
    let fit = ((drawing::PDF_PAGE_WIDTH - 2.0 * drawing::PDF_MARGIN) / (layout.width * drawing::PT_PER_MM))
        .min((drawing::PDF_PAGE_HEIGHT - 2.0 * drawing::PDF_MARGIN) / (layout.height * drawing::PT_PER_MM))
        .max(0.001);
    let scale = drawing::PT_PER_MM * fit;
    let mut pdf = drawing::Pdf {
        content: String::new(),
        height: layout.height,
        scale,
        origin_x: (drawing::PDF_PAGE_WIDTH - layout.width * scale) / 2.0,
        origin_y: (drawing::PDF_PAGE_HEIGHT - layout.height * scale) / 2.0,
    };
    push_pdf_board(&mut pdf, &plan);
    push_pdf_legend(&mut pdf, &layout);
    push_pdf_rules(&mut pdf, &plan, &layout);
    push_pdf_title(&mut pdf, &plan, &layout, &format!("A4 landscape fit; 1 mm = {} pt x fit {fit:.3}", drawing::PT_PER_MM));
    Ok(StencilDrawing { text: drawing::pdf_page(pdf.content), summary: plan.summary })
}

struct Aperture {
    side: Side,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    rotation: f64,
    circle: bool,
}

struct RuleLine {
    package: String,
    family: String,
    shape: String,
    size: String,
    reduction: String,
    count: u32,
}

struct Plan {
    id: String,
    units: String,
    revision: u32,
    board_width: f64,
    board_depth: f64,
    apertures: Vec<Aperture>,
    labels: Vec<drawing::PlacedLabel>,
    rules: Vec<RuleLine>,
    summary: StencilDrawingSummary,
}

struct StencilLayout {
    width: f64,
    height: f64,
    side_x: f64,
    rules_y: f64,
    content_bottom: f64,
    title_y: f64,
    title_h: f64,
}

impl StencilLayout {
    fn new(plan: &Plan) -> Self {
        let side_x = drawing::DRAW_MARGIN_MM + plan.board_width + drawing::COLUMN_GAP_MM;
        // The legend ends with its three notes; the rules table starts just
        // below them instead of leaving a band of empty sheet.
        let rules_y = drawing::DRAW_MARGIN_MM + drawing::LEGEND_HEIGHT_MM;
        let rules_height = 12.0 + plan.rules.len() as f64 * drawing::FEEDER_ROW_MM;
        let content_bottom = (drawing::DRAW_MARGIN_MM + plan.board_depth)
            .max(drawing::DRAW_MARGIN_MM + drawing::LEGEND_HEIGHT_MM)
            .max(rules_y + rules_height);
        let width = side_x + drawing::SIDE_WIDTH_MM + drawing::DRAW_MARGIN_MM;
        let laid = drawing::lay_out(&title_text(plan, "1 mm = 8 px"), width - 2.0 * drawing::DRAW_MARGIN_MM - 5.0);
        let title_h = drawing::block_height(&laid) + 6.0;
        let title_y = content_bottom + drawing::COLUMN_GAP_MM;
        Self { width, height: title_y + title_h + drawing::DRAW_MARGIN_MM, side_x, rules_y, content_bottom, title_y, title_h }
    }
}

fn build_plan(document: &SmtDocument, id: &str) -> Result<Plan, SmtError> {
    let positive = |value: f64| value.is_finite() && value > 0.0;
    if !positive(document.board.width) || !positive(document.board.depth) {
        return Err(SmtError::new("board/invalid", "board width and depth must be finite and greater than zero"));
    }
    let report = report(document);
    let (id, mut dropped) = drawing::ascii(id);
    let (units, count) = drawing::ascii(&document.units);
    dropped += count;

    // A bottom stencil is viewed through the board from below, so its sheet
    // view mirrors the single-board report about the board centre line:
    // `x' = board_width - x` and `rotation' = -rotation`. The report itself
    // keeps the absolute single-board coordinates.
    let board_width = document.board.width;
    let mirrored_x = |side: Side, value: f64| if side == Side::Bottom { round3(board_width - value) } else { value };
    let mirrored_rotation = |side: Side, value: f64| if side == Side::Bottom { -value } else { value };
    let apertures = report
        .entries
        .iter()
        .map(|entry| Aperture {
            side: entry.side,
            x: mirrored_x(entry.side, entry.x),
            y: entry.y,
            width: entry.width,
            height: entry.height,
            rotation: mirrored_rotation(entry.side, entry.rotation),
            circle: entry.shape == "circle",
        })
        .collect::<Vec<_>>();
    // Reference labels take the first free slot of eight candidates around the
    // whole aperture cluster of their component; a label that finds no free
    // slot is omitted instead of printed over a neighbour.
    let frame = (0.4, 0.4, document.board.width - 0.4, document.board.depth - 0.4);
    let mut occupied: Vec<drawing::Box4> = report
        .entries
        .iter()
        .map(|entry| drawing::rotated_box(mirrored_x(entry.side, entry.x), entry.y, entry.rotation, entry.width, entry.height))
        .collect();
    let mut labels = Vec::with_capacity(document.components.len());
    for component in &document.components {
        let (refdes, count) = drawing::ascii(&component.refdes);
        dropped += count;
        let geometry = packages::geometry(&component.package);
        let centre_x = mirrored_x(component.side, component.x);
        let rotation = mirrored_rotation(component.side, component.rotation);
        let mut body = drawing::rotated_box(centre_x, component.y, rotation, geometry.body_length, geometry.body_width);
        // Leads reach past the body outline, so the label clears the whole
        // aperture cluster of this component.
        for entry in report.entries.iter().filter(|entry| entry.refdes == component.refdes) {
            let box_ =
                drawing::rotated_box(mirrored_x(entry.side, entry.x), entry.y, entry.rotation, entry.width, entry.height);
            body = (body.0.min(box_.0), body.1.min(box_.1), body.2.max(box_.2), body.3.max(box_.3));
        }
        let font = drawing::label_font(geometry.body_length.min(geometry.body_width));
        if let Some(label) = drawing::place_label(&refdes, font, body, frame, &mut occupied) {
            labels.push(label);
        }
    }

    let mut rules = Vec::with_capacity(report.rules.len());
    for rule in &report.rules {
        let (package, count) = drawing::ascii(&rule.package);
        dropped += count;
        rules.push(RuleLine {
            package,
            family: rule.family.clone(),
            shape: rule.shape.clone(),
            size: format!("{} x {}", drawing::number(rule.aperture_width), drawing::number(rule.aperture_height)),
            reduction: drawing::number(rule.area_reduction),
            count: rule.apertures_per_component,
        });
    }

    let summary = StencilDrawingSummary {
        units: units.clone(),
        board_width: document.board.width,
        board_depth: document.board.depth,
        components: report.components,
        apertures: report.apertures,
        unmatched_packages: report.unmatched_packages.len() as u32,
        dropped_characters: dropped as u32,
    };
    Ok(Plan {
        id,
        units,
        revision: document.revision,
        board_width: document.board.width,
        board_depth: document.board.depth,
        apertures,
        labels,
        rules,
        summary,
    })
}

/// Rotated rectangle corners of one aperture, in sheet millimetres.
fn aperture_corners(aperture: &Aperture, origin_x: f64, origin_y: f64) -> [(f64, f64); 4] {
    let half_width = aperture.width / 2.0;
    let half_height = aperture.height / 2.0;
    let radians = aperture.rotation.to_radians();
    let (cosine, sine) = (radians.cos(), radians.sin());
    [(-half_width, -half_height), (half_width, -half_height), (half_width, half_height), (-half_width, half_height)]
        .map(|(px, py)| (origin_x + aperture.x + px * cosine - py * sine, origin_y + aperture.y + px * sine + py * cosine))
}

fn push_svg_board(svg: &mut String, plan: &Plan) {
    let x = drawing::DRAW_MARGIN_MM;
    let y = drawing::DRAW_MARGIN_MM;
    svg.push_str(&format!("  <g id=\"stencil\" transform=\"translate({} {})\">\n", drawing::number(x), drawing::number(y)));
    svg.push_str(&format!(
        "    <rect data-board=\"true\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.45\"/>\n",
        drawing::number(plan.board_width),
        drawing::number(plan.board_depth),
        drawing::hex(drawing::BOARD_FILL),
        drawing::hex(drawing::BOARD_EDGE)
    ));
    for side in [Side::Top, Side::Bottom] {
        let (fill, edge) = drawing::colors(side);
        for aperture in plan.apertures.iter().filter(|aperture| aperture.side == side) {
            if aperture.circle {
                svg.push_str(&format!(
                    "    <circle data-side=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.2\"/>\n",
                    side.as_str(),
                    drawing::number(aperture.x),
                    drawing::number(aperture.y),
                    drawing::number(aperture.width / 2.0),
                    drawing::hex(fill),
                    drawing::hex(edge)
                ));
            }
            else {
                let points: Vec<String> = aperture_corners(aperture, x, y)
                    .iter()
                    .map(|(px, py)| format!("{},{}", drawing::number(px - x), drawing::number(py - y)))
                    .collect();
                svg.push_str(&format!(
                    "    <polygon data-side=\"{}\" points=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.2\"/>\n",
                    side.as_str(),
                    points.join(" "),
                    drawing::hex(fill),
                    drawing::hex(edge)
                ));
            }
        }
    }
    for label in &plan.labels {
        drawing::push_labels_svg(svg, std::slice::from_ref(label), drawing::INK);
    }
    svg.push_str("  </g>\n");
}

fn push_svg_legend(svg: &mut String, layout: &StencilLayout) {
    let x = layout.side_x;
    let y = drawing::DRAW_MARGIN_MM;
    svg.push_str(&format!("  <g id=\"legend\" transform=\"translate({} {})\">\n", drawing::number(x), drawing::number(y)));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"4.6\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" font-weight=\"bold\" fill=\"#{}\">LEGEND</text>\n",
        drawing::number(drawing::FONT_SECTION),
        drawing::hex(drawing::INK)
    ));
    let rows = [
        (drawing::BOARD_FILL, drawing::BOARD_EDGE, "board outline, single stencil"),
        (drawing::TOP_FILL, drawing::TOP_EDGE, "top-side aperture"),
        (drawing::BOTTOM_FILL, drawing::BOTTOM_EDGE, "bottom-side aperture"),
    ];
    for (index, (fill, edge, text)) in rows.iter().enumerate() {
        let top = 9.0 + index as f64 * 9.0;
        svg.push_str(&format!(
            "    <rect x=\"0\" y=\"{}\" width=\"10\" height=\"5\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.35\"/>\n",
            drawing::number(top),
            drawing::hex(fill),
            drawing::hex(edge)
        ));
        svg.push_str(&svg_legend_text(15.0, top + 3.9, drawing::FONT_BODY, text));
    }
    let notes = [
        "aperture side = pad side x sqrt(reduction)",
        "unmatched packages use the generic rule",
        "QFN thermal pads are not modelled",
    ];
    let mut offset = 39.0;
    for note in notes {
        for line in drawing::wrap_text(note, drawing::SIDE_WIDTH_MM - 2.0, drawing::FONT_NOTE) {
            svg.push_str(&svg_legend_text(0.0, offset, drawing::FONT_NOTE, &line));
            offset += drawing::FONT_NOTE * drawing::LINE_ADVANCE;
        }
    }
    svg.push_str("  </g>\n");
}

fn push_svg_rules(svg: &mut String, plan: &Plan, layout: &StencilLayout) {
    let x = layout.side_x;
    let y = layout.rules_y;
    svg.push_str(&format!("  <g id=\"rules\" transform=\"translate({} {})\">\n", drawing::number(x), drawing::number(y)));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"4.6\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" font-weight=\"bold\" fill=\"#{}\">STENCIL RULES</text>\n",
        drawing::number(drawing::FONT_SECTION),
        drawing::hex(drawing::INK)
    ));
    if plan.rules.is_empty() {
        svg.push_str(&svg_legend_text(0.0, 11.0, drawing::FONT_BODY, "no components; no apertures generated"));
        svg.push_str("  </g>\n");
        return;
    }
    let rows = plan
        .rules
        .iter()
        .map(|rule| {
            vec![
                rule.package.clone(),
                rule.family.clone(),
                rule.shape.clone(),
                rule.size.clone(),
                rule.reduction.clone(),
                rule.count.to_string(),
            ]
        })
        .collect();
    drawing::Table::new(
        &[
            ("PACKAGE", drawing::Align::Left),
            ("FAMILY", drawing::Align::Left),
            ("SHAPE", drawing::Align::Left),
            ("SIZE MM", drawing::Align::Right),
            ("RED", drawing::Align::Right),
            ("PER", drawing::Align::Right),
        ],
        rows,
        drawing::FONT_TABLE,
        3.6,
    )
    .with_row(5.4)
    .fit_width(drawing::SIDE_WIDTH_MM)
    .push_svg(svg, 0.0, 7.0, "stencil-rules");
    svg.push_str("  </g>\n");
}

fn push_pdf_board(pdf: &mut drawing::Pdf, plan: &Plan) {
    let x = drawing::DRAW_MARGIN_MM;
    let y = drawing::DRAW_MARGIN_MM;
    pdf.rect(x, y, plan.board_width, plan.board_depth, Some(drawing::BOARD_FILL), Some((drawing::BOARD_EDGE, 0.45)));
    for side in [Side::Top, Side::Bottom] {
        let (fill, edge) = drawing::colors(side);
        for aperture in plan.apertures.iter().filter(|aperture| aperture.side == side) {
            if aperture.circle {
                let radius = aperture.width / 2.0;
                let points: Vec<(f64, f64)> = (0..12)
                    .map(|step| {
                        let angle = step as f64 * std::f64::consts::FRAC_PI_6;
                        (x + aperture.x + angle.cos() * radius, y + aperture.y + angle.sin() * radius)
                    })
                    .collect();
                pdf.polygon(&points, Some(fill), Some(edge), 0.2);
            }
            else {
                let points: Vec<(f64, f64)> = aperture_corners(aperture, x, y).into_iter().collect();
                pdf.polygon(&points, Some(fill), Some(edge), 0.2);
            }
        }
    }
    for label in &plan.labels {
        if let Some((start, end)) = label.leader {
            pdf.line(x + start.0, y + start.1, x + end.0, y + end.1, drawing::INK, 0.18);
        }
        pdf.text(x + label.x, y + label.y, label.size, drawing::INK, &label.text);
    }
}

fn push_pdf_legend(pdf: &mut drawing::Pdf, layout: &StencilLayout) {
    let x = layout.side_x;
    let y = drawing::DRAW_MARGIN_MM;
    pdf.text(x, y + 4.6, drawing::FONT_SECTION, drawing::INK, "LEGEND");
    let rows = [
        (drawing::BOARD_FILL, drawing::BOARD_EDGE, "board outline, single stencil"),
        (drawing::TOP_FILL, drawing::TOP_EDGE, "top-side aperture"),
        (drawing::BOTTOM_FILL, drawing::BOTTOM_EDGE, "bottom-side aperture"),
    ];
    for (index, (fill, edge, text)) in rows.iter().enumerate() {
        let top = y + 9.0 + index as f64 * 9.0;
        pdf.rect(x, top, 10.0, 5.0, Some(fill), Some((edge, 0.35)));
        pdf.text(x + 15.0, top + 3.9, drawing::FONT_BODY, drawing::INK, text);
    }
    let notes = [
        "aperture side = pad side x sqrt(reduction)",
        "unmatched packages use the generic rule",
        "QFN thermal pads are not modelled",
    ];
    let mut offset = y + 39.0;
    for note in notes {
        for line in drawing::wrap_text(note, drawing::SIDE_WIDTH_MM - 2.0, drawing::FONT_NOTE) {
            pdf.text(x, offset, drawing::FONT_NOTE, drawing::INK, &line);
            offset += drawing::FONT_NOTE * drawing::LINE_ADVANCE;
        }
    }
}

fn push_pdf_rules(pdf: &mut drawing::Pdf, plan: &Plan, layout: &StencilLayout) {
    let x = layout.side_x;
    let y = layout.rules_y;
    pdf.text(x, y + 4.6, drawing::FONT_SECTION, drawing::INK, "STENCIL RULES");
    if plan.rules.is_empty() {
        pdf.text(x, y + 11.0, drawing::FONT_BODY, drawing::INK, "no components; no apertures generated");
        return;
    }
    let rows = plan
        .rules
        .iter()
        .map(|rule| {
            vec![
                rule.package.clone(),
                rule.family.clone(),
                rule.shape.clone(),
                rule.size.clone(),
                rule.reduction.clone(),
                rule.count.to_string(),
            ]
        })
        .collect();
    drawing::Table::new(
        &[
            ("PACKAGE", drawing::Align::Left),
            ("FAMILY", drawing::Align::Left),
            ("SHAPE", drawing::Align::Left),
            ("SIZE MM", drawing::Align::Right),
            ("RED", drawing::Align::Right),
            ("PER", drawing::Align::Right),
        ],
        rows,
        drawing::FONT_TABLE,
        3.6,
    )
    .with_row(5.4)
    .fit_width(drawing::SIDE_WIDTH_MM)
    .push_pdf(pdf, x, y + 7.0);
}

fn push_pdf_title(pdf: &mut drawing::Pdf, plan: &Plan, layout: &StencilLayout, scale_note: &str) {
    let x = drawing::DRAW_MARGIN_MM;
    let y = layout.title_y;
    let width = layout.width - 2.0 * drawing::DRAW_MARGIN_MM;
    pdf.rect(x, y, width, layout.title_h, Some("1 1 1"), Some((drawing::PANEL_EDGE, 0.45)));
    let lines = title_text(plan, scale_note);
    drawing::push_block_pdf(pdf, x + 2.5, y + 3.0, &drawing::lay_out(&lines, width - 5.0));
}

fn title_text(plan: &Plan, scale_note: &str) -> Vec<drawing::TextLine> {
    let title = |text: String| drawing::TextLine { text, size: drawing::FONT_TITLE, emphasis: true };
    let body = |text: String| drawing::TextLine { text, size: drawing::FONT_BODY, emphasis: false };
    vec![
        title(format!("OpenSMT stencil drawing | drawing OSMT-ST-01 | revision {}", plan.revision)),
        body(drawing::title_stamp(
            "stencil",
            "OSMT-ST-01",
            &plan.id,
            &plan.units,
            "1:1",
            "top + bottom (bottom mirrored about X)",
        )),
        body(format!(
            "{} component(s) | {} aperture(s) | {} package rule(s) | {} unmatched package(s)",
            plan.summary.components,
            plan.summary.apertures,
            plan.rules.len(),
            plan.summary.unmatched_packages
        )),
        body("apertures from the package rules table; QFN thermal pads not modelled".to_string()),
        body(scale_note.to_string()),
    ]
}

fn svg_legend_text(x: f64, y: f64, size: f64, text: &str) -> String {
    format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{}</text>\n",
        drawing::number(x),
        drawing::number(y),
        drawing::number(size),
        drawing::hex(drawing::INK),
        drawing::escape_xml(text)
    )
}

/// Applies the area reduction to one pad side.
fn reduced(pad: f64, reduction: f64) -> f64 {
    round3(pad * reduction.sqrt())
}

/// Rounds to three decimals, the sheet precision.
fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
