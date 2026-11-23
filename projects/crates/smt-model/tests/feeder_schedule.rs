use smt_model::{Board, Component, Feeder, FeederUsage, SmtDocument, feeder_schedule_report, plan_feeder_schedule};

fn document(capacity: u32, loaded: u32, components: usize) -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components =
        (1..=components).map(|index| Component::new(format!("R{index}"), "0603", 10.0 * index as f64, 10.0)).collect();
    let mut feeder = Feeder::new("F1", "0603", 4.0, capacity);
    feeder.loaded = loaded;
    document.feeders = vec![feeder];
    document
}

fn lane(assigned: u32, loaded: u32, capacity: u32) -> FeederUsage {
    FeederUsage {
        id: "F1".to_string(),
        package: "0603".to_string(),
        loaded,
        capacity,
        assigned,
        remaining: loaded.saturating_sub(assigned),
        refdes: Vec::new(),
    }
}

#[test]
fn a_run_that_fits_the_loaded_reel_installs_no_reel() {
    let document = document(10, 10, 3);
    let report = plan_feeder_schedule(&document, 3).expect("valid schedule");

    assert_eq!(report.format, "open-smt-feeder-schedule");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!(report.boards, 3);
    assert_eq!(report.change_count, 0);
    assert_eq!(report.reel_count, 1);
    assert!(report.ok);
    assert_eq!(report.lanes.len(), 1);

    let lane = &report.lanes[0];
    assert_eq!(lane.id, "F1");
    assert_eq!(lane.package, "0603");
    assert_eq!(lane.picks_per_board, 3);
    assert_eq!(lane.capacity, 10);
    assert_eq!(lane.initial_loaded, 10);
    assert_eq!(lane.picks, 9);
    assert_eq!(lane.reels, 1);
    assert!(lane.changes.is_empty());
    assert!(!lane.short);
    assert_eq!(lane.remaining_after_run, 1);
}

#[test]
fn a_lane_replenishes_after_the_board_that_exhausts_its_stock() {
    let document = document(4, 4, 3);
    let report = plan_feeder_schedule(&document, 4).expect("valid schedule");

    let lane = &report.lanes[0];
    assert_eq!(lane.picks, 12);
    assert_eq!(lane.reels, 3);
    assert_eq!(lane.remaining_after_run, 0);
    assert_eq!(report.change_count, 2);
    assert_eq!(report.reel_count, 3);
    assert!(report.ok);

    let changes: Vec<(u32, u64, &str)> =
        lane.changes.iter().map(|change| (change.board, change.reels, change.reason.as_str())).collect();
    assert_eq!(changes, vec![(1, 2, "replenish"), (2, 3, "replenish")]);
}

#[test]
fn a_long_run_uses_multiple_reels() {
    let document = document(2, 2, 2);
    let report = plan_feeder_schedule(&document, 3).expect("valid schedule");

    let lane = &report.lanes[0];
    assert_eq!(lane.picks_per_board, 2);
    assert_eq!(lane.picks, 6);
    assert_eq!(lane.reels, 3);
    assert_eq!(lane.remaining_after_run, 0);
    assert_eq!(lane.changes.len(), 2);
    assert_eq!(lane.changes.last().map(|change| change.reels), Some(3));
    assert!(report.ok);
}

#[test]
fn a_lane_loaded_below_one_board_is_short_and_still_scheduled() {
    let report = feeder_schedule_report("mm", 2, &[lane(2, 1, 10)]);

    let lane = &report.lanes[0];
    assert!(lane.short);
    assert_eq!(lane.initial_loaded, 1);
    assert_eq!(lane.picks_per_board, 2);
    assert_eq!(lane.picks, 4);
    assert_eq!(lane.reels, 2);
    assert_eq!(lane.remaining_after_run, 8);
    assert_eq!(lane.changes.len(), 1);
    assert_eq!(lane.changes[0].board, 1);
    assert_eq!(lane.changes[0].reels, 2);
    assert!(!report.ok);
}

#[test]
fn a_zero_capacity_lane_with_demand_is_short() {
    let report = feeder_schedule_report("mm", 2, &[lane(2, 0, 0)]);
    let lane = &report.lanes[0];

    assert!(lane.short);
    assert_eq!(lane.capacity, 0);
    assert_eq!(lane.reels, 1);
    assert_eq!(lane.remaining_after_run, 0);
    assert!(lane.changes.is_empty());
    assert_eq!(report.change_count, 0);
    assert!(!report.ok);
}

#[test]
fn a_zero_capacity_document_is_rejected_by_validation() {
    let document = document(0, 0, 1);
    let error = plan_feeder_schedule(&document, 1).expect_err("zero capacity");
    assert_eq!(error.code, "feeder/invalid");
}

#[test]
fn an_empty_document_schedules_nothing() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let report = plan_feeder_schedule(&document, 4).expect("valid schedule");

    assert_eq!(report.boards, 4);
    assert_eq!(report.change_count, 0);
    assert_eq!(report.reel_count, 0);
    assert!(report.ok);
    assert!(report.lanes.is_empty());
}

#[test]
fn a_zero_board_count_reports_schedule_invalid() {
    let document = document(10, 10, 3);
    let error = plan_feeder_schedule(&document, 0).expect_err("zero boards");
    assert_eq!(error.code, "schedule/invalid");
}

#[test]
fn missing_and_undersized_feeders_propagate_like_the_consumption_plan() {
    let mut missing = document(10, 10, 3);
    missing.feeders.clear();
    assert_eq!(plan_feeder_schedule(&missing, 1).expect_err("no feeders").code, "feeder/missing");

    let undersized = document(10, 2, 3);
    assert_eq!(plan_feeder_schedule(&undersized, 1).expect_err("too few loaded").code, "feeder/capacity");
}

#[test]
fn lanes_are_reported_in_feeder_id_order() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![Component::new("R1", "0603", 10.0, 10.0), Component::new("C1", "0805", 20.0, 10.0)];
    document.feeders = vec![Feeder::new("F2", "0805", 8.0, 10), Feeder::new("F1", "0603", 4.0, 10)];

    let report = plan_feeder_schedule(&document, 1).expect("valid schedule");
    assert_eq!(report.lanes.iter().map(|lane| lane.id.as_str()).collect::<Vec<_>>(), vec!["F1", "F2"]);
    assert_eq!(report.reel_count, 2);
}

#[test]
fn the_schedule_is_deterministic_and_leaves_the_document_unchanged() {
    let mut document = document(4, 4, 3);
    document.revision = 2;
    let before = document.clone();
    let first = plan_feeder_schedule(&document, 7).expect("valid schedule");
    let second = plan_feeder_schedule(&document, 7).expect("valid schedule");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(document, before);
}
