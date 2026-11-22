use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

use smt_model::{Board, Component, ExportFormat, Feeder, OpenSmt, Panel};

fn workspace(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("open-smt-{}-{name}", process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("create test directory");
    directory
}

#[test]
fn create_edit_reopen_and_export() {
    let directory = workspace("session");
    let path = directory.join("panel.osmt");

    let mut session = OpenSmt::new();
    let created = session.create(&path).expect("create");
    assert_eq!(created.revision, 0);
    assert_eq!(created.components, 0);

    session.set_board(Board::new(120.0, 90.0, 1.6)).expect("board");
    session.set_panel(Panel::new(2, 2, 5.0, 5.0)).expect("panel");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    session.add_component(Component::new("C1", "0603", 30.0, 20.0)).expect("C1");
    let summary = session.add_feeder(Feeder::new("F1", "0603", 4.0, 500)).expect("feeder");
    assert_eq!(summary.components, 2);
    assert_eq!(summary.revision, 5);

    let reopened = OpenSmt::new().open(&path).expect("reopen");
    assert_eq!(reopened.id, "panel");
    assert_eq!(reopened.board.width, 120.0);
    assert_eq!(reopened.panel.columns, 2);

    let report = session.report().expect("report");
    assert!(report.validation.ok);
    assert_eq!(report.placement.board_count, 4);
    assert_eq!(report.placement.entries.len(), 2);
    assert!(report.program_lines >= 3);
    assert_eq!(report.reflow.zones.len(), 4);

    let target = directory.join("program.csv");
    let exported = session.export(ExportFormat::Program, &target).expect("export");
    assert_eq!(exported.bytes, fs::metadata(&target).expect("metadata").len() as usize);
    let program = fs::read_to_string(&target).expect("program");
    assert!(program.starts_with("index,refdes,package,side,x,y,rotation,feeder\n"));
    assert!(program.contains("R1,0603,top,10,10,0,F1"));
}

#[test]
fn the_only_feeder_of_a_used_package_cannot_be_removed() {
    let directory = workspace("in-use");
    let path = directory.join("board.osmt");
    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("component");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 100)).expect("feeder");

    let error = session.remove_feeder("F1").expect_err("feeder is in use");
    assert_eq!(error.code, "feeder/in-use");
}

#[test]
fn an_invalid_component_is_rejected_before_it_is_saved() {
    let directory = workspace("invalid");
    let path = directory.join("board.osmt");
    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    let error = session.add_component(Component::new("R1", "0603", 500.0, 10.0)).expect_err("outside the board");
    assert_eq!(error.code, "component/outside-board");
    assert_eq!(session.summary().expect("summary").components, 0);
}

#[test]
fn create_refuses_an_existing_file() {
    let directory = workspace("exists");
    let path = directory.join("board.osmt");
    fs::write(&path, "{}").expect("seed");
    let error = OpenSmt::new().create(&path).expect_err("file exists");
    assert_eq!(error.code, "file/exists");
}

#[test]
fn missing_document_reports_no_document() {
    let error = OpenSmt::new().validate().expect_err("nothing open");
    assert_eq!(error.code, "session/no-document");
}

#[test]
fn save_as_retargets_the_session() {
    let directory = workspace("save-as");
    let source = directory.join("source.osmt");
    let target = directory.join("copy.osmt");
    let mut session = OpenSmt::new();
    session.create(&source).expect("create");
    let summary = session.save(Some(Path::new(&target))).expect("save as");
    assert!(summary.path.ends_with("copy.osmt"));
    assert!(target.exists());
}
