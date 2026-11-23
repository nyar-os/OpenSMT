use smt_model::{Board, Component, Feeder, SmtDocument, plan_changeover};

fn current() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.feeders =
        vec![Feeder::new("F3", "0805", 4.0, 100), Feeder::new("F1", "0603", 4.0, 500), Feeder::new("F2", "QFN-32", 12.0, 50)];
    document
}

fn next() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.feeders =
        vec![Feeder::new("F1", "0603", 4.0, 500), Feeder::new("F2", "QFN-48", 12.0, 50), Feeder::new("F0", "1206", 4.0, 200)];
    document
}

#[test]
fn removals_swaps_and_additions_run_in_that_order() {
    let report = plan_changeover(&current(), &next(), 30.0).expect("planned");

    assert_eq!(report.format, "open-smt-changeover");
    assert_eq!(report.version, 1);
    assert_eq!(report.current_feeders, 3);
    assert_eq!(report.next_feeders, 3);
    assert_eq!(report.unchanged, ["F1"]);

    let kinds: Vec<&str> = report.actions.iter().map(|action| action.kind.as_str()).collect();
    assert_eq!(kinds, ["remove", "swap", "add"]);
    assert_eq!(report.actions[0].id, "F3");
    assert_eq!(report.actions[0].package.as_deref(), Some("0805"));
    assert_eq!(report.actions[1].id, "F2");
    assert_eq!(report.actions[1].from_package.as_deref(), Some("QFN-32"));
    assert_eq!(report.actions[1].to_package.as_deref(), Some("QFN-48"));
    assert_eq!(report.actions[2].id, "F0");
    assert_eq!(report.actions[2].capacity, Some(200));
    assert_eq!(report.actions[2].loaded, Some(200));

    let steps: Vec<u32> = report.actions.iter().map(|action| action.step).collect();
    assert_eq!(steps, [1, 2, 3]);
}

#[test]
fn a_swap_counts_as_two_reel_changes() {
    let report = plan_changeover(&current(), &next(), 30.0).expect("planned");

    assert_eq!(report.changes, 4);
    assert!((report.estimated_seconds - 120.0).abs() < 1e-9);

    let custom = plan_changeover(&current(), &next(), 15.0).expect("planned");
    assert!((custom.estimated_seconds - 60.0).abs() < 1e-9);
}

#[test]
fn a_pitch_change_alone_is_a_swap() {
    let mut target = next();
    target.feeders[0].pitch = 8.0;
    let report = plan_changeover(&current(), &target, 30.0).expect("planned");

    let swap = report.actions.iter().find(|action| action.id == "F1").expect("F1 swapped");
    assert_eq!(swap.kind.as_str(), "swap");
    assert!((swap.from_pitch.expect("from") - 4.0).abs() < 1e-9);
    assert!((swap.to_pitch.expect("to") - 8.0).abs() < 1e-9);
    assert!(report.unchanged.is_empty());
}

#[test]
fn identical_setups_change_nothing() {
    let document = current();
    let report = plan_changeover(&document, &document, 30.0).expect("planned");

    assert!(report.actions.is_empty());
    assert_eq!(report.unchanged, ["F1", "F2", "F3"]);
    assert_eq!(report.changes, 0);
    assert_eq!(report.estimated_seconds, 0.0);
}

#[test]
fn an_empty_current_document_only_adds() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let report = plan_changeover(&document, &next(), 30.0).expect("planned");

    assert_eq!(report.actions.len(), 3);
    assert!(report.actions.iter().all(|action| action.kind.as_str() == "add"));
    let ids: Vec<&str> = report.actions.iter().map(|action| action.id.as_str()).collect();
    assert_eq!(ids, ["F0", "F1", "F2"]);
}

#[test]
fn feeder_coverage_does_not_block_the_plan() {
    let mut target = next();
    target.components = vec![Component::new("R1", "0603", 10.0, 10.0)];
    target.feeders.clear();
    let report = plan_changeover(&current(), &target, 30.0).expect("planned");

    assert_eq!(report.next_feeders, 0);
    assert_eq!(report.changes, 3);
}

#[test]
fn structurally_invalid_documents_are_rejected() {
    let mut broken = next();
    broken.units = "in".to_string();
    let error = plan_changeover(&current(), &broken, 30.0).expect_err("wrong units");
    assert_eq!(error.code, "document/units");

    let mut broken = current();
    broken.board.width = f64::NAN;
    let error = plan_changeover(&broken, &next(), 30.0).expect_err("broken board");
    assert_eq!(error.code, "board/invalid");
}

#[test]
fn invalid_change_seconds_report_changeover_invalid() {
    for seconds in [0.0, -30.0, f64::NAN, f64::INFINITY] {
        let error = plan_changeover(&current(), &next(), seconds).expect_err("invalid seconds");
        assert_eq!(error.code, "changeover/invalid", "seconds: {seconds}");
    }
}

#[test]
fn the_plan_is_deterministic_and_leaves_both_documents_unchanged() {
    let before_current = current();
    let before_next = next();
    let first = plan_changeover(&before_current, &before_next, 30.0).expect("planned");
    let second = plan_changeover(&before_current, &before_next, 30.0).expect("planned");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(before_current, current());
    assert_eq!(before_next, next());
}
