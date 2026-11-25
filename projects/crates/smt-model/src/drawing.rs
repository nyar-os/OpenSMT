//! Deterministic placement drawings.
//!
//! The model stores geometry, not artwork, so both drawings are built from the
//! stored board, panel, component and feeder fields only. A component body
//! takes its length and width from the shared package rules table
//! ([`crate::packages::geometry`]); an unrecognised package falls back to the
//! default footprint of [`DEFAULT_COMPONENT_WIDTH_MM`] by
//! [`DEFAULT_COMPONENT_HEIGHT_MM`]. The SVG viewBox counts millimetres and opens at
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
    packages, panel_map, place,
};

/// Pixels per millimetre used for the SVG canvas size.
pub const SVG_PX_PER_MM: f64 = 8.0;
/// PostScript points per millimetre used for PDF coordinates.
pub const PT_PER_MM: f64 = 2.834_646;
/// Width of the fixed A4 landscape page shared by every drawing, in
/// millimetres.
pub const SHEET_WIDTH_MM: f64 = 297.0;
/// Height of the fixed A4 landscape page shared by every drawing, in
/// millimetres.
pub const SHEET_HEIGHT_MM: f64 = 210.0;
/// Pixel width of the SVG root. The viewBox stays in millimetres; the root is
/// sized so a 1600 × 1131 screenshot window shows the whole page without
/// cropping.
pub const SHEET_PX_WIDTH: f64 = 1600.0;
/// Pixel height of the SVG root.
pub const SHEET_PX_HEIGHT: f64 = 1131.0;
/// Inset of the outer page frame from the sheet edge, in millimetres.
pub(crate) const SHEET_BORDER_MM: f64 = 5.0;
/// Inset of the inner frame that bounds the drawing area, in millimetres.
pub(crate) const SHEET_INNER_MM: f64 = 8.0;
/// Width of the generic fallback footprint in millimetres.
pub const DEFAULT_COMPONENT_WIDTH_MM: f64 = 2.0;
/// Height of the generic fallback footprint in millimetres.
pub const DEFAULT_COMPONENT_HEIGHT_MM: f64 = 1.2;

pub(crate) const DRAW_MARGIN_MM: f64 = 8.0;
pub(crate) const COLUMN_GAP_MM: f64 = 6.0;
pub(crate) const SIDE_WIDTH_MM: f64 = 78.0;
pub(crate) const LEGEND_HEIGHT_MM: f64 = 50.0;
pub(crate) const TITLE_HEIGHT_MM: f64 = 42.0;
pub(crate) const FEEDER_ROW_MM: f64 = 6.0;

/// Sheet heading size in millimetres; every sheet reuses this scale so the
/// drawings read the same at print size.
pub(crate) const FONT_TITLE: f64 = 4.2;
/// Subtitle or secondary heading size in millimetres.
pub(crate) const FONT_SUBTITLE: f64 = 2.8;
/// Section heading size in millimetres.
pub(crate) const FONT_SECTION: f64 = 3.3;
/// Body text size in millimetres.
pub(crate) const FONT_BODY: f64 = 2.7;
/// Compact note size in millimetres.
pub(crate) const FONT_NOTE: f64 = 2.4;
/// Table cell size in millimetres.
pub(crate) const FONT_TABLE: f64 = 2.5;
/// Table header size in millimetres.
pub(crate) const FONT_TABLE_HEAD: f64 = 2.2;
/// Multiplier from font size to a comfortable line advance.
pub(crate) const LINE_ADVANCE: f64 = 1.38;

/// Largest uniform enlargement applied when a drawing is smaller than the
/// sheet, so a sparse document never blows its text up to poster size.
const SHEET_MAX_SCALE: f64 = 1.25;

/// Composes one fixed A4 landscape sheet around a body drawn in the body's own
/// millimetre coordinates.
///
/// The body is scaled uniformly to fit the area inside the inner frame above
/// the title block and centred there, so nothing is cropped and no text is
/// truncated. Every sheet therefore has the same 297 × 210 mm page, the same
/// outer and inner frame and the same bottom title block. `content` is the
/// body's own bounding box `(left, top, right, bottom)` in millimetres.
pub(crate) fn render_sheet(content: Box4, body: &str, title: &[TextLine]) -> String {
    let title_width = SHEET_WIDTH_MM - 2.0 * SHEET_INNER_MM - 6.0;
    let laid = lay_out(title, title_width);
    let title_h = (block_height(&laid) + 6.0).clamp(20.0, 42.0);
    let inner_right = SHEET_WIDTH_MM - SHEET_INNER_MM;
    let inner_bottom = SHEET_HEIGHT_MM - SHEET_INNER_MM;
    let title_top = inner_bottom - title_h;
    let area = (SHEET_INNER_MM, SHEET_INNER_MM, inner_right, title_top - 4.0);
    let content_w = (content.2 - content.0).max(1e-6);
    let content_h = (content.3 - content.1).max(1e-6);
    let scale = ((area.2 - area.0) / content_w).min((area.3 - area.1) / content_h).min(SHEET_MAX_SCALE);
    let offset_x = area.0 + ((area.2 - area.0) - content_w * scale) / 2.0 - content.0 * scale;
    let offset_y = area.1 + ((area.3 - area.1) - content_h * scale) / 2.0 - content.1 * scale;
    let mut svg = String::with_capacity(body.len() + 2048);
    svg.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{:.0}\" height=\"{:.0}\">\n",
        number(SHEET_WIDTH_MM),
        number(SHEET_HEIGHT_MM),
        SHEET_PX_WIDTH,
        SHEET_PX_HEIGHT
    ));
    svg.push_str(&format!(
        "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>\n",
        number(SHEET_WIDTH_MM),
        number(SHEET_HEIGHT_MM)
    ));
    svg.push_str(&format!(
        "  <rect id=\"frame\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"0.7\"/>\n",
        number(SHEET_BORDER_MM),
        number(SHEET_BORDER_MM),
        number(SHEET_WIDTH_MM - 2.0 * SHEET_BORDER_MM),
        number(SHEET_HEIGHT_MM - 2.0 * SHEET_BORDER_MM),
        hex(PANEL_EDGE)
    ));
    svg.push_str(&format!(
        "  <rect id=\"inner-frame\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        number(SHEET_INNER_MM),
        number(SHEET_INNER_MM),
        number(inner_right - SHEET_INNER_MM),
        number(inner_bottom - SHEET_INNER_MM),
        hex(PANEL_EDGE)
    ));
    svg.push_str(&format!(
        "  <g id=\"content\" transform=\"translate({} {}) scale({})\">\n",
        number(offset_x),
        number(offset_y),
        number(scale)
    ));
    svg.push_str(body);
    svg.push_str("  </g>\n");
    svg.push_str(&format!(
        "  <g id=\"title-block\" transform=\"translate({} {})\">\n",
        number(SHEET_INNER_MM),
        number(title_top)
    ));
    svg.push_str(&format!(
        "    <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#{}\" stroke-width=\"0.45\"/>\n",
        number(inner_right - SHEET_INNER_MM),
        number(title_h),
        hex(PANEL_EDGE)
    ));
    push_block_svg(&mut svg, 2.5, 3.0, &laid);
    svg.push_str("  </g>\n</svg>\n");
    svg
}

