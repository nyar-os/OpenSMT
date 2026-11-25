//! Feeder consumption drawing.
//!
//! The upper chart stacks the estimated picks and scrap of every feeder lane
//! against the supply of its first reel; a bar that runs past the threshold needs
//! a splice. The timeline below marks the board number at which each lane runs
//! out of a reel, so a line can be stopped at the right board. Both views use the
//! same [`consumption::plan`] report as the `consumption` export.

use serde::Serialize;

use crate::{
    SmtError,
    consumption::{self, ConsumptionReport, FeederConsumption},
    drawing::{self, DrawingLoss},
    model::SmtDocument,
};

/// Board count of the export drawing when the caller does not override it.
pub const BOARDS: u32 = 100;

/// Counts of a rendered consumption drawing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumptionDrawingSummary {
    /// Unit system taken from the document.
    pub units: String,
    /// Board count of the run.
    pub boards: u32,
    /// Number of feeder lanes drawn.
    pub lanes: u32,
    /// Estimated picks over the run.
    pub picks: u64,
    /// Picks plus start-up scrap.
    pub gross: u64,
    /// Scrap consumed by the run.
    pub scrap: u64,
    /// Number of reel changes over the run.
    pub splices: u32,
    /// Number of lanes whose supply is short.
    pub short_lanes: u32,
}

impl ConsumptionDrawingSummary {
    /// Returns the aggregated text losses of this drawing.
    pub fn losses(&self) -> Vec<DrawingLoss> {
        Vec::new()
    }
}

/// One rendered consumption drawing and its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsumptionDrawing {
    /// Complete file contents.
    pub text: String,
    /// Counts of the rendered drawing.
    pub summary: ConsumptionDrawingSummary,
}

const MARGIN: f64 = drawing::DRAW_MARGIN_MM;
const LABEL_W: f64 = 30.0;
const VALUE_W: f64 = 64.0;
const CHART_W: f64 = 128.0;
const ROW_H: f64 = 9.0;
const BAR_H: f64 = 5.6;
const BAND_H: f64 = 22.0;
const TIMELINE_H: f64 = 5.6;

/// Renders a 100-board consumption sheet for `document` as an SVG.
pub fn render_svg(document: &SmtDocument, id: &str) -> Result<ConsumptionDrawing, SmtError> {
    let report = consumption::plan(document, BOARDS)?;
    let summary = ConsumptionDrawingSummary {
        units: report.units.clone(),
        boards: report.boards,
        lanes: report.feeders.len() as u32,
        picks: report.total_picks,
        gross: report.total_gross,
        scrap: report.total_scrap,
        splices: report.splices,
        short_lanes: report.feeders.iter().filter(|feeder| feeder.short).count() as u32,
    };
    let width = MARGIN * 2.0 + LABEL_W + CHART_W + VALUE_W;
    let chart_top = MARGIN + 52.0;
    let rows = report.feeders.len().max(1);
    let chart_h = rows as f64 * ROW_H;
    let timeline_top = chart_top + chart_h + 10.0;
    let timeline_h = rows as f64 * TIMELINE_H + 14.0;
    let content_bottom = timeline_top + timeline_h + 22.0;

    let mut svg = String::new();
    svg.push_str("  <g id=\"consumption\">\n");
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" font-weight=\"bold\" fill=\"#{}\">FEEDER CONSUMPTION</text>\n",
        drawing::number(MARGIN),
        drawing::number(MARGIN + 5.0),
        drawing::number(drawing::FONT_TITLE),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" fill=\"#{}\">{} board run; bars show gross picks, the tick marks the first-reel threshold, a bar past it needs a splice</text>\n",
        drawing::number(MARGIN),
        drawing::number(MARGIN + 11.0),
        drawing::number(drawing::FONT_SUBTITLE),
        drawing::hex(drawing::MUTED),
        report.boards
    ));
    push_band(&mut svg, &report, MARGIN, MARGIN + 15.0, width - 2.0 * MARGIN);
    push_bars(&mut svg, &report, MARGIN, chart_top);
    push_timeline(&mut svg, &report, MARGIN, timeline_top, LABEL_W + CHART_W);
    push_legend(&mut svg, MARGIN, timeline_top + timeline_h + 2.0);
    svg.push_str("  </g>\n");
    let text = drawing::render_sheet((0.0, 0.0, width, content_bottom), &svg, &title_text(&report, id, "A4 landscape"));
    Ok(ConsumptionDrawing { text, summary })
}

