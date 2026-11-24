use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

use smt_model::{
    Board, Component, Feeder, FeederUpdate, OpenSmt, SmtDocument, Splice, feeder_schedule_report, plan_consumption,
    plan_feeder_schedule,
};

fn workspace(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("open-smt-splice-{}-{name}", process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("create test directory");
    directory
}

fn read_document(path: &Path) -> SmtDocument {
    serde_json::from_str(&fs::read_to_string(path).expect("document text")).expect("parse document")
}

fn document(capacity: u32, loaded: u32, components: usize) -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components =
        (1..=components).map(|index| Component::new(format!("R{index}"), "0603", 10.0 * index as f64, 10.0)).collect();
    let mut feeder = Feeder::new("F1", "0603", 4.0, capacity);
    feeder.loaded = loaded;
    document.feeders = vec![feeder];
    document
}

#[test]
fn a_splice_continues_consumption_without_resetting_the_used_quantity() {
    let mut document = document(4, 4, 3);
    document.splices = vec![Splice { feeder: "F1".to_string(), sequence: 3, leader_waste: 1, trailer_waste: 1, scrap: 2 }];

    let consumption = plan_consumption(&document, 3).expect("consumption");
    let lane = &consumption.feeders[0];
    assert_eq!(lane.picks, 9);
    assert_eq!(lane.splices, 1);
    assert_eq!(lane.gross, lane.picks + lane.scrap);
    assert_eq!(lane.scrap, 4);
    assert!(lane.short, "the loaded reel plus the spliced reel cannot cover gross consumption");

    let schedule = plan_feeder_schedule(&document, 3).expect("schedule");
    let lane = &schedule.lanes[0];
    assert_eq!(lane.picks, 9);
    assert_eq!(lane.gross, 13);
    assert_eq!(lane.scrap, 4);
    assert_eq!(lane.splices, 1);
    assert_eq!(lane.reels, 4);
    assert_eq!(lane.remaining_after_run, 3);
    assert_eq!(lane.changes[0].reason, "splice");
    assert_eq!(lane.changes[0].sequence, Some(3));
    assert_eq!(lane.changes.iter().filter(|change| change.reason == "splice").count(), 1);
}

#[test]
fn a_splice_moves_the_predicted_replenishment_later() {
    let document = document(6, 6, 3);
    let baseline = plan_feeder_schedule(&document, 4).expect("baseline");
    assert_eq!(baseline.lanes[0].changes.len(), 1);
    assert_eq!(baseline.lanes[0].changes[0].board, 2);

    let mut spliced = document.clone();
    spliced.splices = vec![Splice::new("F1", 2)];
    let report = plan_feeder_schedule(&spliced, 4).expect("spliced");
    let lane = &report.lanes[0];

    assert_eq!(lane.reels, 2);
    assert_eq!(lane.changes.len(), 1);
    assert_eq!(lane.changes[0].reason, "splice");
    assert_eq!(lane.changes[0].board, 1);
    assert_eq!(lane.changes[0].sequence, Some(2));
    assert_eq!(lane.remaining_after_run, 0);
    assert_eq!(report.change_count, 1);
}

#[test]
fn leader_trailer_and_manual_scrap_are_reported_separately_from_net_picks() {
    let mut document = document(10, 10, 3);
    let feeder = &mut document.feeders[0];
    feeder.leader_waste = 2;
    feeder.trailer_waste = 1;
    feeder.scrap = 3;

    let consumption = plan_consumption(&document, 2).expect("consumption");
    let lane = &consumption.feeders[0];
    assert_eq!(lane.picks, 6);
    assert_eq!(lane.scrap, 9);
    assert_eq!(lane.gross, 15);
    assert_eq!(lane.reels, 2);
    assert!(lane.short);

    let schedule = plan_feeder_schedule(&document, 2).expect("schedule");
    let lane = &schedule.lanes[0];
    assert_eq!(lane.picks, 6);
    assert_eq!(lane.scrap, 9);
    assert_eq!(lane.gross, 15);
    assert_eq!(lane.reels, 2);
    assert_eq!(lane.remaining_after_run, 5);
    assert!(!lane.short);
}

#[test]
fn manual_scrap_can_be_edited_through_the_feeder_update() {
    let directory = workspace("scrap");
    let path = directory.join("panel.osmt");
    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("component");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 10)).expect("feeder");
    session
        .update_feeder(
            "F1",
            FeederUpdate {
                package: None,
                pitch: None,
                capacity: None,
                loaded: None,
                leader_waste: Some(1),
                trailer_waste: Some(1),
                scrap: Some(4),
            },
        )
        .expect("update");

    let report = session.consumption(1).expect("consumption");
    assert_eq!(report.feeders[0].scrap, 6);
    assert_eq!(report.feeders[0].gross, 7);
    assert_eq!(read_document(&path).feeders[0].scrap, 4);
}

#[test]
fn invalid_splices_are_rejected_without_writing() {
    let directory = workspace("invalid");
    let path = directory.join("panel.osmt");
    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    session.add_component(Component::new("C1", "0603", 20.0, 10.0)).expect("C1");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 10)).expect("feeder");
    let before = read_document(&path);

    assert_eq!(session.add_splice(Splice::new("F1", 0)).expect_err("zero step").code, "feeder/splice-invalid");
    assert_eq!(session.add_splice(Splice::new("F1", 3)).expect_err("past the program").code, "feeder/splice-invalid");
    assert_eq!(session.add_splice(Splice::new("F9", 1)).expect_err("unknown feeder").code, "feeder/unknown-id");

    let summary = session.add_splice(Splice::new("F1", 1)).expect("valid splice");
    assert_eq!(summary.splices, 1);
    assert_eq!(summary.revision, before.revision + 1);
    assert_eq!(session.add_splice(Splice::new("F1", 1)).expect_err("duplicate step").code, "feeder/splice-invalid");

    let after = read_document(&path);
    assert_eq!(after.splices.len(), 1);
    assert_eq!(after.splices[0].feeder, "F1");
    assert_eq!(after.splices[0].sequence, 1);
    assert_eq!(after.revision, before.revision + 1);
}

#[test]
fn reports_are_deterministic_and_leave_the_document_unchanged() {
    let mut document = document(4, 4, 3);
    document.splices = vec![Splice { feeder: "F1".to_string(), sequence: 2, leader_waste: 1, trailer_waste: 0, scrap: 1 }];
    let before = document.clone();

    let first = plan_consumption(&document, 4).expect("first");
    let second = plan_consumption(&document, 4).expect("second");
    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));

    let first = plan_feeder_schedule(&document, 4).expect("first schedule");
    let second = plan_feeder_schedule(&document, 4).expect("second schedule");
    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(document, before);
}

#[test]
fn a_splice_on_a_zero_capacity_lane_is_still_simulated() {
    let report = feeder_schedule_report(
        "mm",
        2,
        &[smt_model::FeederUsage {
            id: "F1".to_string(),
            package: "0603".to_string(),
            loaded: 0,
            capacity: 0,
            assigned: 1,
            remaining: 0,
            refdes: Vec::new(),
        }],
    );
    assert!(report.lanes[0].short);
    assert_eq!(report.lanes[0].reels, 1);
}