/// Title-block stamp shared by every sheet: name, drawing number, work order,
/// units and face.
pub(crate) fn title_stamp(name: &str, number: &str, document: &str, units: &str, scale: &str, face: &str) -> String {
    format!("{name} | drawing {number} | work order {document} | units {units} | scale {scale} | face {face}")
}

pub(crate) const PDF_PAGE_WIDTH: f64 = 841.89;
pub(crate) const PDF_PAGE_HEIGHT: f64 = 595.28;
pub(crate) const PDF_MARGIN: f64 = 24.0;

pub(crate) const INK: &str = "0.06 0.09 0.16";
pub(crate) const MUTED: &str = "0.32 0.38 0.47";
pub(crate) const RULE: &str = "0.78 0.82 0.87";
pub(crate) const BOARD_FILL: &str = "0.973 0.980 0.988";
pub(crate) const BOARD_EDGE: &str = "0.20 0.25 0.33";
pub(crate) const PANEL_EDGE: &str = "0.06 0.09 0.16";
pub(crate) const TOP_FILL: &str = "0.86 0.92 1.00";
pub(crate) const TOP_EDGE: &str = "0.11 0.31 0.85";
pub(crate) const BOTTOM_FILL: &str = "1.00 0.93 0.86";
pub(crate) const BOTTOM_EDGE: &str = "0.76 0.25 0.05";
pub(crate) const UNUSED_FILL: &str = "0.945 0.957 0.973";
pub(crate) const UNUSED_EDGE: &str = "0.835 0.859 0.894";

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
    /// Number of component rectangles that fell back to the default footprint.
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
    let mut body = String::new();
    push_svg_panel(&mut body, &plan, &layout);
    push_svg_legend(&mut body, &plan, &layout);
    push_svg_feeders(&mut body, &plan, &layout);
    push_svg_packages(&mut body, &plan, &layout);
    push_svg_notes(&mut body, &plan, &layout);
    let content = (0.0, 0.0, layout.width, layout.content_bottom);
    let text = render_sheet(content, &body, &drawing_text(&plan, "A4 landscape"));
    Ok(Drawing { text, summary: plan.summary })
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
    push_pdf_packages(&mut pdf, &plan, &layout);
    push_pdf_notes(&mut pdf, &plan, &layout);
    let scale_note = format!("A4 landscape fit; 1 mm = {PT_PER_MM} pt x fit {fit:.3}");
    push_pdf_title(&mut pdf, &plan, &layout, &scale_note);

    Ok(Drawing { text: pdf_page(pdf.content), summary: plan.summary })
}

/// Assembles one A4 landscape page around a drawing content stream.
pub(crate) fn pdf_page(content: String) -> String {
    let frame = format!(
        "{PANEL_EDGE} RG 0.8 w {:.2} {:.2} {:.2} {:.2} re S\n",
        PDF_MARGIN / 2.0,
        PDF_MARGIN / 2.0,
        PDF_PAGE_WIDTH - PDF_MARGIN,
        PDF_PAGE_HEIGHT - PDF_MARGIN
    );
    let content = format!("{frame}{content}");
    pdf_pages(vec![content])
}

/// Assembles a multi-page A4 landscape PDF with one content stream per page.
pub(crate) fn pdf_pages(contents: Vec<String>) -> String {
    let pages = contents.len().max(1);
    let font_object = 3 + pages;
    let first_content = font_object + 1;
    let kids = (0..pages).map(|index| format!("{} 0 R", 3 + index)).collect::<Vec<_>>().join(" ");
    let mut objects =
        vec!["<< /Type /Catalog /Pages 2 0 R >>".to_string(), format!("<< /Type /Pages /Kids [{kids}] /Count {pages} >>")];
    for index in 0..pages {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PDF_PAGE_WIDTH:.2} {PDF_PAGE_HEIGHT:.2}] \
             /Resources << /Font << /F1 {font_object} 0 R >> >> /Contents {} 0 R >>",
            first_content + index
        ));
    }
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
    for content in &contents {
        objects.push(format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content));
    }
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
    text
}

struct Part {
    refdes: String,
    package: String,
    side: Side,
    rotation: f64,
    body_length: f64,
    body_width: f64,
    geometry: packages::Geometry,
    feeder: Option<String>,
    instances: Vec<(u32, f64, f64)>,
    counts: u32,
}

/// One package row of the legend table.
struct PackageEntry {
    package: String,
    feeder: String,
    side: Side,
    count: u32,
}

/// Axis-aligned box `(left, top, right, bottom)` in sheet millimetres.
pub(crate) type Box4 = (f64, f64, f64, f64);

/// Label font size in millimetres for a body of the given smaller side.
/// The lower bound keeps the reference readable in print (≥ 2.2 mm ≈ 6.2 pt).
pub(crate) fn label_font(min_body: f64) -> f64 {
    (min_body * 0.9).clamp(2.2, 3.2)
}

/// Width of one glyph in units of the font size. Narrow letters, digits and
/// punctuation are measured separately so a table column is never sized from a
/// flat average that truncates wide labels or wastes space on narrow ones.
pub(crate) fn glyph_width(character: char) -> f64 {
    match character {
        'i' | 'l' | 'j' | 'I' | '.' | ',' | ':' | ';' | '|' | '!' | '\'' | ' ' | '(' | ')' | '[' | ']' => 0.32,
        'f' | 't' | 'r' | '-' | '/' | '\\' => 0.42,
        'm' | 'M' | 'W' | 'w' => 0.85,
        '0'..='9' => 0.6,
        'A'..='Z' => 0.68,
        _ => 0.58,
    }
}

/// Estimated text width of one label in millimetres.
pub(crate) fn text_width(text: &str, size: f64) -> f64 {
    text.chars().map(glyph_width).sum::<f64>() * size
}

/// True when two boxes overlap by more than the 0.05 mm label clearance.
pub(crate) fn collides(candidate: &Box4, occupied: &[Box4]) -> bool {
    occupied.iter().any(|box_| {
        candidate.0 < box_.2 - 0.05 && box_.0 < candidate.2 - 0.05 && candidate.1 < box_.3 - 0.05 && box_.1 < candidate.3 - 0.05
    })
}

/// True when a box lies inside the frame with the given inset.
pub(crate) fn inside(candidate: &Box4, frame: &Box4) -> bool {
    candidate.0 >= frame.0 && candidate.1 >= frame.1 && candidate.2 <= frame.2 && candidate.3 <= frame.3
}

