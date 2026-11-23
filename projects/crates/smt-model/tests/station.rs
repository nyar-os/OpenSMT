use smt_model::{Board, Component, Feeder, SmtDocument, assign_stations};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![Component::new("R1", "0603", 90.0, 10.0), Component::new("C1", "0805", 10.0, 10.0)];
    document.feeders = vec![Feeder::new("F1", "0603", 4.0, 100), Feeder::new("F2", "0805", 4.0, 100)];
    document
}

#[test]
fn the_mean_x_order_shortens_the_tape_bank_travel() {
    let document = document();
    let report = assign_stations(&document, 16.0).expect("assigned");

    assert_eq!(report.format, "open-smt-station");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!(report.slot_pitch, 16.0);
    assert_eq!(report.slot_count, 2);
    assert_eq!(report.stations.len(), 2);

    // F2 serves x=10 and is placed first even though F1 is stored first.
    assert_eq!(report.stations[0].feeder_id, "F2");
    assert_eq!(report.stations[0].slot, 1);
    assert_eq!(report.stations[0].position_mm, 0.0);
    assert_eq!(report.stations[0].picks, 1);
    assert!((report.stations[0].mean_x.expect("used") - 10.0).abs() < 1e-9);
    assert_eq!(report.stations[1].feeder_id, "F1");
    assert_eq!(report.stations[1].slot, 2);
    assert_eq!(report.stations[1].position_mm, 16.0);

    // Program order is C1 (F2) then R1 (F1): baseline 16 + 16, optimized 0 + 16.
    assert!((report.baseline_travel_mm - 32.0).abs() < 1e-9);
    assert!((report.travel_mm - 16.0).abs() < 1e-9);
    assert!((report.travel_saved - 16.0).abs() < 1e-9);
}

#[test]
fn a_matching_stored_order_saves_nothing() {
    let mut document = document();
    document.feeders.reverse();
    let report = assign_stations(&document, 16.0).expect("assigned");

    assert_eq!(report.stations[0].feeder_id, "F2");
    assert!((report.travel_saved - 0.0).abs() < 1e-9);
    assert!((report.travel_mm - report.baseline_travel_mm).abs() < 1e-9);
}

#[test]
fn mean_x_ties_break_by_feeder_id_and_unused_lanes_come_last() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![Component::new("R1", "0603", 20.0, 10.0), Component::new("R2", "0805", 20.0, 30.0)];
    document.feeders = vec![
        Feeder::new("F3", "0805", 4.0, 100),
        Feeder::new("F1", "0603", 4.0, 100),
        Feeder::new("F9", "1206", 4.0, 100),
        Feeder::new("F0", "1210", 4.0, 100),
    ];
    let report = assign_stations(&document, 16.0).expect("assigned");

    let ids: Vec<&str> = report.stations.iter().map(|station| station.feeder_id.as_str()).collect();
    // F1 and F3 share mean x=20, so the id order decides; F0 and F9 are unused.
    assert_eq!(ids, ["F1", "F3", "F0", "F9"]);
    assert_eq!(report.stations[2].picks, 0);
    assert_eq!(report.stations[2].mean_x, None);
    assert_eq!(report.stations[3].mean_x, None);
}

#[test]
fn an_empty_document_has_no_slots_and_no_travel() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let report = assign_stations(&document, 16.0).expect("assigned");

    assert_eq!(report.slot_count, 0);
    assert!(report.stations.is_empty());
    assert_eq!(report.baseline_travel_mm, 0.0);
    assert_eq!(report.travel_mm, 0.0);
    assert_eq!(report.travel_saved, 0.0);
}

#[test]
fn missing_feeder_coverage_fails_like_the_placement_report() {
    let mut document = document();
    document.feeders.clear();
    let error = assign_stations(&document, 16.0).expect_err("no feeders");

    assert_eq!(error.code, "feeder/missing");
}

#[test]
fn invalid_slot_pitches_report_station_invalid() {
    let document = document();
    for pitch in [0.0, -16.0, f64::NAN, f64::INFINITY] {
        let error = assign_stations(&document, pitch).expect_err("invalid pitch");
        assert_eq!(error.code, "station/invalid", "pitch: {pitch}");
    }
}

#[test]
fn the_report_is_deterministic_and_leaves_the_document_unchanged() {
    let mut document = document();
    document.revision = 4;
    let before = document.clone();
    let first = assign_stations(&document, 16.0).expect("assigned");
    let second = assign_stations(&document, 16.0).expect("assigned");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(document, before);
    assert_eq!(document.revision, 4);
}
