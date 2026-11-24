//! Deterministic placement drawings.
//!
//! The model stores geometry, not artwork, so both drawings are built from the
//! stored board, panel, component and feeder fields only. A component rectangle
//! uses a fixed default footprint of [`DEFAULT_COMPONENT_WIDTH_MM`] by
//! [`DEFAULT_COMPONENT_HEIGHT_MM`] because [`crate::model::Component`] carries
//! no body size. The SVG viewBox counts millimetres and opens at
//! [`SVG_PX_PER_MM`] pixels per millimetre; the PDF is one A4 landscape page
//! fitted and centred from the same millimetre layout using Helvetica.
//!
//! Both renderers share one plan: the same coordinate rounding, the same
//! document-order traversal and the same ASCII-only labels. A non-ASCII
//! character is written as `?` and counted in [`DrawingSummary`], so two calls
//! on the same document are byte-identical and no timestamp is emitted.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    SmtError,
    model::{Side, SmtDocument},
    panel_map, place,
};

/// Pixels per millimetre used for the SVG canvas size.
pub const SVG_PX_PER_MM: f64 = 8.0;
/// PostScript points per millimetre used for PDF coordinates.
pub const PT_PER_MM: f64 = 2.834_646;
/// Width of the default component footprint in millimetres.
pub const DEFAULT_COMPONENT_WIDTH_MM: f64 = 2.0;
/// Height of the default component footprint in millimetres.
pub const DEFAULT_COMPONENT_HEIGHT_MM: f64 = 1.2;

const DRAW_MARGIN_MM: f64 = 8.0;
const COLUMN_GAP_MM: f64 = 6.0;
const SIDE_WIDTH_MM: f64 = 66.0;
const LEGEND_HEIGHT_MM: f64 = 44.0;
const TITLE_HEIGHT_MM: f64 = 32.0;
const FEEDER_ROW_MM: f64 = 4.6;
const FEEDER_TOP_MM: f64 = DRAW_MARGIN_MM + LEGEND_HEIGHT_MM + COLUMN_GAP_MM;

const PDF_PAGE_WIDTH: f64 = 841.89;
const PDF_PAGE_HEIGHT: f64 = 595.28;
const PDF_MARGIN: f64 = 24.0;

const INK: &str = "0.06 0.09 0.16";
const MUTED: &str = "0.32 0.38 0.47";
const RULE: &str = "0.78 0.82 0.87";
const BOARD_FILL: &str = "0.973 0.980 0.988";
const BOARD_EDGE: &str = "0.20 0.25 0.33";
const PANEL_EDGE: &str = "0.06 0.09 0.16";
const TOP_FILL: &str = "0.86 0.92 1.00";
const TOP_EDGE: &str = "0.11 0.31 0.85";
const BOTTOM_FILL: &str = "1.00 0.93 0.86";
const BOTTOM_EDGE: &str = "0.76 0.25 0.05";

/// Count of a text loss in a drawing export.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingLoss {
    /// Stable code describing the loss; `drawing/non-ascii-text`.
    pub code: String,
    /// Number of characters replaced by `?`.
    pub count: u32,
}

/// Summary of one rendered drawing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingSummary {
    /// Unit system taken from the document.
    pub units: String,
    /// Panel width including column gaps, in millimetres.
    pub panel_width: f64,
    /// Panel depth including row gaps, in millimetres.
    pub panel_depth: f64,
    /// Number of board instances drawn.
    pub boards: u32,
    /// Number of components per board.
    pub components: u32,
    /// Number of component rectangles drawn over every board.
    pub placements: u32,
    /// Number of component rectangles that used the default footprint.
    pub default_size_components: u32,
    /// Number of declared feeder lanes listed beside the drawing.
    pub feeder_lanes: u32,
    /// Number of non-ASCII characters written as `?`.
    pub dropped_characters: u32,
}

impl DrawingSummary {
    /// Returns the aggregated text losses of this drawing.
    pub fn losses(&self) -> Vec<DrawingLoss> {
        if self.dropped_characters == 0 {
            Vec::new()
        }
        else {
            vec![DrawingLoss { code: "drawing/non-ascii-text".to_string(), count: self.dropped_characters }]
        }
    }
}