/// Splits `text` into lines no wider than `max_width` at `size`. Words are kept
/// whole where possible; a single over-long word is hard-split so no line ever
/// leaves its column.
pub(crate) fn wrap_text(text: &str, max_width: f64, size: f64) -> Vec<String> {
    if max_width <= size || text.is_empty() {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let width = text_width(&current, size) + if current.is_empty() { 0.0 } else { size * glyph_width(' ') };
        if !current.is_empty() && width + text_width(word, size) > max_width {
            lines.push(std::mem::take(&mut current));
        }
        if text_width(word, size) > max_width {
            // Hard-split a word that cannot fit a whole line.
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            let mut chunk = String::new();
            for character in word.chars() {
                if text_width(&chunk, size) + size * glyph_width(character) > max_width && !chunk.is_empty() {
                    lines.push(std::mem::take(&mut chunk));
                }
                chunk.push(character);
            }
            current = chunk;
            continue;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// One text paragraph written into a frame.
#[derive(Debug, Clone)]
pub(crate) struct TextLine {
    /// Paragraph text before wrapping.
    pub(crate) text: String,
    /// Font size in millimetres.
    pub(crate) size: f64,
    /// Whether the paragraph is set in ink instead of the muted colour.
    pub(crate) emphasis: bool,
}

/// A paragraph laid out onto a baseline inside a frame.
#[derive(Debug, Clone)]
pub(crate) struct LaidLine {
    /// Wrapped text of one line.
    pub(crate) text: String,
    /// Font size in millimetres.
    pub(crate) size: f64,
    /// Whether the line is emphasised.
    pub(crate) emphasis: bool,
    /// Baseline offset from the top of the block, in millimetres.
    pub(crate) y: f64,
}

/// Lays out paragraphs into lines no wider than `width`, returning the wrapped
/// lines with their baseline offsets. The block never overflows the width.
pub(crate) fn lay_out(lines: &[TextLine], width: f64) -> Vec<LaidLine> {
    let mut laid = Vec::new();
    let mut y = 0.0;
    for line in lines {
        let advance = line.size * LINE_ADVANCE;
        let wrapped = wrap_text(&line.text, width, line.size);
        for (index, text) in wrapped.into_iter().enumerate() {
            y += if index == 0 { line.size } else { advance };
            laid.push(LaidLine { text, size: line.size, emphasis: line.emphasis, y });
        }
        y += advance - line.size;
    }
    laid
}

/// Height of a laid-out block plus the given vertical padding.
pub(crate) fn block_height(laid: &[LaidLine]) -> f64 {
    laid.last().map_or(0.0, |line| line.y) + FONT_NOTE * 0.8
}

/// Writes a laid-out block into an SVG group at `(x, y)`.
pub(crate) fn push_block_svg(svg: &mut String, x: f64, y: f64, laid: &[LaidLine]) {
    for line in laid {
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{}</text>\n",
            number(x),
            number(y + line.y),
            number(line.size),
            hex(if line.emphasis { INK } else { MUTED }),
            escape_xml(&line.text)
        ));
    }
}

/// Writes a laid-out block into a PDF content stream at `(x, y)`.
pub(crate) fn push_block_pdf(pdf: &mut Pdf, x: f64, y: f64, laid: &[LaidLine]) {
    for line in laid {
        pdf.text(x, y + line.y, line.size, if line.emphasis { INK } else { MUTED }, &line.text);
    }
}

/// Alignment of one table column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    /// Left-aligned text.
    Left,
    /// Right-aligned number.
    Right,
}

/// A table whose columns are measured from their contents. Empty columns are
/// dropped, numeric columns are right-aligned and the row height follows the
/// cell font, so no cell is clipped and no two columns overlap.
#[derive(Debug, Clone)]
pub(crate) struct Table {
    /// Column headers and alignments that still carry a value.
    pub(crate) columns: Vec<(String, Align)>,
    /// Column widths in millimetres.
    pub(crate) widths: Vec<f64>,
    /// Cell rows.
    pub(crate) rows: Vec<Vec<String>>,
    /// Cell font size in millimetres.
    pub(crate) size: f64,
    /// Space between columns in millimetres.
    pub(crate) gap: f64,
    /// Height of one cell row in millimetres.
    pub(crate) row: f64,
    /// Height of the header row in millimetres.
    pub(crate) head: f64,
}

impl Table {
    /// Builds a table, measuring each column and dropping the columns whose
    /// every cell is empty.
    pub(crate) fn new(headers: &[(&str, Align)], rows: Vec<Vec<String>>, size: f64, gap: f64) -> Self {
        let mut columns = Vec::new();
        let mut widths = Vec::new();
        for (index, (header, align)) in headers.iter().enumerate() {
            let has_value = rows.iter().any(|row| row.get(index).is_some_and(|value| !value.is_empty()));
            if !has_value && header.is_empty() {
                continue;
            }
            let width = rows
                .iter()
                .filter_map(|row| row.get(index))
                .map(|value| text_width(value, size))
                .fold(text_width(header, FONT_TABLE_HEAD), f64::max);
            columns.push(((*header).to_string(), *align));
            widths.push(width);
        }
        Self { columns, widths, rows, size, gap, row: size * 1.55, head: FONT_TABLE_HEAD * 2.4 }
    }

    /// Sets the row height in millimetres.
    pub(crate) fn with_row(mut self, row: f64) -> Self {
        self.row = row.max(self.size * 1.2);
        self
    }

    /// Sets the header row height independently of the data rows.
    pub(crate) fn with_head(mut self, head: f64) -> Self {
        self.head = head.max(FONT_TABLE_HEAD * 1.8);
        self
    }

    /// Widens the columns proportionally so the table spans `available`
    /// millimetres. Narrow natural columns still stay legible because the extra
    /// space is shared by their measured width.
    pub(crate) fn fit_width(mut self, available: f64) -> Self {
        let natural = self.width();
        if natural > 0.0 && available > natural {
            let extra = available - natural;
            for width in &mut self.widths {
                *width += extra * (*width / natural);
            }
        }
        self
    }

    /// Widens the last column by any space left over, so a left-aligned free
    /// text column fills the page while measured numeric columns keep their
    /// natural width.
    pub(crate) fn fill_last(mut self, available: f64) -> Self {
        let natural = self.width();
        if natural < available {
            if let Some(last) = self.widths.last_mut() {
                *last += available - natural;
            }
        }
        self
    }

    /// Total width of the table in millimetres.
    pub(crate) fn width(&self) -> f64 {
        self.widths.iter().sum::<f64>() + self.gap * (self.widths.len().saturating_sub(1)) as f64
    }

    /// Height of one cell row in millimetres.
    pub(crate) fn row_height(&self) -> f64 {
        self.row
    }

    /// Total height including the header row.
    pub(crate) fn height(&self) -> f64 {
        self.head + self.rows.len() as f64 * self.row_height()
    }

