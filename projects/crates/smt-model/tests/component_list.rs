use smt_model::{Board, Component, Feeder, Side, SmtDocument, allocate_feeders, component_list_report};

fn document() -> SmtDocument {
    SmtDocument::new(Board::new(100.0, 80.0, 1.6))
}

fn add(document: &mut SmtDocument, refdes: &str, package: &str, side: Side) {
    let mut component = Component::new(refdes, package, 10.0, 10.0);
    component.side = side;
    document.components.push(component);
}

#[test]
fn components_keep_document_order_and_packages_keep_first_seen_order() {
    let mut document = document();
    add(&mut document, "U1", "QFN-32", Side::Top);
    add(&mut document, "R2", "0603", Side::Bottom);
    add(&mut document, "R1", "0603", Side::Top);
    add(&mut document, "C1", "0805", Side::Top);

    let report = component_list_report(&document);
    assert_eq!(report.format, "open-smt-component-list");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!(report.component_count, 4);
    assert_eq!(report.unassigned_count, 4);

    let order: Vec<&str> = report.components.iter().map(|row| row.refdes.as_str()).collect();
    assert_eq!(order, vec!["U1", "R2", "R1", "C1"]);
    assert_eq!(report.components[1].side, Side::Bottom);

    let packages: Vec<(&str, u32, Vec<&str>)> = report
        .packages
        .iter()
        .map(|entry| (entry.package.as_str(), entry.count, entry.refdes.iter().map(String::as_str).collect()))
        .collect();
    assert_eq!(packages, vec![("QFN-32", 1, vec!["U1"]), ("0603", 2, vec!["R2", "R1"]), ("0805", 1, vec!["C1"])]);
    for row in &report.components {
        assert_eq!(row.feeder, None);
    }
}

#[test]
fn height_is_omitted_when_unknown_and_reported_when_present() {
    let mut document = document();
    document.components.push(Component::new("R1", "0603", 10.0, 10.0));
    let mut tall = Component::new("R2", "0603", 20.0, 10.0);
    tall.height = Some(1.2);
    document.components.push(tall);

    let report = component_list_report(&document);
    assert_eq!(report.components[0].height, None);
    assert_eq!(report.components[1].height, Some(1.2));

    let json = serde_json::to_value(&report).expect("serialize report");
    let rows = json["components"].as_array().expect("component rows");
    assert!(rows[0].get("height").is_none());
    assert_eq!(rows[1]["height"], serde_json::json!(1.2));
    assert!(rows[0].get("feeder").is_some());
    assert_eq!(rows[0]["feeder"], serde_json::Value::Null);
}

#[test]
fn a_package_is_split_across_loaded_lanes_like_the_feeders_report() {
    let mut document = document();
    add(&mut document, "R1", "0603", Side::Top);
    add(&mut document, "R2", "0603", Side::Top);
    add(&mut document, "R3", "0603", Side::Top);
    let mut first = Feeder::new("F1", "0603", 4.0, 10);
    first.loaded = 1;
    document.feeders.push(first);
    let mut second = Feeder::new("F2", "0603", 4.0, 10);
    second.loaded = 2;
    document.feeders.push(second);

    let report = component_list_report(&document);
    assert_eq!(report.unassigned_count, 0);
    let feeders: Vec<(Option<&str>, Option<&str>)> =
        report.components.iter().map(|row| (Some(row.refdes.as_str()), row.feeder.as_deref())).collect();
    assert_eq!(feeders, vec![(Some("R1"), Some("F1")), (Some("R2"), Some("F2")), (Some("R3"), Some("F2"))]);

    let allocation = allocate_feeders(&document).expect("valid allocation");
    for lane in &allocation.feeders {
        for refdes in &lane.refdes {
            let row = report.components.iter().find(|row| &row.refdes == refdes).expect("listed component");
            assert_eq!(row.feeder.as_deref(), Some(lane.id.as_str()));
        }
    }
}

#[test]
fn a_component_without_a_loaded_lane_is_unassigned() {
    let mut document = document();
    add(&mut document, "R1", "0603", Side::Top);
    add(&mut document, "R2", "0603", Side::Top);
    let mut feeder = Feeder::new("F1", "0603", 4.0, 10);
    feeder.loaded = 1;
    document.feeders.push(feeder);

    let report = component_list_report(&document);
    assert_eq!(report.unassigned_count, 1);
    assert_eq!(report.components[0].feeder.as_deref(), Some("F1"));
    assert_eq!(report.components[1].feeder, None);
}

#[test]
fn an_empty_document_reports_zero_counts_and_no_rows() {
    let report = component_list_report(&document());
    assert_eq!(report.component_count, 0);
    assert_eq!(report.unassigned_count, 0);
    assert!(report.packages.is_empty());
    assert!(report.components.is_empty());
}

#[test]
fn the_report_is_deterministic_and_leaves_the_document_unchanged() {
    let mut document = document();
    add(&mut document, "R1", "0603", Side::Top);
    add(&mut document, "C1", "0805", Side::Bottom);
    document.feeders.push(Feeder::new("F1", "0603", 4.0, 10));

    let before = serde_json::to_string(&document).expect("serialize document");
    let first = serde_json::to_string(&component_list_report(&document)).expect("serialize report");
    let second = serde_json::to_string(&component_list_report(&document)).expect("serialize report");
    let after = serde_json::to_string(&document).expect("serialize document");

    assert_eq!(first, second);
    assert_eq!(before, after);
}
