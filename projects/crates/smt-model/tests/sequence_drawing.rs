//! Pick-and-place sequence drawing: nozzle routes, step numbers and losses.

use std::fs;

use smt_model::{
    Board, Component, ExportFormat, Feeder, OpenSmt, SequenceDrawingSummary, Side, SmtDocument, render_sequence_pdf,
    render_sequence_svg,
};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(120.0, 80.0, 1.6));
    document.components = vec![
        Component::new("R1", "0603", 10.0, 10.0),
        Component::new("R2", "0603", 60.0, 20.0),
        Component::new("C1", "0805", 100.0, 50.0),
        Component::new("U1", "SOP8", 40.0, 60.0),
    ];
    document.components[3].side = Side::Bottom;
    document.feeders =
        vec![Feeder::new("F1", "0603", 4.0, 100), Feeder::new("F2", "0805", 8.0, 50), Feeder::new("F3", "SOP8", 12.0, 30)];
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
fn the_svg_draws_one_route_per_side_with_step_numbers() {
    let drawing = render_sequence_svg(&document(), "seq").expect("rendered");
    let svg = &drawing.text;

    assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(svg.contains(">PICK AND PLACE SEQUENCE</text>"));
    assert!(svg.contains("data-route=\"top\""));
    assert!(svg.contains("data-route=\"bottom\""));
    assert!(svg.contains("data-step=\"1\""));
    assert!(svg.contains("data-step=\"4\""));
    assert!(svg.contains("ORIGIN"));
    // The route visits every placement in program order.
    assert_eq!(svg.matches("<polyline").count(), 2);
    assert_eq!(svg.matches("data-step=\"").count(), 4);

    let summary = &drawing.summary;
    assert_eq!(summary.units, "mm");
    assert_eq!(summary.steps, 4);
    assert_eq!(summary.top_steps, 3);
    assert_eq!(summary.bottom_steps, 1);
    assert_eq!(summary.board_width, 120.0);
    assert_eq!(summary.board_depth, 80.0);
    assert!(summary.travel_length > 0.0);
    assert_eq!(summary.dropped_characters, 0);
}

#[test]
fn the_travel_length_matches_the_drawn_segments() {
    let drawing = render_sequence_svg(&document(), "seq").expect("rendered");
    let SequenceDrawingSummary { travel_length, .. } = drawing.summary;
    // The nearest-neighbour order is R1 (10,10), R2 (60,20), C1 (100,50) on top
    // and U1 (40,60) on the bottom, each route starting at the origin.
    let top = 10.0_f64.hypot(10.0) + 50.0_f64.hypot(10.0) + 40.0_f64.hypot(30.0);
    let bottom = 40.0_f64.hypot(60.0);
    assert!((travel_length - (top + bottom)).abs() < 1e-6, "{travel_length} != {}", top + bottom);
}