    /// Writes the table as an SVG group at `(x, y)`, with zebra striping and a
    /// rule under the header. Returns the used height.
    pub(crate) fn push_svg(&self, svg: &mut String, x: f64, y: f64, id: &str) -> f64 {
        if self.columns.is_empty() {
            return 0.0;
        }
        let row_height = self.row_height();
        let width = self.width();
        let head = self.head;
        svg.push_str(&format!("  <g id=\"{}\" transform=\"translate({} {})\">\n", escape_xml(id), number(x), number(y)));
        svg.push_str(&format!(
            "    <line x1=\"0\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.25\"/>\n",
            number(head),
            number(width),
            number(head),
            hex(RULE)
        ));
        let mut offset = 0.0;
        for (index, (header, _)) in self.columns.iter().enumerate() {
            svg.push_str(&format!(
                "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{}</text>\n",
                number(offset),
                number(head / 2.0 + FONT_TABLE_HEAD * 0.35),
                number(FONT_TABLE_HEAD),
                hex(MUTED),
                escape_xml(header)
            ));
            offset += self.widths[index] + self.gap;
        }
        for (row_index, row) in self.rows.iter().enumerate() {
            let top = head + row_height * row_index as f64;
            if row_index % 2 == 1 {
                svg.push_str(&format!(
                    "    <rect x=\"0\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\"/>\n",
                    number(top),
                    number(width),
                    number(row_height),
                    hex(BOARD_FILL)
                ));
            }
            let baseline = top + row_height / 2.0 + self.size * 0.35;
            let mut offset = 0.0;
            for (index, (_, align)) in self.columns.iter().enumerate() {
                let value = row.get(index).map(String::as_str).unwrap_or("");
                let x = match align {
                    Align::Left => offset,
                    Align::Right => offset + self.widths[index] - text_width(value, self.size),
                };
                svg.push_str(&format!(
                    "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{}</text>\n",
                    number(x),
                    number(baseline),
                    number(self.size),
                    hex(INK),
                    escape_xml(value)
                ));
                offset += self.widths[index] + self.gap;
            }
        }
        svg.push_str("  </g>\n");
        self.height()
    }

    /// Writes the table as PDF text at `(x, y)` and returns the used height.
    pub(crate) fn push_pdf(&self, pdf: &mut Pdf, x: f64, y: f64) -> f64 {
        if self.columns.is_empty() {
            return 0.0;
        }
        let row_height = self.row_height();
        let head = self.head;
        let mut offset = 0.0;
        for (index, (header, _)) in self.columns.iter().enumerate() {
            pdf.text(x + offset, y + head / 2.0 + FONT_TABLE_HEAD * 0.35, FONT_TABLE_HEAD, MUTED, header);
            offset += self.widths[index] + self.gap;
        }
        for (row_index, row) in self.rows.iter().enumerate() {
            let baseline = y + head + row_height * row_index as f64 + row_height / 2.0 + self.size * 0.35;
            let mut offset = 0.0;
            for (index, (_, align)) in self.columns.iter().enumerate() {
                let value = row.get(index).map(String::as_str).unwrap_or("");
                let dx = match align {
                    Align::Left => offset,
                    Align::Right => offset + self.widths[index] - text_width(value, self.size),
                };
                pdf.text(x + dx, baseline, self.size, INK, value);
                offset += self.widths[index] + self.gap;
            }
        }
        self.height()
    }
}

/// One label placed around a body.
#[derive(Debug, Clone)]
pub(crate) struct PlacedLabel {
    /// Text of the label.
    pub(crate) text: String,
    /// Font size in millimetres.
    pub(crate) size: f64,
    /// Left edge of the label in sheet millimetres.
    pub(crate) x: f64,
    /// Baseline of the label in sheet millimetres.
    pub(crate) y: f64,
    /// Optional leader from the body edge to the label.
    pub(crate) leader: Option<((f64, f64), (f64, f64))>,
    /// Whether the label is set in the muted colour.
    pub(crate) muted: bool,
}

/// Places one label in the first free slot of eight candidates around `body`:
/// above-left, above-centre, above-right, below-left, below-centre, below-right,
/// right-middle and left-middle. The label is left-aligned so its measured width
/// decides the box, and a leader line is emitted when the label moves clear of
/// its body. Returns `None` when every candidate collides or leaves the frame.
pub(crate) fn place_label(text: &str, size: f64, body: Box4, frame: Box4, occupied: &mut Vec<Box4>) -> Option<PlacedLabel> {
    let width = text_width(text, size);
    let height = size;
    let clearance = 0.5;
    let centre = (body.0 + body.2) / 2.0;
    let candidates: [(f64, f64); 8] = [
        (centre - width / 2.0, body.1 - clearance - height),
        (body.0, body.1 - clearance - height),
        (body.2 - width, body.1 - clearance - height),
        (centre - width / 2.0, body.3 + clearance),
        (body.0, body.3 + clearance),
        (body.2 - width, body.3 + clearance),
        (body.2 + clearance, (body.1 + body.3) / 2.0 + height / 2.0),
        (body.0 - clearance - width, (body.1 + body.3) / 2.0 + height / 2.0),
    ];
    for (index, (x, baseline)) in candidates.iter().enumerate() {
        let box_ = (*x, baseline - height, x + width, *baseline);
        if !inside(&box_, &frame) || collides(&box_, occupied) {
            continue;
        }
        occupied.push(box_);
        // A label that sits above or below its body reads without a leader; a
        // side label that clears the body by more than half a millimetre gets
        // one so the association stays explicit on a dense board.
        let leader = match index {
            6 => Some(((body.2, (body.1 + body.3) / 2.0), (*x, *baseline - height / 2.0))),
            7 => Some(((body.0, (body.1 + body.3) / 2.0), (*x + width, *baseline - height / 2.0))),
            _ => None,
        };
        return Some(PlacedLabel { text: text.to_string(), size, x: *x, y: *baseline, leader, muted: false });
    }
    None
}

/// Writes placed labels as an SVG group.
pub(crate) fn push_labels_svg(svg: &mut String, labels: &[PlacedLabel], color: &str) {
    for label in labels {
        let color = if label.muted { MUTED } else { color };
        if let Some((start, end)) = label.leader {
            svg.push_str(&format!(
                "    <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.18\"/>\n",
                number(start.0),
                number(start.1),
                number(end.0),
                number(end.1),
                hex(color)
            ));
        }
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{}</text>\n",
            number(label.x),
            number(label.y),
            number(label.size),
            hex(color),
            escape_xml(&label.text)
        ));
    }
}

/// Writes placed labels as PDF text.
pub(crate) fn push_labels_pdf(pdf: &mut Pdf, labels: &[PlacedLabel], color: &str) {
    for label in labels {
        let color = if label.muted { MUTED } else { color };
        if let Some((start, end)) = label.leader {
            pdf.line(start.0, start.1, end.0, end.1, color, 0.18);
        }
        pdf.text(label.x, label.y, label.size, color, &label.text);
    }
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
    labels: Vec<PlacedLabel>,
    packages: Vec<PackageEntry>,
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
    packages_y: f64,
    notes_y: f64,
    content_bottom: f64,
    title_y: f64,
    title_h: f64,
}