/// One rendered drawing and its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawing {
    /// Complete file contents.
    pub text: String,
    /// Counts and losses of the rendered drawing.
    pub summary: DrawingSummary,
}

/// Renders the document as a millimetre SVG sheet.
///
/// The document is read through [`panel_map::map`], so an invalid board or panel
/// reports `board/invalid` or `panel/invalid`. A valid document without
/// components or feeder lanes still produces a framed sheet with its title
/// block. The result is deterministic.
pub fn render_svg(document: &SmtDocument, id: &str) -> Result<Drawing, SmtError> {
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
    push_svg_panel(&mut svg, &plan, &layout);
    push_svg_legend(&mut svg, &plan, &layout);
    push_svg_feeders(&mut svg, &plan, &layout);
    push_svg_title(&mut svg, &plan, &layout, &format!("scale 1:1 in millimetres; 1 mm = {} px", SVG_PX_PER_MM));
    svg.push_str("</svg>\n");
    Ok(Drawing { text: svg, summary: plan.summary })
}

/// Renders the document as a one-page A4 landscape PDF.
///
/// The same millimetre layout as [`render_svg`] is fitted uniformly into the
/// page and centred, using [`PT_PER_MM`] as the source millimetre-to-point
/// constant. The page uses Helvetica and never carries a timestamp. An invalid
/// board or panel reports the same codes as [`render_svg`]; an empty document
/// still produces a complete page with its title block.
pub fn render_pdf(document: &SmtDocument, id: &str) -> Result<Drawing, SmtError> {
    let plan = build_plan(document, id)?;
    let layout = Layout::new(&plan);
    let fit = ((PDF_PAGE_WIDTH - 2.0 * PDF_MARGIN) / (layout.width * PT_PER_MM))
        .min((PDF_PAGE_HEIGHT - 2.0 * PDF_MARGIN) / (layout.height * PT_PER_MM))
        .max(0.001);
    let scale = PT_PER_MM * fit;
    let origin_x = (PDF_PAGE_WIDTH - layout.width * scale) / 2.0;
    let origin_y = (PDF_PAGE_HEIGHT - layout.height * scale) / 2.0;
    let mut pdf = Pdf { content: String::new(), height: layout.height, scale, origin_x, origin_y };
    push_pdf_panel(&mut pdf, &plan, &layout);
    push_pdf_legend(&mut pdf, &plan, &layout);
    push_pdf_feeders(&mut pdf, &plan, &layout);
    let scale_note = format!("A4 landscape fit; 1 mm = {PT_PER_MM} pt x fit {fit:.3}");
    push_pdf_title(&mut pdf, &plan, &layout, &scale_note);

    let content = pdf.content;
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PDF_PAGE_WIDTH:.2} {PDF_PAGE_HEIGHT:.2}] \
             /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content),
    ];
    let mut text = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(text.len());
        text.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = text.len();
    text.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    text.push_str("0000000000 65535 f \n");
    for offset in offsets {
        text.push_str(&format!("{offset:010} 00000 n \n"));
    }
    text.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1));
    Ok(Drawing { text, summary: plan.summary })
}

struct Part {
    refdes: String,
    package: String,
    side: Side,
    rotation: f64,
    feeder: Option<String>,
    instances: Vec<(u32, f64, f64)>,
}

struct FeederLine {
    id: String,
    package: String,
    pitch: f64,
    loaded: u32,
    capacity: u32,
}

struct Plan {
    id: String,
    units: String,
    revision: u32,
    board_width: f64,
    board_depth: f64,
    panel_width: f64,
    panel_depth: f64,
    boards: Vec<panel_map::PanelInstance>,
    parts: Vec<Part>,
    feeders: Vec<FeederLine>,
    summary: DrawingSummary,
}

struct Layout {
    width: f64,
    height: f64,
    panel_x: f64,
    panel_y: f64,
    side_x: f64,
    feeder_y: f64,
    title_y: f64,
}