#[test]
fn every_svg_fill_and_stroke_is_a_valid_hex_color() {
    let svg = render_sequence_svg(&document(), "seq").expect("rendered").text;
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
fn the_pdf_is_one_a4_landscape_page_with_helvetica_and_the_routes() {
    let drawing = render_sequence_pdf(&document(), "seq").expect("rendered");
    let pdf = &drawing.text;

    assert!(pdf.starts_with("%PDF-1.4\n"));
    assert!(pdf.ends_with("%%EOF\n"));
    assert!(pdf.contains("/Type /Page /Parent 2 0 R"));
    assert!(pdf.contains("/Type /Pages"));
    assert!(pdf.contains("/Count 1"));
    assert!(pdf.contains("/MediaBox [0 0 841.89 595.28]"));
    assert!(pdf.contains("/BaseFont /Helvetica"));
    assert!(pdf.contains("(1 R1)"));
    assert!(pdf.contains("(4 U1)"));
    assert!(pdf.contains("(PICK AND PLACE SEQUENCE)"));
    assert_eq!(drawing.summary, render_sequence_svg(&document(), "seq").expect("rendered").summary);
}

#[test]
fn an_empty_program_still_produces_a_framed_sheet() {
    let document = SmtDocument::new(Board::new(120.0, 80.0, 1.6));
    let drawing = render_sequence_svg(&document, "empty").expect("rendered");
    assert!(drawing.text.contains("no placements in the program"));
    assert!(!drawing.text.contains("data-route="));
    assert_eq!(drawing.summary.steps, 0);
    assert_eq!(drawing.summary.travel_length, 0.0);
}

#[test]
fn a_missing_feeder_reports_the_allocation_code() {
    let mut document = document();
    document.feeders.retain(|feeder| feeder.id != "F3");
    let error = render_sequence_svg(&document, "seq").expect_err("U1 has no lane");
    assert_eq!(error.code, "feeder/missing");
}

#[test]
fn two_renders_are_byte_identical() {
    let first = render_sequence_svg(&document(), "seq").expect("first");
    let second = render_sequence_svg(&document(), "seq").expect("second");
    assert_eq!(first.text, second.text);
    assert_eq!(first.summary, second.summary);
    let first_pdf = render_sequence_pdf(&document(), "seq").expect("first");
    let second_pdf = render_sequence_pdf(&document(), "seq").expect("second");
    assert_eq!(first_pdf.text, second_pdf.text);
}

#[test]
fn non_ascii_text_is_dropped_and_counted() {
    let mut document = document();
    document.components[0].refdes = "电阻1".to_string();
    let drawing = render_sequence_svg(&document, "seq").expect("rendered");
    assert!(drawing.text.is_ascii());
    assert!(drawing.summary.dropped_characters >= 2);
    let losses = drawing.summary.losses();
    assert_eq!(losses[0].code, "sequence/non-ascii-text");
    assert_eq!(losses[0].count, drawing.summary.dropped_characters);
}

#[test]
fn the_pdf_draws_the_origin_cross_like_the_svg() {
    let drawing = render_sequence_pdf(&document(), "seq").expect("rendered");
    let pdf = &drawing.text;
    assert!(pdf.contains("(ORIGIN \\(0,0\\)) Tj"));
    // Four INK-coloured strokes: the two ORIGIN cross segments, the sheet
    // frame and the title-block frame. The SVG cross is a path; the PDF now
    // matches it with two line operators.
    assert_eq!(pdf.matches("0.06 0.09 0.16 RG ").count(), 4);
    let svg = render_sequence_svg(&document(), "seq").expect("rendered").text;
    assert!(svg.contains("ORIGIN"));
}

#[test]
fn a_panel_document_marks_the_routes_as_per_board() {
    let mut panel_document = document();
    panel_document.panel = smt_model::Panel::new(2, 2, 5.0, 5.0);
    let drawing = render_sequence_svg(&panel_document, "seq").expect("rendered");
    assert!(drawing.text.contains("per board (x4 on the panel)"));
    assert!(drawing.text.contains("4 step(s) | top 3 | bottom 1"));

    let pdf = render_sequence_pdf(&panel_document, "seq").expect("rendered");
    assert!(pdf.text.contains("per board \\(x4 on the panel\\)"));

    // A single-board sheet carries no multiplier.
    let single = render_sequence_svg(&document(), "seq").expect("rendered");
    assert!(!single.text.contains("per board"));
    assert!(single.text.contains("routes in program order, one route per side"));
    assert!(!single.text.contains("nozzle"));
}

#[test]
fn the_export_formats_accept_the_sequence_sheets() {
    assert_eq!("sequence-svg".parse::<ExportFormat>().expect("sequence-svg"), ExportFormat::SequenceSvg);
    assert_eq!("sequence-pdf".parse::<ExportFormat>().expect("sequence-pdf"), ExportFormat::SequencePdf);
    assert_eq!(ExportFormat::SequenceSvg.as_str(), "sequence-svg");
    assert_eq!(ExportFormat::SequencePdf.as_str(), "sequence-pdf");
}

#[test]
fn the_session_writes_the_sequence_sheet_through_the_export_format() {
    let directory = std::env::temp_dir().join(format!("open-smt-seq-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("the temp directory exists");
    let mut session = OpenSmt::new();
    session.create(&directory.join("seq.osmt")).expect("a document is created");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    session.add_component(Component::new("R2", "0603", 60.0, 20.0)).expect("R2");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 100)).expect("F1");
    let report = session.export(ExportFormat::SequenceSvg, &directory.join("seq.sequence-svg")).expect("exported");
    assert_eq!(report.format, "sequence-svg");
    let text = fs::read_to_string(&report.files[0]).expect("the sheet is written");
    assert!(text.contains("data-step=\"1\""));
    assert!(text.contains("data-step=\"2\""));
    assert!(report.losses.is_empty());
    let _ = fs::remove_dir_all(&directory);
}

/// Collects the `(x, y)` anchors of the small step labels (`font-size` 1.7).
fn label_anchors(svg: &str) -> Vec<(String, String)> {
    let mut anchors = Vec::new();
    let mut rest = svg;
    while let Some(position) = rest.find("<text x=\"") {
        let after = &rest[position + "<text x=\"".len()..];
        let x = &after[..after.find('"').expect("x quote")];
        let y_after = after.split_once("y=\"").expect("y attribute").1;
        let y = &y_after[..y_after.find('"').expect("y quote")];
        let element = &after[..after.find("</text>").unwrap_or(after.len())];
        if element.contains("font-size=\"2.2\"") {
            anchors.push((x.to_string(), y.to_string()));
        }
        rest = &after[x.len()..];
    }
    anchors
}

#[test]
fn dense_step_labels_are_moved_apart_instead_of_overlapping() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![
        Component::new("R1", "0603", 30.0, 40.0),
        Component::new("R2", "0603", 30.4, 40.0),
        Component::new("R3", "0603", 30.8, 40.0),
    ];
    document.feeders = vec![Feeder::new("F1", "0603", 4.0, 100)];
    let svg = render_sequence_svg(&document, "panel").expect("rendered").text;

    // Every marker is still drawn.
    assert_eq!(svg.matches("data-step=").count(), 3);
    // The labels are placed at distinct anchors, so they cannot overlap each
    // other the way the fixed offset did.
    let anchors = label_anchors(&svg);
    assert!(anchors.len() >= 2, "dense labels are still drawn");
    let distinct: std::collections::HashSet<(String, String)> = anchors.iter().cloned().collect();
    assert_eq!(distinct.len(), anchors.len(), "no two step labels share an anchor");
    assert!(anchors.iter().any(|(_, y)| y != "58.600"), "a colliding label is moved off the default row");
}
