//! Deterministic setup table: slot, part number, nozzle, quantity and tape
//! width for every feeder lane.
//!
//! The rows come from the same [`crate::place::allocate`] report as the
//! `feeders` CSV, so a missing or undersized lane fails the same way. The model
//! has no nozzle or tape-width field, so both are derived from the package
//! rules: the nozzle from the family and body length, the tape width from the
//! family. The derivation is stated on every page.
//!
//! Rows are paginated. The SVG stacks one framed page per [`ROWS_PER_PAGE`]
//! rows; the PDF emits one A4 landscape page per chunk with a repeated header.

use serde::Serialize;

use crate::{
    SmtError,
    drawing::{self, DrawingLoss, PT_PER_MM, Pdf},
    model::SmtDocument,
    packages::{self, Family},
    place,
};

/// Page width of one setup page, in millimetres (A4 landscape).
pub const PAGE_WIDTH_MM: f64 = 297.0;
/// Page height of one setup page, in millimetres.
pub const PAGE_HEIGHT_MM: f64 = 210.0;
/// Rows drawn on one page before the table continues on the next page.
pub const ROWS_PER_PAGE: usize = 20;

const PAGE_MARGIN_MM: f64 = 8.0;
const HEADER_HEIGHT_MM: f64 = 28.0;
const ROW_HEIGHT_MM: f64 = 6.0;
const FOOTER_HEIGHT_MM: f64 = 22.0;
/// Height of the bottom title block of one SVG page, in millimetres.
const TITLE_BAR_MM: f64 = 30.0;
/// Height of the table header row, in millimetres.
const TABLE_HEAD_MM: f64 = 6.0;

/// Counts of a rendered setup sheet.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupSheetSummary {
    /// Unit system taken from the document.
    pub units: String,
    /// Number of feeder rows.
    pub rows: u32,
    /// Number of pages rendered.
    pub pages: u32,
    /// Number of components the allocation could not assign.
    pub unassigned_components: u32,
    /// Number of non-ASCII characters written as `?`.
    pub dropped_characters: u32,
}

impl SetupSheetSummary {
    /// Returns the aggregated text losses of this drawing.
    pub fn losses(&self) -> Vec<DrawingLoss> {
        if self.dropped_characters == 0 {
            Vec::new()
        }
        else {
            vec![DrawingLoss { code: "setup/non-ascii-text".to_string(), count: self.dropped_characters }]
        }
    }
}

/// One rendered setup sheet and its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct SetupSheet {
    /// Complete file contents.
    pub text: String,
    /// Counts and losses of the rendered drawing.
    pub summary: SetupSheetSummary,
}

struct Row {
    lane: String,
    package: String,
    nozzle: String,
    tape: f64,
    quantity: String,
    assigned: u32,
    refdes: String,
}

struct Plan {
    id: String,
    units: String,
    revision: u32,
    rows: Vec<Row>,
    unassigned: u32,
    dropped: u32,
}

impl Plan {
    fn pages(&self) -> usize {
        self.rows.len().div_ceil(ROWS_PER_PAGE).max(1)
    }
}

fn build_plan(document: &SmtDocument, id: &str) -> Result<Plan, SmtError> {
    let allocation = place::allocate(document)?;
    let (id, mut dropped) = drawing::ascii(id);
    let (units, count) = drawing::ascii(&document.units);
    dropped += count;

    let mut rows = Vec::with_capacity(allocation.feeders.len());
    for lane in &allocation.feeders {
        let (lane_id, count) = drawing::ascii(&lane.id);
        dropped += count;
        let (package, count) = drawing::ascii(&lane.package);
        dropped += count;
        let geometry = packages::geometry(&lane.package);
        let mut refdes = Vec::with_capacity(lane.refdes.len());
        for reference in &lane.refdes {
            let (text, count) = drawing::ascii(reference);
            dropped += count;
            refdes.push(text);
        }
        rows.push(Row {
            lane: lane_id,
            package,
            nozzle: nozzle(geometry.family, geometry.body_length).to_string(),
            tape: tape_width(geometry.family),
            quantity: format!("{}/{}", lane.loaded, lane.capacity),
            assigned: lane.assigned,
            refdes: truncate(&refdes.join(", "), 60),
        });
    }

    Ok(Plan {
        id,
        units,
        revision: document.revision,
        rows,
        unassigned: allocation.unassigned.len() as u32,
        dropped: dropped as u32,
    })
}