impl Layout {
    fn new(plan: &Plan) -> Self {
        let side_x = DRAW_MARGIN_MM + plan.panel_width + COLUMN_GAP_MM;
        let feeder_y = FEEDER_TOP_MM;
        let feeder_height = 12.0 + plan.feeders.len() as f64 * FEEDER_ROW_MM;
        let content_bottom =
            (DRAW_MARGIN_MM + plan.panel_depth).max(DRAW_MARGIN_MM + LEGEND_HEIGHT_MM).max(feeder_y + feeder_height);
        let title_y = content_bottom + COLUMN_GAP_MM;
        Self {
            width: side_x + SIDE_WIDTH_MM + DRAW_MARGIN_MM,
            height: title_y + TITLE_HEIGHT_MM + DRAW_MARGIN_MM,
            panel_x: DRAW_MARGIN_MM,
            panel_y: DRAW_MARGIN_MM,
            side_x,
            feeder_y,
            title_y,
        }
    }
}

fn build_plan(document: &SmtDocument, id: &str) -> Result<Plan, SmtError> {
    let map = panel_map::map(document)?;
    let (id, mut dropped) = ascii(id);
    let (units, count) = ascii(&document.units);
    dropped += count;

    let allocation = place::allocate(document).ok();
    let mut lane_of: BTreeMap<String, String> = BTreeMap::new();
    if let Some(report) = &allocation {
        for lane in &report.feeders {
            for refdes in &lane.refdes {
                lane_of.insert(refdes.clone(), lane.id.clone());
            }
        }
    }

    let mut parts = Vec::with_capacity(map.components.len());
    for mapped in &map.components {
        let feeder = lane_of.get(&mapped.refdes).cloned().or_else(|| {
            document.feeders.iter().find(|feeder| feeder.package == mapped.package).map(|feeder| feeder.id.clone())
        });
        let (refdes, count) = ascii(&mapped.refdes);
        dropped += count;
        let (package, count) = ascii(&mapped.package);
        dropped += count;
        let feeder = match feeder {
            Some(value) => {
                let (text, count) = ascii(&value);
                dropped += count;
                Some(text)
            }
            None => None,
        };
        parts.push(Part {
            refdes,
            package,
            side: mapped.side,
            rotation: mapped.rotation,
            feeder,
            instances: mapped.instances.iter().map(|placement| (placement.board, placement.x, placement.y)).collect(),
        });
    }

    let mut feeders = Vec::with_capacity(document.feeders.len());
    for feeder in &document.feeders {
        let (lane, count) = ascii(&feeder.id);
        dropped += count;
        let (package, count) = ascii(&feeder.package);
        dropped += count;
        feeders.push(FeederLine { id: lane, package, pitch: feeder.pitch, loaded: feeder.loaded, capacity: feeder.capacity });
    }

    let placements = parts.iter().map(|part| part.instances.len()).sum::<usize>();
    let summary = DrawingSummary {
        units: units.clone(),
        panel_width: map.panel_width,
        panel_depth: map.panel_depth,
        boards: map.board_count,
        components: parts.len() as u32,
        placements: placements as u32,
        default_size_components: parts.len() as u32,
        feeder_lanes: feeders.len() as u32,
        dropped_characters: dropped as u32,
    };
    Ok(Plan {
        id,
        units,
        revision: document.revision,
        board_width: map.width,
        board_depth: map.depth,
        panel_width: map.panel_width,
        panel_depth: map.panel_depth,
        boards: map.instances,
        parts,
        feeders,
        summary,
    })
}