impl Layout {
    fn new(plan: &Plan) -> Self {
        let side_x = DRAW_MARGIN_MM + plan.panel_width + COLUMN_GAP_MM;
        let legend_h = 48.0;
        let feeder_y = DRAW_MARGIN_MM + legend_h + COLUMN_GAP_MM;
        let feeder_h = 6.0 + (plan.feeders.len() as f64 + 1.0) * 5.6;
        let packages_y = feeder_y + feeder_h + COLUMN_GAP_MM;
        let packages_h = 6.0 + (plan.packages.len() as f64 + 1.0) * 5.6;
        let notes = side_notes(plan);
        let notes_block = notes.len() as f64 * FONT_NOTE * LINE_ADVANCE + 5.0;
        let notes_y = (packages_y + packages_h + COLUMN_GAP_MM)
            .max(DRAW_MARGIN_MM + plan.panel_depth - notes_block)
            .max(packages_y + packages_h + 4.0);
        let side_bottom = notes_y + notes_block;
        let content_bottom = (DRAW_MARGIN_MM + plan.panel_depth).max(side_bottom);
        let title_width = side_x + SIDE_WIDTH_MM + DRAW_MARGIN_MM - 2.0 * DRAW_MARGIN_MM;
        let laid = lay_out(&drawing_text(plan, "1 mm = 8 px"), title_width - 5.0);
        let title_h = block_height(&laid) + 6.0;
        let title_y = content_bottom + COLUMN_GAP_MM;
        Self {
            width: side_x + SIDE_WIDTH_MM + DRAW_MARGIN_MM,
            height: title_y + title_h + DRAW_MARGIN_MM,
            panel_x: DRAW_MARGIN_MM,
            panel_y: DRAW_MARGIN_MM,
            side_x,
            feeder_y,
            packages_y,
            notes_y,
            content_bottom,
            title_y,
            title_h,
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
    let mut default_size_components = 0;
    for mapped in &map.components {
        let feeder = lane_of.get(&mapped.refdes).cloned().or_else(|| {
            document.feeders.iter().find(|feeder| feeder.package == mapped.package).map(|feeder| feeder.id.clone())
        });
        let (refdes, count) = ascii(&mapped.refdes);
        dropped += count;
        let (package, count) = ascii(&mapped.package);
        dropped += count;
        let geometry = packages::geometry(&mapped.package);
        if geometry.family == packages::Family::Generic {
            default_size_components += 1;
        }
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
            body_length: geometry.body_length,
            body_width: geometry.body_width,
            geometry,
            feeder,
            instances: mapped.instances.iter().map(|placement| (placement.board, placement.x, placement.y)).collect(),
            counts: mapped.instances.len() as u32,
        });
    }

    let mut packages: Vec<PackageEntry> = Vec::new();
    for part in &parts {
        if let Some(entry) = packages.iter_mut().find(|entry| {
            entry.package == part.package && entry.feeder == part.feeder.clone().unwrap_or_default() && entry.side == part.side
        }) {
            entry.count += part.counts;
        }
        else {
            packages.push(PackageEntry {
                package: part.package.clone(),
                feeder: part.feeder.clone().unwrap_or_else(|| "-".to_string()),
                side: part.side,
                count: part.counts,
            });
        }
    }
    packages.sort_by(|left, right| {
        left.side
            .as_str()
            .cmp(right.side.as_str())
            .then_with(|| left.package.cmp(&right.package))
            .then_with(|| left.feeder.cmp(&right.feeder))
    });

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
        default_size_components,
        feeder_lanes: feeders.len() as u32,
        dropped_characters: dropped as u32,
    };
    let labels = build_labels(&parts, map.width, map.depth);
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
        labels,
        packages,
        feeders,
        summary,
    })
}

/// Axis-aligned box of a `length × width` rectangle rotated clockwise about
/// `(x, y)`, in sheet millimetres.
pub(crate) fn rotated_box(x: f64, y: f64, rotation: f64, length: f64, width: f64) -> Box4 {
    let corners = corners(x, y, rotation, length, width);
    let xs: Vec<f64> = corners.iter().map(|(px, _)| *px).collect();
    let ys: Vec<f64> = corners.iter().map(|(_, py)| *py).collect();
    (
        xs.iter().cloned().fold(f64::INFINITY, f64::min),
        ys.iter().cloned().fold(f64::INFINITY, f64::min),
        xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    )
}

/// Axis-aligned outline box of one part, body plus leads, at the given board
/// coordinates.
fn body_box(part: &Part, x: f64, y: f64) -> Box4 {
    let shape = package_shape(&part.geometry, x, y, part.rotation);
    let mut points: Vec<(f64, f64)> = shape.body.to_vec();
    for lead in &shape.leads {
        points.extend_from_slice(lead);
    }
    (
        points.iter().map(|(px, _)| *px).fold(f64::INFINITY, f64::min),
        points.iter().map(|(_, py)| *py).fold(f64::INFINITY, f64::min),
        points.iter().map(|(px, _)| *px).fold(f64::NEG_INFINITY, f64::max),
        points.iter().map(|(_, py)| *py).fold(f64::NEG_INFINITY, f64::max),
    )
}

