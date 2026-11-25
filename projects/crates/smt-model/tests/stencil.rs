//! Stencil aperture generation: rules, geometry, reports, sheets and errors.

use std::fs;

use smt_model::{
    Board, Component, ExportFormat, OpenSmt, Side, SmtDocument, render_stencil_pdf, render_stencil_svg, stencil_report,
};

fn board_document() -> SmtDocument {
    SmtDocument::new(Board::new(100.0, 80.0, 1.6))
}

fn add(document: &mut SmtDocument, refdes: &str, package: &str, x: f64, y: f64, rotation: f64) {
    let mut component = Component::new(refdes, package, x, y);
    component.rotation = rotation;
    document.components.push(component);
}

#[test]
fn chip_rules_generate_two_reduced_apertures_at_the_terminal_offsets() {
    let mut document = board_document();
    add(&mut document, "R1", "0603", 10.0, 10.0, 0.0);

    let report = stencil_report(&document);
    assert_eq!(report.format, "open-smt-stencil");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!(report.board_width, 100.0);
    assert_eq!(report.board_depth, 80.0);
    assert_eq!(report.components, 1);
    assert_eq!(report.apertures, 2);
    assert!(report.unmatched_packages.is_empty());

    let rule = &report.rules[0];
    assert_eq!((rule.package.as_str(), rule.family.as_str(), rule.shape.as_str()), ("0603", "chip", "rect"));
    assert_eq!((rule.pad_width, rule.pad_height, rule.area_reduction), (0.65, 0.80, 0.90));
    assert_eq!((rule.aperture_width, rule.aperture_height), (0.617, 0.759));
    assert_eq!(rule.apertures_per_component, 2);

    let expected: Vec<(&str, f64, f64, u32)> = vec![("R1", 9.525, 10.0, 1), ("R1", 10.475, 10.0, 2)];
    let actual: Vec<(&str, f64, f64, u32)> =
        report.entries.iter().map(|entry| (entry.refdes.as_str(), entry.x, entry.y, entry.index)).collect();
    assert_eq!(actual, expected);
    for entry in &report.entries {
        assert_eq!((entry.width, entry.height, entry.rotation), (0.617, 0.759, 0.0));
        assert_eq!(entry.side, Side::Top);
    }
}

#[test]
fn rotated_components_rotate_the_aperture_offsets() {
    let mut document = board_document();
    add(&mut document, "R1", "0603", 10.0, 10.0, 90.0);

    let report = stencil_report(&document);
    let actual: Vec<(f64, f64, f64)> = report.entries.iter().map(|entry| (entry.x, entry.y, entry.rotation)).collect();
    assert_eq!(actual, vec![(10.0, 9.525, 90.0), (10.0, 10.475, 90.0)]);
}

#[test]
fn lead_families_generate_one_aperture_per_terminal() {
    let mut document = board_document();
    add(&mut document, "U1", "SOIC-8", 20.0, 20.0, 0.0);
    add(&mut document, "U2", "TQFP-44", 50.0, 40.0, 0.0);
    add(&mut document, "U3", "QFN-32", 80.0, 20.0, 0.0);
    add(&mut document, "Q1", "SOT-23", 20.0, 60.0, 0.0);
    add(&mut document, "U4", "LGA-16", 50.0, 70.0, 0.0);

    let report = stencil_report(&document);
    let per_package: Vec<(&str, &str, u32)> =
        report.rules.iter().map(|rule| (rule.package.as_str(), rule.family.as_str(), rule.apertures_per_component)).collect();
    assert_eq!(
        per_package,
        vec![
            ("SOIC-8", "soic", 8),
            ("TQFP-44", "qfp", 44),
            ("QFN-32", "qfn", 32),
            ("SOT-23", "sot23", 3),
            ("LGA-16", "lga", 16)
        ]
    );
    assert_eq!(report.apertures, 8 + 44 + 32 + 3 + 16);
    assert_eq!(report.entries.iter().filter(|entry| entry.shape == "circle").count(), 16);

    let soic: Vec<(f64, f64)> =
        report.entries.iter().filter(|entry| entry.package == "SOIC-8").map(|entry| (entry.x, entry.y)).collect();
    assert_eq!(
        soic,
        vec![
            (17.3, 18.095),
            (17.3, 19.365),
            (17.3, 20.635),
            (17.3, 21.905),
            (22.7, 18.095),
            (22.7, 19.365),
            (22.7, 20.635),
            (22.7, 21.905)
        ]
    );
    let soic_rule = report.rules.iter().find(|rule| rule.package == "SOIC-8").expect("SOIC rule");
    assert_eq!((soic_rule.aperture_width, soic_rule.aperture_height), (0.553, 0.645));
}

