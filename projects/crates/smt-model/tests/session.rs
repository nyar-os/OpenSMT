use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

use smt_model::{Board, Component, ComponentUpdate, ExportFormat, Feeder, OpenSmt, Panel, Side, SmtDocument};

fn workspace(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("open-smt-{}-{name}", process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("create test directory");
    directory
}

fn read_document(path: &Path) -> SmtDocument {
    serde_json::from_str(&fs::read_to_string(path).expect("document text")).expect("parse document")
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
fn first_article_export_writes_a_check_sheet() {
    let directory = workspace("fai-export");
    let path = directory.join("panel.osmt");

    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("component");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 100)).expect("feeder");

    let target = directory.join("first-article.csv");
    let exported = session.export(ExportFormat::FirstArticle, &target).expect("export");
    assert_eq!(exported.format, "fai");
    assert_eq!(exported.bytes, fs::metadata(&target).expect("metadata").len() as usize);
    let sheet = fs::read_to_string(&target).expect("sheet");
    assert!(sheet.starts_with("step,refdes,package,side,x,y,rotation,feeder,pitch,placement_ok"));
    assert!(sheet.contains("\n1,R1,0603,top,10,10,0,F1,4,,,,"));
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

#[test]
fn update_component_moves_a_component_and_keeps_untouched_fields() {
    let directory = workspace("update-move");
    let path = directory.join("board.osmt");
    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    session
        .add_component(Component {
            refdes: "R1".to_string(),
            package: "0603".to_string(),
            side: Side::Top,
            x: 10.0,
            y: 10.0,
            rotation: 90.0,
            height: Some(1.2),
        })
        .expect("add R1");
    let before = session.summary().expect("summary");

    let summary = session
        .update_component("R1", ComponentUpdate { x: Some(40.0), y: Some(30.0), ..ComponentUpdate::default() })
        .expect("move R1");
    assert_eq!(summary.components, 1);
    assert_eq!(summary.revision, before.revision + 1);

    let document = read_document(&path);
    assert_eq!(document.revision, summary.revision);
    let component = &document.components[0];
    assert_eq!(component.refdes, "R1");
    assert_eq!(component.x, 40.0);
    assert_eq!(component.y, 30.0);
    assert_eq!(component.package, "0603");
    assert_eq!(component.side, Side::Top);
    assert_eq!(component.rotation, 90.0);
    assert_eq!(component.height, Some(1.2));
}

#[test]
fn update_component_normalises_rotation_and_manages_height_and_side() {
    let directory = workspace("update-fields");
    let path = directory.join("board.osmt");
    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("add R1");

    session
        .update_component("R1", ComponentUpdate { rotation: Some(450.0), height: Some(0.8), ..ComponentUpdate::default() })
        .expect("rotation and height");
    let document = read_document(&path);
    assert_eq!(document.components[0].rotation, 90.0);
    assert_eq!(document.components[0].height, Some(0.8));

    session.update_component("R1", ComponentUpdate { clear_height: true, ..ComponentUpdate::default() }).expect("clear height");
    let document = read_document(&path);
    assert_eq!(document.components[0].height, None);
    assert_eq!(document.components[0].rotation, 90.0);
    assert_eq!(document.components[0].x, 10.0);

    session.update_component("R1", ComponentUpdate { side: Some(Side::Bottom), ..ComponentUpdate::default() }).expect("side");
    let document = read_document(&path);
    assert_eq!(document.components[0].side, Side::Bottom);
}

#[test]
fn update_component_package_changes_feeder_allocation() {
    let directory = workspace("update-feeder");
    let path = directory.join("board.osmt");
    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("add R1");
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 100)).expect("F1");
    session.add_feeder(Feeder::new("F2", "0805", 4.0, 100)).expect("F2");

    let before = session.feeders().expect("allocation");
    assert!(before.feeders.iter().find(|feeder| feeder.id == "F1").expect("F1").refdes.contains(&"R1".to_string()));

    session
        .update_component("R1", ComponentUpdate { package: Some("0805".to_string()), ..ComponentUpdate::default() })
        .expect("package");
    let after = session.feeders().expect("allocation");
    assert_eq!(after.feeders.iter().find(|feeder| feeder.id == "F2").expect("F2").refdes, vec!["R1".to_string()]);
    assert!(after.feeders.iter().find(|feeder| feeder.id == "F1").expect("F1").refdes.is_empty());
}

#[test]
fn update_component_rejects_invalid_requests_without_writing() {
    let directory = workspace("update-invalid");
    let path = directory.join("board.osmt");
    let mut session = OpenSmt::new();
    session.create(&path).expect("create");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("add R1");
    let before = session.summary().expect("summary");

    let error = session
        .update_component("R9", ComponentUpdate { x: Some(20.0), ..ComponentUpdate::default() })
        .expect_err("unknown refdes");
    assert_eq!(error.code, "component/unknown-refdes");

    let error = session.update_component("R1", ComponentUpdate::default()).expect_err("empty request");
    assert_eq!(error.code, "component/invalid");

    let error = session
        .update_component("R1", ComponentUpdate { height: Some(1.0), clear_height: true, ..ComponentUpdate::default() })
        .expect_err("set and clear");
    assert_eq!(error.code, "component/invalid");

    let error = session
        .update_component("R1", ComponentUpdate { x: Some(500.0), ..ComponentUpdate::default() })
        .expect_err("outside the board");
    assert_eq!(error.code, "component/outside-board");

    let error = session
        .update_component("R1", ComponentUpdate { y: Some(f64::NAN), ..ComponentUpdate::default() })
        .expect_err("non-finite");
    assert_eq!(error.code, "component/invalid");

    let error = session
        .update_component("R1", ComponentUpdate { rotation: Some(f64::INFINITY), ..ComponentUpdate::default() })
        .expect_err("non-finite rotation");
    assert_eq!(error.code, "component/invalid");

    let error = session
        .update_component("R1", ComponentUpdate { package: Some("  ".to_string()), ..ComponentUpdate::default() })
        .expect_err("empty package");
    assert_eq!(error.code, "component/invalid");

    let after = session.summary().expect("summary");
    assert_eq!(after.revision, before.revision);
    let document = read_document(&path);
    assert_eq!(document.revision, before.revision);
    assert_eq!(document.components[0], Component::new("R1", "0603", 10.0, 10.0));
}

#[test]
fn update_component_is_deterministic_and_round_trips() {
    let directory = workspace("update-determinism");
    let first = directory.join("first.osmt");
    let second = directory.join("second.osmt");
    for path in [&first, &second] {
        let mut session = OpenSmt::new();
        session.create(path).expect("create");
        session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("add R1");
        session
            .update_component(
                "R1",
                ComponentUpdate {
                    package: Some("0805".to_string()),
                    side: Some(Side::Bottom),
                    x: Some(20.0),
                    y: Some(25.0),
                    rotation: Some(450.0),
                    height: Some(0.5),
                    clear_height: false,
                },
            )
            .expect("update R1");
    }
    assert_eq!(fs::read(&first).expect("first"), fs::read(&second).expect("second"));

    let reopened = OpenSmt::new().open(&first).expect("reopen");
    assert_eq!(reopened.revision, 2);
    let document = read_document(&first);
    let component = &document.components[0];
    assert_eq!(component.package, "0805");
    assert_eq!(component.side, Side::Bottom);
    assert_eq!(component.x, 20.0);
    assert_eq!(component.y, 25.0);
    assert_eq!(component.rotation, 90.0);
    assert_eq!(component.height, Some(0.5));
}
