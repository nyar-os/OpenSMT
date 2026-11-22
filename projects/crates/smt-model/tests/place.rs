use smt_model::{Board, Component, Feeder, Panel, Side, SmtDocument, allocate_feeders, sequence_placement};

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

#[test]
fn feeder_allocation_follows_id_order() {
    let feeders = allocate_feeders(&document()).expect("valid document");
    assert!(feeders.unassigned.is_empty());
    let lane = feeders.feeders.iter().find(|lane| lane.id == "F1").expect("F1");
    assert_eq!(lane.assigned, 3);
    assert_eq!(lane.remaining, 97);
    assert_eq!(lane.refdes, vec!["C1", "R1", "R2"]);
}

#[test]
fn placement_visits_nearest_components_top_first() {
    let document = document();
    let feeders = allocate_feeders(&document).expect("valid");
    let placement = sequence_placement(&document, &feeders).expect("sequence");
    let order: Vec<&str> = placement.entries.iter().map(|entry| entry.refdes.as_str()).collect();
    assert_eq!(order, vec!["R1", "R2", "C1", "U1"]);
    assert_eq!(placement.entries[3].side, Side::Bottom);
    assert_eq!(placement.entries[0].feeder.as_deref(), Some("F1"));
    assert_eq!(placement.entries[3].feeder.as_deref(), Some("F2"));
    assert!((placement.travel_length - 122.4264).abs() < 1e-3);
    assert!((placement.placement_seconds - (4.0 * 0.6 + 122.4264 / 200.0)).abs() < 1e-3);
}

#[test]
fn placement_is_deterministic() {
    let document = document();
    let first = sequence_placement(&document, &allocate_feeders(&document).expect("valid")).expect("sequence");
    let second = sequence_placement(&document, &allocate_feeders(&document).expect("valid")).expect("sequence");
    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
}

#[test]
fn panel_dimensions_include_gaps() {
    let mut document = document();
    document.panel = Panel::new(2, 3, 5.0, 4.0);
    let feeders = allocate_feeders(&document).expect("valid");
    let placement = sequence_placement(&document, &feeders).expect("sequence");
    assert_eq!(placement.board_count, 6);
    assert!((placement.panel_width - 310.0).abs() < 1e-9);
    assert!((placement.panel_depth - 164.0).abs() < 1e-9);
}

#[test]
fn multiple_feeders_split_one_package_by_capacity() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    for index in 0..4 {
        document.components.push(Component::new(format!("R{index}"), "0603", 10.0 + index as f64, 10.0));
    }
    let mut first = Feeder::new("F1", "0603", 4.0, 10);
    first.loaded = 2;
    document.feeders.push(first);
    document.feeders.push(Feeder::new("F2", "0603", 4.0, 10));
    let feeders = allocate_feeders(&document).expect("valid");
    let f1 = feeders.feeders.iter().find(|lane| lane.id == "F1").expect("F1");
    let f2 = feeders.feeders.iter().find(|lane| lane.id == "F2").expect("F2");
    assert_eq!(f1.assigned, 2);
    assert_eq!(f2.assigned, 2);
}