#[test]
fn soic_apertures_clear_the_body_outline() {
    let mut document = board_document();
    add(&mut document, "U1", "SOIC-8", 50.0, 40.0, 0.0);

    let report = stencil_report(&document);
    let xs: Vec<f64> = report.entries.iter().map(|entry| entry.x).collect();
    let left = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let right = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let rule = &report.rules[0];
    // JEDEC SO-8 pad span: 5.4 mm centre distance, wider than the 3.7 mm body.
    assert_eq!((left, right), (47.3, 52.7));
    assert!((right - left - 5.4).abs() < 1e-6);
    // The inner aperture edge sits outside the body outline on both rows
    // (the SOIC-8 body is 3.7 mm across the lead rows, a 1.85 mm half).
    let inner_edge = 2.7 - rule.aperture_width / 2.0;
    assert!(inner_edge > 1.85, "inner edge {inner_edge} must exceed the body half-width 1.85");
}

#[test]
fn unknown_packages_fall_back_to_the_generic_rule() {
    let mut document = board_document();
    add(&mut document, "X1", "FOO", 30.0, 40.0, 0.0);
    add(&mut document, "X2", "BAR-9", 60.0, 40.0, 0.0);

    let report = stencil_report(&document);
    assert_eq!(report.unmatched_packages, vec!["FOO".to_string(), "BAR-9".to_string()]);
    assert_eq!(report.apertures, 2);
    for entry in &report.entries {
        assert_eq!(entry.shape, "rect");
        assert_eq!((entry.width, entry.height), (1.518, 0.949));
    }
    assert_eq!(report.entries[0].x, 30.0);
    assert_eq!(report.entries[1].x, 60.0);
}

#[test]
fn report_is_deterministic_and_leaves_the_document_untouched() {
    let mut document = board_document();
    add(&mut document, "R1", "0603", 10.0, 10.0, 45.0);
    add(&mut document, "U1", "QFP-64", 50.0, 40.0, 0.0);
    let before = document.clone();

    let first = stencil_report(&document);
    let second = stencil_report(&document);
    assert_eq!(first, second);
    assert_eq!(document, before);
    assert_eq!(first.entries.iter().filter(|entry| entry.rotation == 45.0).count(), 2);
}

