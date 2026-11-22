use smt_model::{Board, Component, Side, SmtDocument, feeder_loading_report};

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