/// Places the board-1 labels. The reference designator is the priority label
/// and takes the first free slot of eight candidates around its body; the
/// package and lane line follows in a muted size. A label that finds no free
/// slot is omitted, and the package table in the side column still names every
/// package, so a dense board never prints an unreadable overlap.
fn build_labels(parts: &[Part], board_width: f64, board_depth: f64) -> Vec<PlacedLabel> {
    let frame = (0.4, 0.4, board_width - 0.4, board_depth - 0.4);
    let mut occupied: Vec<Box4> = Vec::with_capacity(parts.len());
    let mut centres: Vec<(f64, f64, &Part)> = Vec::new();
    for part in parts {
        if let Some((_, x, y)) = part.instances.iter().find(|(board, ..)| *board == 1) {
            occupied.push(body_box(part, *x, *y));
            centres.push((*x, *y, part));
        }
    }
    let mut labels = Vec::with_capacity(centres.len() * 2);
    for (x, y, part) in &centres {
        let body = body_box(part, *x, *y);
        let font = label_font(part.body_length.min(part.body_width));
        if let Some(label) = place_label(&part.refdes, font, body, frame, &mut occupied) {
            labels.push(label);
        }
        let text = format!("{} {}", part.package, part.feeder.as_deref().unwrap_or("-"));
        let size = (font * 0.82).max(2.2);
        if text_width(&text, size) <= board_width - 1.0 {
            if let Some(mut label) = place_label(&text, size, body, frame, &mut occupied) {
                label.muted = true;
                labels.push(label);
            }
        }
    }
    labels
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
            let (fill, edge) = (hex(fill), hex(edge));
            for (_, x, y) in &part.instances {
                let shape = package_shape(&part.geometry, *x, *y, part.rotation);
                let points = |corners: &[(f64, f64); 4]| {
                    corners.iter().map(|(px, py)| format!("{},{}", number(*px), number(*py))).collect::<Vec<_>>().join(" ")
                };
                for lead in &shape.leads {
                    svg.push_str(&format!(
                        "      <polygon data-lead=\"{}\" points=\"{}\" fill=\"#{edge}\" stroke=\"#{edge}\" stroke-width=\"0.1\"/>\n",
                        escape_xml(&part.refdes),
                        points(lead)
                    ));
                }
                svg.push_str(&format!(
                    "      <polygon data-body=\"true\" data-refdes=\"{}\" data-rotation=\"{}\" points=\"{}\" fill=\"#{fill}\" stroke=\"#{edge}\" stroke-width=\"0.25\"/>\n",
                    escape_xml(&part.refdes),
                    number(part.rotation),
                    points(&shape.body)
                ));
                if let Some((px, py)) = shape.polarity {
                    svg.push_str(&format!(
                        "      <circle data-pin1=\"{}\" cx=\"{}\" cy=\"{}\" r=\"0.22\" fill=\"#{edge}\"/>\n",
                        escape_xml(&part.refdes),
                        number(px),
                        number(py)
                    ));
                }
            }
        }
        svg.push_str("    </g>\n");
    }
    svg.push_str("    <g id=\"labels\">\n");
    push_labels_svg(svg, &plan.labels, INK);
    svg.push_str("    </g>\n  </g>\n");
}

fn push_svg_legend(svg: &mut String, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = DRAW_MARGIN_MM;
    svg.push_str(&format!("  <g id=\"legend\" transform=\"translate({} {})\">\n", number(x), number(y)));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"4.6\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" font-weight=\"bold\" fill=\"#{}\">LEGEND</text>\n",
        number(FONT_SECTION),
        hex(INK)
    ));
    let rows = [
        (BOARD_FILL, BOARD_EDGE, "board outline and panel gap"),
        (TOP_FILL, TOP_EDGE, "top-side component"),
        (BOTTOM_FILL, BOTTOM_EDGE, "bottom-side component"),
    ];
    for (index, (fill, edge, text)) in rows.iter().enumerate() {
        let top = 9.0 + index as f64 * 9.0;
        svg.push_str(&format!(
            "    <rect x=\"0\" y=\"{}\" width=\"10\" height=\"5\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.35\"/>\n",
            number(top),
            hex(fill),
            hex(edge)
        ));
        svg.push_str(&legend_text(15.0, top + 3.9, FONT_BODY, text));
    }
    let notes = ["bodies come from the package rules table".to_string(), labels_note(plan)];
    let mut y_offset = 39.0;
    for note in notes {
        for line in wrap_text(&note, SIDE_WIDTH_MM - 2.0, FONT_NOTE) {
            svg.push_str(&legend_text(0.0, y_offset, FONT_NOTE, &line));
            y_offset += FONT_NOTE * LINE_ADVANCE;
        }
    }
    svg.push_str("  </g>\n");
}

fn push_svg_feeders(svg: &mut String, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = layout.feeder_y;
    svg.push_str(&format!("  <g id=\"feeders\" transform=\"translate({} {})\">\n", number(x), number(y)));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"4.6\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" font-weight=\"bold\" fill=\"#{}\">FEEDER LANES</text>\n",
        number(FONT_SECTION),
        hex(INK)
    ));
    if plan.feeders.is_empty() {
        svg.push_str(&legend_text(0.0, 11.0, FONT_BODY, "no feeder lanes declared"));
        svg.push_str("  </g>\n");
        return;
    }
    let rows = plan
        .feeders
        .iter()
        .map(|feeder| {
            vec![
                feeder.id.clone(),
                feeder.package.clone(),
                number(feeder.pitch),
                format!("{}/{}", feeder.loaded, feeder.capacity),
            ]
        })
        .collect();
    let table = Table::new(
        &[("LANE", Align::Left), ("PACKAGE", Align::Left), ("PITCH", Align::Right), ("LOAD", Align::Right)],
        rows,
        FONT_TABLE,
        4.0,
    )
    .with_row(5.6)
    .fit_width(SIDE_WIDTH_MM);
    table.push_svg(svg, 0.0, 7.0, "feeder-table");
    svg.push_str("  </g>\n");
}

fn push_svg_packages(svg: &mut String, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = layout.packages_y;
    svg.push_str(&format!("  <g id=\"packages\" transform=\"translate({} {})\">\n", number(x), number(y)));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"4.6\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" font-weight=\"bold\" fill=\"#{}\">PACKAGES</text>\n",
        number(FONT_SECTION),
        hex(INK)
    ));
    if plan.packages.is_empty() {
        svg.push_str(&legend_text(0.0, 11.0, FONT_BODY, "no components declared"));
        svg.push_str("  </g>\n");
        return;
    }
    let rows = plan
        .packages
        .iter()
        .map(|entry| {
            vec![entry.package.clone(), entry.feeder.clone(), entry.side.as_str().to_string(), entry.count.to_string()]
        })
        .collect();
    let table = Table::new(
        &[("PACKAGE", Align::Left), ("LANE", Align::Left), ("SIDE", Align::Left), ("QTY", Align::Right)],
        rows,
        FONT_TABLE,
        4.0,
    )
    .with_row(5.6)
    .fit_width(SIDE_WIDTH_MM);
    table.push_svg(svg, 0.0, 7.0, "package-table");
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
                let shape = package_shape(&part.geometry, *cx, *cy, part.rotation);
                let points = |corners: &[(f64, f64); 4]| -> Vec<(f64, f64)> {
                    corners.iter().map(|(px, py)| (x + px, y + py)).collect()
                };
                for lead in &shape.leads {
                    pdf.polygon(&points(lead), Some(edge), Some(edge), 0.1);
                }
                pdf.polygon(&points(&shape.body), Some(fill), Some(edge), 0.25);
                if let Some((px, py)) = shape.polarity {
                    pdf.marker(x + px, y + py, 0.22, edge, edge);
                }
            }
        }
    }
    push_labels_pdf(pdf, &plan.labels, INK);
}

