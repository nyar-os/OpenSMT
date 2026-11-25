//! Line-balance bar chart: station bars, bottleneck highlight and target line.

use smt_model::{Board, Component, ExportFormat, Feeder, LINE_BALANCE_MACHINES, SmtDocument, render_line_balance_svg};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![
        Component::new("R1", "0603", 10.0, 10.0),
        Component::new("R2", "0603", 20.0, 20.0),
        Component::new("C1", "0805", 30.0, 30.0),
        Component::new("U1", "SOIC-8", 60.0, 40.0),
    ];
    document.feeders =
        vec![Feeder::new("F1", "0603", 4.0, 100), Feeder::new("F2", "0805", 8.0, 50), Feeder::new("F3", "SOIC-8", 4.0, 50)];
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
fn the_svg_draws_one_bar_per_station_with_a_bottleneck_and_a_target_line() {
    let drawing = render_line_balance_svg(&document(), "line").expect("rendered");
    let svg = &drawing.text;

    assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(svg.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains(">LINE BALANCE</text>"));
    assert_eq!(svg.matches("data-machine=").count(), 3);
    for machine in 1..=3 {
        assert!(svg.contains(&format!("data-machine=\"{machine}\"")), "missing station {machine}");
    }
    assert_eq!(svg.matches("data-target=\"true\"").count(), 1);
    assert!(svg.contains(">cycle time (s)</text>"));
    assert!(svg.contains(">bottleneck</text>"));
    assert!(svg.contains(">average target</text>"));
    assert!(svg.contains("drawing OSMT-LB-01"));
    assert!(svg.contains("work order line"));
    assert!(svg.contains("face n/a (line)"));

    assert_eq!(drawing.summary.units, "mm");
    assert_eq!(drawing.summary.machines, LINE_BALANCE_MACHINES);
    assert!(drawing.summary.max_seconds > 0.0);
    assert!(drawing.summary.total_seconds >= drawing.summary.max_seconds);
    assert!((0.0..1.0).contains(&drawing.summary.imbalance));
    assert_eq!(drawing.summary.dropped_characters, 0);
}

#[test]
fn every_svg_fill_and_stroke_is_a_valid_hex_color() {
    let svg = render_line_balance_svg(&document(), "line").expect("rendered").text;
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
fn two_renders_are_byte_identical_and_leave_the_document_unchanged() {
    let document = document();
    let before = document.clone();
    let first = render_line_balance_svg(&document, "line").expect("first");
    let second = render_line_balance_svg(&document, "line").expect("second");
    assert_eq!(first.text, second.text);
    assert_eq!(first.summary, second.summary);
    assert_eq!(document, before);
}

#[test]
fn non_ascii_text_is_dropped_and_counted() {
    let drawing = render_line_balance_svg(&document(), "线平衡").expect("rendered");
    assert!(drawing.text.is_ascii());
    assert!(drawing.summary.dropped_characters >= 3);
    let losses = drawing.summary.losses();
    assert_eq!(losses[0].code, "balance/non-ascii-text");
    assert_eq!(losses[0].count, drawing.summary.dropped_characters);
}

#[test]
fn an_empty_document_still_draws_a_framed_chart() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let drawing = render_line_balance_svg(&document, "empty").expect("rendered");
    assert!(drawing.text.contains("id=\"frame\""));
    assert!(drawing.text.contains(">LINE BALANCE</text>"));
    assert_eq!(drawing.summary.machines, LINE_BALANCE_MACHINES);
    assert_eq!(drawing.summary.max_seconds, 0.0);
}

#[test]
fn the_export_format_accepts_balance_svg() {
    assert_eq!("balance-svg".parse::<ExportFormat>().expect("balance-svg"), ExportFormat::BalanceSvg);
    assert_eq!("line-balance-svg".parse::<ExportFormat>().expect("line-balance-svg"), ExportFormat::BalanceSvg);
    assert_eq!(ExportFormat::BalanceSvg.as_str(), "balance-svg");
}