fn push_svg_panel(svg: &mut String, plan: &Plan, layout: &Layout) {
    svg.push_str(&format!(
        "  <g id=\"panel\" transform=\"translate({} {})\">\n",
        number(layout.panel_x),
        number(layout.panel_y)
    ));
    svg.push_str(&format!(
        "    <rect data-panel=\"true\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#{}\" stroke-width=\"0.45\"/>\n",
        number(plan.panel_width),
        number(plan.panel_depth),
        hex(PANEL_EDGE)
    ));
    for instance in &plan.boards {
        svg.push_str(&format!(
            "    <g data-board=\"{}\">\n      <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
            instance.board,
            number(instance.offset_x),
            number(instance.offset_y),
            number(plan.board_width),
            number(plan.board_depth),
            hex(BOARD_FILL),
            hex(BOARD_EDGE)
        ));
        svg.push_str(&format!(
            "      <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.6\" fill=\"#{}\">B{}</text>\n",
            number(instance.offset_x + 1.2),
            number(instance.offset_y + 3.4),
            hex(MUTED),
            instance.board
        ));
        svg.push_str("    </g>\n");
    }
    for side in [Side::Top, Side::Bottom] {
        svg.push_str(&format!("    <g data-side=\"{}\">\n", side.as_str()));
        for part in plan.parts.iter().filter(|part| part.side == side) {
            let (fill, edge) = colors(side);
            for (_, x, y) in &part.instances {
                let corners = corners(*x, *y, part.rotation);
                let points =
                    corners.iter().map(|(px, py)| format!("{},{}", number(*px), number(*py))).collect::<Vec<_>>().join(" ");
                svg.push_str(&format!(
                    "      <polygon data-refdes=\"{}\" data-rotation=\"{}\" points=\"{points}\" fill=\"#{fill}\" stroke=\"#{edge}\" stroke-width=\"0.25\"/>\n",
                    escape_xml(&part.refdes),
                    number(part.rotation)
                ));
            }
        }
        svg.push_str("    </g>\n");
    }
    svg.push_str("    <g id=\"labels\">\n");
    for part in &plan.parts {
        if let Some((_, x, y)) = part.instances.iter().find(|(board, _, _)| *board == 1) {
            let label = part.feeder.as_deref().unwrap_or("-");
            svg.push_str(&format!(
                "      <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" text-anchor=\"middle\" fill=\"#{}\">{}</text>\n",
                number(*x),
                number(*y - DEFAULT_COMPONENT_HEIGHT_MM / 2.0 - 0.8),
                hex(INK),
                escape_xml(&part.refdes)
            ));
            svg.push_str(&format!(
                "      <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" text-anchor=\"middle\" fill=\"#{}\">{} {}</text>\n",
                number(*x),
                number(*y + DEFAULT_COMPONENT_HEIGHT_MM / 2.0 + 2.1),
                hex(MUTED),
                escape_xml(&part.package),
                escape_xml(label)
            ));
        }
    }
    svg.push_str("    </g>\n  </g>\n");
}

fn push_svg_legend(svg: &mut String, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = DRAW_MARGIN_MM;
    svg.push_str(&format!("  <g id=\"legend\" transform=\"translate({} {})\">\n", number(x), number(y)));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"4\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.9\" font-weight=\"bold\" fill=\"#{}\">LEGEND</text>\n",
        hex(INK)
    ));
    svg.push_str(&format!(
        "    <rect x=\"0\" y=\"7\" width=\"7\" height=\"3.4\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        hex(BOARD_FILL),
        hex(BOARD_EDGE)
    ));
    svg.push_str(&legend_text(13.0, 9.7, 2.2, "board outline, panel gap"));
    svg.push_str(&format!(
        "    <rect x=\"0\" y=\"13\" width=\"7\" height=\"3.4\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        hex(TOP_FILL),
        hex(TOP_EDGE)
    ));
    svg.push_str(&legend_text(13.0, 15.7, 2.2, "top-side component"));
    svg.push_str(&format!(
        "    <rect x=\"0\" y=\"19\" width=\"7\" height=\"3.4\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        hex(BOTTOM_FILL),
        hex(BOTTOM_EDGE)
    ));
    svg.push_str(&legend_text(13.0, 21.7, 2.2, "bottom-side component"));
    svg.push_str(&legend_text(0.0, 29.0, 2.0, "refdes above, package + lane below"));
    svg.push_str(&legend_text(0.0, 34.0, 2.0, "default footprint 2.0 x 1.2 mm"));
    svg.push_str(&legend_text(0.0, 39.0, 2.0, &labels_note(plan)));
    svg.push_str("  </g>\n");
}