/// Nozzle class for a package family and body length. The model has no nozzle
/// field, so this is a documented default.
fn nozzle(family: Family, body_length: f64) -> &'static str {
    match family {
        Family::Chip if body_length <= 1.05 => "N-0402",
        Family::Chip if body_length <= 1.65 => "N-0603",
        Family::Chip if body_length <= 2.1 => "N-0805",
        Family::Chip => "N-1206",
        Family::Sot23 => "N-SOT23",
        Family::Soic => "N-SOIC",
        Family::Qfp => "N-QFP",
        Family::Qfn => "N-QFN",
        Family::Lga => "N-LGA",
        Family::Generic => "N-STD",
    }
}

/// Carrier tape width in millimetres for a package family.
fn tape_width(family: Family) -> f64 {
    match family {
        Family::Chip | Family::Sot23 => 8.0,
        Family::Soic | Family::Qfn | Family::Lga | Family::Generic => 12.0,
        Family::Qfp => 16.0,
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut shortened: String = text.chars().take(limit.saturating_sub(1)).collect();
    shortened.push('~');
    shortened
}

/// Renders the setup table as one fixed A4 landscape SVG page per chunk.
///
/// Every page is a complete 297 × 210 mm sheet with the shared outer frame,
/// inner frame and a bottom title block. Several pages stack vertically with a
/// gap, so each page of the file is still A4.
pub fn render_svg(document: &SmtDocument, id: &str) -> Result<SetupSheet, SmtError> {
    let plan = build_plan(document, id)?;
    let pages = plan.pages();
    let gap = 8.0;
    let width = PAGE_WIDTH_MM;
    let height = pages as f64 * PAGE_HEIGHT_MM + (pages.saturating_sub(1)) as f64 * gap;
    let mut svg = String::new();
    svg.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{:.0}\" height=\"{:.0}\">\n",
        drawing::number(width),
        drawing::number(height),
        drawing::SHEET_PX_WIDTH,
        (drawing::SHEET_PX_WIDTH * height / width).round()
    ));
    for page in 0..pages {
        let top = page as f64 * (PAGE_HEIGHT_MM + gap);
        svg.push_str(&format!(
            "  <rect x=\"0\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>\n",
            drawing::number(top),
            drawing::number(width),
            drawing::number(PAGE_HEIGHT_MM)
        ));
        push_page(&mut svg, &plan, page, top);
    }
    svg.push_str("</svg>\n");
    Ok(SetupSheet { text: svg, summary: summary(plan) })
}

/// Renders the setup table as one A4 landscape PDF page per chunk.
pub fn render_pdf(document: &SmtDocument, id: &str) -> Result<SetupSheet, SmtError> {
    let plan = build_plan(document, id)?;
    let pages = plan.pages();
    let mut contents = Vec::with_capacity(pages);
    for page in 0..pages {
        let mut pdf = Pdf { content: String::new(), height: PAGE_HEIGHT_MM, scale: PT_PER_MM, origin_x: 0.0, origin_y: 0.0 };
        push_page_pdf(&mut pdf, &plan, page);
        contents.push(pdf.content);
    }
    Ok(SetupSheet { text: drawing::pdf_pages(contents), summary: summary(plan) })
}

fn summary(plan: Plan) -> SetupSheetSummary {
    let pages = plan.pages() as u32;
    SetupSheetSummary {
        units: plan.units,
        rows: plan.rows.len() as u32,
        pages,
        unassigned_components: plan.unassigned,
        dropped_characters: plan.dropped,
    }
}

fn title_line(plan: &Plan, page: usize) -> String {
    format!("OpenSMT setup sheet | drawing OSMT-SS-01 | revision {} | page {} of {}", plan.revision, page + 1, plan.pages())
}

