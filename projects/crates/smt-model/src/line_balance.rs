//! Deterministic line-balance bar chart.
//!
//! The chart plots the estimated cycle time of every station of a
//! [`crate::balance::balance`] run, highlights the slowest station and draws the
//! average as a target line. Export formats carry no options, so the sheet
//! assumes a fixed [`MACHINES`]-machine line and says so in its title block.

use serde::Serialize;

use crate::{
    SmtError,
    balance::{self, BalanceReport},
    drawing::{self, DrawingLoss},
    model::SmtDocument,
};

/// Machine count the sheet balances the document across.
pub const MACHINES: u32 = 3;

/// Counts of a rendered line-balance chart.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineBalanceSummary {
    /// Unit system taken from the document.
    pub units: String,
    /// Number of stations drawn.
    pub machines: u32,
    /// Zero-based index of the slowest station.
    pub bottleneck: u32,
    /// Cycle time of the slowest station, in seconds.
    pub max_seconds: f64,
    /// Sum of every station's cycle time, in seconds.
    pub total_seconds: f64,
    /// `(max - min) / max`, or zero for an empty line.
    pub imbalance: f64,
    /// Number of non-ASCII characters written as `?`.
    pub dropped_characters: u32,
}

impl LineBalanceSummary {
    /// Returns the aggregated text losses of this drawing.
    pub fn losses(&self) -> Vec<DrawingLoss> {
        if self.dropped_characters == 0 {
            Vec::new()
        }
        else {
            vec![DrawingLoss { code: "balance/non-ascii-text".to_string(), count: self.dropped_characters }]
        }
    }
}

/// One rendered line-balance chart and its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct LineBalanceDrawing {
    /// Complete file contents.
    pub text: String,
    /// Counts and losses of the rendered drawing.
    pub summary: LineBalanceSummary,
}