fn push_svg_feeders(svg: &mut String, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = layout.feeder_y;
    svg.push_str(&format!("  <g id=\"feeders\" transform=\"translate({} {})\">\n", number(x), number(y)));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"4\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.8\" font-weight=\"bold\" fill=\"#{}\">FEEDER LANES</text>\n",
        hex(INK)
    ));
    svg.push_str(&format!(
        "    <line x1=\"0\" y1=\"6.2\" x2=\"{}\" y2=\"6.2\" stroke=\"#{}\" stroke-width=\"0.2\"/>\n",
        number(SIDE_WIDTH_MM - 2.0),
        hex(RULE)
    ));
    if plan.feeders.is_empty() {
        svg.push_str(&legend_text(0.0, 11.0, 2.1, "no feeder lanes declared"));
        svg.push_str("  </g>\n");
        return;
    }
    for (column, label) in ["LANE", "PACKAGE", "PITCH", "LOAD"].iter().enumerate() {
        let offset = [0.0, 14.0, 42.0, 53.0][column];
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"10\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"1.8\" fill=\"#{}\">{label}</text>\n",
            number(offset),
            hex(MUTED)
        ));
    }
    for (index, feeder) in plan.feeders.iter().enumerate() {
        let line_y = 15.0 + index as f64 * FEEDER_ROW_MM;
        svg.push_str(&format!(
            "    <text x=\"0\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" fill=\"#{}\">{}</text>\n",
            number(line_y),
            hex(INK),
            escape_xml(&feeder.id)
        ));
        svg.push_str(&format!(
            "    <text x=\"14\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" fill=\"#{}\">{}</text>\n",
            number(line_y),
            hex(INK),
            escape_xml(&feeder.package)
        ));
        svg.push_str(&format!(
            "    <text x=\"42\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" fill=\"#{}\">{}</text>\n",
            number(line_y),
            hex(INK),
            number(feeder.pitch)
        ));
        svg.push_str(&format!(
            "    <text x=\"53\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" fill=\"#{}\">{}/{}</text>\n",
            number(line_y),
            hex(INK),
            feeder.loaded,
            feeder.capacity
        ));
    }
    svg.push_str("  </g>\n");
}

fn push_svg_title(svg: &mut String, plan: &Plan, layout: &Layout, scale_note: &str) {
    let x = DRAW_MARGIN_MM;
    let y = layout.title_y;
    let width = layout.width - 2.0 * DRAW_MARGIN_MM;
    svg.push_str(&format!("  <g id=\"title-block\" transform=\"translate({} {})\">\n", number(x), number(y)));
    svg.push_str(&format!(
        "    <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#{}\" stroke-width=\"0.45\"/>\n",
        number(width),
        number(TITLE_HEIGHT_MM),
        hex(PANEL_EDGE)
    ));
    for line in title_lines(plan, scale_note) {
        svg.push_str(&format!(
            "    <text x=\"2.5\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{}</text>\n",
            number(line.y),
            number(line.size),
            if line.emphasis { hex(INK) } else { hex(MUTED) },
            escape_xml(&line.text)
        ));
    }
    svg.push_str("  </g>\n");
}

