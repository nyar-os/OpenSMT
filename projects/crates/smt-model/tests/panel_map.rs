use smt_model::{Board, Component, Panel, Side, SmtDocument, panel_map};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.panel = Panel::new(2, 2, 5.0, 4.0);
    document.components = vec![Component::new("R1", "0603", 10.0, 10.0), Component::new("C1", "0805", 20.0, 30.0)];
    document.components[1].side = Side::Bottom;
    document
}

#[test]
fn every_board_instance_gets_its_row_major_offset() {
    let report = panel_map(&document()).expect("valid map");

    assert_eq!(report.format, "open-smt-panel-map");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!((report.width, report.depth, report.thickness), (100.0, 80.0, 1.6));
    assert_eq!((report.rows, report.columns), (2, 2));
    assert_eq!(report.board_count, 4);
    assert!((report.panel_width - 205.0).abs() < 1e-9);
    assert!((report.panel_depth - 164.0).abs() < 1e-9);

    let origins: Vec<(u32, u32, u32, f64, f64)> = report
        .instances
        .iter()
        .map(|instance| (instance.board, instance.row, instance.column, instance.offset_x, instance.offset_y))
        .collect();
    assert_eq!(origins, vec![(1, 1, 1, 0.0, 0.0), (2, 1, 2, 105.0, 0.0), (3, 2, 1, 0.0, 84.0), (4, 2, 2, 105.0, 84.0),]);
}

#[test]
fn components_keep_document_order_and_map_into_every_board() {
    let report = panel_map(&document()).expect("valid map");

    assert_eq!(report.components.len(), 2);
    assert_eq!(report.components[0].refdes, "R1");
    assert_eq!(report.components[1].refdes, "C1");
    assert_eq!(report.components[0].side, Side::Top);
    assert_eq!(report.components[1].side, Side::Bottom);

    let mapped: Vec<(u32, f64, f64)> =
        report.components[0].instances.iter().map(|placement| (placement.board, placement.x, placement.y)).collect();
    assert_eq!(mapped, vec![(1, 10.0, 10.0), (2, 115.0, 10.0), (3, 10.0, 94.0), (4, 115.0, 94.0)]);

    let mapped: Vec<(u32, f64, f64)> =
        report.components[1].instances.iter().map(|placement| (placement.board, placement.x, placement.y)).collect();
    assert_eq!(mapped, vec![(1, 20.0, 30.0), (2, 125.0, 30.0), (3, 20.0, 114.0), (4, 125.0, 114.0)]);
    assert!(report.ok);
    assert_eq!(report.outside_board, 0);
}

#[test]
fn a_single_board_panel_maps_the_identity() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![Component::new("R1", "0603", 10.0, 10.0)];
    let report = panel_map(&document).expect("valid map");

    assert_eq!(report.board_count, 1);
    assert_eq!(report.instances.len(), 1);
    assert!((report.panel_width - 100.0).abs() < 1e-9);
    assert!((report.panel_depth - 80.0).abs() < 1e-9);
    assert_eq!(report.components[0].instances[0].board, 1);
    assert_eq!(report.components[0].instances[0].x, 10.0);
    assert_eq!(report.components[0].instances[0].y, 10.0);
}

#[test]
fn an_outside_component_is_counted_and_still_mapped() {
    let mut document = document();
    document.components[0].x = 120.0;
    let report = panel_map(&document).expect("valid map");

    assert_eq!(report.outside_board, 1);
    assert!(!report.ok);
    assert_eq!(report.components[0].instances.len(), 4);
    assert_eq!(report.components[0].instances[1].x, 225.0);
    assert_eq!(report.components[1].instances.len(), 4);
}

#[test]
fn a_non_finite_component_has_no_instances() {
    let mut document = document();
    document.components[0].x = f64::NAN;
    let report = panel_map(&document).expect("valid map");

    assert_eq!(report.outside_board, 1);
    assert!(!report.ok);
    assert!(report.components[0].instances.is_empty());
    assert_eq!(report.components[1].instances.len(), 4);
}

#[test]
fn an_empty_document_maps_every_board_without_components() {
    let mut document = document();
    document.components.clear();
    let report = panel_map(&document).expect("valid map");

    assert!(report.components.is_empty());
    assert_eq!(report.instances.len(), 4);
    assert_eq!(report.outside_board, 0);
    assert!(report.ok);
}

#[test]
fn an_invalid_board_or_panel_reports_its_code() {
    let mut invalid_board = document();
    invalid_board.board.width = 0.0;
    assert_eq!(panel_map(&invalid_board).expect_err("bad board").code, "board/invalid");

    let mut non_finite_board = document();
    non_finite_board.board.depth = f64::INFINITY;
    assert_eq!(panel_map(&non_finite_board).expect_err("bad board").code, "board/invalid");

    let mut zero_rows = document();
    zero_rows.panel.rows = 0;
    assert_eq!(panel_map(&zero_rows).expect_err("bad panel").code, "panel/invalid");

    let mut negative_gap = document();
    negative_gap.panel.gap_x = -1.0;
    assert_eq!(panel_map(&negative_gap).expect_err("bad panel").code, "panel/invalid");

    let mut non_finite_gap = document();
    non_finite_gap.panel.gap_y = f64::NAN;
    assert_eq!(panel_map(&non_finite_gap).expect_err("bad panel").code, "panel/invalid");
}

#[test]
fn the_map_is_deterministic_and_leaves_the_document_unchanged() {
    let mut document = document();
    document.revision = 3;
    let before = document.clone();
    let first = panel_map(&document).expect("valid map");
    let second = panel_map(&document).expect("valid map");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(document, before);
    assert_eq!(document.revision, 3);
}