/// Renders the line balance as a millimetre SVG bar chart.
pub fn render_svg(document: &SmtDocument, id: &str) -> Result<LineBalanceDrawing, SmtError> {
    let report = balance::balance(document, MACHINES)?;
    let (id, mut dropped) = drawing::ascii(id);
    let (units, count) = drawing::ascii(&document.units);
    dropped += count;

    let plot_x = 34.0;
    let plot_y = 26.0;
    let plot_w = 170.0;
    let plot_h = 100.0;
    let width = plot_x + plot_w + 12.0;
    let title_y = plot_y + plot_h + 24.0;
    let laid_title = drawing::lay_out(&title_text(document, &report, &id, &units), width - 20.0 - 5.0);
    let title_h = drawing::block_height(&laid_title) + 6.0;
    let height = title_y + title_h + 10.0;

    let target = report.total_seconds / MACHINES as f64;
    let raw_top = (report.max_seconds.max(target) * 1.18).max(1e-9);
    let step = nice_step(raw_top / 4.0);
    let ticks = (raw_top / step).ceil().max(1.0) as u32;
    let top = step * ticks as f64;
    let base_y = plot_y + plot_h;
    let bar_slot = plot_w / MACHINES as f64;
    let bar_w = bar_slot * 0.46;
    let bottleneck = bottleneck(&report);

    let mut svg = String::new();
    drawing::svg_begin(&mut svg, width, height);

    svg.push_str(&format!(
        "  <text x=\"12\" y=\"17\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"4.2\" font-weight=\"bold\" fill=\"#{}\">LINE BALANCE</text>\n",
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "  <text x=\"12\" y=\"22.4\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.8\" fill=\"#{}\">station cycle time | bottleneck highlighted | dashed line = average target</text>\n",
        drawing::hex(drawing::MUTED)
    ));

    // Plot frame, y ticks and axis labels.
    svg.push_str(&format!(
        "  <rect data-role=\"plot\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        drawing::number(plot_x),
        drawing::number(plot_y),
        drawing::number(plot_w),
        drawing::number(plot_h),
        drawing::hex(drawing::BOARD_FILL),
        drawing::hex(drawing::BOARD_EDGE)
    ));
    for tick in 0..=ticks {
        let value = step * tick as f64;
        let y = base_y - value / top * plot_h;
        svg.push_str(&format!(
            "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.2\"/>\n",
            drawing::number(plot_x),
            drawing::number(y),
            drawing::number(plot_x + plot_w),
            drawing::number(y),
            drawing::hex(drawing::RULE)
        ));
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" text-anchor=\"end\" fill=\"#{}\">{:.1} s</text>\n",
            drawing::number(plot_x - 1.8),
            drawing::number(y + 0.6),
            drawing::hex(drawing::MUTED),
            value
        ));
    }
    svg.push_str(&format!(
        "  <text x=\"10\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" text-anchor=\"middle\" transform=\"rotate(-90 10 {})\" fill=\"#{}\">cycle time (s)</text>\n",
        drawing::number(plot_y + plot_h / 2.0),
        drawing::number(plot_y + plot_h / 2.0),
        drawing::hex(drawing::MUTED)
    ));

    // One bar per station, with the bottleneck in the accent colour.
    for machine in &report.machines {
        let centre = plot_x + bar_slot * (machine.index as f64 + 0.5);
        let bar_h = machine.seconds / top * plot_h;
        let colour = if machine.index == bottleneck { "0.78 0.22 0.18" } else { drawing::TOP_FILL };
        let edge = if machine.index == bottleneck { "0.55 0.12 0.10" } else { drawing::TOP_EDGE };
        svg.push_str(&format!(
            "  <rect data-machine=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.25\"/>\n",
            machine.index + 1,
            drawing::number(centre - bar_w / 2.0),
            drawing::number(base_y - bar_h),
            drawing::number(bar_w),
            drawing::number(bar_h.max(0.2)),
            drawing::hex(colour),
            drawing::hex(edge)
        ));
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.5\" text-anchor=\"middle\" fill=\"#{}\">{:.2} s</text>\n",
            drawing::number(centre),
            drawing::number(base_y - bar_h - 1.4),
            drawing::hex(drawing::INK),
            machine.seconds
        ));
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.8\" text-anchor=\"middle\" fill=\"#{}\">M{}</text>\n",
            drawing::number(centre),
            drawing::number(base_y + 5.0),
            drawing::hex(drawing::INK),
            machine.index + 1
        ));
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" text-anchor=\"middle\" fill=\"#{}\">{} part(s)</text>\n",
            drawing::number(centre),
            drawing::number(base_y + 8.6),
            drawing::hex(drawing::MUTED),
            machine.components.len()
        ));
    }

    // Average target line.
    let target_y = base_y - target / top * plot_h;
    svg.push_str(&format!(
        "  <line data-target=\"true\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.4\" stroke-dasharray=\"2 1.4\"/>\n",
        drawing::number(plot_x),
        drawing::number(target_y),
        drawing::number(plot_x + plot_w),
        drawing::number(target_y),
        drawing::hex(drawing::TOP_EDGE)
    ));
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.3\" text-anchor=\"end\" fill=\"#{}\">target {:.2} s</text>\n",
        drawing::number(plot_x + plot_w - 1.0),
        drawing::number(target_y - 1.2),
        drawing::hex(drawing::TOP_EDGE),
        target
    ));

    // Legend: only the two bar states and the target line are used.
    let legend_y = base_y + 14.0;
    svg.push_str(&format!(
        "  <rect x=\"{}\" y=\"{}\" width=\"7\" height=\"3.2\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.2\"/>\n",
        drawing::number(plot_x),
        drawing::number(legend_y - 2.6),
        drawing::hex(drawing::TOP_FILL),
        drawing::hex(drawing::TOP_EDGE)
    ));
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.5\" fill=\"#{}\">station cycle</text>\n",
        drawing::number(plot_x + 9.0),
        drawing::number(legend_y),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "  <rect x=\"{}\" y=\"{}\" width=\"7\" height=\"3.2\" fill=\"#c7382e\" stroke=\"#8c1f1a\" stroke-width=\"0.2\"/>\n",
        drawing::number(plot_x + 44.0),
        drawing::number(legend_y - 2.6)
    ));
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.5\" fill=\"#{}\">bottleneck</text>\n",
        drawing::number(plot_x + 53.0),
        drawing::number(legend_y),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.4\" stroke-dasharray=\"2 1.4\"/>\n",
        drawing::number(plot_x + 84.0),
        drawing::number(legend_y - 1.0),
        drawing::number(plot_x + 98.0),
        drawing::number(legend_y - 1.0),
        drawing::hex(drawing::TOP_EDGE)
    ));
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.5\" fill=\"#{}\">average target</text>\n",
        drawing::number(plot_x + 100.0),
        drawing::number(legend_y),
        drawing::hex(drawing::INK)
    ));

    push_title(&mut svg, 10.0, title_y, width - 20.0, title_h, &laid_title);
    drawing::svg_end(&mut svg);

    let summary = LineBalanceSummary {
        units,
        machines: report.machines.len() as u32,
        bottleneck,
        max_seconds: report.max_seconds,
        total_seconds: report.total_seconds,
        imbalance: report.imbalance,
        dropped_characters: dropped as u32,
    };
    Ok(LineBalanceDrawing { text: svg, summary })
}

