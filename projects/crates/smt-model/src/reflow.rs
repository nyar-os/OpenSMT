//! Reflow profile evaluation.
//!
//! The model assumes each zone is entered at the previous zone's set-point,
//! starting from [`AMBIENT`] degrees Celsius, and ramps linearly to its own
//! set-point over `length / conveyor_speed`. Time above liquidus is integrated
//! over those linear ramps, so a profile that crosses the liquidus inside a zone
//! is still measured exactly.

use serde::Serialize;

use crate::{
    SmtError,
    drawing::{self, DrawingLoss},
    model::SmtDocument,
    validate,
};

/// Temperature assumed at the conveyor entrance, in degrees Celsius.
pub const AMBIENT: f64 = 25.0;

/// One zone of the evaluated profile.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneReport {
    /// Zone name.
    pub name: String,
    /// Zone length along the conveyor.
    pub length: f64,
    /// Temperature entering the zone.
    pub entry_temperature: f64,
    /// Set-point temperature leaving the zone.
    pub exit_temperature: f64,
    /// Time spent in the zone, in seconds.
    pub seconds: f64,
    /// Time above the liquidus spent in this zone, in seconds.
    pub seconds_above_liquidus: f64,
}

/// Evaluated reflow profile.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflowReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Unit of every temperature field.
    pub temperature_unit: String,
    /// Conveyor speed in millimetres per minute.
    pub conveyor_speed: f64,
    /// Alloy liquidus in degrees Celsius.
    pub liquidus: f64,
    /// Highest set-point temperature.
    pub peak_temperature: f64,
    /// Total time above liquidus, in seconds.
    pub time_above_liquidus: f64,
    /// Total time from entrance to exit, in seconds.
    pub total_seconds: f64,
    /// One entry per zone, in conveyor order.
    pub zones: Vec<ZoneReport>,
}

/// Evaluates the reflow profile of a document.
///
/// The document is validated first, so an inconsistent profile returns an error
/// instead of a partial evaluation.
pub fn evaluate(document: &SmtDocument) -> Result<ReflowReport, SmtError> {
    let report = validate::validate(document);
    if let Some(error) = validate::first_error(&report) {
        return Err(error);
    }

    let profile = &document.profile;
    let mut temperature = AMBIENT;
    let mut time_above_liquidus = 0.0;
    let mut total_seconds = 0.0;
    let mut peak_temperature = AMBIENT;
    let mut zones = Vec::with_capacity(profile.zones.len());

    for zone in &profile.zones {
        let seconds = zone.length / profile.conveyor_speed * 60.0;
        let above = seconds_above(temperature, zone.temperature, seconds, profile.liquidus);
        zones.push(ZoneReport {
            name: zone.name.clone(),
            length: zone.length,
            entry_temperature: temperature,
            exit_temperature: zone.temperature,
            seconds,
            seconds_above_liquidus: above,
        });
        temperature = zone.temperature;
        peak_temperature = peak_temperature.max(zone.temperature);
        time_above_liquidus += above;
        total_seconds += seconds;
    }

    Ok(ReflowReport {
        format: "open-smt-reflow".to_string(),
        version: 1,
        units: document.units.clone(),
        temperature_unit: "celsius".to_string(),
        conveyor_speed: profile.conveyor_speed,
        liquidus: profile.liquidus,
        peak_temperature,
        time_above_liquidus,
        total_seconds,
        zones,
    })
}

fn seconds_above(entry: f64, exit: f64, seconds: f64, liquidus: f64) -> f64 {
    if entry <= liquidus && exit <= liquidus {
        return 0.0;
    }
    if entry >= liquidus && exit >= liquidus {
        return seconds;
    }
    let difference = (exit - entry).abs();
    if difference <= 1e-12 {
        return 0.0;
    }
    if exit > entry { seconds * (exit - liquidus) / difference } else { seconds * (entry - liquidus) / difference }
}

/// Counts of a rendered reflow time-temperature drawing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflowDrawingSummary {
    /// Unit system taken from the document.
    pub units: String,
    /// Unit of every temperature value.
    pub temperature_unit: String,
    /// Number of zones plotted.
    pub zones: u32,
    /// Total conveyor time of the profile, in seconds.
    pub total_seconds: f64,
    /// Highest set-point temperature, in degrees Celsius.
    pub peak_temperature: f64,
    /// Alloy liquidus, in degrees Celsius.
    pub liquidus: f64,
    /// Total time above the liquidus, in seconds.
    pub time_above_liquidus: f64,
    /// Number of non-ASCII characters written as `?`.
    pub dropped_characters: u32,
}

impl ReflowDrawingSummary {
    /// Returns the aggregated text losses of this drawing.
    pub fn losses(&self) -> Vec<DrawingLoss> {
        if self.dropped_characters == 0 {
            Vec::new()
        }
        else {
            vec![DrawingLoss { code: "reflow/non-ascii-text".to_string(), count: self.dropped_characters }]
        }
    }
}

/// One rendered reflow time-temperature drawing and its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct ReflowDrawing {
    /// Complete file contents.
    pub text: String,
    /// Counts and losses of the rendered drawing.
    pub summary: ReflowDrawingSummary,
}