fn push_pdf_panel(pdf: &mut Pdf, plan: &Plan, layout: &Layout) {
    let x = layout.panel_x;
    let y = layout.panel_y;
    pdf.rect(x, y, plan.panel_width, plan.panel_depth, None, Some((PANEL_EDGE, 0.45)));
    for instance in &plan.boards {
        pdf.rect(
            x + instance.offset_x,
            y + instance.offset_y,
            plan.board_width,
            plan.board_depth,
            Some(BOARD_FILL),
            Some((BOARD_EDGE, 0.3)),
        );
        pdf.text(x + instance.offset_x + 1.2, y + instance.offset_y + 3.4, 2.6, MUTED, &format!("B{}", instance.board));
    }
    for side in [Side::Top, Side::Bottom] {
        let (fill, edge) = colors(side);
        for part in plan.parts.iter().filter(|part| part.side == side) {
            for (_, cx, cy) in &part.instances {
                let corners = corners(*cx, *cy, part.rotation);
                let points: Vec<(f64, f64)> = corners.iter().map(|(px, py)| (x + px, y + py)).collect();
                pdf.polygon(&points, Some(fill), Some(edge), 0.25);
            }
        }
    }
    for part in &plan.parts {
        let Some((_, cx, cy)) = part.instances.iter().find(|(board, _, _)| *board == 1)
        else {
            continue;
        };
        let label = part.feeder.as_deref().unwrap_or("-");
        pdf.text_centered(x + cx, y + cy - DEFAULT_COMPONENT_HEIGHT_MM / 2.0 - 0.8, 2.4, INK, &part.refdes);
        pdf.text_centered(
            x + cx,
            y + cy + DEFAULT_COMPONENT_HEIGHT_MM / 2.0 + 2.1,
            2.0,
            MUTED,
            &format!("{} {label}", part.package),
        );
    }
}

fn push_pdf_legend(pdf: &mut Pdf, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = DRAW_MARGIN_MM;
    pdf.text(x, y + 4.0, 2.9, INK, "LEGEND");
    pdf.rect(x, y + 7.0, 7.0, 3.4, Some(BOARD_FILL), Some((BOARD_EDGE, 0.3)));
    pdf.text(x + 13.0, y + 9.7, 2.2, INK, "board outline, panel gap");
    pdf.rect(x, y + 13.0, 7.0, 3.4, Some(TOP_FILL), Some((TOP_EDGE, 0.3)));
    pdf.text(x + 13.0, y + 15.7, 2.2, INK, "top-side component");
    pdf.rect(x, y + 19.0, 7.0, 3.4, Some(BOTTOM_FILL), Some((BOTTOM_EDGE, 0.3)));
    pdf.text(x + 13.0, y + 21.7, 2.2, INK, "bottom-side component");
    pdf.text(x, y + 29.0, 2.0, INK, "refdes above, package + lane below");
    pdf.text(x, y + 34.0, 2.0, INK, "default footprint 2.0 x 1.2 mm");
    pdf.text(x, y + 39.0, 2.0, INK, &labels_note(plan));
}

fn push_pdf_feeders(pdf: &mut Pdf, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = layout.feeder_y;
    pdf.text(x, y + 4.0, 2.8, INK, "FEEDER LANES");
    pdf.line(x, y + 6.2, x + SIDE_WIDTH_MM - 2.0, y + 6.2, RULE, 0.2);
    if plan.feeders.is_empty() {
        pdf.text(x, y + 11.0, 2.1, INK, "no feeder lanes declared");
        return;
    }
    for (column, label) in ["LANE", "PACKAGE", "PITCH", "LOAD"].iter().enumerate() {
        let offset = [0.0, 14.0, 42.0, 53.0][column];
        pdf.text(x + offset, y + 10.0, 1.8, MUTED, label);
    }
    for (index, feeder) in plan.feeders.iter().enumerate() {
        let line_y = y + 15.0 + index as f64 * FEEDER_ROW_MM;
        pdf.text(x, line_y, 2.0, INK, &feeder.id);
        pdf.text(x + 14.0, line_y, 2.0, INK, &feeder.package);
        pdf.text(x + 42.0, line_y, 2.0, INK, &number(feeder.pitch));
        pdf.text(x + 53.0, line_y, 2.0, INK, &format!("{}/{}", feeder.loaded, feeder.capacity));
    }
}

fn push_pdf_title(pdf: &mut Pdf, plan: &Plan, layout: &Layout, scale_note: &str) {
    let x = layout.panel_x;
    let y = layout.title_y;
    let width = layout.width - 2.0 * DRAW_MARGIN_MM;
    pdf.rect(x, y, width, TITLE_HEIGHT_MM, Some("1 1 1"), Some((PANEL_EDGE, 0.45)));
    for line in title_lines(plan, scale_note) {
        pdf.text(x + 2.5, y + line.y, line.size, if line.emphasis { INK } else { MUTED }, &line.text);
    }
}