fn push_pdf_legend(pdf: &mut Pdf, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = DRAW_MARGIN_MM;
    pdf.text(x, y + 4.6, FONT_SECTION, INK, "LEGEND");
    let rows = [
        (BOARD_FILL, BOARD_EDGE, "board outline and panel gap"),
        (TOP_FILL, TOP_EDGE, "top-side component"),
        (BOTTOM_FILL, BOTTOM_EDGE, "bottom-side component"),
    ];
    for (index, (fill, edge, text)) in rows.iter().enumerate() {
        let top = y + 9.0 + index as f64 * 9.0;
        pdf.rect(x, top, 10.0, 5.0, Some(fill), Some((edge, 0.35)));
        pdf.text(x + 15.0, top + 3.9, FONT_BODY, INK, text);
    }
    let notes = ["bodies come from the package rules table".to_string(), labels_note(plan)];
    for line in lay_out(
        &notes.iter().map(|text| TextLine { text: text.clone(), size: FONT_NOTE, emphasis: false }).collect::<Vec<_>>(),
        SIDE_WIDTH_MM - 2.0,
    ) {
        pdf.text(x, y + 39.0 + line.y, line.size, INK, &line.text);
    }
}

fn push_pdf_feeders(pdf: &mut Pdf, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = layout.feeder_y;
    pdf.text(x, y + 4.6, FONT_SECTION, INK, "FEEDER LANES");
    if plan.feeders.is_empty() {
        pdf.text(x, y + 11.0, FONT_BODY, INK, "no feeder lanes declared");
        return;
    }
    let rows = plan
        .feeders
        .iter()
        .map(|feeder| {
            vec![
                feeder.id.clone(),
                feeder.package.clone(),
                number(feeder.pitch),
                format!("{}/{}", feeder.loaded, feeder.capacity),
            ]
        })
        .collect();
    Table::new(
        &[("LANE", Align::Left), ("PACKAGE", Align::Left), ("PITCH", Align::Right), ("LOAD", Align::Right)],
        rows,
        FONT_TABLE,
        4.0,
    )
    .with_row(5.6)
    .fit_width(SIDE_WIDTH_MM)
    .push_pdf(pdf, x, y + 7.0);
}

fn push_pdf_packages(pdf: &mut Pdf, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = layout.packages_y;
    pdf.text(x, y + 4.6, FONT_SECTION, INK, "PACKAGES");
    if plan.packages.is_empty() {
        pdf.text(x, y + 11.0, FONT_BODY, INK, "no components declared");
        return;
    }
    let rows = plan
        .packages
        .iter()
        .map(|entry| {
            vec![entry.package.clone(), entry.feeder.clone(), entry.side.as_str().to_string(), entry.count.to_string()]
        })
        .collect();
    Table::new(
        &[("PACKAGE", Align::Left), ("LANE", Align::Left), ("SIDE", Align::Left), ("QTY", Align::Right)],
        rows,
        FONT_TABLE,
        4.0,
    )
    .with_row(5.6)
    .fit_width(SIDE_WIDTH_MM)
    .push_pdf(pdf, x, y + 7.0);
}

fn push_svg_notes(svg: &mut String, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = layout.notes_y;
    svg.push_str(&format!("  <g id=\"notes\" transform=\"translate({} {})\">\n", number(x), number(y)));
    svg.push_str(&format!(
        "    <line x1=\"0\" y1=\"0\" x2=\"{}\" y2=\"0\" stroke=\"#{}\" stroke-width=\"0.25\"/>\n",
        number(SIDE_WIDTH_MM),
        hex(RULE)
    ));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"3.6\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" font-weight=\"bold\" fill=\"#{}\">NOTES</text>\n",
        number(FONT_SECTION),
        hex(INK)
    ));
    let mut offset = 3.6 + FONT_NOTE * LINE_ADVANCE;
    for line in lay_out(
        &side_notes(plan)
            .iter()
            .map(|text| TextLine { text: text.clone(), size: FONT_NOTE, emphasis: false })
            .collect::<Vec<_>>(),
        SIDE_WIDTH_MM,
    ) {
        offset += line.size;
        svg.push_str(&format!(
            "    <text x=\"0\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{}</text>\n",
            number(offset),
            number(line.size),
            hex(MUTED),
            escape_xml(&line.text)
        ));
        offset += line.size * (LINE_ADVANCE - 1.0);
    }
    svg.push_str("  </g>\n");
}

fn side_notes(plan: &Plan) -> Vec<String> {
    let top = plan.parts.iter().filter(|part| part.side == Side::Top).count();
    let bottom = plan.parts.iter().filter(|part| part.side == Side::Bottom).count();
    vec![
        format!(
            "panel {:.1} x {:.1} mm | {} board(s) | {:.1} x {:.1} mm boards",
            plan.panel_width, plan.panel_depth, plan.summary.boards, plan.board_width, plan.board_depth
        ),
        format!(
            "{} component(s) per board | {} placement(s) | {} top / {} bottom",
            plan.summary.components, plan.summary.placements, top, bottom
        ),
        "top side first; nearest-neighbour route; labels are collision-free".to_string(),
        "omitted package names stay listed in PACKAGES".to_string(),
    ]
}

fn push_pdf_notes(pdf: &mut Pdf, plan: &Plan, layout: &Layout) {
    let x = layout.side_x;
    let y = layout.notes_y;
    pdf.line(x, y, x + SIDE_WIDTH_MM, y, RULE, 0.25);
    pdf.text(x, y + 3.6, FONT_SECTION, INK, "NOTES");
    let mut offset = y + 3.6 + FONT_NOTE * LINE_ADVANCE;
    for line in lay_out(
        &side_notes(plan)
            .iter()
            .map(|text| TextLine { text: text.clone(), size: FONT_NOTE, emphasis: false })
            .collect::<Vec<_>>(),
        SIDE_WIDTH_MM,
    ) {
        offset += line.size;
        pdf.text(x, offset, line.size, MUTED, &line.text);
        offset += line.size * (LINE_ADVANCE - 1.0);
    }
}

fn push_pdf_title(pdf: &mut Pdf, plan: &Plan, layout: &Layout, scale_note: &str) {
    let x = layout.panel_x;
    let y = layout.title_y;
    let width = layout.width - 2.0 * DRAW_MARGIN_MM;
    pdf.rect(x, y, width, layout.title_h, Some("1 1 1"), Some((PANEL_EDGE, 0.45)));
    push_block_pdf(pdf, x + 2.5, y + 3.0, &lay_out(&drawing_text(plan, scale_note), width - 5.0));
}

/// Title-block paragraphs of the placement sheet, measured and wrapped by
/// [`lay_out`] so no line leaves the block.
fn drawing_text(plan: &Plan, scale_note: &str) -> Vec<TextLine> {
    let title = |text: String| TextLine { text, size: FONT_TITLE, emphasis: true };
    let body = |text: String| TextLine { text, size: FONT_BODY, emphasis: false };
    vec![
        title(format!("OpenSMT placement drawing | drawing OSMT-PL-01 | revision {}", plan.revision)),
        body(title_stamp("placement", "OSMT-PL-01", &plan.id, &plan.units, "1:1", "top and bottom")),
        body(format!(
            "{} board(s) | {} component(s) per board | {} placement(s) | {} feeder lane(s)",
            plan.summary.boards, plan.summary.components, plan.summary.placements, plan.summary.feeder_lanes
        )),
        body(format!(
            "component bodies from the package rules table; unmatched packages use {:.1} x {:.1} mm; rotation clockwise",
            DEFAULT_COMPONENT_WIDTH_MM, DEFAULT_COMPONENT_HEIGHT_MM
        )),
        body(format!("{}; feeder tag = first matching lane in document order", labels_note(plan))),
        body(format!("millimetres; {scale_note}")),
    ]
}