/// Rounds an axis step up to a readable 1-2-5 value.
fn nice_step(value: f64) -> f64 {
    if value <= 0.2 {
        0.2
    }
    else if value <= 0.5 {
        0.5
    }
    else if value <= 1.0 {
        1.0
    }
    else if value <= 2.0 {
        2.0
    }
    else if value <= 5.0 {
        5.0
    }
    else {
        10.0
    }
}

fn bottleneck(report: &BalanceReport) -> u32 {
    report
        .machines
        .iter()
        .max_by(|left, right| left.seconds.partial_cmp(&right.seconds).unwrap_or(std::cmp::Ordering::Equal))
        .map_or(0, |machine| machine.index)
}

fn title_text(document: &SmtDocument, report: &BalanceReport, id: &str, units: &str) -> Vec<drawing::TextLine> {
    let title = |text: String| drawing::TextLine { text, size: drawing::FONT_TITLE, emphasis: true };
    let body = |text: String| drawing::TextLine { text, size: drawing::FONT_BODY, emphasis: false };
    vec![
        title(format!("OpenSMT line balance drawing | drawing OSMT-LB-01 | revision {}", document.revision)),
        body(drawing::title_stamp("line balance", "OSMT-LB-01", id, units, "1:1 (seconds)", "n/a (line)")),
        body(format!(
            "{} station(s) | total {:.2} s | bottleneck M{} at {:.2} s | imbalance {:.1}%",
            report.machines.len(),
            report.total_seconds,
            bottleneck(report) + 1,
            report.max_seconds,
            report.imbalance * 100.0
        )),
        body(format!("assumes a {MACHINES}-machine line; stations are balanced by smallest completion time")),
        body("millimetres; 1 mm = 8 px".to_string()),
    ]
}

fn push_title(svg: &mut String, x: f64, y: f64, width: f64, height: f64, laid: &[drawing::LaidLine]) {
    svg.push_str(&format!(
        "  <g id=\"title-block\" transform=\"translate({} {})\">\n    <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#{}\" stroke-width=\"0.45\"/>\n",
        drawing::number(x),
        drawing::number(y),
        drawing::number(width),
        drawing::number(height),
        drawing::hex(drawing::PANEL_EDGE)
    ));
    drawing::push_block_svg(svg, 2.5, 3.0, laid);
    svg.push_str("  </g>\n");
}