fn stamp(plan: &Plan) -> String {
    drawing::title_stamp("setup sheet", "OSMT-SS-01", &plan.id, &plan.units, "1:1", "n/a (feeder bank)")
}

fn rows_of(plan: &Plan, page: usize) -> &[Row] {
    let start = page * ROWS_PER_PAGE;
    let end = (start + ROWS_PER_PAGE).min(plan.rows.len());
    &plan.rows[start..end]
}

fn push_page(svg: &mut String, plan: &Plan, page: usize, offset_y: f64) {
    let inner = PAGE_MARGIN_MM;
    let content_w = PAGE_WIDTH_MM - 2.0 * inner;
    let title_top = PAGE_HEIGHT_MM - inner - TITLE_BAR_MM;
    let mut out = String::new();
    out.push_str(&format!("  <g data-page=\"{}\" transform=\"translate(0 {})\">\n", page + 1, drawing::number(offset_y)));
    out.push_str(&format!(
        "    <rect id=\"frame\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"0.7\"/>\n",
        drawing::number(5.0),
        drawing::number(5.0),
        drawing::number(PAGE_WIDTH_MM - 10.0),
        drawing::number(PAGE_HEIGHT_MM - 10.0),
        drawing::hex(drawing::PANEL_EDGE)
    ));
    out.push_str(&format!(
        "    <rect id=\"inner-frame\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        drawing::number(inner),
        drawing::number(inner),
        drawing::number(content_w),
        drawing::number(title_top + TITLE_BAR_MM - inner),
        drawing::hex(drawing::PANEL_EDGE)
    ));
    out.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"4.6\" font-weight=\"bold\" fill=\"#{}\">SMT SETUP SHEET</text>\n",
        drawing::number(inner + 4.0),
        drawing::number(inner + 8.0),
        drawing::hex(drawing::INK)
    ));
    out.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.6\" fill=\"#{}\">nozzle and feeder schedule | nozzle class and tape width derived from the package rules</text>\n",
        drawing::number(inner + 4.0),
        drawing::number(inner + 13.2),
        drawing::hex(drawing::MUTED)
    ));
    // The table sits right under the header; its rows grow with the free page
    // height so a short schedule does not leave a band of white space between
    // the heading and the first row.
    let table_top = inner + 17.0;
    let rows: Vec<Vec<String>> = rows_of(plan, page)
        .iter()
        .enumerate()
        .map(|(index, row)| {
            vec![
                format!("{:02}", page * ROWS_PER_PAGE + index + 1),
                row.lane.clone(),
                row.package.clone(),
                row.nozzle.clone(),
                format!("{:.0}", row.tape),
                row.quantity.clone(),
                row.assigned.to_string(),
                row.refdes.clone(),
            ]
        })
        .collect();
    let row_count = rows.len().max(1) as f64;
    let available = title_top - 4.0 - table_top - TABLE_HEAD_MM;
    let row_height = (available / row_count).clamp(ROW_HEIGHT_MM, 20.0);
    let table = drawing::Table::new(
        &[
            ("SLOT", drawing::Align::Right),
            ("LANE", drawing::Align::Left),
            ("PART NUMBER", drawing::Align::Left),
            ("NOZZLE", drawing::Align::Left),
            ("TAPE MM", drawing::Align::Right),
            ("QTY", drawing::Align::Right),
            ("ASSIGNED", drawing::Align::Right),
            ("REFERENCE DESIGNATORS", drawing::Align::Left),
        ],
        rows,
        drawing::FONT_TABLE,
        4.0,
    )
    .with_head(TABLE_HEAD_MM)
    .with_row(row_height)
    .fill_last(content_w - 8.0);
    table.push_svg(&mut out, inner + 4.0, table_top, "setup-table");
    if plan.rows.is_empty() {
        out.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.9\" fill=\"#{}\">no feeder lanes declared</text>\n",
            drawing::number(inner + 4.0),
            drawing::number(table_top + 10.0),
            drawing::hex(drawing::MUTED)
        ));
    }
    // Bottom title block, identical to every other sheet.
    out.push_str(&format!(
        "    <g id=\"title-block\" transform=\"translate({} {})\">\n",
        drawing::number(inner),
        drawing::number(title_top)
    ));
    out.push_str(&format!(
        "      <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#{}\" stroke-width=\"0.45\"/>\n",
        drawing::number(content_w),
        drawing::number(TITLE_BAR_MM),
        drawing::hex(drawing::PANEL_EDGE)
    ));
    out.push_str(&format!(
        "      <text x=\"3\" y=\"7\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"3.1\" font-weight=\"bold\" fill=\"#{}\">{}</text>\n",
        drawing::hex(drawing::INK),
        drawing::escape_xml(&title_line(plan, page))
    ));
    out.push_str(&format!(
        "      <text x=\"3\" y=\"13\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.7\" fill=\"#{}\">{}</text>\n",
        drawing::hex(drawing::MUTED),
        drawing::escape_xml(&stamp(plan))
    ));
    out.push_str(&format!(
        "      <text x=\"3\" y=\"18.6\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.7\" fill=\"#{}\">unassigned components: {} | {} row(s) on this page</text>\n",
        drawing::hex(drawing::MUTED),
        plan.unassigned,
        row_count as u32
    ));
    out.push_str("    </g>\n");
    out.push_str("  </g>\n");
    svg.push_str(&out);
}

