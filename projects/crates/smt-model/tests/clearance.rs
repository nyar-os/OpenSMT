//! Clearance check: height, overlap, gap findings, limits and errors.

use std::fs;

use smt_model::{Board, ClearanceLimits, Component, OpenSmt, Side, SmtDocument, clearance_check};

fn board_document() -> SmtDocument {
    SmtDocument::new(Board::new(100.0, 80.0, 1.6))
}

fn add(document: &mut SmtDocument, refdes: &str, package: &str, x: f64, y: f64) {
    document.components.push(Component::new(refdes, package, x, y));
}

#[test]
fn a_clean_document_reports_nothing() {
    let mut document = board_document();
    add(&mut document, "R1", "0603", 10.0, 10.0);
    add(&mut document, "R2", "0603", 30.0, 30.0);

    let report = clearance_check(&document, &ClearanceLimits::default()).expect("the check runs");
    assert_eq!(report.format, "open-smt-clearance");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!((report.min_gap, report.max_height), (0.5, 12.0));
    assert_eq!(report.components, 2);
    assert_eq!(report.pairs_checked, 1);
    assert!(report.findings.is_empty());
    assert!(report.ok);
}

#[test]
fn a_tall_component_is_a_height_warning() {
    let mut document = board_document();
    let mut tall = Component::new("U1", "SOIC-8", 20.0, 20.0);
    tall.height = Some(15.0);
    document.components.push(tall);

    let report = clearance_check(&document, &ClearanceLimits::default()).expect("the check runs");
    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert_eq!((finding.severity.as_str(), finding.code.as_str()), ("warning", "clearance/height"));
    assert_eq!((finding.refdes.as_str(), finding.other.as_deref()), ("U1", None));
    assert_eq!(finding.height, Some(15.0));
    assert_eq!(finding.gap, None);
    assert_eq!(finding.side, Some(Side::Top));
    assert_eq!(finding.message, "U1 is 15 mm tall; the maximum height is 12 mm");
    assert!(!report.ok);
}

#[test]
fn overlapping_bodies_are_overlap_warnings() {
    let mut document = board_document();
    add(&mut document, "R1", "0603", 10.0, 10.0);
    add(&mut document, "R2", "0603", 10.5, 10.0);

    let report = clearance_check(&document, &ClearanceLimits::default()).expect("the check runs");
    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert_eq!(finding.code, "clearance/overlap");
    assert_eq!((finding.refdes.as_str(), finding.other.as_deref()), ("R1", Some("R2")));
    assert_eq!(finding.gap, Some(0.0));
    assert_eq!(finding.message, "R1 overlaps R2 by 0.8 mm on the top side");
}

#[test]
fn a_small_body_gap_is_a_gap_warning() {
    let mut document = board_document();
    add(&mut document, "R1", "0603", 10.0, 10.0);
    add(&mut document, "R2", "0603", 10.0, 11.0);

    let report = clearance_check(&document, &ClearanceLimits::default()).expect("the check runs");
    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert_eq!(finding.code, "clearance/gap");
    assert_eq!(finding.gap, Some(0.2));
    assert_eq!(finding.message, "R1 is 0.2 mm from R2 on the top side; the minimum gap is 0.5 mm");
}

#[test]
fn custom_limits_change_the_findings() {
    let mut document = board_document();
    add(&mut document, "R1", "0603", 10.0, 10.0);
    add(&mut document, "R2", "0603", 10.0, 11.0);

    let loose = ClearanceLimits { min_gap: Some(0.1), max_height: None };
    let report = clearance_check(&document, &loose).expect("the check runs");
    assert!(report.ok);
    assert_eq!(report.min_gap, 0.1);

    let mut tall = Component::new("C1", "0603", 60.0, 60.0);
    tall.height = Some(1.2);
    document.components.push(tall);
    let low = ClearanceLimits { min_gap: None, max_height: Some(1.0) };
    let report = clearance_check(&document, &low).expect("the check runs");
    assert_eq!(report.findings.iter().filter(|finding| finding.code == "clearance/height").count(), 1);
    assert_eq!(report.max_height, 1.0);
}