struct TitleLine {
    y: f64,
    size: f64,
    emphasis: bool,
    text: String,
}

fn title_lines(plan: &Plan, scale_note: &str) -> Vec<TitleLine> {
    vec![
        TitleLine {
            y: 5.6,
            size: 2.9,
            emphasis: true,
            text: format!("OpenSMT placement drawing | {} | revision {} | units {}", plan.id, plan.revision, plan.units),
        },
        TitleLine {
            y: 11.2,
            size: 2.2,
            emphasis: false,
            text: format!(
                "{} board(s) | {} component(s) per board | {} placement(s) | {} feeder lane(s)",
                plan.summary.boards, plan.summary.components, plan.summary.placements, plan.summary.feeder_lanes
            ),
        },
        TitleLine {
            y: 16.8,
            size: 2.2,
            emphasis: false,
            text: format!(
                "default component footprint {} x {} mm (the model stores no body size); rotation clockwise",
                DEFAULT_COMPONENT_WIDTH_MM, DEFAULT_COMPONENT_HEIGHT_MM
            ),
        },
        TitleLine {
            y: 22.4,
            size: 2.2,
            emphasis: false,
            text: format!("{}; feeder tag = first matching lane in document order", labels_note(plan)),
        },
        TitleLine { y: 28.0, size: 2.2, emphasis: false, text: scale_note.to_string() },
    ]
}

fn labels_note(plan: &Plan) -> String {
    if plan.summary.boards > 1 { "labels on board 1 only".to_string() } else { "labels on the single board".to_string() }
}

fn colors(side: Side) -> (&'static str, &'static str) {
    match side {
        Side::Top => (TOP_FILL, TOP_EDGE),
        Side::Bottom => (BOTTOM_FILL, BOTTOM_EDGE),
    }
}

fn corners(x: f64, y: f64, rotation: f64) -> [(f64, f64); 4] {
    let half_width = DEFAULT_COMPONENT_WIDTH_MM / 2.0;
    let half_height = DEFAULT_COMPONENT_HEIGHT_MM / 2.0;
    let radians = rotation.to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    [(-half_width, -half_height), (half_width, -half_height), (half_width, half_height), (-half_width, half_height)]
        .map(|(px, py)| (x + px * cosine - py * sine, y + px * sine + py * cosine))
}

fn legend_text(x: f64, y: f64, size: f64, text: &str) -> String {
    format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{}</text>\n",
        number(x),
        number(y),
        number(size),
        hex(INK),
        escape_xml(text)
    )
}

fn number(value: f64) -> String {
    let value = if value.abs() < 0.0005 { 0.0 } else { value };
    format!("{value:.3}")
}

