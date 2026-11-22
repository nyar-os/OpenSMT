use smt_model::{Board, Component, Feeder, ReflowProfile, SmtDocument, validate::validate};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components.push(Component::new("R1", "0603", 10.0, 10.0));
    document.feeders.push(Feeder::new("F1", "0603", 4.0, 100));
    document
}

#[test]
fn an_empty_document_is_valid_with_warnings() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let report = validate(&document);
    assert!(report.ok);
    assert!(report.issues.iter().any(|issue| issue.code == "component/none"));
    assert!(report.issues.iter().any(|issue| issue.code == "feeder/none"));
}

#[test]
fn a_component_outside_the_board_is_an_error() {
    let mut document = document();
    document.components.push(Component::new("R2", "0603", 120.0, 10.0));
    let report = validate(&document);
    assert!(!report.ok);
    assert!(report.issues.iter().any(|issue| issue.code == "component/outside-board"));
}

#[test]
fn duplicate_reference_designators_are_errors() {
    let mut document = document();
    document.components.push(Component::new("R1", "0603", 20.0, 20.0));
    let report = validate(&document);
    assert!(!report.ok);
    assert!(report.issues.iter().any(|issue| issue.code == "component/duplicate-refdes"));
}

#[test]
fn a_used_package_without_a_feeder_is_an_error() {
    let mut document = document();
    document.components.push(Component::new("U1", "QFN-32", 20.0, 20.0));
    let report = validate(&document);
    assert!(!report.ok);
    assert!(report.issues.iter().any(|issue| issue.code == "feeder/missing"));
}

#[test]
fn insufficient_loaded_parts_is_a_capacity_error() {
    let mut document = document();
    document.components.push(Component::new("R2", "0603", 20.0, 20.0));
    let mut feeder = Feeder::new("F1", "0603", 4.0, 100);
    feeder.loaded = 1;
    document.feeders = vec![feeder];
    let report = validate(&document);
    assert!(!report.ok);
    assert!(report.issues.iter().any(|issue| issue.code == "feeder/capacity"));
}

#[test]
fn a_feeder_cannot_be_loaded_beyond_capacity() {
    let mut document = document();
    let mut feeder = Feeder::new("F1", "0603", 4.0, 10);
    feeder.loaded = 20;
    document.feeders = vec![feeder];
    let report = validate(&document);
    assert!(!report.ok);
    assert!(report.issues.iter().any(|issue| issue.code == "feeder/invalid"));
}

#[test]
fn duplicate_feeder_ids_are_errors() {
    let mut document = document();
    document.feeders.push(Feeder::new("F1", "0603", 4.0, 100));
    let report = validate(&document);
    assert!(!report.ok);
    assert!(report.issues.iter().any(|issue| issue.code == "feeder/duplicate-id"));
}

#[test]
fn an_empty_profile_is_an_error() {
    let mut document = document();
    document.profile = ReflowProfile { conveyor_speed: 600.0, liquidus: 217.0, zones: vec![] };
    let report = validate(&document);
    assert!(!report.ok);
    assert!(report.issues.iter().any(|issue| issue.code == "profile/invalid"));
}

#[test]
fn a_non_right_angle_rotation_is_only_a_warning() {
    let mut document = document();
    document.components[0].rotation = 45.0;
    let report = validate(&document);
    assert!(report.ok);
    assert!(report.issues.iter().any(|issue| issue.code == "component/rotation"));
}
