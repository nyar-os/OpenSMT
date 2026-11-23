use std::{fs, path::PathBuf, process};

use smt_model::{Board, Component, OpenSmt, SmtDocument, bom_check};

fn document() -> SmtDocument {
    SmtDocument::new(Board::new(100.0, 80.0, 1.6))
}

fn placed(components: &[(&str, &str)]) -> SmtDocument {
    let mut document = document();
    document.components = components
        .iter()
        .enumerate()
        .map(|(index, (refdes, package))| Component::new(*refdes, *package, 10.0 + index as f64 * 10.0, 10.0))
        .collect();
    document
}

fn workspace(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("open-smt-{}-{name}", process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("create test directory");
    directory
}

#[test]
fn a_matching_bom_reports_no_findings() {
    let document = placed(&[("R1", "0603"), ("R2", "0603"), ("U1", "QFN-32")]);
    let report = bom_check(&document, "package,quantity,refdes\n0603,2,R1;R2\nQFN-32,1,U1\n").expect("valid check");

    assert_eq!(report.format, "open-smt-bom-check");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert!(report.ok);
    assert_eq!(report.bom_lines, 2);
    assert_eq!(report.components, 3);
    assert!(report.missing.is_empty());
    assert!(report.extra.is_empty());
    assert!(report.quantity_mismatches.is_empty());
}

#[test]
fn missing_extra_and_quantity_findings_are_reported() {
    let document = placed(&[("R1", "0603"), ("R2", "0603"), ("C1", "0805")]);
    let report = bom_check(&document, "package,quantity,refdes\n0603,3,R1;R2;R3\nQFN-32,1,U1\n").expect("valid check");

    assert!(!report.ok);
    let missing: Vec<(&str, &str)> = report.missing.iter().map(|f| (f.package.as_str(), f.refdes.as_str())).collect();
    assert_eq!(missing, vec![("0603", "R3"), ("QFN-32", "U1")]);
    let extra: Vec<(&str, &str)> = report.extra.iter().map(|f| (f.package.as_str(), f.refdes.as_str())).collect();
    assert_eq!(extra, vec![("0805", "C1")]);
    let quantities: Vec<(&str, u32, u32, i64)> = report
        .quantity_mismatches
        .iter()
        .map(|m| (m.package.as_str(), m.bom_quantity, m.placed_quantity, m.difference))
        .collect();
    assert_eq!(quantities, vec![("0603", 3, 2, -1), ("0805", 0, 1, 1), ("QFN-32", 1, 0, -1)]);
}

#[test]
fn a_refdes_placed_under_another_package_is_missing_and_extra() {
    let document = placed(&[("R1", "0805")]);
    let report = bom_check(&document, "package,quantity,refdes\n0603,1,R1\n").expect("valid check");

    assert!(!report.ok);
    assert_eq!(report.missing.len(), 1);
    assert_eq!((report.missing[0].package.as_str(), report.missing[0].refdes.as_str()), ("0603", "R1"));
    assert_eq!(report.extra.len(), 1);
    assert_eq!((report.extra[0].package.as_str(), report.extra[0].refdes.as_str()), ("0805", "R1"));
    assert_eq!(report.quantity_mismatches.len(), 2);
}

#[test]
fn quantity_only_bom_rows_skip_refdes_level_checks() {
    let document = placed(&[("R1", "0603"), ("R2", "0603"), ("R3", "0603")]);
    let report = bom_check(&document, "package,quantity\n0603,2\n").expect("valid check");

    assert!(!report.ok);
    assert!(report.missing.is_empty());
    assert!(report.extra.is_empty());
    assert_eq!(report.quantity_mismatches.len(), 1);
    assert_eq!((report.quantity_mismatches[0].bom_quantity, report.quantity_mismatches[0].placed_quantity), (2, 3));
    assert_eq!(report.quantity_mismatches[0].difference, 1);

    let matching = bom_check(&document, "package,quantity\n0603,3\n").expect("valid check");
    assert!(matching.ok);
}

#[test]
fn an_empty_document_and_an_empty_bom_match() {
    let report = bom_check(&document(), "package,quantity,refdes\n").expect("valid check");

    assert!(report.ok);
    assert_eq!(report.bom_lines, 0);
    assert_eq!(report.components, 0);
    assert!(report.missing.is_empty() && report.extra.is_empty() && report.quantity_mismatches.is_empty());
}

#[test]
fn headers_are_case_insensitive_and_rows_may_be_quoted() {
    let document = placed(&[("R1", "0603"), ("R2", "0603"), ("R3", "0603")]);
    let report = bom_check(&document, "Quantity,Refdes,PACKAGE\n3,\"R1;R2;R3\",0603\n").expect("valid check");

    assert!(report.ok);
    assert_eq!(report.bom_lines, 1);
}

#[test]
fn the_report_is_deterministic_and_order_independent() {
    let document = placed(&[("R1", "0603"), ("C1", "0805")]);
    let first = bom_check(&document, "package,quantity,refdes\n0603,2,R1;R2\n0805,1,C1\n").expect("valid check");
    let second = bom_check(&document, "package,quantity,refdes\n0603,2,R1;R2\n0805,1,C1\n").expect("valid check");
    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));

    let shuffled = bom_check(&document, "package,quantity,refdes\n0805,1,C1\n0603,2,R1;R2\n").expect("valid check");
    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&shuffled).expect("json"));
}

#[test]
fn invalid_bom_files_report_stable_codes() {
    let document = document();
    let cases: Vec<(&str, &str)> = vec![
        ("", "bom/parse"),
        ("\"unclosed\n", "bom/parse"),
        ("package\n0603\n", "bom/missing-column"),
        ("package,quantity,footprint\n0603,2,FP\n", "bom/unknown-column"),
        ("package,quantity,package\n0603,2,0805\n", "bom/duplicate-column"),
        ("package,,quantity\n0603,2\n", "bom/invalid-header"),
        ("package,quantity\n0603\n", "bom/invalid-row"),
        ("package,quantity\n,2\n", "bom/invalid-value"),
        ("package,quantity\n0603,two\n", "bom/invalid-number"),
        ("package,quantity\n0603,0\n", "bom/invalid-value"),
        ("package,quantity,refdes\n0603,2,R1;R1\n", "bom/duplicate-refdes"),
        ("package,quantity,refdes\n0603,1,R1\n0805,1,R1\n", "bom/duplicate-refdes"),
    ];
    for (source, code) in cases {
        let error = bom_check(&document, source).expect_err("invalid BOM");
        assert_eq!(error.code, code, "source `{source}`");
    }
}

#[test]
fn the_check_never_changes_the_document() {
    let directory = workspace("bom");
    let path = directory.join("panel.osmt");
    let bom_path = directory.join("bom.csv");
    fs::write(&bom_path, "package,quantity,refdes\n0603,2,R1;R2\n").expect("write bom");

    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    let before = fs::read_to_string(&path).expect("document bytes");

    let report = session.bom_check(&bom_path).expect("check");
    assert!(!report.ok);
    assert_eq!(report.missing.len(), 1);
    assert_eq!(session.summary().expect("summary").revision, 1);
    assert_eq!(fs::read_to_string(&path).expect("document bytes"), before);
}

#[test]
fn a_missing_bom_file_reports_io_read() {
    let directory = workspace("bom-missing");
    let path = directory.join("panel.osmt");
    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    let error = session.bom_check(&directory.join("absent.csv")).expect_err("absent file");
    assert_eq!(error.code, "io/read");
}