fn labels_note(plan: &Plan) -> String {
    if plan.summary.boards > 1 {
        "labels on board 1 only, colliding labels omitted".to_string()
    }
    else {
        "labels on the single board, colliding labels omitted".to_string()
    }
}

pub(crate) fn colors(side: Side) -> (&'static str, &'static str) {
    match side {
        Side::Top => (TOP_FILL, TOP_EDGE),
        Side::Bottom => (BOTTOM_FILL, BOTTOM_EDGE),
    }
}

fn corners(x: f64, y: f64, rotation: f64, body_length: f64, body_width: f64) -> [(f64, f64); 4] {
    let half_width = body_length / 2.0;
    let half_height = body_width / 2.0;
    let radians = rotation.to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    [(-half_width, -half_height), (half_width, -half_height), (half_width, half_height), (-half_width, half_height)]
        .map(|(px, py)| (x + px * cosine - py * sine, y + px * sine + py * cosine))
}

/// Real package outline used by the assembly drawing: the body rectangle, one
/// lead per terminal and a pin-1 dot for polarised families.
pub(crate) struct PackageShape {
    pub(crate) body: [(f64, f64); 4],
    pub(crate) leads: Vec<[(f64, f64); 4]>,
    pub(crate) polarity: Option<(f64, f64)>,
}

/// Builds the body, lead and polarity geometry of one placed package.
pub(crate) fn package_shape(geometry: &packages::Geometry, x: f64, y: f64, rotation: f64) -> PackageShape {
    let radians = rotation.to_radians();
    let (cosine, sine) = (radians.cos(), radians.sin());
    let mut leads = Vec::new();
    let gulled = matches!(geometry.family, packages::Family::Qfp | packages::Family::Qfn);
    for terminal in packages::terminals(geometry) {
        let mut dx = terminal.dx;
        let mut dy = terminal.dy;
        let mut width = terminal.width;
        let mut height = terminal.height;
        // QFP/QFN pads sit flush with the body, so grow them outwards to read
        // as gull-wing leads instead of a dashed outline.
        if gulled {
            if terminal.dx.abs() >= terminal.dy.abs() {
                dx += terminal.dx.signum() * 0.35;
                width += 0.7;
            }
            else {
                dy += terminal.dy.signum() * 0.35;
                height += 0.7;
            }
        }
        // Gull-wing pads sit clear of the body; bridge the gap so the lead
        // reads as one piece instead of a floating pad.
        if geometry.family == packages::Family::Soic {
            let gap = terminal.dx.abs() - terminal.width / 2.0 - geometry.body_length / 2.0;
            if gap > 0.0 {
                dx -= terminal.dx.signum() * gap / 2.0;
                width += gap;
            }
        }
        let centre_x = x + dx * cosine - dy * sine;
        let centre_y = y + dx * sine + dy * cosine;
        leads.push(corners(centre_x, centre_y, rotation, width, height));
    }
    let polarised = matches!(
        geometry.family,
        packages::Family::Sot23
            | packages::Family::Soic
            | packages::Family::Qfp
            | packages::Family::Qfn
            | packages::Family::Lga
    );
    let polarity = polarised.then(|| {
        let px = -geometry.body_length / 2.0 + 0.55;
        let py = -geometry.body_width / 2.0 + 0.55;
        (x + px * cosine - py * sine, y + px * sine + py * cosine)
    });
    PackageShape { body: corners(x, y, rotation, geometry.body_length, geometry.body_width), leads, polarity }
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

pub(crate) fn number(value: f64) -> String {
    let value = if value.abs() < 0.0005 { 0.0 } else { value };
    format!("{value:.3}")
}

pub(crate) fn ascii(text: &str) -> (String, usize) {
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

pub(crate) fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

pub(crate) fn hex(rgb: &str) -> String {
    let values: Vec<f64> = rgb.split_whitespace().filter_map(|value| value.parse::<f64>().ok()).collect();
    if values.len() != 3 {
        return "000000".to_string();
    }
    let component = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    format!("{:02x}{:02x}{:02x}", component(values[0]), component(values[1]), component(values[2]))
}

pub(crate) struct Pdf {
    pub(crate) content: String,
    pub(crate) height: f64,
    pub(crate) scale: f64,
    pub(crate) origin_x: f64,
    pub(crate) origin_y: f64,
}

impl Pdf {
    pub(crate) fn point(&self, x: f64, y: f64) -> (f64, f64) {
        (self.origin_x + x * self.scale, self.origin_y + (self.height - y) * self.scale)
    }

    pub(crate) fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, fill: Option<&str>, stroke: Option<(&str, f64)>) {
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

    pub(crate) fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, stroke: &str, line_width: f64) {
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

    pub(crate) fn polygon(&mut self, points: &[(f64, f64)], fill: Option<&str>, stroke: Option<&str>, line_width: f64) {
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

    /// Draws an open polyline through the sheet millimetre points.
    pub(crate) fn route(&mut self, points: &[(f64, f64)], stroke: &str, line_width: f64) {
        self.content.push_str(&format!("{stroke} RG {:.2} w ", (line_width * self.scale).max(0.2)));
        for (index, (x, y)) in points.iter().enumerate() {
            let (px, py) = self.point(*x, *y);
            if index == 0 {
                self.content.push_str(&format!("{px:.2} {py:.2} m "));
            }
            else {
                self.content.push_str(&format!("{px:.2} {py:.2} l "));
            }
        }
        self.content.push_str("S\n");
    }

    /// Draws a filled circle marker with a 12-segment polygon.
    pub(crate) fn marker(&mut self, x: f64, y: f64, radius: f64, fill: &str, stroke: &str) {
        let points: Vec<(f64, f64)> = (0..12)
            .map(|index| {
                let angle = std::f64::consts::PI * 2.0 * index as f64 / 12.0;
                (x + radius * angle.cos(), y + radius * angle.sin())
            })
            .collect();
        self.polygon(&points, Some(fill), Some(stroke), 0.25);
    }

    pub(crate) fn text(&mut self, x: f64, y: f64, size: f64, color: &str, text: &str) {
        let (px, py) = self.point(x, y);
        let size = (size * self.scale).clamp(3.0, 12.0);
        let (literal, _) = pdf_text(text);
        self.content.push_str(&format!("{color} rg BT /F1 {size:.2} Tf {px:.2} {py:.2} Td {literal} Tj ET\n"));
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
