//! Solder paste volume report: exact arithmetic, invalid inputs, determinism.

use std::fs;

use smt_model::{Board, Component, OpenSmt, PasteOptions, Side, SmtDocument, paste_csv, paste_report, stencil_report};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(120.0, 80.0, 1.6));
    document.components = vec![
        Component::new("R1", "0603", 10.0, 10.0),
        Component::new("C1", "0805", 40.0, 20.0),
        Component::new("U1", "SOP8", 70.0, 40.0),
        Component::new("D1", "LGA16", 100.0, 60.0),
    ];
    document.components[3].side = Side::Bottom;
    document
}

#[test]
fn the_report_is_empty_for_an_empty_document() {
    let empty = SmtDocument::new(Board::new(100.0, 50.0, 1.6));
    let report = paste_report(&empty, &PasteOptions::default()).expect("reported");
    assert_eq!(report.format, "open-smt-paste-volume");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!(report.foil_thickness_mm, 0.12);
    assert_eq!(report.boards, 1);
    assert_eq!(report.waste_percent, 0.0);
    assert_eq!(report.apertures_per_board, 0);
    assert_eq!(report.volume_per_board_mm3, 0.0);
    assert_eq!(report.volume_run_mm3, 0.0);
    assert_eq!(report.waste_volume_mm3, 0.0);
    assert_eq!(report.total_volume_mm3, 0.0);
    assert!(report.packages.is_empty());
    let csv = paste_csv(&report);
    assert_eq!(csv, "package,apertures_per_board,volume_per_aperture_mm3,volume_per_board_mm3,volume_run_mm3\nTOTAL,0,,0,0\n");
}

#[test]
fn volumes_follow_the_aperture_geometry() {
    let report = paste_report(&document(), &PasteOptions::default()).expect("reported");
    // Aperture sides are the rule sizes shrunk by the square root of the area
    // reduction: 0603 0.65x0.80 at 0.90, 0805 0.85x1.25 at 0.90, SOP8
    // 0.60x0.70 at 0.85, LGA16 0.30 circles at 0.85.
    let by_package: Vec<(&str, u32, f64, f64)> = report
        .packages
        .iter()
        .map(|package| {
            (
                package.package.as_str(),
                package.apertures_per_board,
                package.volume_per_aperture_mm3,
                package.volume_per_board_mm3,
            )
        })
        .collect();
    assert_eq!(
        by_package,
        vec![("0603", 2, 0.056, 0.112), ("0805", 2, 0.115, 0.23), ("SOP8", 8, 0.043, 0.344), ("LGA16", 16, 0.007, 0.112),]
    );
    assert_eq!(report.apertures_per_board, 28);
    assert_eq!(report.volume_per_board_mm3, 0.798);
    assert_eq!(report.volume_run_mm3, 0.798);
    assert_eq!(report.waste_volume_mm3, 0.0);
    assert_eq!(report.total_volume_mm3, 0.798);
    assert_eq!(report.top_volume_per_board_mm3, 0.686);
    assert_eq!(report.bottom_volume_per_board_mm3, 0.112);
}

#[test]
fn circle_apertures_use_the_diameter() {
    let mut single = SmtDocument::new(Board::new(40.0, 40.0, 1.6));
    single.components = vec![Component::new("D1", "LGA16", 20.0, 20.0)];
    let report = paste_report(&single, &PasteOptions::default()).expect("reported");
    // pi * (0.277 / 2)^2 * 0.12 = 0.00723... mm3 per deposit.
    assert_eq!(report.packages[0].volume_per_aperture_mm3, 0.007);
    assert_eq!(report.packages[0].apertures_per_board, 16);
    assert_eq!(report.volume_per_board_mm3, 0.112);
    // The stencil report really draws circles there.
    let stencil = stencil_report(&single);
    assert!(stencil.entries.iter().all(|entry| entry.shape == "circle"));
}

#[test]
fn boards_and_waste_scale_the_run() {
    let options = PasteOptions { foil_thickness_mm: None, boards: Some(10), waste_percent: Some(10.0) };
    let report = paste_report(&document(), &options).expect("reported");
    assert_eq!(report.boards, 10);
    assert_eq!(report.waste_percent, 10.0);
    assert_eq!(report.volume_per_board_mm3, 0.798);
    assert_eq!(report.volume_run_mm3, 7.98);
    assert_eq!(report.waste_volume_mm3, 0.798);
    assert_eq!(report.total_volume_mm3, 8.778);
    let package = report.packages.iter().find(|package| package.package == "SOP8").expect("SOP8");
    assert_eq!(package.volume_run_mm3, 3.44);
}

