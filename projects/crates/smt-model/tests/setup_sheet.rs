//! Nozzle and feeder setup table: columns, pagination and losses.

use std::fs;

use smt_model::{
    Board, Component, ExportFormat, Feeder, OpenSmt, SETUP_ROWS_PER_PAGE, SmtDocument, render_setup_pdf, render_setup_svg,
};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![
        Component::new("R1", "0603", 10.0, 10.0),
        Component::new("C1", "0805", 30.0, 30.0),
        Component::new("U1", "SOIC-8", 60.0, 40.0),
    ];
    document.feeders =
        vec![Feeder::new("F1", "0603", 4.0, 100), Feeder::new("F2", "0805", 8.0, 50), Feeder::new("F3", "SOIC-8", 4.0, 50)];
    document
}

/// A document with one distinct chip lane per row, enough to paginate.
fn long_document(rows: usize) -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    for index in 0..rows {
        let package = format!("0603-{index:02}");
        document.components.push(Component::new(format!("R{index}"), &package, 5.0 + index as f64, 10.0));
        document.feeders.push(Feeder::new(format!("F{:02}", index + 1), &package, 4.0, 100));
    }
    document
}

#[test]
fn the_svg_has_aligned_columns_with_slot_nozzle_quantity_and_tape_width() {
    let drawing = render_setup_svg(&document(), "setup").expect("rendered");
    let svg = &drawing.text;

    assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(svg.contains(">SMT SETUP SHEET</text>"));
    for header in ["SLOT", "LANE", "PART NUMBER", "NOZZLE", "TAPE MM", "QTY", "ASSIGNED", "REFERENCE DESIGNATORS"] {
        assert!(svg.contains(&format!(">{header}</text>")), "missing column `{header}`");
    }
    assert!(svg.contains(">01</text>"));
    assert!(svg.contains(">F1</text>"));
    assert!(svg.contains(">0603</text>"));
    assert!(svg.contains(">N-0603</text>"));
    assert!(svg.contains(">N-SOIC</text>"));
    assert!(svg.contains(">12</text>"));
    assert!(svg.contains(">100/100</text>"));
    assert!(svg.contains("drawing OSMT-SS-01"));
    assert!(svg.contains("page 1 of 1"));
    assert!(svg.contains("work order setup"));

    assert_eq!(drawing.summary.units, "mm");
    assert_eq!(drawing.summary.rows, 3);
    assert_eq!(drawing.summary.pages, 1);
    assert_eq!(drawing.summary.unassigned_components, 0);
    assert_eq!(drawing.summary.dropped_characters, 0);
}

#[test]
fn a_long_setup_continues_on_a_second_page() {
    let document = long_document(SETUP_ROWS_PER_PAGE + 3);
    let svg = render_setup_svg(&document, "setup").expect("rendered");
    assert_eq!(svg.summary.rows, (SETUP_ROWS_PER_PAGE + 3) as u32);
    assert_eq!(svg.summary.pages, 2);
    assert_eq!(svg.text.matches(">SMT SETUP SHEET</text>").count(), 2);
    assert!(svg.text.contains("page 1 of 2"));
    assert!(svg.text.contains("page 2 of 2"));
    // The slot counter continues across the page break.
    assert!(svg.text.contains(&format!("{:02}", SETUP_ROWS_PER_PAGE + 1)));

    let pdf = render_setup_pdf(&document, "setup").expect("rendered");
    assert!(pdf.text.contains("/Count 2"));
    assert_eq!(pdf.text.matches("(SMT SETUP SHEET) Tj").count(), 2);
    assert!(pdf.text.contains("page 2 of 2) Tj"));
}

#[test]
fn two_renders_are_byte_identical() {
    let first = render_setup_svg(&document(), "setup").expect("first");
    let second = render_setup_svg(&document(), "setup").expect("second");
    assert_eq!(first.text, second.text);
    assert_eq!(first.summary, second.summary);
}

#[test]
fn non_ascii_text_is_dropped_and_counted() {
    let mut document = document();
    document.feeders[0].id = "料架A".to_string();
    let drawing = render_setup_svg(&document, "setup").expect("rendered");
    assert!(drawing.text.is_ascii());
    assert!(drawing.summary.dropped_characters >= 2);
    let losses = drawing.summary.losses();
    assert_eq!(losses[0].code, "setup/non-ascii-text");
    assert_eq!(losses[0].count, drawing.summary.dropped_characters);
}

#[test]
fn a_missing_feeder_reports_the_allocation_code() {
    let mut document = document();
    document.feeders.retain(|feeder| feeder.id != "F3");
    let error = render_setup_svg(&document, "setup").expect_err("an unused package without a lane cannot be drawn");
    assert_eq!(error.code, "feeder/missing");
}

#[test]
fn the_export_formats_accept_the_setup_sheet() {
    assert_eq!("setup-svg".parse::<ExportFormat>().expect("setup-svg"), ExportFormat::SetupSvg);
    assert_eq!("setup-pdf".parse::<ExportFormat>().expect("setup-pdf"), ExportFormat::SetupPdf);
    assert_eq!(ExportFormat::SetupSvg.as_str(), "setup-svg");
    assert_eq!(ExportFormat::SetupPdf.as_str(), "setup-pdf");
}

#[test]
fn the_session_writes_both_setup_sheet_formats() {
    let directory = std::env::temp_dir().join(format!("open-smt-setup-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("the temp directory exists");
    let mut session = OpenSmt::new();
    session.create(&directory.join("job.osmt")).expect("a document is created");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 100)).expect("F1");

    let svg = session.export(ExportFormat::SetupSvg, &directory.join("job.setup-svg")).expect("exported");
    assert_eq!(svg.format, "setup-svg");
    assert!(fs::read_to_string(&svg.files[0]).expect("the sheet is written").contains(">SMT SETUP SHEET</text>"));

    let pdf = session.export(ExportFormat::SetupPdf, &directory.join("job.setup-pdf")).expect("exported");
    assert_eq!(pdf.format, "setup-pdf");
    assert!(fs::read_to_string(&pdf.files[0]).expect("the sheet is written").starts_with("%PDF-1.4"));
    let _ = fs::remove_dir_all(&directory);
}