fn push_band(svg: &mut String, report: &ConsumptionReport, x: f64, y: f64, width: f64) {
    svg.push_str(&format!(
        "  <g id=\"summary\">\n    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        drawing::number(x),
        drawing::number(y),
        drawing::number(width),
        drawing::number(BAND_H),
        drawing::hex(drawing::BOARD_FILL),
        drawing::hex(drawing::PANEL_EDGE)
    ));
    let short = report.feeders.iter().filter(|feeder| feeder.short).count();
    let metrics = [
        ("PICKS", report.total_picks.to_string()),
        ("GROSS", report.total_gross.to_string()),
        ("SCRAP", report.total_scrap.to_string()),
        ("SPLICES", report.splices.to_string()),
        ("SHORT LANES", short.to_string()),
    ];
    let column = width / metrics.len() as f64;
    for (index, (label, value)) in metrics.iter().enumerate() {
        let left = x + index as f64 * column;
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">{}</text>\n",
            drawing::number(left + 3.0),
            drawing::number(y + 7.0),
            drawing::hex(drawing::MUTED),
            label
        ));
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"4.0\" font-weight=\"bold\" fill=\"#{}\">{}</text>\n",
            drawing::number(left + 3.0),
            drawing::number(y + 15.0),
            drawing::hex(drawing::INK),
            value
        ));
    }
    svg.push_str("  </g>\n");
}

fn push_bars(svg: &mut String, report: &ConsumptionReport, x: f64, top: f64) {
    let max_gross = report.feeders.iter().map(|feeder| feeder.gross).max().unwrap_or(1).max(1) as f64;
    let chart_x = x + LABEL_W;
    svg.push_str("  <g id=\"bars\">\n");
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">LANE</text>\n",
        drawing::number(x),
        drawing::number(top - 3.0),
        drawing::hex(drawing::MUTED)
    ));
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">GROSS PICKS / FIRST REEL</text>\n",
        drawing::number(chart_x),
        drawing::number(top - 3.0),
        drawing::hex(drawing::MUTED)
    ));
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" text-anchor=\"end\" fill=\"#{}\">PICKS / REELS / LEFT</text>\n",
        drawing::number(x + LABEL_W + CHART_W + VALUE_W),
        drawing::number(top - 3.0),
        drawing::hex(drawing::MUTED)
    ));
    for (index, feeder) in report.feeders.iter().enumerate() {
        let row = top + index as f64 * ROW_H;
        let picked = feeder.picks as f64 / max_gross * CHART_W;
        let scrap = feeder.scrap as f64 / max_gross * CHART_W;
        let threshold = f64::from(feeder.loaded) / max_gross * CHART_W;
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.5\" font-weight=\"bold\" fill=\"#{}\">{}</text>\n",
            drawing::number(x),
            drawing::number(row + 4.0),
            drawing::hex(if feeder.short { drawing::BOTTOM_EDGE } else { drawing::INK }),
            drawing::escape_xml(&feeder.id)
        ));
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" fill=\"#{}\">{}</text>\n",
            drawing::number(x + 11.0),
            drawing::number(row + 4.0),
            drawing::hex(drawing::MUTED),
            drawing::escape_xml(&shrink_to_fit(&feeder.package, 18.0, 2.2))
        ));
        svg.push_str(&format!(
            "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\"/>\n",
            drawing::number(chart_x),
            drawing::number(row),
            drawing::number(picked.max(0.4)),
            drawing::number(BAR_H),
            drawing::hex(drawing::TOP_EDGE)
        ));
        svg.push_str(&format!(
            "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\"/>\n",
            drawing::number(chart_x + picked),
            drawing::number(row),
            drawing::number(scrap.max(0.0)),
            drawing::number(BAR_H),
            drawing::hex(drawing::TOP_FILL)
        ));
        if threshold <= CHART_W {
            svg.push_str(&format!(
                "    <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.5\" stroke-dasharray=\"1.2 0.8\"/>\n",
                drawing::number(chart_x + threshold),
                drawing::number(row - 1.2),
                drawing::number(chart_x + threshold),
                drawing::number(row + BAR_H + 1.2),
                drawing::hex(drawing::BOTTOM_EDGE)
            ));
        }
        let value = format!("{} / {} / {}", feeder.picks, feeder.reels, feeder.remainder);
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" text-anchor=\"end\" fill=\"#{}\">{}</text>\n",
            drawing::number(x + LABEL_W + CHART_W + VALUE_W),
            drawing::number(row + 4.0),
            drawing::hex(if feeder.short { drawing::BOTTOM_EDGE } else { drawing::INK }),
            value
        ));
    }
    svg.push_str("  </g>\n");
}