fn ascii(text: &str) -> (String, usize) {
    let mut escaped = String::with_capacity(text.len());
    let mut dropped = 0;
    for character in text.chars() {
        if character.is_ascii() && !character.is_ascii_control() {
            escaped.push(character);
        }
        else if matches!(character, '\n' | '\r' | '\t') {
            escaped.push(' ');
        }
        else {
            escaped.push('?');
            dropped += 1;
        }
    }
    (escaped, dropped)
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

fn hex(rgb: &str) -> String {
    let values: Vec<f64> = rgb.split_whitespace().filter_map(|value| value.parse::<f64>().ok()).collect();
    if values.len() != 3 {
        return "000000".to_string();
    }
    let component = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    format!("{:02x}{:02x}{:02x}", component(values[0]), component(values[1]), component(values[2]))
}

struct Pdf {
    content: String,
    height: f64,
    scale: f64,
    origin_x: f64,
    origin_y: f64,
}

impl Pdf {
    fn point(&self, x: f64, y: f64) -> (f64, f64) {
        (self.origin_x + x * self.scale, self.origin_y + (self.height - y) * self.scale)
    }

    fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, fill: Option<&str>, stroke: Option<(&str, f64)>) {
        let (left, top) = self.point(x, y);
        let bottom = top - height * self.scale;
        if let Some(color) = fill {
            self.content.push_str(&format!("{color} rg "));
        }
        if let Some((color, line_width)) = stroke {
            self.content.push_str(&format!("{color} RG {:.2} w ", (line_width * self.scale).max(0.2)));
        }
        let operator = if fill.is_some() && stroke.is_some() {
            "B"
        }
        else if fill.is_some() {
            "f"
        }
        else {
            "S"
        };
        self.content.push_str(&format!(
            "{:.2} {:.2} {:.2} {:.2} re {operator}\n",
            left,
            bottom,
            width * self.scale,
            height * self.scale
        ));
    }

    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, stroke: &str, line_width: f64) {
        let (start_x, start_y) = self.point(x1, y1);
        let (end_x, end_y) = self.point(x2, y2);
        self.content.push_str(&format!(
            "{stroke} RG {:.2} w {:.2} {:.2} m {:.2} {:.2} l S\n",
            (line_width * self.scale).max(0.2),
            start_x,
            start_y,
            end_x,
            end_y
        ));
    }

    fn polygon(&mut self, points: &[(f64, f64)], fill: Option<&str>, stroke: Option<&str>, line_width: f64) {
        if let Some(color) = fill {
            self.content.push_str(&format!("{color} rg "));
        }
        if let Some(color) = stroke {
            self.content.push_str(&format!("{color} RG {:.2} w ", (line_width * self.scale).max(0.2)));
        }
        for (index, (x, y)) in points.iter().enumerate() {
            let (px, py) = self.point(*x, *y);
            if index == 0 {
                self.content.push_str(&format!("{px:.2} {py:.2} m "));
            }
            else {
                self.content.push_str(&format!("{px:.2} {py:.2} l "));
            }
        }
        let operator = if fill.is_some() && stroke.is_some() {
            "h B"
        }
        else if fill.is_some() {
            "h f"
        }
        else {
            "h S"
        };
        self.content.push_str(&format!("{operator}\n"));
    }

    fn text(&mut self, x: f64, y: f64, size: f64, color: &str, text: &str) {
        let (px, py) = self.point(x, y);
        let size = (size * self.scale).clamp(3.0, 12.0);
        let (literal, _) = pdf_text(text);
        self.content.push_str(&format!("{color} rg BT /F1 {size:.2} Tf {px:.2} {py:.2} Td {literal} Tj ET\n"));
    }

    fn text_centered(&mut self, x: f64, y: f64, size: f64, color: &str, text: &str) {
        let size_pt = (size * self.scale).clamp(3.0, 12.0);
        let width_mm = text.chars().count() as f64 * size_pt * 0.5 / self.scale;
        self.text(x - width_mm / 2.0, y, size, color, text);
    }
}

fn pdf_text(text: &str) -> (String, usize) {
    let mut escaped = String::from("(");
    let mut dropped = 0;
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '(' => escaped.push_str("\\("),
            ')' => escaped.push_str("\\)"),
            '\n' | '\r' | '\t' => escaped.push(' '),
            character if character.is_ascii() && !character.is_ascii_control() => escaped.push(character),
            _ => {
                escaped.push('?');
                dropped += 1;
            }
        }
    }
    escaped.push(')');
    (escaped, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_counts_only_non_ascii_characters() {
        let (text, dropped) = ascii("R1 电阻\tX");
        assert_eq!(text, "R1 ?? X");
        assert_eq!(dropped, 2);
    }

    #[test]
    fn number_never_emits_negative_zero() {
        assert_eq!(number(-0.0), "0.000");
        assert_eq!(number(-0.0001), "0.000");
        assert_eq!(number(1.23456), "1.235");
    }

    #[test]
    fn categories_round_to_stable_hex_colors() {
        assert_eq!(hex(TOP_EDGE), "1c4fd9");
        assert_eq!(hex(PANEL_EDGE), "0f1729");
    }
}
