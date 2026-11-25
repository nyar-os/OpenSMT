//! Read-only takt bottleneck heat map data.
//!
//! The report runs the same feeder allocation and nearest-neighbour sequence as
//! the default `takt` estimate and attributes the time of every program step to
//! the step itself, its package and its side. A step's time is the fixed
//! placement overhead plus the inbound travel of the same side divided by the
//! head speed, so the step times sum to the `takt` board time of the default
//! machine model with no change overhead. Shares are fractions of that total,
//! and the cumulative package share lets a caller draw a Pareto heat map.
//! Nothing is written and the document is unchanged.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    SmtError,
    drawing::{self, DrawingLoss},
    model::{Side, SmtDocument},
    place::{self, HEAD_SPEED, PLACE_SECONDS},
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-heatmap";

/// One program step with its attributed time.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatStep {
    /// Placement step number, starting at 1 in program order.
    pub sequence: u32,
    /// Reference designator.
    pub refdes: String,
    /// Package identifier.
    pub package: String,
    /// Board side.
    pub side: Side,
    /// Feeder lane that supplies the component.
    pub feeder: String,
    /// Placement X coordinate on one board.
    pub x: f64,
    /// Placement Y coordinate on one board.
    pub y: f64,
    /// Inbound travel from the previous step of the same side, in millimetres.
    pub travel_mm: f64,
    /// Fixed placement overhead of the step, in seconds.
    pub place_seconds: f64,
    /// Travel segment of the step, in seconds.
    pub travel_seconds: f64,
    /// `placeSeconds + travelSeconds`.
    pub seconds: f64,
    /// Fraction of the board total, in `[0, 1]`.
    pub share: f64,
}

/// Time aggregated by package, ordered from the largest share downwards.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatPackage {
    /// Package identifier.
    pub package: String,
    /// Number of program steps of this package.
    pub steps: u32,
    /// Summed placement overhead, in seconds.
    pub place_seconds: f64,
    /// Summed travel segment, in seconds.
    pub travel_seconds: f64,
    /// `placeSeconds + travelSeconds`.
    pub seconds: f64,
    /// Fraction of the board total, in `[0, 1]`.
    pub share: f64,
    /// Running share of this package and every package before it.
    pub cumulative_share: f64,
}

/// Time aggregated by board side.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatSide {
    /// Board side.
    pub side: Side,
    /// Number of program steps on the side.
    pub steps: u32,
    /// Summed step time, in seconds.
    pub seconds: f64,
    /// Fraction of the board total, in `[0, 1]`.
    pub share: f64,
}

/// Takt bottleneck heat map of one board.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Number of placement steps on one board.
    pub components: u32,
    /// Summed placement overhead, in seconds.
    pub place_seconds: f64,
    /// Summed travel segment, in seconds.
    pub travel_seconds: f64,
    /// `placeSeconds + travelSeconds`, the board time of the default model.
    pub total_seconds: f64,
    /// Package with the largest step time; `null` for an empty document.
    pub bottleneck: Option<String>,
    /// Packages from the largest share downwards.
    pub packages: Vec<HeatPackage>,
    /// Sides that carry at least one step, `top` before `bottom`.
    pub sides: Vec<HeatSide>,
    /// Steps in program order.
    pub steps: Vec<HeatStep>,
}