#[test]
fn invalid_limits_report_clearance_invalid() {
    let document = board_document();
    let negative = ClearanceLimits { min_gap: Some(-0.1), max_height: None };
    let error = clearance_check(&document, &negative).expect_err("a negative gap is invalid");
    assert_eq!(error.code, "clearance/invalid");

    let zero = ClearanceLimits { min_gap: None, max_height: Some(0.0) };
    let error = clearance_check(&document, &zero).expect_err("a zero height is invalid");
    assert_eq!(error.code, "clearance/invalid");

    let nan = ClearanceLimits { min_gap: Some(f64::NAN), max_height: None };
    let error = clearance_check(&document, &nan).expect_err("a NaN gap is invalid");
    assert_eq!(error.code, "clearance/invalid");
}

#[test]
fn opposite_sides_are_never_paired() {
    let mut document = board_document();
    add(&mut document, "R1", "0603", 10.0, 10.0);
    let mut bottom = Component::new("R2", "0603", 10.0, 10.0);
    bottom.side = Side::Bottom;
    document.components.push(bottom);

    let report = clearance_check(&document, &ClearanceLimits::default()).expect("the check runs");
    assert_eq!(report.pairs_checked, 0);
    assert!(report.ok);
}

#[test]
fn rotated_bodies_are_compared_as_bounding_boxes() {
    let mut document = board_document();
    add(&mut document, "R1", "0603", 10.0, 10.0);
    let mut rotated = Component::new("R2", "0603", 10.0, 11.6);
    rotated.rotation = 90.0;
    document.components.push(rotated);

    // Rotated 90 degrees the 0603 body is 0.8 wide and 1.6 tall, so the vertical
    // body gap is 1.6 - 0.8 - 0.4 = 0.4 mm, below the default minimum gap.
    let report = clearance_check(&document, &ClearanceLimits::default()).expect("the check runs");
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].code, "clearance/gap");
    assert_eq!(report.findings[0].gap, Some(0.4));
}

#[test]
fn non_finite_coordinates_are_skipped() {
    let mut document = board_document();
    add(&mut document, "R1", "0603", 10.0, 10.0);
    document.components.push(Component::new("R2", "0603", f64::NAN, 10.0));

    let report = clearance_check(&document, &ClearanceLimits::default()).expect("the check runs");
    assert_eq!(report.components, 2);
    assert_eq!(report.pairs_checked, 0);
    assert!(report.ok);
}

#[test]
fn the_check_is_deterministic_and_leaves_the_document_untouched() {
    let mut document = board_document();
    add(&mut document, "R1", "0603", 10.0, 10.0);
    add(&mut document, "R2", "0603", 10.5, 10.0);
    add(&mut document, "U1", "TQFP-44", 50.0, 50.0);
    let before = document.clone();

    let limits = ClearanceLimits::default();
    let first = clearance_check(&document, &limits).expect("the first check runs");
    let second = clearance_check(&document, &limits).expect("the second check runs");
    assert_eq!(first, second);
    assert_eq!(document, before);
    assert_eq!(first.pairs_checked, 3);
}

#[test]
fn the_session_clearance_method_forwards_the_open_document() {
    let directory = std::env::temp_dir().join(format!("open-smt-clearance-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("the temp directory exists");
    let mut session = OpenSmt::new();
    session.create(&directory.join("board.osmt")).expect("a document is created");
    session.set_board(Board::new(40.0, 30.0, 1.6)).expect("the board is set");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("a chip is added");
    let mut close = Component::new("R2", "0603", 11.0, 10.0);
    close.height = Some(13.0);
    session.add_component(close).expect("a second chip is added");

    let report = session.clearance(&ClearanceLimits::default()).expect("the report runs");
    assert_eq!(report.components, 2);
    assert_eq!(report.findings.len(), 2);
    assert!(report.findings.iter().any(|finding| finding.code == "clearance/height"));
    assert!(report.findings.iter().any(|finding| finding.code == "clearance/overlap"));
    assert!(!report.ok);

    let error = session.clearance(&ClearanceLimits { max_height: Some(-1.0), ..ClearanceLimits::default() });
    assert_eq!(error.expect_err("an invalid limit is rejected").code, "clearance/invalid");

    let _ = fs::remove_dir_all(&directory);
}
