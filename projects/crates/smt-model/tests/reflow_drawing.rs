//! Reflow time-temperature drawing: zone ramps, liquidus line and losses.

use std::fs;

use smt_model::{Board, Component, ExportFormat, Feeder, OpenSmt, SmtDocument, render_reflow_svg};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![Component::new("R1", "0603", 10.0, 10.0)];
    document.feeders = vec![Feeder::new("F1", "0603", 4.0, 100)];
    document
}

/// Collects every `fill="..."` and `stroke="..."` value of the sheet.
fn color_attributes(svg: &str) -> Vec<String> {
    let mut values = Vec::new();
    for attribute in ["fill=\"", "stroke=\""] {
        let mut rest = svg;
        while let Some(index) = rest.find(attribute) {
            let after = &rest[index + attribute.len()..];
            let end = after.find('"').expect("closing quote");
            values.push(after[..end].to_string());
            rest = &after[end..];
        }
    }
    values
}

#[test]
fn the_svg_plots_the_default_four_zone_profile() {
    let drawing = render_reflow_svg(&document(), "profile").expect("rendered");
    let svg = &drawing.text;

    assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(svg.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains(">REFLOW TIME-TEMPERATURE PROFILE</text>"));
    assert_eq!(svg.matches("data-role=\"plot\"").count(), 1);
    assert_eq!(svg.matches("data-role=\"profile\"").count(), 1);
    assert_eq!(svg.matches("data-role=\"peak\"").count(), 1);
    // The liquidus is the only dashed temperature reference line.
    assert!(svg.contains("LIQUIDUS 217 C"));
    assert!(svg.contains("PEAK 245 C"));
    assert!(svg.contains("AMBIENT 25 C"));
    assert!(svg.contains(">temperature (C)</text>"));
    assert!(svg.contains(">time (s)</text>"));
    for name in ["preheat", "soak", "reflow", "cool"] {
        assert!(svg.contains(&format!(">{name}</text>")), "missing zone name `{name}`");
    }

    assert_eq!(drawing.summary.units, "mm");
    assert_eq!(drawing.summary.temperature_unit, "celsius");
    assert_eq!(drawing.summary.zones, 4);
    assert!((drawing.summary.total_seconds - 140.0).abs() < 1e-9);
    assert!((drawing.summary.peak_temperature - 245.0).abs() < 1e-9);
    assert!((drawing.summary.liquidus - 217.0).abs() < 1e-9);
    assert!(drawing.summary.time_above_liquidus > 0.0);
    assert_eq!(drawing.summary.dropped_characters, 0);
}

#[test]
fn every_svg_fill_and_stroke_is_a_valid_hex_color() {
    let svg = render_reflow_svg(&document(), "profile").expect("rendered").text;
    let colors = color_attributes(&svg);
    assert!(!colors.is_empty());
    for color in &colors {
        assert!(color == "none" || color.starts_with('#'), "missing # prefix in `{color}`");
        if color == "none" {
            continue;
        }
        let digits = &color[1..];
        assert_eq!(digits.len(), 6, "invalid color `{color}`");
        assert!(digits.chars().all(|character| character.is_ascii_hexdigit()), "invalid color `{color}`");
    }
}

#[test]
fn two_renders_are_byte_identical() {
    let first = render_reflow_svg(&document(), "profile").expect("first");
    let second = render_reflow_svg(&document(), "profile").expect("second");
    assert_eq!(first.text, second.text);
    assert_eq!(first.summary, second.summary);
}

#[test]
fn non_ascii_text_is_dropped_and_counted() {
    let mut document = document();
    document.profile.zones[0].name = "预热".to_string();
    let drawing = render_reflow_svg(&document, "profile").expect("rendered");
    assert!(drawing.text.is_ascii());
    assert!(drawing.summary.dropped_characters >= 2);
    let losses = drawing.summary.losses();
    assert_eq!(losses[0].code, "reflow/non-ascii-text");
    assert_eq!(losses[0].count, drawing.summary.dropped_characters);
}

#[test]
fn the_export_format_accepts_reflow_svg() {
    assert_eq!("reflow-svg".parse::<ExportFormat>().expect("reflow-svg"), ExportFormat::ReflowSvg);
    assert_eq!(ExportFormat::ReflowSvg.as_str(), "reflow-svg");
}

#[test]
fn the_session_writes_the_reflow_sheet_through_the_export_format() {
    let directory = std::env::temp_dir().join(format!("open-smt-reflow-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("the temp directory exists");
    let mut session = OpenSmt::new();
    session.create(&directory.join("job.osmt")).expect("a document is created");
    let report = session.export(ExportFormat::ReflowSvg, &directory.join("job.reflow-svg")).expect("exported");
    assert_eq!(report.format, "reflow-svg");
    let text = fs::read_to_string(&report.files[0]).expect("the sheet is written");
    assert!(text.contains(">REFLOW TIME-TEMPERATURE PROFILE</text>"));
    assert!(text.contains("data-role=\"profile\""));
    assert!(report.losses.is_empty());
    let _ = fs::remove_dir_all(&directory);
}