fn push_timeline(svg: &mut String, report: &ConsumptionReport, x: f64, top: f64, width: f64) {
    svg.push_str("  <g id=\"changeovers\">\n");
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" font-weight=\"bold\" fill=\"#{}\">REEL CHANGE TIMELINE (BOARDS)</text>\n",
        drawing::number(x),
        drawing::number(top),
        drawing::number(drawing::FONT_SECTION),
        drawing::hex(drawing::INK)
    ));
    let axis_y = top + 6.0;
    let left = x + LABEL_W;
    svg.push_str(&format!(
        "    <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        drawing::number(left),
        drawing::number(axis_y),
        drawing::number(left + width),
        drawing::number(axis_y),
        drawing::hex(drawing::RULE)
    ));
    let step = board_tick(report.boards);
    let mut board = 0;
    while board <= report.boards {
        let px = left + f64::from(board) / f64::from(report.boards.max(1)) * width;
        svg.push_str(&format!(
            "    <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.25\"/>\n",
            drawing::number(px),
            drawing::number(axis_y),
            drawing::number(px),
            drawing::number(axis_y + 2.0),
            drawing::hex(drawing::RULE)
        ));
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" text-anchor=\"middle\" fill=\"#{}\">{}</text>\n",
            drawing::number(px),
            drawing::number(axis_y + 5.6),
            drawing::hex(drawing::MUTED),
            board
        ));
        board += step;
    }
    for (index, feeder) in report.feeders.iter().enumerate() {
        let row = axis_y + 10.0 + index as f64 * TIMELINE_H;
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.3\" fill=\"#{}\">{}</text>\n",
            drawing::number(x),
            drawing::number(row + 1.8),
            drawing::hex(drawing::INK),
            drawing::escape_xml(&feeder.id)
        ));
        svg.push_str(&format!(
            "    <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
            drawing::number(left),
            drawing::number(row),
            drawing::number(left + width),
            drawing::number(row),
            drawing::hex(drawing::BOARD_EDGE)
        ));
        let mut points = String::new();
        let mut remaining = feeder.loaded;
        let capacity = feeder.capacity.max(feeder.loaded).max(1);
        for board in 0..=report.boards {
            let px = left + f64::from(board) / f64::from(report.boards.max(1)) * width;
            let ratio = f64::from(remaining) / f64::from(capacity);
            let py = row + 1.6 - 3.2 * ratio;
            points.push_str(&format!("{},{} ", drawing::number(px), drawing::number(py)));
            remaining = remaining.saturating_sub(feeder.picks_per_board);
            if remaining == 0 && feeder.picks_per_board > 0 {
                remaining = capacity;
            }
        }
        svg.push_str(&format!(
            "    <polyline points=\"{}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"0.55\"/>\n",
            points.trim_end(),
            drawing::hex(drawing::TOP_EDGE)
        ));
        let points = changeover_boards(feeder, report.boards);
        for point in &points {
            let px = left + f64::from(*point) / f64::from(report.boards.max(1)) * width;
            svg.push_str(&format!(
                "    <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.8\"/>\n",
                drawing::number(px),
                drawing::number(row - 1.8),
                drawing::number(px),
                drawing::number(row + 1.8),
                drawing::hex(drawing::BOTTOM_EDGE)
            ));
        }
    }
    svg.push_str("  </g>\n");
}

