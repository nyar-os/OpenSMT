use smt_model::{Board, Component, Feeder, SmtDocument, TaktOptions, allocate_feeders, sequence_placement, takt_estimate};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![
        Component::new("R1", "0603", 10.0, 10.0),
        Component::new("R2", "0603", 50.0, 10.0),
        Component::new("R3", "0603", 50.0, 50.0),
    ];
    document.feeders = vec![Feeder::new("F1", "0603", 4.0, 100)];
    document
}

#[test]
fn defaults_follow_the_placement_program() {
    let document = document();
    let placement = sequence_placement(&document, &allocate_feeders(&document).expect("allocated")).expect("sequenced");
    let report = takt_estimate(&document, &TaktOptions::default()).expect("valid estimate");

    assert_eq!(report.format, "open-smt-takt");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!(report.components, 3);
    assert_eq!(report.picks, 3);
    assert!((report.travel_mm - placement.travel_length).abs() < 1e-9);
    let expected_place = 3.0 * 0.6;
    let expected_travel = placement.travel_length / 200.0;
    assert!((report.place_seconds - expected_place).abs() < 1e-9);
    assert!((report.travel_seconds - expected_travel).abs() < 1e-9);
    assert_eq!(report.overhead_seconds, 0.0);
    assert!((report.board_seconds - (expected_place + expected_travel)).abs() < 1e-9);
    assert_eq!(report.machine_count, 1);
    assert!((report.machine_seconds - report.board_seconds).abs() < 1e-9);
    assert!((report.boards_per_hour - 3600.0 / report.board_seconds).abs() < 1e-9);
    assert!((report.components_per_hour - 3.0 * report.boards_per_hour).abs() < 1e-9);
    assert!(!report.optimized);
    assert_eq!(report.travel_saved, None);
}

#[test]
fn changeover_options_add_the_overhead_segment() {
    let document = document();
    let options = TaktOptions { feeder_changes: 2, nozzle_changes: 3, ..TaktOptions::default() };
    let base = takt_estimate(&document, &TaktOptions::default()).expect("valid estimate");
    let report = takt_estimate(&document, &options).expect("valid estimate");

    assert!((report.overhead_seconds - (2.0 * 30.0 + 3.0 * 5.0)).abs() < 1e-9);
    assert!((report.board_seconds - (base.board_seconds + 75.0)).abs() < 1e-9);
    assert!((report.place_seconds - base.place_seconds).abs() < 1e-9);
    assert!((report.travel_seconds - base.travel_seconds).abs() < 1e-9);
}

#[test]
fn custom_machine_parameters_replace_the_defaults() {
    let document = document();
    let options = TaktOptions { place_seconds: 0.2, head_speed: 100.0, ..TaktOptions::default() };
    let report = takt_estimate(&document, &options).expect("valid estimate");
    let placement = sequence_placement(&document, &allocate_feeders(&document).expect("allocated")).expect("sequenced");

    let expected = 3.0 * 0.2 + placement.travel_length / 100.0;
    assert!((report.board_seconds - expected).abs() < 1e-9);
}

#[test]
fn machines_split_the_board_time_evenly() {
    let document = document();
    let single = takt_estimate(&document, &TaktOptions::default()).expect("valid estimate");
    let options = TaktOptions { machine_count: 2, ..TaktOptions::default() };
    let report = takt_estimate(&document, &options).expect("valid estimate");

    assert_eq!(report.machine_count, 2);
    assert!((report.machine_seconds - single.board_seconds / 2.0).abs() < 1e-9);
    assert!((report.boards_per_hour - 3600.0 / (single.board_seconds / 2.0)).abs() < 1e-9);
    assert!((report.components_per_hour - 2.0 * single.components_per_hour).abs() < 1e-9);
}

#[test]
fn an_empty_document_estimates_zero_time_and_zero_throughput() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let report = takt_estimate(&document, &TaktOptions::default()).expect("valid estimate");

    assert_eq!(report.components, 0);
    assert_eq!(report.picks, 0);
    assert_eq!(report.travel_mm, 0.0);
    assert_eq!(report.board_seconds, 0.0);
    assert_eq!(report.boards_per_hour, 0.0);
    assert_eq!(report.components_per_hour, 0.0);
}

#[test]
fn optimize_reports_the_saved_travel() {
    let document = document();
    let options = TaktOptions { optimize: true, ..TaktOptions::default() };
    let report = takt_estimate(&document, &options).expect("valid estimate");

    assert!(report.optimized);
    let saved = report.travel_saved.expect("saving present");
    assert!(saved >= 0.0);
}

#[test]
fn missing_feeder_coverage_fails_like_the_placement_report() {
    let mut document = document();
    document.feeders.clear();
    let error = takt_estimate(&document, &TaktOptions::default()).expect_err("no feeders");

    assert_eq!(error.code, "feeder/missing");
}

#[test]
fn invalid_machine_parameters_report_takt_invalid() {
    let document = document();
    let invalid = vec![
        TaktOptions { place_seconds: -1.0, ..TaktOptions::default() },
        TaktOptions { place_seconds: f64::NAN, ..TaktOptions::default() },
        TaktOptions { head_speed: 0.0, ..TaktOptions::default() },
        TaktOptions { head_speed: -200.0, ..TaktOptions::default() },
        TaktOptions { head_speed: f64::INFINITY, ..TaktOptions::default() },
        TaktOptions { feeder_change_seconds: -1.0, ..TaktOptions::default() },
        TaktOptions { nozzle_change_seconds: f64::NAN, ..TaktOptions::default() },
        TaktOptions { machine_count: 0, ..TaktOptions::default() },
        TaktOptions { machine_count: 65, ..TaktOptions::default() },
    ];
    for options in invalid {
        let error = takt_estimate(&document, &options).expect_err("invalid option");
        assert_eq!(error.code, "takt/invalid", "options: {options:?}");
    }
}

#[test]
fn empty_options_use_the_defaults() {
    let options: TaktOptions = serde_json::from_str("{}").expect("empty object");
    assert_eq!(options, TaktOptions::default());
}

#[test]
fn the_report_is_deterministic() {
    let document = document();
    let first = takt_estimate(&document, &TaktOptions::default()).expect("valid estimate");
    let second = takt_estimate(&document, &TaktOptions::default()).expect("valid estimate");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
}

#[test]
fn the_document_is_unchanged() {
    let mut document = document();
    document.revision = 7;
    let before = document.clone();
    let before_json = serde_json::to_string(&document).expect("json");

    let _ = takt_estimate(&document, &TaktOptions::default()).expect("valid estimate");

    assert_eq!(document, before);
    assert_eq!(document.revision, 7);
    assert_eq!(serde_json::to_string(&document).expect("json"), before_json);
}