/// Builds the takt bottleneck heat map of a document.
///
/// Every side restarts at the machine origin, exactly like the placement
/// program, so the inbound travel of a side's first step is measured from
/// `(0, 0)`. Step times sum to the default `takt` board time. Packages are
/// ordered by descending time, ties by package name; sides keep `top` before
/// `bottom`. The allocation and the sequence run like the placement program, so
/// feeder coverage errors propagate. The document is not modified.
pub fn heatmap(document: &SmtDocument) -> Result<HeatmapReport, SmtError> {
    let allocation = place::allocate(document)?;
    let placement = place::sequence(document, &allocation)?;

    let mut steps = Vec::with_capacity(placement.entries.len());
    let mut side: Option<Side> = None;
    let mut x = 0.0;
    let mut y = 0.0;
    for entry in &placement.entries {
        if side != Some(entry.side) {
            x = 0.0;
            y = 0.0;
            side = Some(entry.side);
        }
        let travel_mm = place::distance(x, y, entry.x, entry.y);
        x = entry.x;
        y = entry.y;
        let travel_seconds = travel_mm / HEAD_SPEED;
        steps.push(HeatStep {
            sequence: entry.index.saturating_add(1),
            refdes: entry.refdes.clone(),
            package: entry.package.clone(),
            side: entry.side,
            feeder: entry.feeder.clone().unwrap_or_default(),
            x: entry.x,
            y: entry.y,
            travel_mm,
            place_seconds: PLACE_SECONDS,
            travel_seconds,
            seconds: PLACE_SECONDS + travel_seconds,
            share: 0.0,
        });
    }

    let total_seconds: f64 = steps.iter().map(|step| step.seconds).sum();
    let place_seconds = steps.len() as f64 * PLACE_SECONDS;
    let travel_seconds = placement.travel_length / HEAD_SPEED;
    for step in &mut steps {
        step.share = share(step.seconds, total_seconds);
    }

    let mut by_package: BTreeMap<&str, (u32, f64, f64)> = BTreeMap::new();
    let mut by_side: BTreeMap<&str, (Side, u32, f64)> = BTreeMap::new();
    for step in &steps {
        let package = by_package.entry(step.package.as_str()).or_insert((0, 0.0, 0.0));
        package.0 += 1;
        package.1 += step.place_seconds;
        package.2 += step.travel_seconds;
        let side = by_side.entry(step.side.as_str()).or_insert((step.side, 0, 0.0));
        side.1 += 1;
        side.2 += step.seconds;
    }

    let mut packages: Vec<HeatPackage> = by_package
        .into_iter()
        .map(|(package, (steps, place_seconds, travel_seconds))| {
            let seconds = place_seconds + travel_seconds;
            HeatPackage {
                package: package.to_string(),
                steps,
                place_seconds,
                travel_seconds,
                seconds,
                share: share(seconds, total_seconds),
                cumulative_share: 0.0,
            }
        })
        .collect();
    packages.sort_by(|left, right| {
        right
            .seconds
            .partial_cmp(&left.seconds)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.package.cmp(&right.package))
    });
    let mut cumulative = 0.0;
    for package in &mut packages {
        cumulative += package.share;
        package.cumulative_share = cumulative;
    }

    let sides = [Side::Top, Side::Bottom]
        .into_iter()
        .filter_map(|side| by_side.remove(side.as_str()))
        .map(|(side, steps, seconds)| HeatSide { side, steps, seconds, share: share(seconds, total_seconds) })
        .collect();

    let bottleneck = packages.first().map(|package| package.package.clone());
    Ok(HeatmapReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        components: steps.len() as u32,
        place_seconds,
        travel_seconds,
        total_seconds,
        bottleneck,
        packages,
        sides,
        steps,
    })
}

/// Fraction of a total, zero for an empty total.
fn share(value: f64, total: f64) -> f64 {
    if total > 0.0 { value / total } else { 0.0 }
}

/// Counts of a rendered takt heat map drawing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapDrawingSummary {
    /// Unit system taken from the document.
    pub units: String,
    /// Number of placement steps on one board.
    pub components: u32,
    /// Board time of the default model, in seconds.
    pub total_seconds: f64,
    /// Number of distinct packages in the Pareto column.
    pub packages: u32,
    /// Whether a bottleneck package is reported.
    pub has_bottleneck: bool,
    /// Number of non-ASCII characters written as `?`.
    pub dropped_characters: u32,
}

impl HeatmapDrawingSummary {
    /// Returns the aggregated text losses of this drawing.
    pub fn losses(&self) -> Vec<DrawingLoss> {
        if self.dropped_characters == 0 {
            Vec::new()
        }
        else {
            vec![DrawingLoss { code: "heatmap/non-ascii-text".to_string(), count: self.dropped_characters }]
        }
    }
}

/// One rendered takt heat map drawing and its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct HeatmapDrawing {
    /// Complete file contents.
    pub text: String,
    /// Counts and losses of the rendered drawing.
    pub summary: HeatmapDrawingSummary,
}

/// Maps a normalised heat value in `[0, 1]` to a blue-to-red hex colour.
fn heat_color(value: f64) -> String {
    let t = value.clamp(0.0, 1.0);
    let (red, green, blue) = if t < 0.5 {
        let k = t / 0.5;
        (37.0 + (245.0 - 37.0) * k, 99.0 + (158.0 - 99.0) * k, 235.0 + (11.0 - 235.0) * k)
    }
    else {
        let k = (t - 0.5) / 0.5;
        (245.0 + (220.0 - 245.0) * k, 158.0 + (38.0 - 158.0) * k, 11.0 + (38.0 - 11.0) * k)
    };
    format!("{:02x}{:02x}{:02x}", red.round() as u32, green.round() as u32, blue.round() as u32)
}

