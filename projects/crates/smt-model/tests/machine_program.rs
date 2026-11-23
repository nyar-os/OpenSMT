use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process,
};

use smt_model::{
    Board, Component, Feeder, OpenSmt, Side, SmtDocument, build_machine_program, parse_machine_program, render_machine_program,
    verify_machine_program,
};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let mut r2 = Component::new("R2", "0603", 50.0, 10.0);
    r2.rotation = 90.0;
    let mut c1 = Component::new("C1", "0805", 20.0, 20.0);
    c1.side = Side::Bottom;
    c1.rotation = 270.0;
    document.components = vec![Component::new("R1", "0603", 10.0, 10.0), r2, c1];
    document.feeders = vec![Feeder::new("F1", "0603", 4.0, 100), Feeder::new("F2", "0805", 8.0, 50)];
    document
}

fn nozzles() -> BTreeMap<String, String> {
    BTreeMap::from([("0603".to_string(), "N1".to_string()), ("0805".to_string(), "N2".to_string())])
}

fn workspace(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("open-smt-machine-{}-{name}", process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("create test directory");
    directory
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("program text")
}

#[test]
fn the_program_lists_every_step_with_station_pitch_nozzle_and_speed() {
    let document = document();
    let program = build_machine_program(&document, &nozzles()).expect("built");

    assert_eq!(program.format, "open-smt-machine-program");
    assert_eq!(program.version, 1);
    assert_eq!(program.units, "mm");
    assert_eq!(program.head_speed, 200.0);
    assert_eq!(program.components, 3);
    assert_eq!(program.station_count, 2);

    assert_eq!(program.steps.len(), 3);
    assert_eq!(program.steps[0].refdes, "R1");
    assert_eq!(program.steps[0].sequence, 1);
    assert_eq!(program.steps[0].side, Side::Top);
    assert_eq!(program.steps[0].station, 1);
    assert!((program.steps[0].pitch - 4.0).abs() < 1e-9);
    assert_eq!(program.steps[0].nozzle.as_deref(), Some("N1"));
    assert_eq!(program.steps[1].refdes, "R2");
    assert_eq!(program.steps[1].sequence, 2);
    assert_eq!(program.steps[1].station, 1);
    assert_eq!(program.steps[1].nozzle.as_deref(), Some("N1"));
    assert_eq!(program.steps[2].refdes, "C1");
    assert_eq!(program.steps[2].sequence, 3);
    assert_eq!(program.steps[2].side, Side::Bottom);
    assert_eq!(program.steps[2].station, 2);
    assert!((program.steps[2].pitch - 8.0).abs() < 1e-9);
    assert_eq!(program.steps[2].nozzle.as_deref(), Some("N2"));
    assert!((program.steps[0].x - 10.0).abs() < 1e-9);
    assert!((program.steps[1].rotation - 90.0).abs() < 1e-9);
    assert!((program.steps[2].rotation - 270.0).abs() < 1e-9);
}

#[test]
fn the_csv_uses_the_fixed_header_and_program_order() {
    let document = document();
    let program = build_machine_program(&document, &nozzles()).expect("built");
    let text = render_machine_program(&program);

    assert_eq!(
        text,
        "sequence,side,refdes,package,x_mm,y_mm,rotation_deg,feeder,station,pitch_mm,nozzle,speed_mm_s\n\
         1,top,R1,0603,10,10,0,F1,1,4,N1,200\n\
         2,top,R2,0603,50,10,90,F1,1,4,N1,200\n\
         3,bottom,C1,0805,20,20,270,F2,2,8,N2,200\n"
    );
}

#[test]
fn render_and_parse_round_trip_without_a_difference() {
    let document = document();
    let program = build_machine_program(&document, &nozzles()).expect("built");
    let parsed = parse_machine_program(&render_machine_program(&program)).expect("parsed");

    assert_eq!(parsed.steps, program.steps);
    assert_eq!(parsed.components, program.components);
    assert!(verify_machine_program(&program, &parsed).is_empty());
}

#[test]
fn coordinates_are_canonical_to_three_decimals() {
    let mut document = document();
    document.components[0].x = 10.12349;
    let program = build_machine_program(&document, &nozzles()).expect("built");
    assert!((program.steps[0].x - 10.123).abs() < 1e-12);

    let parsed = parse_machine_program(&render_machine_program(&program)).expect("parsed");
    assert_eq!(parsed.steps[0].x, 10.123);
    assert!(verify_machine_program(&program, &parsed).is_empty());
}

#[test]
fn a_tampered_file_reports_the_field_and_step() {
    let document = document();
    let program = build_machine_program(&document, &nozzles()).expect("built");
    let text = render_machine_program(&program).replace("2,top,R2,0603,50,10,90", "2,top,R2,0603,55,10,90");
    let parsed = parse_machine_program(&text).expect("parsed");
    let mismatches = verify_machine_program(&program, &parsed);

    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].sequence, 2);
    assert_eq!(mismatches[0].field, "x");
    assert_eq!(mismatches[0].expected, "50");
    assert_eq!(mismatches[0].actual, "55");
}