#[test]
fn a_custom_foil_thickness_scales_every_volume() {
    let options = PasteOptions { foil_thickness_mm: Some(0.15), boards: None, waste_percent: None };
    let report = paste_report(&document(), &options).expect("reported");
    assert_eq!(report.foil_thickness_mm, 0.15);
    // 0603: 0.617 * 0.759 * 0.15 = 0.07024... mm3; the per-board total sums the
    // rounded apertures: 2*0.07 + 2*0.143 + 8*0.054 + 16*0.009 = 1.002.
    assert_eq!(report.packages[0].volume_per_aperture_mm3, 0.07);
    assert_eq!(report.volume_per_board_mm3, 1.002);
}

#[test]
fn invalid_options_fail_with_paste_invalid() {
    let cases = [
        PasteOptions { foil_thickness_mm: Some(0.0), boards: None, waste_percent: None },
        PasteOptions { foil_thickness_mm: Some(-0.1), boards: None, waste_percent: None },
        PasteOptions { foil_thickness_mm: Some(f64::NAN), boards: None, waste_percent: None },
        PasteOptions { foil_thickness_mm: Some(f64::INFINITY), boards: None, waste_percent: None },
        PasteOptions { foil_thickness_mm: None, boards: Some(0), waste_percent: None },
        PasteOptions { foil_thickness_mm: None, boards: None, waste_percent: Some(-1.0) },
        PasteOptions { foil_thickness_mm: None, boards: None, waste_percent: Some(101.0) },
        PasteOptions { foil_thickness_mm: None, boards: None, waste_percent: Some(f64::NAN) },
    ];
    for options in cases {
        let error = paste_report(&document(), &options).expect_err("invalid");
        assert_eq!(error.code, "paste/invalid", "{error}");
    }
}

#[test]
fn the_csv_lists_packages_in_first_aperture_order() {
    let options = PasteOptions { foil_thickness_mm: None, boards: Some(10), waste_percent: Some(10.0) };
    let report = paste_report(&document(), &options).expect("reported");
    assert_eq!(
        paste_csv(&report),
        "package,apertures_per_board,volume_per_aperture_mm3,volume_per_board_mm3,volume_run_mm3\n0603,2,0.056,0.112,1.12\n0805,2,0.115,0.23,2.3\nSOP8,8,0.043,0.344,3.44\nLGA16,16,0.007,0.112,1.12\nTOTAL,28,,0.798,7.98\n"
    );
}

#[test]
fn the_report_is_byte_identical_on_repeat() {
    let options = PasteOptions { foil_thickness_mm: Some(0.1), boards: Some(3), waste_percent: Some(2.5) };
    let first = paste_report(&document(), &options).expect("first");
    let second = paste_report(&document(), &options).expect("second");
    assert_eq!(first, second);
    assert_eq!(paste_csv(&first).as_bytes(), paste_csv(&second).as_bytes());
}

#[test]
fn the_report_never_modifies_the_document() {
    let document = document();
    let before = document.clone();
    let options = PasteOptions { foil_thickness_mm: None, boards: Some(5), waste_percent: Some(7.5) };
    paste_report(&document, &options).expect("reported");
    paste_csv(&paste_report(&document, &options).expect("reported"));
    assert_eq!(document, before);
}

#[test]
fn the_session_writes_the_paste_report_and_csv() {
    let directory = std::env::temp_dir().join(format!("open-smt-paste-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("the temp directory exists");
    let mut session = OpenSmt::new();
    session.create(&directory.join("job.osmt")).expect("a document is created");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    session.add_component(Component::new("C1", "0805", 40.0, 20.0)).expect("C1");
    let options = PasteOptions { foil_thickness_mm: None, boards: Some(2), waste_percent: Some(5.0) };
    let report = session.paste(&options).expect("reported");
    assert_eq!(report.apertures_per_board, 4);
    let before = session.summary().expect("open").revision;
    let export = session.paste_csv(&options, &directory.join("paste.csv")).expect("written");
    assert_eq!(export.format, "paste-csv");
    assert_eq!(export.files.len(), 1);
    assert_eq!(export.bytes, fs::metadata(&export.files[0]).expect("file").len() as usize);
    let text = fs::read_to_string(&export.files[0]).expect("the sheet is written");
    assert!(text.starts_with("package,apertures_per_board,"));
    assert!(text.contains("TOTAL,4,,0.342,0.684\n"));
    assert_eq!(session.summary().expect("open").revision, before);
    let _ = fs::remove_dir_all(&directory);
}
