use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

use smt_model::{Board, Component, Feeder, FeederUpdate, OpenSmt, Side, SmtDocument, feeder_loading_report};

fn workspace(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("open-smt-{}-{name}", process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("create test directory");
    directory
}

fn read_document(path: &Path) -> SmtDocument {
    serde_json::from_str(&fs::read_to_string(path).expect("document text")).expect("parse document")
}

fn seeded(name: &str) -> (PathBuf, OpenSmt) {
    let path = workspace(name).join("board.osmt");
    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    (path, session)
}

fn document() -> SmtDocument {
    SmtDocument::new(Board::new(100.0, 80.0, 1.6))
}

fn add(document: &mut SmtDocument, refdes: &str, package: &str, side: Side) {
    let mut component = Component::new(refdes, package, 10.0, 10.0);
    component.side = side;
    document.components.push(component);
}

#[test]
fn packages_are_slotted_in_name_order() {
    let mut document = document();
    add(&mut document, "U1", "QFN-32", Side::Top);
    add(&mut document, "R1", "0603", Side::Top);
    add(&mut document, "R2", "0603", Side::Top);
    add(&mut document, "C1", "0805", Side::Top);

    let report = feeder_loading_report(&document);
    assert_eq!(report.format, "open-smt-feeder-report");
    assert_eq!(report.version, 1);
    assert_eq!(report.total_placements, 4);
    assert_eq!(report.unique_packages, 3);
    let packages: Vec<(&str, u32, u32)> =
        report.slots.iter().map(|slot| (slot.package.as_str(), slot.slot, slot.count)).collect();
    assert_eq!(packages, vec![("0603", 1, 2), ("0805", 2, 1), ("QFN-32", 3, 1)]);
}

#[test]
fn refdes_keep_document_order() {
    let mut document = document();
    add(&mut document, "R2", "0603", Side::Top);
    add(&mut document, "R1", "0603", Side::Top);
    add(&mut document, "C3", "0603", Side::Top);

    let report = feeder_loading_report(&document);
    assert_eq!(report.slots.len(), 1);
    assert_eq!(report.slots[0].refdes, vec!["R2", "R1", "C3"]);
}

#[test]
fn sides_are_deduplicated_and_sorted() {
    let mut document = document();
    add(&mut document, "R1", "0603", Side::Top);
    add(&mut document, "R2", "0603", Side::Bottom);
    add(&mut document, "R3", "0603", Side::Top);

    let report = feeder_loading_report(&document);
    assert_eq!(report.slots[0].sides, vec!["bottom", "top"]);
}

#[test]
fn an_empty_document_has_no_slots() {
    let report = feeder_loading_report(&document());
    assert_eq!(report.total_placements, 0);
    assert_eq!(report.unique_packages, 0);
    assert!(report.slots.is_empty());
}

#[test]
fn the_report_is_deterministic() {
    let mut document = document();
    add(&mut document, "U1", "QFN-32", Side::Bottom);
    add(&mut document, "R1", "0603", Side::Top);
    add(&mut document, "C1", "0805", Side::Bottom);

    let first = serde_json::to_string(&feeder_loading_report(&document)).expect("json");
    let second = serde_json::to_string(&feeder_loading_report(&document)).expect("json");
    assert_eq!(first, second);
}

#[test]
fn update_feeder_changes_values_and_keeps_untouched_fields() {
    let (path, mut session) = seeded("feeder-update-values");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 10)).expect("F1");
    let before = session.summary().expect("summary");

    let summary = session.update_feeder("F1", FeederUpdate { pitch: Some(8.0), ..FeederUpdate::default() }).expect("pitch");
    assert_eq!(summary.revision, before.revision + 1);

    let document = read_document(&path);
    assert_eq!(document.revision, summary.revision);
    let feeder = &document.feeders[0];
    assert_eq!(feeder.id, "F1");
    assert_eq!(feeder.package, "0603");
    assert_eq!(feeder.pitch, 8.0);
    assert_eq!(feeder.capacity, 10);
    assert_eq!(feeder.loaded, 10);
    assert_eq!(document.components[0], Component::new("R1", "0603", 10.0, 10.0));

    session
        .update_feeder("F1", FeederUpdate { capacity: Some(20), loaded: Some(15), ..FeederUpdate::default() })
        .expect("capacity and loaded");
    let reopened = OpenSmt::new().open(&path).expect("reopen");
    assert_eq!(reopened.revision, before.revision + 2);
    let document = read_document(&path);
    let feeder = &document.feeders[0];
    assert_eq!(feeder.pitch, 8.0);
    assert_eq!(feeder.capacity, 20);
    assert_eq!(feeder.loaded, 15);
}

