use smt_model::{Board, Component, DEFAULT_PICK_CYCLE_HEADS, Feeder, MAX_PICK_CYCLE_HEADS, Side, SmtDocument, pick_cycle_plan};

fn document(components: Vec<Component>, feeders: Vec<Feeder>) -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = components;
    document.feeders = feeders;
    document
}

fn resistors(count: usize) -> Vec<Component> {
    (1..=count).map(|index| Component::new(format!("R{index}"), "0603", 10.0, 10.0 * index as f64)).collect()
}

#[test]
fn consecutive_same_package_picks_share_a_cycle() {
    let document = document(resistors(3), vec![Feeder::new("F1", "0603", 4.0, 100)]);
    let report = pick_cycle_plan(&document, DEFAULT_PICK_CYCLE_HEADS).expect("valid grouping");

    assert_eq!(report.format, "open-smt-pick-cycle");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!(report.heads, DEFAULT_PICK_CYCLE_HEADS);
    assert_eq!(report.components, 3);
    assert_eq!(report.picks, 3);
    assert_eq!(report.cycles, 1);
    assert_eq!(report.head_slots, 4);
    assert_eq!(report.wasted_slots, 1);
    assert!((report.utilization - 0.75).abs() < 1e-12);
    assert_eq!(report.single_head_cycles, 3);
    assert_eq!(report.cycles_saved, 2);

    assert_eq!(report.entries.len(), 1);
    let cycle = &report.entries[0];
    assert_eq!(cycle.cycle, 1);
    assert_eq!(cycle.package, "0603");
    assert_eq!(cycle.side, Side::Top);
    assert_eq!(cycle.picks, 3);
    assert_eq!(cycle.unused_slots, 1);
    assert_eq!(cycle.refdes, vec!["R1", "R2", "R3"]);
}

#[test]
fn a_package_change_starts_a_new_cycle() {
    let components = vec![
        Component::new("R1", "0603", 10.0, 10.0),
        Component::new("C1", "0805", 20.0, 10.0),
        Component::new("R2", "0603", 30.0, 10.0),
    ];
    let document = document(components, vec![Feeder::new("F1", "0603", 4.0, 100), Feeder::new("F2", "0805", 4.0, 100)]);
    let report = pick_cycle_plan(&document, DEFAULT_PICK_CYCLE_HEADS).expect("valid grouping");

    assert_eq!(report.cycles, 3);
    assert_eq!(report.picks, 3);
    let packages: Vec<&str> = report.entries.iter().map(|cycle| cycle.package.as_str()).collect();
    assert_eq!(packages, vec!["0603", "0805", "0603"]);
    for cycle in &report.entries {
        assert_eq!(cycle.picks, 1);
        assert_eq!(cycle.unused_slots, 3);
        assert_eq!(cycle.refdes.len(), 1);
    }
    assert_eq!(report.cycles_saved, 0);
}

#[test]
fn a_side_change_starts_a_new_cycle() {
    let mut bottom = Component::new("R2", "0603", 10.0, 10.0);
    bottom.side = Side::Bottom;
    let document = document(vec![Component::new("R1", "0603", 10.0, 10.0), bottom], vec![Feeder::new("F1", "0603", 4.0, 100)]);
    let report = pick_cycle_plan(&document, DEFAULT_PICK_CYCLE_HEADS).expect("valid grouping");

    assert_eq!(report.cycles, 2);
    assert_eq!(report.entries[0].side, Side::Top);
    assert_eq!(report.entries[1].side, Side::Bottom);
    assert_eq!(report.entries[0].refdes, vec!["R1"]);
    assert_eq!(report.entries[1].refdes, vec!["R2"]);
}

#[test]
fn one_head_has_one_cycle_per_pick_and_saves_nothing() {
    let document = document(resistors(3), vec![Feeder::new("F1", "0603", 4.0, 100)]);
    let report = pick_cycle_plan(&document, 1).expect("valid grouping");

    assert_eq!(report.heads, 1);
    assert_eq!(report.cycles, 3);
    assert_eq!(report.head_slots, 3);
    assert_eq!(report.wasted_slots, 0);
    assert_eq!(report.utilization, 1.0);
    assert_eq!(report.single_head_cycles, 3);
    assert_eq!(report.cycles_saved, 0);
    for cycle in &report.entries {
        assert_eq!(cycle.picks, 1);
        assert_eq!(cycle.unused_slots, 0);
    }
}

#[test]
fn a_partial_last_cycle_records_the_unused_head_slots() {
    let document = document(resistors(5), vec![Feeder::new("F1", "0603", 4.0, 100)]);
    let report = pick_cycle_plan(&document, 4).expect("valid grouping");

    assert_eq!(report.components, 5);
    assert_eq!(report.cycles, 2);
    assert_eq!(report.head_slots, 8);
    assert_eq!(report.wasted_slots, 3);
    assert!((report.utilization - 5.0 / 8.0).abs() < 1e-12);
    assert_eq!(report.entries[0].picks, 4);
    assert_eq!(report.entries[0].unused_slots, 0);
    assert_eq!(report.entries[1].picks, 1);
    assert_eq!(report.entries[1].unused_slots, 3);
    assert_eq!(report.cycles_saved, 3);
}

#[test]
fn an_empty_document_reports_zero_cycles() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let report = pick_cycle_plan(&document, DEFAULT_PICK_CYCLE_HEADS).expect("valid grouping");

    assert_eq!(report.components, 0);
    assert_eq!(report.cycles, 0);
    assert_eq!(report.picks, 0);
    assert_eq!(report.head_slots, 0);
    assert_eq!(report.utilization, 0.0);
    assert_eq!(report.wasted_slots, 0);
    assert_eq!(report.single_head_cycles, 0);
    assert_eq!(report.cycles_saved, 0);
    assert!(report.entries.is_empty());
}

#[test]
fn invalid_head_counts_report_pick_cycle_invalid() {
    let document = document(resistors(2), vec![Feeder::new("F1", "0603", 4.0, 100)]);
    for heads in [0, MAX_PICK_CYCLE_HEADS + 1] {
        let error = pick_cycle_plan(&document, heads).expect_err("invalid head count");
        assert_eq!(error.code, "pick-cycle/invalid", "heads: {heads}");
    }
}

#[test]
fn missing_feeder_coverage_fails_like_the_placement_report() {
    let document = document(resistors(2), Vec::new());
    let error = pick_cycle_plan(&document, DEFAULT_PICK_CYCLE_HEADS).expect_err("no feeders");

    assert_eq!(error.code, "feeder/missing");
}

#[test]
fn the_report_is_deterministic_and_leaves_the_document_unchanged() {
    let mut document = document(resistors(5), vec![Feeder::new("F1", "0603", 4.0, 100)]);
    document.revision = 7;
    let before = serde_json::to_string(&document).expect("json");

    let first = pick_cycle_plan(&document, 2).expect("valid grouping");
    let second = pick_cycle_plan(&document, 2).expect("valid grouping");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(serde_json::to_string(&document).expect("json"), before);
    assert_eq!(document.revision, 7);
}
