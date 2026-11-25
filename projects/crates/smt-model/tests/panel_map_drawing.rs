//! Panel map drawing: board instances, component dots and losses.

use std::fs;

use smt_model::{Board, Component, ExportFormat, OpenSmt, Panel, Side, SmtDocument, render_panel_map_svg};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.panel = Panel::new(2, 2, 5.0, 4.0);
    document.components = vec![Component::new("R1", "0603", 10.0, 10.0), Component::new("C1", "0805", 20.0, 30.0)];
    document.components[1].side = Side::Bottom;
    document
}

/// Collects every `fill="..."` and `stroke="..."` value of the sheet.
fn color_attributes(svg: &str) -> Vec<String> {
    let mut values = Vec::new();
    for attribute in ["fill=\"", "stroke=\""] {
        let mut rest = svg;
        while let Some(index) = rest.find(attribute) {
            let after = &rest[index + attribute.len()..];
            let end = after.find('"').expect("closing quote");
            values.push(after[..end].to_string());
            rest = &after[end..];
        }
    }
    values
}

#[test]
fn the_svg_numbers_boards_row_major_and_maps_component_dots() {
    let drawing = render_panel_map_svg(&document(), "panel").expect("rendered");
    let svg = &drawing.text;

    assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(svg.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains(">PANEL MAP</text>"));
    assert_eq!(svg.matches("data-role=\"panel\"").count(), 1);
    for board in 1..=4 {
        assert!(svg.contains(&format!("data-board=\"{board}\"")), "missing board {board}");
        assert!(svg.contains(&format!(">B{board}</text>")), "missing board label {board}");
    }
    // Board 1 runs before board 2, which runs before board 3.
    let first = svg.find("data-board=\"1\"").expect("board 1");
    let second = svg.find("data-board=\"2\"").expect("board 2");
    let third = svg.find("data-board=\"3\"").expect("board 3");
    assert!(first < second && second < third);
    // Every component is drawn on every board instance.
    assert_eq!(svg.matches("data-refdes=\"R1\"").count(), 4);
    assert_eq!(svg.matches("data-refdes=\"C1\"").count(), 4);
    assert!(svg.contains("ORIGIN (0,0)"));

    assert_eq!(drawing.summary.units, "mm");
    assert_eq!((drawing.summary.rows, drawing.summary.columns), (2, 2));
    assert_eq!(drawing.summary.board_count, 4);
    assert_eq!(drawing.summary.components, 2);
    assert_eq!(drawing.summary.outside_board, 0);
    assert_eq!(drawing.summary.dropped_characters, 0);
}

#[test]
fn every_svg_fill_and_stroke_is_a_valid_hex_color() {
    let svg = render_panel_map_svg(&document(), "panel").expect("rendered").text;
    let colors = color_attributes(&svg);
    assert!(!colors.is_empty());
    for color in &colors {
        assert!(color == "none" || color.starts_with('#'), "missing # prefix in `{color}`");
        if color == "none" {
            continue;
        }
        let digits = &color[1..];
        assert_eq!(digits.len(), 6, "invalid color `{color}`");
        assert!(digits.chars().all(|character| character.is_ascii_hexdigit()), "invalid color `{color}`");
    }
}

#[test]
fn two_renders_are_byte_identical() {
    let first = render_panel_map_svg(&document(), "panel").expect("first");
    let second = render_panel_map_svg(&document(), "panel").expect("second");
    assert_eq!(first.text, second.text);
    assert_eq!(first.summary, second.summary);
}

#[test]
fn non_ascii_text_is_dropped_and_counted() {
    let drawing = render_panel_map_svg(&document(), "面板图").expect("rendered");
    assert!(drawing.text.is_ascii());
    assert!(drawing.summary.dropped_characters >= 3);
    let losses = drawing.summary.losses();
    assert_eq!(losses[0].code, "panel-map/non-ascii-text");
    assert_eq!(losses[0].count, drawing.summary.dropped_characters);
}

#[test]
fn the_export_format_accepts_panel_svg_and_the_alias() {
    assert_eq!("panel-svg".parse::<ExportFormat>().expect("panel-svg"), ExportFormat::PanelMapSvg);
    assert_eq!("panel-map-svg".parse::<ExportFormat>().expect("panel-map-svg"), ExportFormat::PanelMapSvg);
    assert_eq!(ExportFormat::PanelMapSvg.as_str(), "panel-svg");
}

#[test]
fn the_session_writes_the_panel_sheet_through_the_export_format() {
    let directory = std::env::temp_dir().join(format!("open-smt-panel-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("the temp directory exists");
    let mut session = OpenSmt::new();
    session.create(&directory.join("job.osmt")).expect("a document is created");
    session.set_panel(Panel::new(2, 3, 4.0, 4.0)).expect("a panel is set");
    session.add_component(Component::new("R1", "0603", 10.0, 10.0)).expect("R1");
    let report = session.export(ExportFormat::PanelMapSvg, &directory.join("job.panel-svg")).expect("exported");
    assert_eq!(report.format, "panel-svg");
    let text = fs::read_to_string(&report.files[0]).expect("the sheet is written");
    assert!(text.contains(">PANEL MAP</text>"));
    assert!(text.contains("data-board=\"6\""));
    assert!(text.contains("data-refdes=\"R1\""));
    assert!(report.losses.is_empty());
    let _ = fs::remove_dir_all(&directory);
}