/// Board numbers at which a lane consumes a reel. The first change is when the
/// loaded quantity is exhausted, later ones every full reel; the list stops at
/// the end of the run.
pub(crate) fn changeover_boards(feeder: &FeederConsumption, boards: u32) -> Vec<u32> {
    if feeder.picks_per_board == 0 {
        return Vec::new();
    }
    let mut points = Vec::new();
    let mut consumed = 0u64;
    let mut threshold = u64::from(feeder.loaded.max(1));
    let capacity = u64::from(feeder.capacity.max(feeder.loaded.max(1)));
    let mut board = 1u32;
    while board <= boards {
        if consumed >= threshold {
            points.push(board);
            consumed -= threshold;
            threshold = capacity;
        }
        consumed += u64::from(feeder.picks_per_board);
        board += 1;
    }
    points
}

fn board_tick(boards: u32) -> u32 {
    match boards {
        0..=10 => 1,
        11..=25 => 2,
        26..=50 => 5,
        51..=100 => 10,
        _ => 20,
    }
}

fn push_legend(svg: &mut String, x: f64, y: f64) {
    svg.push_str(&format!("  <g id=\"legend\" transform=\"translate({} {})\">\n", drawing::number(x), drawing::number(y)));
    svg.push_str(&format!(
        "    <rect x=\"0\" y=\"0\" width=\"9\" height=\"4\" fill=\"#{}\"/>\n    <text x=\"12\" y=\"3.2\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">picks</text>\n",
        drawing::hex(drawing::TOP_EDGE),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "    <rect x=\"34\" y=\"0\" width=\"9\" height=\"4\" fill=\"#{}\"/>\n    <text x=\"46\" y=\"3.2\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">start-up scrap</text>\n",
        drawing::hex(drawing::TOP_FILL),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "    <line x1=\"102\" y1=\"0\" x2=\"102\" y2=\"4\" stroke=\"#{}\" stroke-width=\"0.5\" stroke-dasharray=\"1.2 0.8\"/>\n    <text x=\"105\" y=\"3.2\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">first-reel threshold</text>\n",
        drawing::hex(drawing::BOTTOM_EDGE),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "    <line x1=\"182\" y1=\"0\" x2=\"182\" y2=\"4\" stroke=\"#{}\" stroke-width=\"0.8\"/>\n    <text x=\"185\" y=\"3.2\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" fill=\"#{}\">reel change</text>\n",
        drawing::hex(drawing::BOTTOM_EDGE),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "    <text x=\"0\" y=\"9.4\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" fill=\"#{}\">timeline line = remaining parts on the loaded reel; the sawtooth resets on a reel change</text>\n",
        drawing::hex(drawing::MUTED)
    ));
    svg.push_str("  </g>\n");
}

fn title_text(report: &ConsumptionReport, id: &str, scale_note: &str) -> Vec<drawing::TextLine> {
    let title = |text: String| drawing::TextLine { text, size: drawing::FONT_TITLE, emphasis: true };
    let body = |text: String| drawing::TextLine { text, size: drawing::FONT_BODY, emphasis: false };
    vec![
        title("OpenSMT consumption drawing | drawing OSMT-CN-01 | revision 1".to_string()),
        body(drawing::title_stamp("consumption", "OSMT-CN-01", id, &report.units, "not to scale", "100-board run")),
        body(format!(
            "{} board(s) | {} feeder lane(s) | {} pick(s) | {} gross | {} scrap | {} splice(s)",
            report.boards,
            report.feeders.len(),
            report.total_picks,
            report.total_gross,
            report.total_scrap,
            report.splices
        )),
        body(
            "gross = picks x (1 + scrap ratio) per lane; reels = ceil(gross / capacity); left = remaining parts on the last reel"
                .to_string(),
        ),
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
