use smt_model::{Board, Panel, SmtDocument, panel_fit};

fn document() -> SmtDocument {
    SmtDocument::new(Board::new(100.0, 80.0, 1.6))
}

#[test]
fn an_exact_fit_reports_two_by_three_boards() {
    let report = panel_fit(&document(), 310.0, 164.0, Some(5.0), Some(4.0)).expect("valid fit");

    assert_eq!(report.columns, 3);
    assert_eq!(report.rows, 2);
    assert_eq!(report.board_count, 6);
    assert!((report.panel_width - 310.0).abs() < 1e-9);
    assert!((report.panel_depth - 164.0).abs() < 1e-9);
    assert!(report.fits);
    assert_eq!(report.wasted_width, 0.0);
    assert_eq!(report.wasted_depth, 0.0);
    assert!((report.utilization - 1.0).abs() < 1e-12);
}

#[test]
fn leftover_space_is_reported_as_waste() {
    let report = panel_fit(&document(), 320.0, 180.0, Some(5.0), Some(4.0)).expect("valid fit");

    assert_eq!((report.columns, report.rows), (3, 2));
    assert_eq!(report.board_count, 6);
    assert!((report.panel_width - 310.0).abs() < 1e-9);
    assert!((report.panel_depth - 164.0).abs() < 1e-9);
    assert!((report.wasted_width - 10.0).abs() < 1e-9);
    assert!((report.wasted_depth - 16.0).abs() < 1e-9);
    let expected = 310.0 * 164.0 / (320.0 * 180.0);
    assert!((report.utilization - expected).abs() < 1e-12);
}

#[test]
fn one_dimension_too_small_fits_no_boards() {
    let report = panel_fit(&document(), 99.9, 160.0, Some(0.0), Some(0.0)).expect("valid fit");

    assert_eq!(report.columns, 0);
    assert_eq!(report.rows, 2);
    assert_eq!(report.board_count, 0);
    assert!(!report.fits);
    assert_eq!(report.panel_width, 0.0);
    assert!((report.panel_depth - 160.0).abs() < 1e-9);
    assert_eq!(report.wasted_width, 0.0);
    assert_eq!(report.wasted_depth, 0.0);
    assert_eq!(report.utilization, 0.0);
    assert!(report.wasted_width >= 0.0 && report.wasted_depth >= 0.0);
}

#[test]
fn an_exact_fit_with_no_gaps_has_zero_waste() {
    let report = panel_fit(&document(), 300.0, 160.0, Some(0.0), Some(0.0)).expect("valid fit");

    assert_eq!((report.columns, report.rows), (3, 2));
    assert!((report.panel_width - 300.0).abs() < 1e-9);
    assert!((report.panel_depth - 160.0).abs() < 1e-9);
    assert_eq!(report.wasted_width, 0.0);
    assert_eq!(report.wasted_depth, 0.0);
}

#[test]
fn larger_gaps_reduce_the_board_count() {
    let tight = panel_fit(&document(), 300.0, 160.0, Some(0.0), Some(0.0)).expect("valid fit");
    let loose = panel_fit(&document(), 300.0, 160.0, Some(10.0), Some(10.0)).expect("valid fit");

    assert_eq!((tight.columns, tight.rows), (3, 2));
    assert_eq!((loose.columns, loose.rows), (2, 1));
    assert_eq!(loose.board_count, 2);
    assert!((loose.panel_width - 210.0).abs() < 1e-9);
    assert!((loose.panel_depth - 80.0).abs() < 1e-9);
}

#[test]
fn utilization_is_the_panel_area_ratio() {
    let report = panel_fit(&document(), 250.0, 200.0, Some(5.0), Some(4.0)).expect("valid fit");

    assert_eq!((report.columns, report.rows), (2, 2));
    let expected = report.panel_width * report.panel_depth / (250.0 * 200.0);
    assert!((report.utilization - expected).abs() < 1e-12);
}

#[test]
fn gaps_default_to_the_stored_panel_and_can_be_overridden() {
    let mut document = document();
    document.panel = Panel::new(1, 1, 5.0, 4.0);

    let stored = panel_fit(&document, 300.0, 160.0, None, None).expect("valid fit");
    assert_eq!((stored.gap_x, stored.gap_y), (5.0, 4.0));
    assert_eq!((stored.columns, stored.rows), (2, 1));
    assert_eq!(stored.board_count, 2);

    let overridden = panel_fit(&document, 300.0, 160.0, Some(0.0), Some(0.0)).expect("valid fit");
    assert_eq!((overridden.gap_x, overridden.gap_y), (0.0, 0.0));
    assert_eq!((overridden.columns, overridden.rows), (3, 2));
    assert_eq!(overridden.board_count, 6);
}

#[test]
fn invalid_maxima_and_gaps_report_panel_invalid() {
    let document = document();
    let invalid: Vec<(f64, f64, Option<f64>, Option<f64>)> = vec![
        (0.0, 100.0, None, None),
        (-1.0, 100.0, None, None),
        (f64::NAN, 100.0, None, None),
        (100.0, 0.0, None, None),
        (100.0, -1.0, None, None),
        (100.0, f64::NAN, None, None),
        (100.0, 100.0, Some(-1.0), None),
        (100.0, 100.0, None, Some(-1.0)),
        (100.0, 100.0, Some(f64::NAN), None),
        (100.0, 100.0, None, Some(f64::INFINITY)),
    ];
    for (max_width, max_depth, gap_x, gap_y) in invalid {
        let error = panel_fit(&document, max_width, max_depth, gap_x, gap_y).expect_err("invalid input");
        assert_eq!(error.code, "panel/invalid", "input ({max_width}, {max_depth}, {gap_x:?}, {gap_y:?})");
    }
}

#[test]
fn the_report_is_deterministic() {
    let document = document();
    let first = panel_fit(&document, 320.0, 180.0, Some(5.0), Some(4.0)).expect("valid fit");
    let second = panel_fit(&document, 320.0, 180.0, Some(5.0), Some(4.0)).expect("valid fit");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
}

#[test]
fn the_document_is_unchanged() {
    let mut document = document();
    document.panel = Panel::new(2, 3, 5.0, 4.0);
    document.revision = 7;
    let before = document.clone();
    let before_json = serde_json::to_string(&document).expect("json");

    let _ = panel_fit(&document, 320.0, 180.0, None, None).expect("valid fit");

    assert_eq!(document, before);
    assert_eq!(document.revision, 7);
    assert_eq!(serde_json::to_string(&document).expect("json"), before_json);
}
