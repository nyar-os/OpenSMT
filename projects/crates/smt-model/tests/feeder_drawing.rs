//! Feeder setup drawing: lane slots, load bars, component mapping and losses.

use std::fs;

use smt_model::{Board, Component, ExportFormat, Feeder, OpenSmt, SmtDocument, render_feeder_svg};

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
fn the_svg_lists_lanes_in_id_order_with_slots_load_bars_and_refdes() {
    let drawing = render_feeder_svg(&document(), "setup").expect("rendered");
    let svg = &drawing.text;

    assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(svg.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains("data-lane=\"F1\""));
    assert!(svg.contains("data-lane=\"F2\""));
    assert!(svg.contains(">FEEDER SETUP</text>"));
    // The allocation maps both chips to F1 and the 0805 to F2.
    assert!(svg.contains(">R1, R2</text>"));
    assert!(svg.contains(">C1</text>"));
    // Lanes run in id order.
    let first = svg.find("data-lane=\"F1\"").expect("F1");
    let second = svg.find("data-lane=\"F2\"").expect("F2");
    assert!(first < second);
    // The load bar shows the loaded share of the capacity.
    assert_eq!(svg.matches("data-load-bar=\"true\"").count(), 2);

    assert_eq!(drawing.summary.units, "mm");
    assert_eq!(drawing.summary.lanes, 2);
    assert_eq!(drawing.summary.mapped_components, 3);
    assert_eq!(drawing.summary.unassigned_components, 0);
    assert_eq!(drawing.summary.dropped_characters, 0);
}

#[test]
fn every_svg_fill_and_stroke_is_a_valid_hex_color() {
    let svg = render_feeder_svg(&document(), "setup").expect("rendered").text;
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
fn a_missing_feeder_reports_the_allocation_code() {
    let mut document = document();
    document.feeders.retain(|feeder| feeder.id != "F2");
    let error = render_feeder_svg(&document, "setup").expect_err("a used package without a lane cannot be drawn");
    assert_eq!(error.code, "feeder/missing");
}

#[test]
fn an_empty_setup_still_produces_a_framed_sheet() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let drawing = render_feeder_svg(&document, "empty").expect("rendered");
    assert!(drawing.text.contains("no feeder lanes declared"));
    assert!(drawing.text.contains(">FEEDER SETUP</text>"));
    assert_eq!(drawing.summary.lanes, 0);
    assert_eq!(drawing.summary.mapped_components, 0);
}

#[test]
fn two_renders_are_byte_identical() {
    let first = render_feeder_svg(&document(), "setup").expect("first");
    let second = render_feeder_svg(&document(), "setup").expect("second");
    assert_eq!(first.text, second.text);
    assert_eq!(first.summary, second.summary);
}

#[test]
fn non_ascii_text_is_dropped_and_counted() {
    let mut document = document();
    document.feeders[0].id = "料架A".to_string();
    let drawing = render_feeder_svg(&document, "setup").expect("rendered");
    assert!(drawing.text.is_ascii());
    assert!(drawing.summary.dropped_characters >= 2);
    let losses = drawing.summary.losses();
    assert_eq!(losses[0].code, "feeder/non-ascii-text");
    assert_eq!(losses[0].count, drawing.summary.dropped_characters);
}

#[test]
fn the_title_has_one_scale_stamp_and_annotates_the_panel_multiplier() {
    let single = render_feeder_svg(&document(), "setup").expect("rendered").text;
    // The title block carries exactly one scale entry.
    assert_eq!(single.matches("scale 1:1").count(), 1);
    // A single board carries no multiplier note.
    assert!(!single.contains("on the panel"));

    // A 2x3 panel annotates the per-board allocation with its multiplier.
    let mut panelled = document();
    panelled.panel = smt_model::Panel::new(2, 3, 4.0, 4.0);
    let panel = render_feeder_svg(&panelled, "setup").expect("rendered").text;
    assert!(panel.contains("per board (x6 on the panel)"), "panel multiplier must be annotated");
}

#[test]
fn the_export_format_accepts_feeder_svg() {
    assert_eq!("feeder-svg".parse::<ExportFormat>().expect("feeder-svg"), ExportFormat::FeederSvg);
    assert_eq!(ExportFormat::FeederSvg.as_str(), "feeder-svg");
}

#[test]
fn the_session_writes_the_feeder_sheet_through_the_export_format() {
    let directory = std::env::temp_dir().join(format!("open-smt-feeder-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("the temp directory exists");
    let mut session = OpenSmt::new();
    session.create(&directory.join("setup.osmt")).expect("a document is created");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    session.add_component(Component::new("C1", "0805", 30.0, 30.0)).expect("C1");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 100)).expect("F1");
    session.add_feeder(Feeder::new("F2", "0805", 8.0, 50)).expect("F2");
    let report = session.export(ExportFormat::FeederSvg, &directory.join("setup.feeder-svg")).expect("exported");
    assert_eq!(report.format, "feeder-svg");
    let text = fs::read_to_string(&report.files[0]).expect("the sheet is written");
    assert!(text.contains("data-lane=\"F1\""));
    assert!(text.contains("data-lane=\"F2\""));
    assert!(text.contains(">R1</text>"));
    assert!(text.contains(">C1</text>"));
    assert!(report.losses.is_empty());
    let _ = fs::remove_dir_all(&directory);
}