#[test]
fn a_reordered_file_is_reported_step_by_step() {
    let document = document();
    let program = build_machine_program(&document, &nozzles()).expect("built");
    let text = render_machine_program(&program);
    let mut lines: Vec<&str> = text.lines().collect();
    lines.swap(1, 2);
    let parsed = parse_machine_program(&lines.join("\n")).expect("parsed");
    let mismatches = verify_machine_program(&program, &parsed);

    assert!(mismatches.iter().any(|mismatch| mismatch.field == "refdes"));
    assert!(mismatches.iter().any(|mismatch| mismatch.sequence == 1));
}

#[test]
fn a_missing_or_extra_step_is_a_program_level_mismatch() {
    let document = document();
    let program = build_machine_program(&document, &nozzles()).expect("built");
    let text = render_machine_program(&program);
    let mut lines: Vec<&str> = text.lines().collect();
    lines.pop();
    let parsed = parse_machine_program(&lines.join("\n")).expect("parsed");
    let mismatches = verify_machine_program(&program, &parsed);

    assert_eq!(mismatches[0].sequence, 0);
    assert_eq!(mismatches[0].field, "steps");
    assert_eq!(mismatches[0].expected, "3");
    assert_eq!(mismatches[0].actual, "2");
}

#[test]
fn malformed_programs_report_program_invalid() {
    let cases = [
        ("", "empty"),
        ("sequence,side\n1,top\n", "header"),
        (
            "sequence,side,refdes,package,x_mm,y_mm,rotation_deg,feeder,station,pitch_mm,nozzle,speed_mm_s\n1,left,R1,0603,10,10,0,F1,1,4,,200\n",
            "side",
        ),
        (
            "sequence,side,refdes,package,x_mm,y_mm,rotation_deg,feeder,station,pitch_mm,nozzle,speed_mm_s\n1,top,,0603,10,10,0,F1,1,4,,200\n",
            "refdes",
        ),
        (
            "sequence,side,refdes,package,x_mm,y_mm,rotation_deg,feeder,station,pitch_mm,nozzle,speed_mm_s\n1,top,R1,0603,10,10,0,F1,0,4,,200\n",
            "station",
        ),
        (
            "sequence,side,refdes,package,x_mm,y_mm,rotation_deg,feeder,station,pitch_mm,nozzle,speed_mm_s\n1,top,R1,0603,10,10,0,F1,1,4,,0\n",
            "speed",
        ),
        (
            "sequence,side,refdes,package,x_mm,y_mm,rotation_deg,feeder,station,pitch_mm,nozzle,speed_mm_s\n1,top,R1,0603,x,10,0,F1,1,4,,200\n",
            "number",
        ),
    ];
    for (source, case) in cases {
        let error = parse_machine_program(source).expect_err(case);
        assert_eq!(error.code, "program/invalid", "case: {case}");
    }
}

#[test]
fn an_empty_document_writes_only_the_header() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let program = build_machine_program(&document, &BTreeMap::new()).expect("built");
    let text = render_machine_program(&program);

    assert_eq!(program.components, 0);
    assert_eq!(program.station_count, 0);
    assert!(text.starts_with("sequence,side,refdes,package,"));
    assert_eq!(text.lines().count(), 1);
    assert!(parse_machine_program(&text).expect("parsed").steps.is_empty());
}

#[test]
fn an_empty_package_or_nozzle_in_the_map_is_rejected() {
    let document = document();
    for map in [BTreeMap::from([("".to_string(), "N1".to_string())]), BTreeMap::from([("0603".to_string(), "".to_string())])] {
        let error = build_machine_program(&document, &map).expect_err("invalid map");
        assert_eq!(error.code, "program/invalid");
    }
}

#[test]
fn missing_feeder_coverage_fails_like_the_placement_report() {
    let mut document = document();
    document.feeders.clear();
    let error = build_machine_program(&document, &nozzles()).expect_err("no feeders");
    assert_eq!(error.code, "feeder/missing");
}

#[test]
fn the_program_is_deterministic_and_leaves_the_document_unchanged() {
    let mut document = document();
    document.revision = 5;
    let before = document.clone();
    let first = build_machine_program(&document, &nozzles()).expect("built");
    let second = build_machine_program(&document, &nozzles()).expect("built");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(document, before);
    assert_eq!(document.revision, 5);
}

#[test]
fn export_writes_verifies_and_verify_reads_the_file_back() {
    let directory = workspace("export");
    let path = directory.join("panel.osmt");
    let target = directory.join("machine.csv");
    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    session.add_component(Component::new("C1", "0805", 20.0, 20.0)).expect("C1");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 100)).expect("F1");
    session.add_feeder(Feeder::new("F2", "0805", 8.0, 50)).expect("F2");
    let before = session.summary().expect("summary").revision;

    let report = session.export_machine_program(&target, &nozzles()).expect("export");
    assert_eq!(report.format, "open-smt-machine-program");
    assert_eq!(report.steps, 2);
    assert_eq!(report.bytes, read(&target).len());
    assert!(report.verified);
    assert_eq!(session.summary().expect("summary").revision, before);

    let verified = session.verify_machine_program(&target, &nozzles()).expect("verify");
    assert_eq!(verified.steps, 2);
    assert!(verified.ok);
    assert!(verified.mismatches.is_empty());

    let tampered = read(&target).replace("10,10,0,F1", "10,11,0,F1");
    fs::write(&target, tampered).expect("write");
    let failed = session.verify_machine_program(&target, &nozzles()).expect("verify");
    assert!(!failed.ok);
    assert_eq!(failed.mismatches[0].field, "y");
}