#[test]
fn update_feeder_moves_a_feeder_and_recomputed_reports_follow() {
    let (path, mut session) = seeded("feeder-update-move");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    session.add_component(Component::new("C1", "0805", 20.0, 20.0)).expect("C1");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 10)).expect("F1");
    session.add_feeder(Feeder::new("F2", "0603", 4.0, 10)).expect("F2");
    session.add_feeder(Feeder::new("F3", "0805", 4.0, 10)).expect("F3");

    let before = session.feeders().expect("allocation before");
    assert!(before.feeders.iter().find(|feeder| feeder.id == "F2").expect("F2").refdes.is_empty());

    session
        .update_feeder("F2", FeederUpdate { package: Some("0805".to_string()), ..FeederUpdate::default() })
        .expect("move F2");

    let after = session.feeders().expect("allocation after");
    let moved = after.feeders.iter().find(|feeder| feeder.id == "F2").expect("F2");
    assert_eq!(moved.package, "0805");
    assert_eq!(moved.refdes, vec!["C1".to_string()]);
    assert_eq!(after.feeders.iter().find(|feeder| feeder.id == "F1").expect("F1").refdes, vec!["R1".to_string()]);
    assert!(after.feeders.iter().find(|feeder| feeder.id == "F3").expect("F3").refdes.is_empty());

    let report = session.feeder_report().expect("loading report");
    let packages: Vec<(&str, u32)> = report.slots.iter().map(|slot| (slot.package.as_str(), slot.count)).collect();
    assert_eq!(packages, vec![("0603", 1), ("0805", 1)]);
    let stored = read_document(&path);
    assert_eq!(stored.feeders.iter().find(|feeder| feeder.id == "F2").expect("F2").package, "0805");
}

#[test]
fn update_feeder_rejects_invalid_requests_without_writing() {
    let (path, mut session) = seeded("feeder-update-invalid");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 10)).expect("F1");
    let before = session.summary().expect("summary");

    let error =
        session.update_feeder("F9", FeederUpdate { capacity: Some(20), ..FeederUpdate::default() }).expect_err("unknown id");
    assert_eq!(error.code, "feeder/unknown-id");

    let error = session.update_feeder("F1", FeederUpdate::default()).expect_err("empty request");
    assert_eq!(error.code, "feeder/invalid");

    let error = session
        .update_feeder("F1", FeederUpdate { capacity: Some(5), ..FeederUpdate::default() })
        .expect_err("capacity below loaded");
    assert_eq!(error.code, "feeder/invalid");

    let error = session
        .update_feeder("F1", FeederUpdate { loaded: Some(11), ..FeederUpdate::default() })
        .expect_err("loaded above capacity");
    assert_eq!(error.code, "feeder/invalid");

    for pitch in [0.0, -4.0] {
        let error = session
            .update_feeder("F1", FeederUpdate { pitch: Some(pitch), ..FeederUpdate::default() })
            .expect_err("invalid pitch");
        assert_eq!(error.code, "feeder/invalid");
    }

    let error = session
        .update_feeder("F1", FeederUpdate { package: Some("  ".to_string()), ..FeederUpdate::default() })
        .expect_err("empty package");
    assert_eq!(error.code, "feeder/invalid");

    let error =
        session.update_feeder("F1", FeederUpdate { capacity: Some(0), ..FeederUpdate::default() }).expect_err("zero capacity");
    assert_eq!(error.code, "feeder/invalid");

    let after = session.summary().expect("summary");
    assert_eq!(after.revision, before.revision);
    let stored = read_document(&path);
    assert_eq!(stored.revision, before.revision);
    assert_eq!(stored.feeders[0], Feeder::new("F1", "0603", 4.0, 10));
}

#[test]
fn update_feeder_is_deterministic_and_round_trips() {
    let directory = workspace("feeder-update-determinism");
    let first = directory.join("first.osmt");
    let second = directory.join("second.osmt");
    for path in [&first, &second] {
        let mut session = OpenSmt::new();
        session.create(path).expect("create");
        session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
        session.add_component(Component::new("C1", "0805", 20.0, 20.0)).expect("C1");
        session.add_feeder(Feeder::new("F1", "0603", 4.0, 10)).expect("F1");
        session.add_feeder(Feeder::new("F2", "0805", 4.0, 10)).expect("F2");
        session
            .update_feeder(
                "F2",
                FeederUpdate { pitch: Some(8.0), capacity: Some(20), loaded: Some(5), ..FeederUpdate::default() },
            )
            .expect("update");
    }
    assert_eq!(fs::read(&first).expect("first"), fs::read(&second).expect("second"));

    let reopened = OpenSmt::new().open(&first).expect("reopen");
    assert_eq!(reopened.revision, 5);
    let document = read_document(&first);
    let feeder = &document.feeders[1];
    assert_eq!(feeder.id, "F2");
    assert_eq!(feeder.pitch, 8.0);
    assert_eq!(feeder.capacity, 20);
    assert_eq!(feeder.loaded, 5);
}
