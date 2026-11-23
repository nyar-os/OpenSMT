use smt_model::{Board, Component, Feeder, Side, SmtDocument, TaktOptions, takt_estimate, takt_heatmap};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![Component::new("R1", "0603", 10.0, 10.0), Component::new("C1", "0805", 10.0, 50.0)];
    document.feeders = vec![Feeder::new("F1", "0603", 4.0, 100), Feeder::new("F2", "0805", 8.0, 50)];
    document
}

#[test]
fn every_step_carries_its_placement_and_inbound_travel_time() {
    let document = document();
    let report = takt_heatmap(&document).expect("heat map");

    assert_eq!(report.format, "open-smt-heatmap");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!(report.components, 2);
    assert_eq!(report.steps.len(), 2);

    let first = &report.steps[0];
    assert_eq!(first.sequence, 1);
    assert_eq!(first.refdes, "R1");
    assert_eq!(first.feeder, "F1");
    assert!((first.travel_mm - 200.0_f64.sqrt()).abs() < 1e-9);
    assert!((first.place_seconds - 0.6).abs() < 1e-12);
    assert!((first.travel_seconds - 200.0_f64.sqrt() / 200.0).abs() < 1e-12);
    assert!((first.seconds - (0.6 + 200.0_f64.sqrt() / 200.0)).abs() < 1e-12);

    let second = &report.steps[1];
    assert_eq!(second.refdes, "C1");
    assert!((second.travel_mm - 40.0).abs() < 1e-9);
    assert!((second.seconds - 0.8).abs() < 1e-12);

    let total = 0.6 + 200.0_f64.sqrt() / 200.0 + 0.8;
    assert!((report.place_seconds - 1.2).abs() < 1e-12);
    assert!((report.travel_seconds - (200.0_f64.sqrt() + 40.0) / 200.0).abs() < 1e-12);
    assert!((report.total_seconds - total).abs() < 1e-12);
    assert!((report.steps.iter().map(|step| step.share).sum::<f64>() - 1.0).abs() < 1e-9);
}

#[test]
fn packages_are_ordered_by_time_with_cumulative_shares() {
    let document = document();
    let report = takt_heatmap(&document).expect("heat map");

    assert_eq!(report.packages.len(), 2);
    let packages: Vec<(&str, u32, f64)> =
        report.packages.iter().map(|package| (package.package.as_str(), package.steps, package.seconds)).collect();
    // C1 is the slower step (travel 40 mm) and leads the heat map.
    assert_eq!(packages[0].0, "0805");
    assert_eq!(packages[0].1, 1);
    assert!((packages[0].2 - 0.8).abs() < 1e-12);
    assert_eq!(packages[1].0, "0603");
    assert_eq!(packages[1].1, 1);
    assert!((packages[1].2 - (0.6 + 200.0_f64.sqrt() / 200.0)).abs() < 1e-12);

    assert!((report.packages[0].share + report.packages[1].share - 1.0).abs() < 1e-9);
    assert!((report.packages[0].cumulative_share - report.packages[0].share).abs() < 1e-12);
    assert!((report.packages[1].cumulative_share - 1.0).abs() < 1e-9);
    assert_eq!(report.bottleneck.as_deref(), Some("0805"));
}

#[test]
fn each_side_restarts_travel_at_the_origin() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let mut bottom = Component::new("R2", "0603", 10.0, 10.0);
    bottom.side = Side::Bottom;
    document.components = vec![Component::new("R1", "0603", 10.0, 10.0), bottom];
    document.feeders = vec![Feeder::new("F1", "0603", 4.0, 100)];
    let report = takt_heatmap(&document).expect("heat map");

    assert_eq!(report.steps.len(), 2);
    for step in &report.steps {
        assert!((step.travel_mm - 200.0_f64.sqrt()).abs() < 1e-9);
    }
    assert_eq!(report.sides.len(), 2);
    assert_eq!(report.sides[0].side, Side::Top);
    assert_eq!(report.sides[1].side, Side::Bottom);
    assert!((report.sides[0].seconds - report.sides[1].seconds).abs() < 1e-12);
    assert!((report.sides[0].share - 0.5).abs() < 1e-9);
}

#[test]
fn the_total_matches_the_default_takt_board_time() {
    let document = document();
    let report = takt_heatmap(&document).expect("heat map");
    let takt = takt_estimate(&document, &TaktOptions::default()).expect("takt");

    assert!((report.total_seconds - takt.board_seconds).abs() < 1e-9);
}

#[test]
fn an_empty_document_has_no_heat() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let report = takt_heatmap(&document).expect("heat map");

    assert_eq!(report.components, 0);
    assert_eq!(report.total_seconds, 0.0);
    assert_eq!(report.place_seconds, 0.0);
    assert_eq!(report.travel_seconds, 0.0);
    assert_eq!(report.bottleneck, None);
    assert!(report.packages.is_empty());
    assert!(report.sides.is_empty());
    assert!(report.steps.is_empty());
}

#[test]
fn missing_feeder_coverage_fails_like_the_placement_report() {
    let mut document = document();
    document.feeders.clear();
    let error = takt_heatmap(&document).expect_err("no feeders");
    assert_eq!(error.code, "feeder/missing");
}

#[test]
fn the_heat_map_is_deterministic_and_leaves_the_document_unchanged() {
    let mut document = document();
    document.revision = 3;
    let before = document.clone();
    let first = takt_heatmap(&document).expect("heat map");
    let second = takt_heatmap(&document).expect("heat map");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(document, before);
    assert_eq!(document.revision, 3);
}
