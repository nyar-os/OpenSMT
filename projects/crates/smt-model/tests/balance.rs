use std::collections::BTreeSet;

use smt_model::{Board, Component, Feeder, Side, SmtDocument, allocate_feeders, balance, sequence_placement};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components.push(Component::new("R1", "0603", 10.0, 10.0));
    document.components.push(Component::new("R2", "0603", 50.0, 10.0));
    document.components.push(Component::new("C1", "0603", 50.0, 50.0));
    let mut u1 = Component::new("U1", "QFN-32", 20.0, 20.0);
    u1.side = Side::Bottom;
    document.components.push(u1);
    document.feeders.push(Feeder::new("F1", "0603", 4.0, 100));
    document.feeders.push(Feeder::new("F2", "QFN-32", 12.0, 20));
    document
}

fn refdes(document: &SmtDocument) -> Vec<&str> {
    document.components.iter().map(|component| component.refdes.as_str()).collect()
}

#[test]
fn a_single_machine_reproduces_the_placement_program() {
    let document = document();
    let feeders = allocate_feeders(&document).expect("valid");
    let placement = sequence_placement(&document, &feeders).expect("sequence");
    let report = balance(&document, 1).expect("balance");

    assert_eq!(report.format, "open-smt-balance");
    assert_eq!(report.version, 1);
    assert_eq!(report.machines.len(), 1);

    let machine = &report.machines[0];
    assert_eq!(machine.index, 0);
    let order: Vec<&str> = machine.components.iter().map(String::as_str).collect();
    let expected: Vec<&str> = placement.entries.iter().map(|entry| entry.refdes.as_str()).collect();
    assert_eq!(order, expected);
    assert!((machine.travel - placement.travel_length).abs() < 1e-9);
    assert!((machine.seconds - placement.placement_seconds).abs() < 1e-9);
    assert!((report.total_seconds - placement.placement_seconds).abs() < 1e-9);
    assert!((report.max_seconds - machine.seconds).abs() < 1e-9);
    assert!((report.min_seconds - machine.seconds).abs() < 1e-9);
    assert_eq!(report.imbalance, 0.0);
    assert!(report.ok);
}

#[test]
fn every_component_is_assigned_exactly_once() {
    let document = document();
    let report = balance(&document, 2).expect("balance");

    let mut assigned: Vec<&str> =
        report.machines.iter().flat_map(|machine| machine.components.iter().map(String::as_str)).collect();
    let total = assigned.len();
    assigned.sort_unstable();
    let unique: BTreeSet<&str> = assigned.iter().copied().collect();

    let mut expected = refdes(&document);
    expected.sort_unstable();
    assert_eq!(assigned, expected);
    assert_eq!(unique.len(), total);
    assert_eq!(total, document.components.len());
}

#[test]
fn each_machine_runs_its_top_side_before_its_bottom_side() {
    let document = document();
    let report = balance(&document, 2).expect("balance");
    for machine in &report.machines {
        let mut seen_bottom = false;
        for refdes in &machine.components {
            let component = document.components.iter().find(|entry| entry.refdes == *refdes).expect("known component");
            if component.side == Side::Bottom {
                seen_bottom = true;
            } else {
                assert!(!seen_bottom, "machine {} placed {refdes} after a bottom-side component", machine.index);
            }
        }
    }
}

#[test]
fn balancing_is_deterministic() {
    let document = document();
    let first = balance(&document, 3).expect("balance");
    let second = balance(&document, 3).expect("balance");
    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
}

#[test]
fn more_machines_than_components_keeps_every_machine() {
    let document = document();
    let report = balance(&document, 10).expect("balance");

    assert_eq!(report.machines.len(), 10);
    assert!(report.machines.iter().any(|machine| machine.components.is_empty()));
    for machine in &report.machines {
        assert_eq!(machine.feeders.len(), document.feeders.len());
        if machine.components.is_empty() {
            assert_eq!(machine.travel, 0.0);
            assert_eq!(machine.seconds, 0.0);
        }
    }
    assert_eq!(report.min_seconds, 0.0);
    assert!(report.max_seconds > 0.0);
    assert!(report.imbalance > 0.0 && report.imbalance <= 1.0);
}

#[test]
fn an_invalid_machine_count_is_rejected() {
    let document = document();
    let error = balance(&document, 0).expect_err("zero machines");
    assert_eq!(error.code, "balance/invalid-machines");
    let error = balance(&document, 65).expect_err("too many machines");
    assert_eq!(error.code, "balance/invalid-machines");
}

#[test]
fn each_machine_flags_its_own_feeder_shortage() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    for index in 0..4 {
        document.components.push(Component::new(format!("R{index}"), "0603", 10.0, 10.0));
    }
    let mut feeder = Feeder::new("F1", "0603", 4.0, 10);
    feeder.loaded = 1;
    document.feeders.push(feeder);

    let report = balance(&document, 2).expect("balance");
    assert!(!report.ok);
    for machine in &report.machines {
        assert_eq!(machine.feeders.len(), 1);
        let feeder = &machine.feeders[0];
        assert_eq!(feeder.id, "F1");
        assert_eq!(feeder.loaded, 1);
        assert_eq!(feeder.assigned, 2);
        assert!(!feeder.ok);
    }
}

#[test]
fn balancing_does_not_modify_the_document() {
    let document = document();
    let before = serde_json::to_string(&document).expect("json");
    let _ = balance(&document, 2).expect("balance");
    assert_eq!(before, serde_json::to_string(&document).expect("json"));
}