fn push_page_pdf(pdf: &mut Pdf, plan: &Plan, page: usize) {
    let x = PAGE_MARGIN_MM;
    let y = PAGE_MARGIN_MM;
    let content_w = PAGE_WIDTH_MM - 2.0 * PAGE_MARGIN_MM;
    pdf.rect(x, y, content_w, PAGE_HEIGHT_MM - 2.0 * PAGE_MARGIN_MM, Some("1 1 1"), Some((drawing::PANEL_EDGE, 0.4)));
    pdf.text(x + 4.0, y + 9.0, 3.2, drawing::INK, "SMT SETUP SHEET");
    pdf.text(
        x + 4.0,
        y + 14.4,
        2.0,
        drawing::MUTED,
        "nozzle and feeder schedule | nozzle class and tape width derived from the package rules",
    );
    let table_y = y + HEADER_HEIGHT_MM;
    let rows: Vec<Vec<String>> = rows_of(plan, page)
        .iter()
        .enumerate()
        .map(|(index, row)| {
            vec![
                format!("{:02}", page * ROWS_PER_PAGE + index + 1),
                row.lane.clone(),
                row.package.clone(),
                row.nozzle.clone(),
                format!("{:.0}", row.tape),
                row.quantity.clone(),
                row.assigned.to_string(),
                row.refdes.clone(),
            ]
        })
        .collect();
    drawing::Table::new(
        &[
            ("SLOT", drawing::Align::Right),
            ("LANE", drawing::Align::Left),
            ("PART NUMBER", drawing::Align::Left),
            ("NOZZLE", drawing::Align::Left),
            ("TAPE MM", drawing::Align::Right),
            ("QTY", drawing::Align::Right),
            ("ASSIGNED", drawing::Align::Right),
            ("REFERENCE DESIGNATORS", drawing::Align::Left),
        ],
        rows,
        drawing::FONT_TABLE,
        4.0,
    )
    .with_row(ROW_HEIGHT_MM)
    .fit_width(content_w - 8.0)
    .push_pdf(pdf, x + 4.0, table_y + 2.0);
    if plan.rows.is_empty() {
        pdf.text(x + 4.0, table_y + 12.0, 2.9, drawing::MUTED, "no feeder lanes declared");
    }
    let footer_y = y + PAGE_HEIGHT_MM - 2.0 * PAGE_MARGIN_MM - FOOTER_HEIGHT_MM;
    pdf.rect(x, footer_y, content_w, FOOTER_HEIGHT_MM, Some("1 1 1"), Some((drawing::PANEL_EDGE, 0.4)));
    pdf.text(x + 3.0, footer_y + 6.0, 2.1, drawing::INK, &title_line(plan, page));
    pdf.text(x + 3.0, footer_y + 11.6, 2.0, drawing::MUTED, &stamp(plan));
    pdf.text(x + 3.0, footer_y + 15.6, 2.0, drawing::MUTED, &format!("unassigned components: {}", plan.unassigned));
}
