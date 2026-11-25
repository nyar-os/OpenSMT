//! Takt bottleneck heat map drawing: step dots, Pareto column and losses.

use std::fs;

use smt_model::{Board, Component, ExportFormat, Feeder, OpenSmt, SmtDocument, render_heatmap_svg};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![
        Component::new("R1", "0603", 10.0, 10.0),
        Component::new("R2", "0603", 20.0, 20.0),
        Component::new("C1", "0805", 30.0, 30.0),
    ];
    document.feeders = vec![Feeder::new("F1", "0603", 4.0, 100), Feeder::new("F2", "0805", 8.0, 50)];
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
fn the_svg_plots_one_heat_dot_per_step_and_a_pareto_column() {
    let drawing = render_heatmap_svg(&document(), "takt").expect("rendered");
    let svg = &drawing.text;

    assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(svg.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains(">TAKT BOTTLENECK HEAT MAP</text>"));
    assert_eq!(svg.matches("data-role=\"board\"").count(), 1);
    assert_eq!(svg.matches("data-step=").count(), 3);
    for step in 1..=3 {
        assert!(svg.contains(&format!("data-step=\"{step}\"")), "missing step {step}");
    }
    assert!(svg.contains(">PACKAGES</text>"));
    assert!(svg.contains("0603"));
    assert!(svg.contains("0805"));

    assert_eq!(drawing.summary.units, "mm");
    assert_eq!(drawing.summary.components, 3);
    assert_eq!(drawing.summary.packages, 2);
    assert!(drawing.summary.has_bottleneck);
    assert!(drawing.summary.total_seconds > 0.0);
    assert_eq!(drawing.summary.dropped_characters, 0);
}

#[test]
fn every_svg_fill_and_stroke_is_a_valid_hex_color() {
    let svg = render_heatmap_svg(&document(), "takt").expect("rendered").text;
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
    let first = render_heatmap_svg(&document(), "takt").expect("first");
    let second = render_heatmap_svg(&document(), "takt").expect("second");
    assert_eq!(first.text, second.text);
    assert_eq!(first.summary, second.summary);
}

#[test]
fn non_ascii_text_is_dropped_and_counted() {
    let drawing = render_heatmap_svg(&document(), "热力图").expect("rendered");
    assert!(drawing.text.is_ascii());
    assert!(drawing.summary.dropped_characters >= 3);
    let losses = drawing.summary.losses();
    assert_eq!(losses[0].code, "heatmap/non-ascii-text");
    assert_eq!(losses[0].count, drawing.summary.dropped_characters);
}

#[test]
fn a_missing_feeder_reports_the_allocation_code() {
    let mut document = document();
    document.feeders.retain(|feeder| feeder.id != "F2");
    let error = render_heatmap_svg(&document, "takt").expect_err("a used package without a lane cannot be drawn");
    assert_eq!(error.code, "feeder/missing");
}

#[test]
fn the_export_format_accepts_heatmap_svg() {
    assert_eq!("heatmap-svg".parse::<ExportFormat>().expect("heatmap-svg"), ExportFormat::HeatmapSvg);
    assert_eq!(ExportFormat::HeatmapSvg.as_str(), "heatmap-svg");
}

#[test]
fn the_session_writes_the_heat_map_sheet_through_the_export_format() {
    let directory = std::env::temp_dir().join(format!("open-smt-heatmap-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("the temp directory exists");
    let mut session = OpenSmt::new();
    session.create(&directory.join("job.osmt")).expect("a document is created");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    session.add_component(Component::new("C1", "0805", 30.0, 30.0)).expect("C1");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 100)).expect("F1");
    session.add_feeder(Feeder::new("F2", "0805", 8.0, 50)).expect("F2");
    let report = session.export(ExportFormat::HeatmapSvg, &directory.join("job.heatmap-svg")).expect("exported");
    assert_eq!(report.format, "heatmap-svg");
    let text = fs::read_to_string(&report.files[0]).expect("the sheet is written");
    assert!(text.contains(">TAKT BOTTLENECK HEAT MAP</text>"));
    assert_eq!(text.matches("data-step=").count(), 2);
    assert!(report.losses.is_empty());
    let _ = fs::remove_dir_all(&directory);
}
