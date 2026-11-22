use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

use smt_model::{Board, Feeder, ImportReport, OpenSmt, Side};

fn workspace(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("open-smt-import-{}-{name}", process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("create test directory");
    directory
}

fn source(directory: &Path, name: &str, text: &str) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, text).expect("write source");
    path
}

fn loss(report: &ImportReport, code: &str) -> u32 {
    report.losses.iter().find(|entry| entry.code == code).map_or(0, |entry| entry.count)
}

fn rectangle() -> &'static str {
    "  (gr_line (start 10 10) (end 60 10) (layer \"Edge.Cuts\") (width 0.1))\n\
     \x20 (gr_line (start 60 10) (end 60 40) (layer \"Edge.Cuts\") (width 0.1))\n\
     \x20 (gr_line (start 60 40) (end 10 40) (layer \"Edge.Cuts\") (width 0.1))\n\
     \x20 (gr_line (start 10 40) (end 10 10) (layer \"Edge.Cuts\") (width 0.1))\n"
}

fn board_with(footprints: &str) -> String {
    format!(
        "(kicad_pcb\n  (version 20240108)\n  (generator \"pcbnew\")\n  (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal))\n{footprints}{}\n)\n",
        rectangle()
    )
}

#[test]
fn footprints_become_millimetre_components() {
    let directory = workspace("components");
    let text = board_with(
        "  (footprint \"Package_SO:SOIC-8\"\n\
         \x20   (layer \"F.Cu\")\n\
         \x20   (at 25 15 90)\n\
         \x20   (property \"Reference\" \"U1\" (at 0 0) (layer \"F.SilkS\"))\n\
         \x20   (property \"Value\" \"LM358\" (at 0 0) (layer \"F.Fab\"))\n\
         \x20   (fp_text user \"U1\" (at 0 0) (layer \"F.Fab\"))\n\
         \x20   (fp_line (start -2 -2) (end 2 -2) (layer \"F.SilkS\"))\n\
         \x20   (pad \"1\" smd rect (at -1 0) (size 1 1) (layers \"F.Cu\"))\n\
         \x20   (model \"SOIC.wrl\")\n\
         \x20 )\n\
         \x20 (footprint \"Resistor_SMD:R_0603\"\n\
         \x20   (layer \"B.Cu\")\n\
         \x20   (at 15 35)\n\
         \x20   (fp_text reference \"R1\" (at 0 0) (layer \"B.SilkS\"))\n\
         \x20   (pad \"1\" smd rect (at 0 0) (size 1 1) (layers \"B.Cu\"))\n\
         \x20 )\n\
         \x20 (zone (net 0) (layer \"F.Cu\") (polygon (pts (xy 0 0))))\n",
    );
    let input = source(&directory, "panel.kicad_pcb", &text);
    let output = directory.join("panel.osmt");

    let mut session = OpenSmt::new();
    let report = session.import(&input, &output).expect("import");
    assert_eq!(report.format, "kicad_pcb");
    assert_eq!(report.component_count, 2);
    assert_eq!(report.board, Board::new(50.0, 30.0, 1.6));

    assert_eq!(loss(&report, "kicad/footprint-graphics"), 1);
    assert_eq!(loss(&report, "kicad/footprint-model"), 1);
    assert_eq!(loss(&report, "kicad/footprint-property"), 1);
    assert_eq!(loss(&report, "kicad/footprint-text"), 1);
    assert_eq!(loss(&report, "kicad/pad-not-imported"), 2);
    assert_eq!(loss(&report, "kicad/zone-not-imported"), 1);

    let mut reopened = OpenSmt::new();
    let summary = reopened.open(&output).expect("reopen");
    assert_eq!(summary.components, 2);

    let validation = reopened.validate().expect("validate");
    let errors: Vec<&str> = validation
        .issues
        .iter()
        .filter(|issue| issue.severity == smt_model::Severity::Error)
        .map(|issue| issue.code.as_str())
        .collect();
    assert!(!errors.is_empty());
    assert!(errors.iter().all(|code| *code == "feeder/missing"));

    reopened.add_feeder(Feeder::new("F1", "Package_SO:SOIC-8", 8.0, 10)).expect("feeder U1");
    reopened.add_feeder(Feeder::new("F2", "Resistor_SMD:R_0603", 4.0, 10)).expect("feeder R1");
    let placement = reopened.placement().expect("placement");
    let u1 = placement.entries.iter().find(|entry| entry.refdes == "U1").expect("U1");
    assert_eq!(u1.package, "Package_SO:SOIC-8");
    assert_eq!(u1.side, Side::Top);
    assert_eq!((u1.x, u1.y, u1.rotation), (15.0, 5.0, 90.0));
    let r1 = placement.entries.iter().find(|entry| entry.refdes == "R1").expect("R1");
    assert_eq!(r1.side, Side::Bottom);
    assert_eq!((r1.x, r1.y, r1.rotation), (5.0, 25.0, 0.0));
}