fn title_text(document: &SmtDocument, report: &ReflowReport, id: &str, units: &str) -> Vec<drawing::TextLine> {
    let title = |text: String| drawing::TextLine { text, size: drawing::FONT_TITLE, emphasis: true };
    let body = |text: String| drawing::TextLine { text, size: drawing::FONT_BODY, emphasis: false };
    let tmin = report.zones.first().map_or(0.0, |zone| zone.entry_temperature);
    let tmax = report.peak_temperature;
    vec![
        title(format!("OpenSMT reflow profile drawing | drawing OSMT-RF-01 | revision {}", document.revision)),
        body(drawing::title_stamp("reflow profile", "OSMT-RF-01", id, units, "1:1 (time axis)", "n/a (process profile)")),
        body(format!(
            "linear-ramp model | total {} s | {} C to {} C",
            drawing::number(report.total_seconds),
            drawing::number(tmin),
            drawing::number(tmax)
        )),
        body("the curve enters each zone at the previous set-point and ramps to its own".to_string()),
        body("millimetres; 1 mm = 8 px".to_string()),
    ]
}

/// Plots the evaluated reflow profile as a millimetre time-temperature SVG.
///
/// The curve is the linear-ramp model of [`evaluate`]: it starts at
/// [`AMBIENT`] degrees and reaches each zone set-point at the zone exit. The
/// liquidus is drawn as a reference line and the time above it is annotated.
/// The result is deterministic and the document is never modified.
pub fn render_svg(document: &SmtDocument, id: &str) -> Result<ReflowDrawing, SmtError> {
    let report = evaluate(document)?;
    let (id, mut dropped) = drawing::ascii(id);
    let (units, count) = drawing::ascii(&document.units);
    dropped += count;

    let plot_x = drawing::DRAW_MARGIN_MM + 28.0;
    let plot_y = drawing::DRAW_MARGIN_MM + 18.0;
    let plot_w = 175.0;
    let plot_h = 110.0;
    let width = plot_x + plot_w + drawing::DRAW_MARGIN_MM;
    let title_y = plot_y + plot_h + 22.0;
    let laid_title = drawing::lay_out(&title_text(document, &report, &id, &units), width - 2.0 * drawing::DRAW_MARGIN_MM - 5.0);
    let title_h = drawing::block_height(&laid_title) + 6.0;
    let height = title_y + title_h + drawing::DRAW_MARGIN_MM;

    let total = report.total_seconds.max(1e-9);
    let tmin = AMBIENT;
    let tmax = report.peak_temperature.max(report.liquidus) + 20.0;
    let tx = |time: f64| plot_x + time / total * plot_w;
    let ty = |temperature: f64| plot_y + plot_h - (temperature - tmin) / (tmax - tmin) * plot_h;

    let mut svg = String::new();
    drawing::svg_begin(&mut svg, width, height);

    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"4.2\" font-weight=\"bold\" fill=\"#{}\">REFLOW TIME-TEMPERATURE PROFILE</text>\n",
        drawing::number(drawing::DRAW_MARGIN_MM),
        drawing::number(drawing::DRAW_MARGIN_MM + 4.0),
        drawing::hex(drawing::INK)
    ));
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.8\" fill=\"#{}\">{} zone(s) | conveyor {} mm/min | peak {} C | time above liquidus {} s</text>\n",
        drawing::number(drawing::DRAW_MARGIN_MM),
        drawing::number(drawing::DRAW_MARGIN_MM + 8.6),
        drawing::hex(drawing::MUTED),
        report.zones.len(),
        drawing::number(report.conveyor_speed),
        drawing::number(report.peak_temperature),
        drawing::number(report.time_above_liquidus)
    ));

    // Plot frame and axes.
    svg.push_str(&format!(
        "  <rect data-role=\"plot\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.3\"/>\n",
        drawing::number(plot_x),
        drawing::number(plot_y),
        drawing::number(plot_w),
        drawing::number(plot_h),
        drawing::hex(drawing::BOARD_FILL),
        drawing::hex(drawing::BOARD_EDGE)
    ));

    // Temperature gridlines and labels (ambient, liquidus, peak).
    for (temperature, label) in [(tmin, "AMBIENT"), (report.liquidus, "LIQUIDUS"), (report.peak_temperature, "PEAK")] {
        let y = ty(temperature);
        let dashed = if temperature == report.liquidus { " stroke-dasharray=\"2 1.4\"" } else { "" };
        svg.push_str(&format!(
            "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.25\"{}/>\n",
            drawing::number(plot_x),
            drawing::number(y),
            drawing::number(plot_x + plot_w),
            drawing::number(y),
            drawing::hex(drawing::RULE),
            dashed
        ));
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" text-anchor=\"end\" fill=\"#{}\">{} {:.0} C</text>\n",
            drawing::number(plot_x - 1.6),
            drawing::number(y + 0.6),
            drawing::hex(drawing::MUTED),
            label,
            temperature
        ));
    }
    // Vertical axis: unit and tick marks.
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" text-anchor=\"middle\" transform=\"rotate(-90 {} {})\" fill=\"#{}\">temperature (C)</text>\n",
        drawing::number(plot_x - 21.0),
        drawing::number(plot_y + plot_h / 2.0),
        drawing::number(plot_x - 21.0),
        drawing::number(plot_y + plot_h / 2.0),
        drawing::hex(drawing::MUTED)
    ));
    // Horizontal axis: four evenly spaced second ticks with values and unit.
    for tick in 0..=4 {
        let time = total * tick as f64 / 4.0;
        let x = tx(time);
        svg.push_str(&format!(
            "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.25\"/>\n",
            drawing::number(x),
            drawing::number(plot_y + plot_h),
            drawing::number(x),
            drawing::number(plot_y + plot_h + 1.6),
            drawing::hex(drawing::RULE)
        ));
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" text-anchor=\"middle\" fill=\"#{}\">{:.0} s</text>\n",
            drawing::number(x),
            drawing::number(plot_y + plot_h + 4.0),
            drawing::hex(drawing::MUTED),
            time
        ));
    }
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" text-anchor=\"middle\" fill=\"#{}\">time (s)</text>\n",
        drawing::number(plot_x + plot_w / 2.0),
        drawing::number(plot_y + plot_h + 19.0),
        drawing::hex(drawing::MUTED)
    ));

    // Zone boundaries and names along the time axis.
    let mut elapsed = 0.0;
    for zone in &report.zones {
        let (name, count) = drawing::ascii(&zone.name);
        dropped += count;
        let start = elapsed;
        elapsed += zone.seconds;
        if start > 0.0 {
            svg.push_str(&format!(
                "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{}\" stroke-width=\"0.25\" stroke-dasharray=\"1.6 1.6\"/>\n",
                drawing::number(tx(start)),
                drawing::number(plot_y),
                drawing::number(tx(start)),
                drawing::number(plot_y + plot_h),
                drawing::hex(drawing::RULE)
            ));
        }
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.4\" text-anchor=\"middle\" fill=\"#{}\">{}</text>\n",
            drawing::number(tx((start + elapsed) / 2.0)),
            drawing::number(plot_y + plot_h + 8.6),
            drawing::hex(drawing::MUTED),
            drawing::escape_xml(&name)
        ));
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"2.2\" text-anchor=\"middle\" fill=\"#{}\">{} s</text>\n",
            drawing::number(tx((start + elapsed) / 2.0)),
            drawing::number(plot_y + plot_h + 12.0),
            drawing::hex(drawing::MUTED),
            drawing::number(zone.seconds)
        ));
    }

    // The profile polyline.
    let mut points = vec![format!("{},{}", drawing::number(tx(0.0)), drawing::number(ty(AMBIENT)))];
    let mut elapsed = 0.0;
    for zone in &report.zones {
        elapsed += zone.seconds;
        points.push(format!("{},{}", drawing::number(tx(elapsed)), drawing::number(ty(zone.exit_temperature))));
    }
    svg.push_str(&format!(
        "  <polyline data-role=\"profile\" points=\"{}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"0.5\" stroke-linejoin=\"round\"/>\n",
        points.join(" "),
        drawing::hex(drawing::TOP_EDGE)
    ));

    // Peak marker.
    let peak_time = report
        .zones
        .iter()
        .scan(0.0, |elapsed, zone| {
            *elapsed += zone.seconds;
            Some((*elapsed, zone.exit_temperature))
        })
        .max_by(|left, right| left.1.partial_cmp(&right.1).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((0.0, report.peak_temperature));
    svg.push_str(&format!(
        "  <circle data-role=\"peak\" cx=\"{}\" cy=\"{}\" r=\"1.1\" fill=\"#{}\" stroke=\"#{}\" stroke-width=\"0.25\"/>\n",
        drawing::number(tx(peak_time.0)),
        drawing::number(ty(peak_time.1)),
        drawing::hex(drawing::BOTTOM_FILL),
        drawing::hex(drawing::BOTTOM_EDGE)
    ));

    // Title block.
    svg.push_str(&format!(
        "  <g id=\"title-block\" transform=\"translate({} {})\">\n    <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#{}\" stroke-width=\"0.45\"/>\n",
        drawing::number(drawing::DRAW_MARGIN_MM),
        drawing::number(title_y),
        drawing::number(width - 2.0 * drawing::DRAW_MARGIN_MM),
        drawing::number(title_h),
        drawing::hex(drawing::PANEL_EDGE)
    ));
    drawing::push_block_svg(&mut svg, 2.5, 3.0, &laid_title);
    svg.push_str("  </g>\n");

    drawing::svg_end(&mut svg);
    let summary = ReflowDrawingSummary {
        units,
        temperature_unit: report.temperature_unit.clone(),
        zones: report.zones.len() as u32,
        total_seconds: report.total_seconds,
        peak_temperature: report.peak_temperature,
        liquidus: report.liquidus,
        time_above_liquidus: report.time_above_liquidus,
        dropped_characters: dropped as u32,
    };
    Ok(ReflowDrawing { text: svg, summary })
}