#[test]
fn stencil_exports_write_csv_svg_and_pdf() {
    let directory = std::env::temp_dir().join(format!("open-smt-stencil-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("the temp directory exists");
    let mut session = OpenSmt::new();
    session.create(&directory.join("board.osmt")).expect("a document is created");
    session.set_board(Board::new(40.0, 30.0, 1.6)).expect("the board is set");
    let mut component = Component::new("R1", "0603", 10.0, 10.0);
    component.side = Side::Bottom;
    session.add_component(component.clone()).expect("a chip is added");
    session.add_component(Component::new("U1", "QFN-32", 25.0, 15.0)).expect("a qfn is added");

    let csv_target = directory.join("stencil.csv");
    let csv = session.export(ExportFormat::Stencil, &csv_target).expect("the csv export runs");
    assert_eq!(csv.format, "stencil");
    assert_eq!(csv.drawing, None);
    let csv_text = fs::read_to_string(&csv_target).expect("the csv exists");
    assert_eq!(csv_text.lines().count(), 35);
    assert!(csv_text.starts_with("refdes,package,side,index,shape,x,y,width,height,rotation\n"));
    assert!(csv_text.contains("R1,0603,bottom,1,rect,"));
    assert!(csv_text.contains("U1,QFN-32,top,1,rect,"));

    let svg_target = directory.join("stencil.svg");
    let svg = session.export(ExportFormat::StencilSvg, &svg_target).expect("the svg export runs");
    assert_eq!(svg.format, "stencil-svg");
    assert!(svg.losses.is_empty());
    let svg_text = fs::read_to_string(&svg_target).expect("the svg exists");
    assert!(svg_text.starts_with("<?xml version=\"1.0\""));
    assert_eq!(svg_text.matches("<polygon").count(), 34);
    assert!(!svg_text.contains("<circle"));
    assert!(svg_text.contains("data-side=\"bottom\""));
    assert!(svg_text.contains("STENCIL RULES"));

    let pdf_target = directory.join("stencil.pdf");
    let pdf = session.export(ExportFormat::StencilPdf, &pdf_target).expect("the pdf export runs");
    assert_eq!(pdf.format, "stencil-pdf");
    let pdf_text = fs::read_to_string(&pdf_target).expect("the pdf exists");
    assert!(pdf_text.starts_with("%PDF-1.4\n"));
    assert!(pdf_text.ends_with("%%EOF\n"));
    assert!(pdf_text.is_ascii());

    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn stencil_labels_clear_the_apertures_in_dense_rows() {
    let mut document = board_document();
    add(&mut document, "R1", "0402", 10.0, 10.0, 0.0);
    add(&mut document, "R2", "0402", 11.6, 10.0, 0.0);
    add(&mut document, "U1", "SOT-23", 30.0, 20.0, 0.0);
    let svg = render_stencil_svg(&document, "panel").expect("stencil sheet rendered");
    // Every reference label is drawn, clear of its aperture cluster.
    assert_eq!(svg.text.matches("text-anchor=\"middle\" fill=\"#0f1729\">").count(), 3);
    // The 0402 label baseline sits 0.4 mm above the body (10 - 0.25 - 0.4).
    assert!(svg.text.contains("y=\"9.350\""));
    // The SOT-23 label clears the leads that reach past the body outline.
    assert!(svg.text.contains(">U1</text>"));
}

#[test]
fn the_bottom_sheet_mirrors_the_single_board_view() {
    let mut bottom = board_document();
    let mut component = Component::new("R1", "0603", 30.0, 40.0);
    component.side = Side::Bottom;
    bottom.components.push(component);

    // The report keeps the absolute single-board coordinates unchanged.
    let report = stencil_report(&bottom);
    assert_eq!(report.entries.iter().map(|entry| entry.x).collect::<Vec<_>>(), vec![29.525, 30.475]);

    // The sheet is the bottom stencil seen through the board, so the view is
    // mirrored about the board centre line: the label sits at 100 - 30 = 70.
    let bottom_svg = render_stencil_svg(&bottom, "panel").expect("rendered").text;
    assert!(bottom_svg.contains("x=\"70.000\""), "bottom label must be mirrored");
    assert!(!bottom_svg.contains("x=\"30.000\""), "bottom label must not keep the single-board x");

    // A top sheet keeps the unmirrored coordinate for the same placement.
    let mut top = board_document();
    top.components.push(Component::new("R1", "0603", 30.0, 40.0));
    let top_svg = render_stencil_svg(&top, "panel").expect("rendered").text;
    assert!(top_svg.contains("x=\"30.000\""), "top label keeps the single-board x");
}

#[test]
fn stencil_export_parses_the_format_names() {
    assert_eq!("stencil".parse::<ExportFormat>().expect("stencil"), ExportFormat::Stencil);
    assert_eq!("stencil-csv".parse::<ExportFormat>().expect("stencil-csv"), ExportFormat::Stencil);
    assert_eq!("stencil-svg".parse::<ExportFormat>().expect("stencil-svg"), ExportFormat::StencilSvg);
    assert_eq!("stencil-pdf".parse::<ExportFormat>().expect("stencil-pdf"), ExportFormat::StencilPdf);
    assert_eq!(ExportFormat::StencilSvg.as_str(), "stencil-svg");
}

#[test]
fn stencil_drawing_rejects_an_invalid_board() {
    let mut document = board_document();
    document.board = Board { width: 0.0, depth: 30.0, thickness: 1.6 };
    let error = render_stencil_svg(&document, "board").expect_err("a zero board cannot be drawn");
    assert_eq!(error.code, "board/invalid");

    let mut other = board_document();
    other.board = Board { width: 40.0, depth: f64::NAN, thickness: 1.6 };
    let error = render_stencil_pdf(&other, "board").expect_err("a NaN board cannot be drawn");
    assert_eq!(error.code, "board/invalid");
}

#[test]
fn non_ascii_labels_are_dropped_and_counted() {
    let directory = std::env::temp_dir().join(format!("open-smt-stencil-ascii-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("the temp directory exists");
    let mut session = OpenSmt::new();
    session.create(&directory.join("板.osmt")).expect("a document is created");
    session.set_board(Board::new(40.0, 30.0, 1.6)).expect("the board is set");
    let mut component = Component::new("R1元件", "0603", 10.0, 10.0);
    component.side = Side::Top;
    session.add_component(component).expect("a chip is added");

    let report = session.stencil().expect("the stencil report runs");
    assert_eq!(report.entries.len(), 2);

    let svg = session.export(ExportFormat::StencilSvg, &directory.join("stencil.svg")).expect("the svg export runs");
    assert_eq!(svg.losses.len(), 1);
    assert_eq!(svg.losses[0].code, "stencil/non-ascii-text");
    assert!(svg.losses[0].count >= 2);
    let svg_text = fs::read_to_string(directory.join("stencil.svg")).expect("the svg exists");
    assert!(svg_text.is_ascii());
    assert!(svg_text.contains("R1??"));

    let _ = fs::remove_dir_all(&directory);
}