#[test]
fn rotation_is_normalised_into_one_turn() {
    let directory = workspace("rotation");
    let text = board_with(
        "  (footprint \"R\" (layer \"F.Cu\") (at 20 20 -90)\n\
         \x20   (fp_text reference \"R1\" (at 0 0) (layer \"F.SilkS\")))\n",
    );
    let input = source(&directory, "rotation.kicad_pcb", &text);
    let output = directory.join("rotation.osmt");

    let report = OpenSmt::new().import(&input, &output).expect("import");
    assert_eq!(report.component_count, 1);
    let mut reopened = OpenSmt::new();
    reopened.open(&output).expect("open");
    reopened.add_feeder(Feeder::new("F1", "R", 4.0, 10)).expect("feeder");
    let placement = reopened.placement().expect("placement");
    assert_eq!(placement.entries[0].rotation, 270.0);
}

#[test]
fn a_missing_outline_is_refused() {
    let directory = workspace("no-outline");
    let text = "(kicad_pcb\n  (version 20240108)\n  (footprint \"R\" (layer \"F.Cu\") (at 1 1)\n\
                \x20   (fp_text reference \"R1\" (at 0 0) (layer \"F.SilkS\")))\n)\n";
    let input = source(&directory, "flat.kicad_pcb", text);
    let error = OpenSmt::new().import(&input, &directory.join("flat.osmt")).expect_err("no outline");
    assert_eq!(error.code, "import/outline-unsupported");
}

#[test]
fn a_non_rectangular_outline_is_refused() {
    let directory = workspace("triangle");
    let text = "(kicad_pcb\n  (version 20240108)\n\
                \x20 (gr_line (start 0 0) (end 50 0) (layer \"Edge.Cuts\"))\n\
                \x20 (gr_line (start 50 0) (end 0 30) (layer \"Edge.Cuts\"))\n\
                \x20 (gr_line (start 0 30) (end 0 0) (layer \"Edge.Cuts\"))\n)\n";
    let input = source(&directory, "triangle.kicad_pcb", text);
    let error = OpenSmt::new().import(&input, &directory.join("triangle.osmt")).expect_err("triangle");
    assert_eq!(error.code, "import/outline-unsupported");
}

#[test]
fn duplicate_and_missing_references_are_losses() {
    let directory = workspace("references");
    let text = board_with(
        "  (footprint \"R\" (layer \"F.Cu\") (at 20 20)\n\
         \x20   (property \"Reference\" \"R1\" (at 0 0) (layer \"F.SilkS\")))\n\
         \x20 (footprint \"R\" (layer \"F.Cu\") (at 30 20)\n\
         \x20   (property \"Reference\" \"R1\" (at 0 0) (layer \"F.SilkS\")))\n\
         \x20 (footprint \"R\" (layer \"F.Cu\") (at 40 20)\n\
         \x20   (fp_text user \"anonymous\" (at 0 0) (layer \"F.SilkS\")))\n",
    );
    let input = source(&directory, "references.kicad_pcb", &text);
    let output = directory.join("references.osmt");

    let report = OpenSmt::new().import(&input, &output).expect("import");
    assert_eq!(report.component_count, 1);
    assert_eq!(loss(&report, "kicad/footprint-duplicate-reference"), 1);
    assert_eq!(loss(&report, "kicad/footprint-without-reference"), 1);
}

#[test]
fn an_empty_board_imports_with_a_warning() {
    let directory = workspace("empty");
    let input = source(&directory, "empty.kicad_pcb", &board_with(""));
    let output = directory.join("empty.osmt");

    let report = OpenSmt::new().import(&input, &output).expect("import");
    assert_eq!(report.component_count, 0);
    assert!(report.losses.is_empty());
    assert_eq!(report.board, Board::new(50.0, 30.0, 1.6));

    let mut reopened = OpenSmt::new();
    reopened.open(&output).expect("open");
    let validation = reopened.validate().expect("validate");
    assert!(validation.ok);
    let codes: Vec<&str> = validation.issues.iter().map(|issue| issue.code.as_str()).collect();
    assert!(codes.contains(&"component/none"));
    assert!(codes.contains(&"feeder/none"));
}

#[test]
fn import_is_deterministic() {
    let directory = workspace("deterministic");
    let text = board_with(
        "  (footprint \"R\" (layer \"F.Cu\") (at 20 20 45)\n\
         \x20   (fp_text reference \"R1\" (at 0 0) (layer \"F.SilkS\")))\n",
    );
    let input = source(&directory, "one.kicad_pcb", &text);
    let first = directory.join("first.osmt");
    let second = directory.join("second.osmt");

    OpenSmt::new().import(&input, &first).expect("first");
    OpenSmt::new().import(&input, &second).expect("second");
    assert_eq!(fs::read_to_string(&first).expect("first"), fs::read_to_string(&second).expect("second"));
}

#[test]
fn an_existing_target_is_not_overwritten() {
    let directory = workspace("exists");
    let input = source(&directory, "empty.kicad_pcb", &board_with(""));
    let output = directory.join("taken.osmt");
    fs::write(&output, "{}").expect("seed");

    let error = OpenSmt::new().import(&input, &output).expect_err("target exists");
    assert_eq!(error.code, "file/exists");
    assert_eq!(fs::read_to_string(&output).expect("unchanged"), "{}");
}

#[test]
fn a_non_kicad_document_is_refused() {
    let directory = workspace("parse");
    let input = source(&directory, "other.txt", "(not_a_board)\n");
    let error = OpenSmt::new().import(&input, &directory.join("other.osmt")).expect_err("wrong root");
    assert_eq!(error.code, "import/parse");
}
