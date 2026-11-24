use smt_model::{Board, Component, ExportFormat, Feeder, Panel, Side, SmtDocument, render_pdf, render_svg};

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.panel = Panel::new(2, 2, 5.0, 5.0);
    document.revision = 3;
    let mut r2 = Component::new("R2", "0603", 30.0, 20.0);
    r2.rotation = 90.0;
    let mut c1 = Component::new("C1", "0805", 60.0, 40.0);
    c1.side = Side::Bottom;
    c1.rotation = 45.0;
    document.components = vec![Component::new("R1", "0603", 10.0, 10.0), r2, c1];
    document.feeders = vec![Feeder::new("F1", "0603", 4.0, 100), Feeder::new("F2", "0805", 8.0, 50)];
    document
}

#[test]
fn the_svg_draws_panel_boards_components_labels_legend_and_title() {
    let drawing = render_svg(&document(), "panel").expect("rendered");
    let svg = &drawing.text;

    assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(svg.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains("viewBox=\"0 0 "));
    assert!(svg.contains("data-panel=\"true\""));
    for board in 1..=4 {
        assert!(svg.contains(&format!("data-board=\"{board}\"")), "board {board}");
    }
    assert!(svg.contains("data-rotation=\"90.000\""));
    assert_eq!(svg.matches("<polygon").count(), 12);
    assert!(svg.contains(">R1</text>"));
    assert_eq!(svg.matches(">R1</text>").count(), 1);
    assert!(svg.contains(">0603 F1</text>"));
    assert!(svg.contains(">LEGEND</text>"));
    assert!(svg.contains(">FEEDER LANES</text>"));
    assert!(svg.contains("revision 3"));
    assert!(svg.contains("2.0 x 1.2 mm"));
    assert!(svg.contains("labels on board 1 only"));

    assert_eq!(drawing.summary.units, "mm");
    assert_eq!(drawing.summary.boards, 4);
    assert_eq!(drawing.summary.components, 3);
    assert_eq!(drawing.summary.placements, 12);
    assert_eq!(drawing.summary.default_size_components, 3);
    assert_eq!(drawing.summary.feeder_lanes, 2);
    assert_eq!(drawing.summary.dropped_characters, 0);
}

#[test]
fn the_pdf_is_one_a4_landscape_page_with_helvetica_and_the_same_labels() {
    let drawing = render_pdf(&document(), "panel").expect("rendered");
    let pdf = &drawing.text;

    assert!(pdf.starts_with("%PDF-1.4\n"));
    assert!(pdf.ends_with("%%EOF\n"));
    assert!(pdf.contains("/MediaBox [0 0 841.89 595.28]"));
    assert!(pdf.contains("/Type /Pages /Kids [3 0 R] /Count 1"));
    assert_eq!(pdf.matches("/Type /Page ").count(), 1);
    assert!(pdf.contains("/BaseFont /Helvetica"));
    assert!(pdf.contains("(R1) Tj"));
    assert!(pdf.contains("(0603 F1) Tj"));
    assert!(pdf.contains("(LEGEND) Tj"));
    assert!(pdf.contains("(FEEDER LANES) Tj"));
    assert!(pdf.is_ascii());
    assert_eq!(drawing.summary.placements, 12);
}

#[test]
fn two_renders_are_byte_identical() {
    let first = render_svg(&document(), "panel").expect("first");
    let second = render_svg(&document(), "panel").expect("second");
    assert_eq!(first.text, second.text);
    assert_eq!(first.summary, second.summary);

    let first = render_pdf(&document(), "panel").expect("first");
    let second = render_pdf(&document(), "panel").expect("second");
    assert_eq!(first.text, second.text);
    assert_eq!(first.summary, second.summary);
}

#[test]
fn an_empty_document_still_produces_a_framed_sheet() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let svg = render_svg(&document, "empty").expect("svg");
    assert!(svg.text.contains("data-panel=\"true\""));
    assert!(svg.text.contains("data-board=\"1\""));
    assert!(svg.text.contains("no feeder lanes declared"));
    assert!(svg.text.contains("0 component(s) per board"));
    assert_eq!(svg.text.matches("<polygon").count(), 0);
    assert_eq!(svg.summary.components, 0);
    assert_eq!(svg.summary.placements, 0);
    assert_eq!(svg.summary.boards, 1);
    assert_eq!(svg.summary.dropped_characters, 0);

    let pdf = render_pdf(&document, "empty").expect("pdf");
    assert!(pdf.text.contains("/Count 1"));
    assert!(pdf.text.contains("0 component\\(s\\) per board"));
    assert!(pdf.text.ends_with("%%EOF\n"));
}

#[test]
fn rotated_components_use_the_rotated_rectangle() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let mut component = Component::new("R1", "0603", 50.0, 40.0);
    component.rotation = 90.0;
    document.components = vec![component];
    let svg = render_svg(&document, "rotated").expect("svg");
    assert!(svg.text.contains("points=\"50.600,39.000 50.600,41.000 49.400,41.000 49.400,39.000\""));
}

#[test]
fn xml_special_characters_are_escaped_and_non_ascii_is_reported_as_loss() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components = vec![Component::new("R1<&\"'>", "电阻", 10.0, 10.0)];
    let svg = render_svg(&document, "金子").expect("svg");
    assert!(svg.text.contains("R1&lt;&amp;&quot;&apos;&gt;"));
    assert!(svg.text.contains(">?? -</text>"));
    assert!(svg.text.is_ascii());
    assert_eq!(svg.summary.dropped_characters, 4);
    assert_eq!(svg.summary.losses(), vec![smt_model::DrawingLoss { code: "drawing/non-ascii-text".to_string(), count: 4 }]);

    let pdf = render_pdf(&document, "金子").expect("pdf");
    assert!(pdf.text.contains("(?? -) Tj"));
    assert!(pdf.text.is_ascii());
    assert_eq!(pdf.summary.dropped_characters, 4);
}

#[test]
fn invalid_boards_and_panels_report_the_model_codes() {
    let mut document = SmtDocument::new(Board::new(0.0, 80.0, 1.6));
    assert_eq!(render_svg(&document, "bad").unwrap_err().code, "board/invalid");
    document.board = Board::new(100.0, 80.0, 1.6);
    document.panel = Panel::new(0, 1, 0.0, 0.0);
    assert_eq!(render_pdf(&document, "bad").unwrap_err().code, "panel/invalid");
}

#[test]
fn the_export_format_accepts_svg_and_pdf() {
    assert_eq!("svg".parse::<ExportFormat>().expect("svg"), ExportFormat::Svg);
    assert_eq!("pdf".parse::<ExportFormat>().expect("pdf"), ExportFormat::Pdf);
    let error = "bmp".parse::<ExportFormat>().unwrap_err();
    assert_eq!(error.code, "cli/unknown-format");
    assert!(error.message.contains("svg or pdf"));
}
