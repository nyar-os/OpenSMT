use smt_model::{Board, Component, Feeder, Panel, Side, SmtDocument, first_article_csv, first_article_sheet};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.panel = Panel::new(2, 2, 5.0, 5.0);
    let mut r2 = Component::new("R2", "0603", 10.0, 10.0);
    r2.rotation = 90.0;
    let mut c1 = Component::new("C1", "0805", 20.0, 20.0);
    c1.side = Side::Bottom;
    document.components = vec![Component::new("R1", "0603", 90.0, 10.0), r2, c1];
    document.feeders = vec![Feeder::new("F1", "0603", 4.0, 100), Feeder::new("F2", "0805", 8.0, 50)];
    document
}

#[test]
fn the_sheet_repeats_the_program_order_with_the_lane_pitch() {
    let document = document();
    let sheet = first_article_sheet(&document).expect("valid sheet");

    assert_eq!(sheet.format, "open-smt-first-article");
    assert_eq!(sheet.version, 1);
    assert_eq!(sheet.units, "mm");
    assert_eq!(sheet.components, 3);
    assert_eq!(sheet.board_count, 4);
    assert!((sheet.panel_width - 205.0).abs() < 1e-9);
    assert!((sheet.panel_depth - 165.0).abs() < 1e-9);

    assert_eq!(sheet.steps.len(), 3);
    let first = &sheet.steps[0];
    assert_eq!(first.step, 1);
    assert_eq!(first.refdes, "R2");
    assert_eq!(first.package, "0603");
    assert_eq!(first.side, Side::Top);
    assert_eq!((first.x, first.y, first.rotation), (10.0, 10.0, 90.0));
    assert_eq!(first.feeder, "F1");
    assert_eq!(first.pitch, 4.0);

    let second = &sheet.steps[1];
    assert_eq!(second.step, 2);
    assert_eq!(second.refdes, "R1");
    assert_eq!((second.x, second.y, second.rotation), (90.0, 10.0, 0.0));
    assert_eq!(second.feeder, "F1");

    let third = &sheet.steps[2];
    assert_eq!(third.step, 3);
    assert_eq!(third.refdes, "C1");
    assert_eq!(third.side, Side::Bottom);
    assert_eq!((third.x, third.y), (20.0, 20.0));
    assert_eq!(third.feeder, "F2");
    assert_eq!(third.pitch, 8.0);
}

#[test]
fn materials_group_by_package_in_name_order() {
    let sheet = first_article_sheet(&document()).expect("valid sheet");

    assert_eq!(sheet.packages, 2);
    assert_eq!(sheet.materials.len(), 2);
    assert_eq!(sheet.materials[0].package, "0603");
    assert_eq!(sheet.materials[0].components, 2);
    assert_eq!(sheet.materials[0].sides, vec![Side::Top]);
    assert_eq!(sheet.materials[0].lanes.len(), 1);
    assert_eq!(sheet.materials[0].lanes[0].id, "F1");
    assert_eq!(sheet.materials[0].lanes[0].pitch, 4.0);
    assert_eq!(sheet.materials[0].lanes[0].assigned, 2);

    assert_eq!(sheet.materials[1].package, "0805");
    assert_eq!(sheet.materials[1].components, 1);
    assert_eq!(sheet.materials[1].sides, vec![Side::Bottom]);
    assert_eq!(sheet.materials[1].lanes[0].id, "F2");
    assert_eq!(sheet.materials[1].lanes[0].assigned, 1);
}

#[test]
fn side_totals_and_the_fixed_checklist_are_reported() {
    let sheet = first_article_sheet(&document()).expect("valid sheet");

    assert_eq!(sheet.top_components, 2);
    assert_eq!(sheet.bottom_components, 1);
    assert_eq!(sheet.feeders, 2);
    assert_eq!(sheet.checklist, vec!["placement", "orientation", "package", "feeder"]);
}

#[test]
fn the_document_revision_is_recorded() {
    let mut document = document();
    document.revision = 5;
    let sheet = first_article_sheet(&document).expect("valid sheet");

    assert_eq!(sheet.revision, 5);
    assert_eq!(sheet.board.width, 100.0);
    assert_eq!(sheet.panel.columns, 2);
}

#[test]
fn an_empty_document_produces_an_empty_sheet() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let sheet = first_article_sheet(&document).expect("valid sheet");

    assert_eq!(sheet.components, 0);
    assert!(sheet.steps.is_empty());
    assert!(sheet.materials.is_empty());
    assert_eq!(sheet.packages, 0);
    assert_eq!(sheet.feeders, 0);
    assert_eq!(sheet.checklist.len(), 4);
}

#[test]
fn feeder_coverage_errors_propagate() {
    let mut missing = document();
    missing.feeders.retain(|feeder| feeder.id != "F1");
    assert_eq!(first_article_sheet(&missing).expect_err("missing lane").code, "feeder/missing");

    let mut short = document();
    short.feeders[0].loaded = 1;
    assert_eq!(first_article_sheet(&short).expect_err("short lane").code, "feeder/capacity");
}

#[test]
fn the_csv_writer_emits_the_rows_in_program_order_with_empty_check_columns() {
    let sheet = first_article_sheet(&document()).expect("valid sheet");
    let csv = first_article_csv(&sheet);

    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "step,refdes,package,side,x,y,rotation,feeder,pitch,placement_ok,orientation_ok,package_ok,feeder_ok");
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[1], "1,R2,0603,top,10,10,90,F1,4,,,,");
    assert_eq!(lines[2], "2,R1,0603,top,90,10,0,F1,4,,,,");
    assert_eq!(lines[3], "3,C1,0805,bottom,20,20,0,F2,8,,,,");
}

#[test]
fn the_sheet_is_deterministic_and_leaves_the_document_unchanged() {
    let mut document = document();
    document.revision = 2;
    let before = document.clone();
    let first = first_article_sheet(&document).expect("valid sheet");
    let second = first_article_sheet(&document).expect("valid sheet");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(document, before);
    assert_eq!(document.revision, 2);
}