/// Draws the takt bottleneck heat map of one board as a millimetre SVG.
///
/// Every program step is plotted on the board outline with a colour and radius
/// that grow with its share of the board time, and a Pareto column ranks the
/// packages by their share. The result is deterministic and the document is
/// never modified.
pub fn render_svg(document: &SmtDocument, id: &str) -> Result<HeatmapDrawing, SmtError> {
    let report = heatmap(document)?;
    let (id, mut dropped) = drawing::ascii(id);
    let (units, count) = drawing::ascii(&document.units);
    dropped += count;

    let plot_x = drawing::DRAW_MARGIN_MM;
    let plot_y = drawing::DRAW_MARGIN_MM + 18.0;
    let plot_w = 110.0;
    let plot_h = 90.0;
    let side_x = plot_x + plot_w + drawing::COLUMN_GAP_MM;
    let side_w = 46.0;
    let width = side_x + side_w + drawing::DRAW_MARGIN_MM;
    let title_y = plot_y + plot_h + 10.0;
    let height = title_y + drawing::TITLE_HEIGHT_MM + drawing::DRAW_MARGIN_MM;

    let board_width = document.board.width.max(1e-9);
    let board_depth = document.board.depth.max(1e-9);
    let scale = (plot_w / board_width).min(plot_h / board_depth);
    let draw_w = board_width * scale;
    let draw_h = board_depth * scale;
    let px = |x: f64| plot_x + x * scale;
    let py = |y: f64| plot_y + y * scale;
    let max_share = report.steps.iter().map(|step| step.share).fold(0.0, f64::max).max(1e-9);

    let mut svg = String::new();
    svg.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{:.0}\" height=\"{:.0}\">\n",
        drawing::number(width),
        drawing::number(height),
        width * drawing::SVG_PX_PER_MM,
        height * drawing::SVG_PX_PER_MM
    ));
    svg.push_str(&format!(
        "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>\n",
        drawing::number(width),
        drawing::number(height)
    ));

    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"3.2\" font-weight=\"bold\" fill=\"#{}\">TAKT BOTTLENECK HEAT MAP</text>\n",
        drawing::number(drawing::DRAW_MARGIN_MM),
        drawing::number(drawing::DRAW_MARGIN_MM + 4.0),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.0\" fill=\"#{}\">{} step(s) | board {} s | bottleneck {}</text>\n",
        drawing::number(drawing::DRAW_MARGIN_MM),
        drawing::number(drawing::DRAW_MARGIN_MM + 8.6),
        drawing::hex(drawing::MUTED),
        report.components,
        drawing::number(report.total_seconds),
        drawing::escape_xml(&report.bottleneck.clone().unwrap_or_else(|| "none".to_string()))
    ));

    // Board outline.
    svg.push_str(&format!(
        "  <rect data-role=\"board\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.4\"/>\n",
        drawing::number(plot_x),
        drawing::number(plot_y),
        drawing::number(draw_w),
        drawing::number(draw_h),
        drawing::hex(drawing::BOARD_FILL),
        drawing::hex(drawing::BOARD_EDGE)
    ));

    // Heat dots per step.
    for step in &report.steps {
        let normalised = step.share / max_share;
        let radius = 0.9 + normalised * 2.2;
        svg.push_str(&format!(
            "  <circle data-step=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.2\"/>\n",
            step.sequence,
            drawing::number(px(step.x)),
            drawing::number(py(step.y)),
            drawing::number(radius),
            heat_color(normalised),
            drawing::hex(drawing::INK)
        ));
    }

    // Pareto column: packages ranked by share with a cumulative marker.
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.6\" font-weight=\"bold\" fill=\"#{}\">PACKAGES</text>\n",
        drawing::number(side_x),
        drawing::number(plot_y + 3.0),
        drawing::hex(drawing::INK)
    ));
    let bar_area = side_w - 14.0;
    let top = report.packages.iter().take(8);
    for (index, package) in top.enumerate() {
        let (name, count) = drawing::ascii(&package.package);
        dropped += count;
        let row = plot_y + 8.0 + index as f64 * 6.4;
        let bar_w = package.share * bar_area / report.packages.first().map_or(1.0, |first| first.share.max(1e-9));
        svg.push_str(&format!(
            "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"3.0\" fill=\"#{}\"/>\n",
            drawing::number(side_x),
            drawing::number(row),
            drawing::number(bar_w.max(0.2)),
            heat_color(package.share / report.packages.first().map_or(1.0, |first| first.share.max(1e-9)))
        ));
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"1.8\" fill=\"#{}\">{} {:.0}%</text>\n",
            drawing::number(side_x + bar_area + 1.4),
            drawing::number(row + 2.4),
            drawing::hex(drawing::MUTED),
            drawing::escape_xml(&name),
            package.share * 100.0
        ));
    }

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
        format!("OpenSMT takt heat map drawing | {id} | revision {} | units {units}", document.revision),
        format!(
            "place {} s + travel {} s = {} s | {} package(s)",
            drawing::number(report.place_seconds),
            drawing::number(report.travel_seconds),
            drawing::number(report.total_seconds),
            report.packages.len()
        ),
        "dot colour and size grow with the share of board time".to_string(),
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

    svg.push_str("</svg>\n");
    let summary = HeatmapDrawingSummary {
        units,
        components: report.components,
        total_seconds: report.total_seconds,
        packages: report.packages.len() as u32,
        has_bottleneck: report.bottleneck.is_some(),
        dropped_characters: dropped as u32,
    };
    Ok(HeatmapDrawing { text: svg, summary })
}
