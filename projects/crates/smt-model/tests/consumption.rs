use smt_model::{Board, Component, Feeder, SmtDocument, plan_consumption};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![
        Component::new("R1", "0603", 10.0, 10.0),
        Component::new("R2", "0603", 50.0, 10.0),
        Component::new("R3", "0603", 50.0, 50.0),
    ];
    let mut feeder = Feeder::new("F1", "0603", 4.0, 10);
    feeder.loaded = 10;
    document.feeders = vec![feeder];
    document
}

#[test]
fn a_run_that_fits_the_loaded_reel_is_ok() {
    let document = document();
    let report = plan_consumption(&document, 3).expect("valid plan");

    assert_eq!(report.format, "open-smt-consumption");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!(report.boards, 3);
    assert_eq!(report.components_per_board, 3);
    assert_eq!(report.total_picks, 9);
    assert_eq!(report.feeders.len(), 1);

    let lane = &report.feeders[0];
    assert_eq!(lane.id, "F1");
    assert_eq!(lane.picks_per_board, 3);
    assert_eq!(lane.picks, 9);
    assert_eq!(lane.capacity, 10);
    assert_eq!(lane.loaded, 10);
    assert_eq!(lane.reels, 1);
    assert_eq!(lane.remainder, 9);
    assert!(!lane.short);

    assert_eq!(report.packages.len(), 1);
    let package = &report.packages[0];
    assert_eq!(package.package, "0603");
    assert_eq!(package.components_per_board, 3);
    assert_eq!(package.total_picks, 9);
    assert_eq!(package.feeders, 1);
    assert_eq!(package.reels, 1);
    assert!(!package.short);
    assert!(report.ok);
}

#[test]
fn a_run_beyond_the_loaded_reel_reports_reels_and_a_shortage() {
    let document = document();
    let report = plan_consumption(&document, 5).expect("valid plan");

    let lane = &report.feeders[0];
    assert_eq!(lane.picks, 15);
    assert_eq!(lane.reels, 2);
    assert_eq!(lane.remainder, 5);
    assert!(lane.short);
    assert!(!report.ok);
    assert_eq!(report.packages[0].reels, 2);
    assert!(report.packages[0].short);
}

#[test]
fn the_last_reel_remainder_reports_the_partial_reel() {
    let mut document = document();
    document.feeders[0].capacity = 4;
    document.feeders[0].loaded = 4;
    let report = plan_consumption(&document, 3).expect("valid plan");
    let lane = &report.feeders[0];

    assert_eq!(lane.picks, 9);
    assert_eq!(lane.reels, 3);
    assert_eq!(lane.remainder, 1);
    // More than one reel means the single loaded reel cannot cover the run.
    assert!(lane.short);
}

#[test]
fn a_package_split_over_two_lanes_reports_both_lanes_and_one_total() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![
        Component::new("R1", "0603", 10.0, 10.0),
        Component::new("R2", "0603", 20.0, 10.0),
        Component::new("R3", "0603", 30.0, 10.0),
        Component::new("R4", "0603", 40.0, 10.0),
    ];
    let mut first = Feeder::new("F1", "0603", 4.0, 100);
    first.loaded = 2;
    let mut second = Feeder::new("F2", "0603", 4.0, 100);
    second.loaded = 2;
    document.feeders = vec![first, second];

    let report = plan_consumption(&document, 3).expect("valid plan");
    assert_eq!(report.components_per_board, 4);
    assert_eq!(report.total_picks, 12);
    assert_eq!(report.feeders.len(), 2);
    for lane in &report.feeders {
        assert_eq!(lane.picks_per_board, 2);
        assert_eq!(lane.picks, 6);
        assert_eq!(lane.reels, 1);
        assert_eq!(lane.remainder, 6);
        assert!(lane.short);
    }
    assert_eq!(report.packages.len(), 1);
    assert_eq!(report.packages[0].feeders, 2);
    assert_eq!(report.packages[0].reels, 2);
    assert_eq!(report.packages[0].total_picks, 12);
    assert!(report.packages[0].short);
    assert!(!report.ok);
}

#[test]
fn an_empty_document_consumes_nothing() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let report = plan_consumption(&document, 4).expect("valid plan");

    assert_eq!(report.components_per_board, 0);
    assert_eq!(report.total_picks, 0);
    assert!(report.packages.is_empty());
    assert!(report.feeders.is_empty());
    assert!(report.ok);
}

#[test]
fn a_zero_board_count_reports_consumption_invalid() {
    let document = document();
    let error = plan_consumption(&document, 0).expect_err("zero boards");
    assert_eq!(error.code, "consumption/invalid");
}

#[test]
fn missing_feeder_coverage_fails_like_the_placement_report() {
    let mut document = document();
    document.feeders.clear();
    let error = plan_consumption(&document, 1).expect_err("no feeders");
    assert_eq!(error.code, "feeder/missing");
}

#[test]
fn the_plan_is_deterministic_and_leaves_the_document_unchanged() {
    let mut document = document();
    document.revision = 2;
    let before = document.clone();
    let first = plan_consumption(&document, 7).expect("valid plan");
    let second = plan_consumption(&document, 7).expect("valid plan");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(document, before);
}
