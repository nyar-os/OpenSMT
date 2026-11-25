//! Visual-quality guards for every drawing: every export is one fixed A4
//! landscape page, the drawn content stays inside the inner frame, no text
//! overlaps and the page is actually used instead of leaving most of it blank.

use smt_model::{
    Board, Component, Feeder, SmtDocument, render_consumption_svg, render_feeder_svg, render_heatmap_svg,
    render_line_balance_svg, render_panel_map_svg, render_pdf, render_pick_cycle_svg, render_reflow_svg, render_sequence_svg,
    render_setup_svg, render_stencil_svg, render_svg,
};

fn document() -> SmtDocument {
    let mut doc = SmtDocument::new(Board::new(120.0, 90.0, 1.6));
    doc.panel = smt_model::Panel::new(2, 2, 5.0, 5.0);
    doc.feeders.push(Feeder::new("F1", "0402", 2.0, 5000));
    doc.feeders.push(Feeder::new("F2", "0603", 4.0, 4000));
    doc.feeders.push(Feeder::new("F3", "SOT-23", 8.0, 2500));
    doc.feeders.push(Feeder::new("F4", "QFN-32", 12.0, 1200));
    let packages = ["0402", "0603", "SOT-23", "QFN-32"];
    for row in 0..6 {
        for column in 0..5 {
            let index = row * 5 + column;
            let package = packages[index % packages.len()];
            let mut component =
                Component::new(format!("U{}", index + 1), package, 12.0 + column as f64 * 20.0, 12.0 + row as f64 * 13.0);
            if index % 4 == 3 {
                component.side = smt_model::Side::Bottom;
            }
            doc.components.push(component);
        }
    }
    doc
}

fn view_box(svg: &str) -> (f64, f64) {
    let rest = svg.split_once("viewBox=\"0 0 ").expect("viewBox").1;
    let mut parts = rest.split('"').next().expect("viewBox values").split_whitespace();
    let width = parts.next().expect("width").parse().expect("width number");
    let height = parts.next().expect("height").parse().expect("height number");
    (width, height)
}

/// Width estimate in millimetres, deliberately generous so the check fails
/// before a real glyph is clipped.
fn text_width(text: &str, size: f64) -> f64 {
    text.chars()
        .map(|character| match character {
            ' ' | '.' | ',' | ':' | ';' | '|' | '!' | '\'' | '(' | ')' | '[' | ']' => 0.32,
            'i' | 'l' | 'j' | 'I' | 't' | 'f' | 'r' | '-' | '/' => 0.42,
            'm' | 'M' | 'W' | 'w' => 0.85,
            '0'..='9' => 0.6,
            'A'..='Z' => 0.68,
            _ => 0.58,
        })
        .sum::<f64>()
        * size
}

fn attribute(element: &str, name: &str) -> Option<f64> {
    let marker = format!("{name}=\"");
    let start = element.find(&marker)?;
    let rest = &element[start + marker.len()..];
    rest[..rest.find('"')?].parse().ok()
}

/// How a group's `transform` maps its local millimetres to sheet millimetres:
/// `x' = tx + scale * x`.
#[derive(Debug, Clone, Copy)]
struct Abs {
    scale: f64,
    tx: f64,
    ty: f64,
}

impl Abs {
    fn identity() -> Self {
        Self { scale: 1.0, tx: 0.0, ty: 0.0 }
    }

    /// Local `translate` then `scale`, composed with this parent mapping.
    fn compose(&self, scale: f64, tx: f64, ty: f64) -> Self {
        Self { scale: self.scale * scale, tx: self.tx + self.scale * tx, ty: self.ty + self.scale * ty }
    }

    fn point(&self, x: f64, y: f64) -> (f64, f64) {
        (self.tx + self.scale * x, self.ty + self.scale * y)
    }
}

fn parse_transform(tag: &str) -> (f64, f64, f64) {
    let mut scale = 1.0;
    let mut tx = 0.0;
    let mut ty = 0.0;
    let Some(marker) = tag.find("transform=\"")
    else {
        return (scale, tx, ty);
    };
    let body = &tag[marker + "transform=\"".len()..];
    let body = &body[..body.find('"').unwrap_or(body.len())];
    if let Some(start) = body.find("translate(") {
        let values = &body[start + "translate(".len()..];
        let values = values.split(')').next().unwrap_or("");
        let mut parts = values.split(|character: char| character == ',' || character.is_whitespace()).filter(|v| !v.is_empty());
        tx = parts.next().and_then(|value| value.parse().ok()).unwrap_or(0.0);
        ty = parts.next().and_then(|value| value.parse().ok()).unwrap_or(0.0);
    }
    if let Some(start) = body.find("scale(") {
        let values = &body[start + "scale(".len()..];
        scale = values.split(')').next().and_then(|value| value.trim().parse().ok()).unwrap_or(1.0);
    }
    (scale, tx, ty)
}

/// Which part of the sheet a group belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Content,
    Title,
    Other,
}

/// One text run inside the drawing content, in sheet millimetres.
#[derive(Debug, Clone)]
struct TextBox {
    text: String,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    size: f64,
}

/// One shape inside the drawing content, in sheet millimetres.
#[derive(Debug, Clone)]
struct ShapeBox {
    bounds: (f64, f64, f64, f64),
}

/// Drawing content of one sheet: its texts, its shapes and the top of the title
/// block.
struct Content {
    texts: Vec<TextBox>,
    shapes: Vec<ShapeBox>,
    title_top: f64,
}

fn content_of(svg: &str) -> Content {
    let mut texts = Vec::new();
    let mut shapes = Vec::new();
    let mut title_top = f64::INFINITY;
    let mut abs_stack: Vec<Abs> = vec![Abs::identity()];
    let mut role_stack: Vec<Role> = vec![Role::Other];
    let mut rest = svg;
    while let Some(position) = rest.find('<') {
        let element = &rest[position..];
        if element.starts_with("</g") {
            if abs_stack.len() > 1 {
                abs_stack.pop();
                role_stack.pop();
            }
            let end = element.find('>').expect("group end") + 1;
            rest = &rest[position + end..];
            continue;
        }
        if element.starts_with("<g") {
            let end = element.find('>').expect("group start") + 1;
            let tag = &element[..end];
            let (scale, tx, ty) = parse_transform(tag);
            let abs = abs_stack.last().copied().unwrap_or_else(Abs::identity).compose(scale, tx, ty);
            let parent = *role_stack.last().unwrap_or(&Role::Other);
            let role = if tag.contains("id=\"content\"") || tag.contains("data-page=\"") {
                Role::Content
            }
            else if tag.contains("id=\"title-block\"") {
                if title_top.is_infinite() {
                    title_top = abs.ty;
                }
                Role::Title
            }
            else {
                parent
            };
            abs_stack.push(abs);
            role_stack.push(role);
            rest = &rest[position + end..];
            continue;
        }
        let role = *role_stack.last().unwrap_or(&Role::Other);
        let abs = abs_stack.last().copied().unwrap_or_else(Abs::identity);
        if element.starts_with("<text") {
            let end = element.find("</text>").expect("closed text") + "</text>".len();
            if role == Role::Content {
                texts.push(text_box(&element[..end], abs));
            }
            rest = &rest[position + end..];
        }
        else if element.starts_with("<rect") || element.starts_with("<circle") {
            let end = element.find("/>").expect("closed shape") + 2;
            let sheet_frame = element.contains("id=\"frame\"") || element.contains("id=\"inner-frame\"");
            if role == Role::Content && !sheet_frame {
                if let Some(bounds) = rect_bounds(&element[..end], abs) {
                    shapes.push(ShapeBox { bounds });
                }
            }
            rest = &rest[position + end..];
        }
        else if element.starts_with("<polyline") {
            let end = element.find("/>").expect("closed shape") + 2;
            if role == Role::Content {
                if let Some(bounds) = polyline_bounds(&element[..end], abs) {
                    shapes.push(ShapeBox { bounds });
                }
            }
            rest = &rest[position + end..];
        }
        else if element.starts_with("<?") || element.starts_with("<!") {
            let end = element.find('>').expect("declaration end") + 1;
            rest = &rest[position + end..];
        }
        else {
            let end = element.find('>').map_or(element.len(), |value| value + 1);
            rest = &rest[position + end..];
        }
    }
    Content { texts, shapes, title_top }
}

fn text_box(element: &str, abs: Abs) -> TextBox {
    let x = attribute(element, "x").expect("x attribute");
    let y = attribute(element, "y").expect("y attribute");
    let size = attribute(element, "font-size").expect("font-size attribute");
    let body = element.split_once('>').map(|(_, value)| value.trim_end_matches("</text>")).unwrap_or("");
    let width = text_width(body, size);
    let (left, right) = if element.contains("text-anchor=\"middle\"") {
        (x - width / 2.0, x + width / 2.0)
    }
    else if element.contains("text-anchor=\"end\"") {
        (x - width, x)
    }
    else {
        (x, x + width)
    };
    let (left, _) = abs.point(left, 0.0);
    let (right, _) = abs.point(right, 0.0);
    let (_, top) = abs.point(0.0, y - size);
    let (_, bottom) = abs.point(0.0, y);
    TextBox { text: body.to_string(), left, top, right, bottom, size: size * abs.scale }
}

fn rect_bounds(element: &str, abs: Abs) -> Option<(f64, f64, f64, f64)> {
    if element.starts_with("<circle") {
        let cx = attribute(element, "cx")?;
        let cy = attribute(element, "cy")?;
        let radius = attribute(element, "r")?;
        let (left, top) = abs.point(cx - radius, cy - radius);
        let (right, bottom) = abs.point(cx + radius, cy + radius);
        return Some((left, top, right, bottom));
    }
    let x = attribute(element, "x")?;
    let y = attribute(element, "y")?;
    let width = attribute(element, "width")?;
    let height = attribute(element, "height")?;
    let (left, top) = abs.point(x, y);
    let (right, bottom) = abs.point(x + width, y + height);
    Some((left, top, right, bottom))
}

fn polyline_bounds(element: &str, abs: Abs) -> Option<(f64, f64, f64, f64)> {
    let points = element.split_once("points=\"").map(|(_, rest)| rest.split('"').next().expect("points"))?;
    let mut bounds = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for point in points.split_whitespace() {
        let mut values = point.split(',');
        let (Some(x), Some(y)) = (values.next(), values.next())
        else {
            continue;
        };
        let (x, y) = abs.point(x.parse().expect("polyline x"), y.parse().expect("polyline y"));
        bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
    }
    bounds.0.is_finite().then_some(bounds)
}

fn drawings() -> Vec<(&'static str, String)> {
    let doc = document();
    vec![
        ("placement", render_svg(&doc, "VIS").expect("placement").text),
        ("stencil", render_stencil_svg(&doc, "VIS").expect("stencil").text),
        ("feeder", render_feeder_svg(&doc, "VIS").expect("feeder").text),
        ("sequence", render_sequence_svg(&doc, "VIS").expect("sequence").text),
        ("reflow", render_reflow_svg(&doc, "VIS").expect("reflow").text),
        ("panel", render_panel_map_svg(&doc, "VIS").expect("panel").text),
        ("heatmap", render_heatmap_svg(&doc, "VIS").expect("heatmap").text),
        ("balance", render_line_balance_svg(&doc, "VIS").expect("balance").text),
        ("setup", render_setup_svg(&doc, "VIS").expect("setup").text),
        ("pick-cycle", render_pick_cycle_svg(&doc, "VIS").expect("pick cycle").text),
        ("consumption", render_consumption_svg(&doc, "VIS").expect("consumption").text),
    ]
}

const SHEET_WIDTH: f64 = 297.0;
const SHEET_HEIGHT: f64 = 210.0;
const INNER: f64 = 8.0;

#[test]
fn every_sheet_is_a_fixed_a4_landscape_page() {
    for (name, svg) in drawings() {
        assert_eq!(view_box(&svg), (SHEET_WIDTH, SHEET_HEIGHT), "{name} is not A4 landscape");
        assert!(svg.contains("id=\"frame\""), "{name} has no outer frame");
        assert!(svg.contains("id=\"inner-frame\""), "{name} has no inner frame");
        assert!(svg.contains("id=\"title-block\""), "{name} has no title block");
    }
}

#[test]
fn content_stays_inside_the_inner_frame() {
    for (name, svg) in drawings() {
        let content = content_of(&svg);
        assert!(!content.texts.is_empty(), "{name} draws no text");
        for text in &content.texts {
            assert!(
                text.left >= INNER - 0.05 && text.right <= SHEET_WIDTH - INNER + 0.05,
                "{name}: `{}` spans {:.2}..{:.2} mm, outside the inner frame",
                text.text,
                text.left,
                text.right
            );
            assert!(
                text.top >= INNER - 0.05 && text.bottom <= SHEET_HEIGHT - INNER + 0.05,
                "{name}: `{}` spans {:.2}..{:.2} mm, outside the inner frame",
                text.text,
                text.top,
                text.bottom
            );
        }
        for shape in &content.shapes {
            let (left, top, right, bottom) = shape.bounds;
            assert!(
                left >= INNER - 0.05
                    && top >= INNER - 0.05
                    && right <= SHEET_WIDTH - INNER + 0.05
                    && bottom <= SHEET_HEIGHT - INNER + 0.05,
                "{name}: a shape spans {left:.2},{top:.2}..{right:.2},{bottom:.2}, outside the inner frame"
            );
        }
    }
}

#[test]
fn content_fills_and_is_centred_in_the_drawing_area() {
    for (name, svg) in drawings() {
        let content = content_of(&svg);
        let area = (INNER, INNER, SHEET_WIDTH - INNER, content.title_top - 4.0);
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for text in &content.texts {
            min_x = min_x.min(text.left);
            min_y = min_y.min(text.top);
            max_x = max_x.max(text.right);
            max_y = max_y.max(text.bottom);
        }
        for shape in &content.shapes {
            min_x = min_x.min(shape.bounds.0);
            min_y = min_y.min(shape.bounds.1);
            max_x = max_x.max(shape.bounds.2);
            max_y = max_y.max(shape.bounds.3);
        }
        let coverage = (max_x - min_x) * (max_y - min_y) / ((area.2 - area.0) * (area.3 - area.1));
        assert!(
            coverage >= 0.70,
            "{name}: content covers only {:.0}% of the drawing area (x {min_x:.1}..{max_x:.1}, y {min_y:.1}..{max_y:.1})",
            coverage * 100.0
        );
        let top_margin = min_y - INNER;
        let bottom_margin = area.3 - max_y;
        assert!(
            (top_margin - bottom_margin).abs() <= SHEET_HEIGHT * 0.15,
            "{name}: content is not vertically centred (top {top_margin:.1} mm, bottom {bottom_margin:.1} mm)"
        );
    }
}

#[test]
fn text_never_overlaps_and_stays_legible() {
    for (name, svg) in drawings() {
        let content = content_of(&svg);
        for (index, left) in content.texts.iter().enumerate() {
            assert!(left.size >= 1.6, "{name}: `{}` is only {:.2} mm tall after scaling", left.text, left.size);
            for right in content.texts.iter().skip(index + 1) {
                let overlap_x = left.left < right.right - 0.05 && right.left < left.right - 0.05;
                let overlap_y = left.top < right.bottom - 0.05 && right.top < left.bottom - 0.05;
                assert!(
                    !(overlap_x && overlap_y),
                    "{name}: `{}` and `{}` overlap ({:?} and {:?})",
                    left.text,
                    right.text,
                    (left.left, left.top, left.right, left.bottom),
                    (right.left, right.top, right.right, right.bottom)
                );
            }
        }
    }
}

#[test]
fn placement_labels_do_not_overlap() {
    let svg = render_svg(&document(), "VIS").expect("placement").text;
    let group = svg.split_once("id=\"labels\">").expect("label group").1;
    let group = group.split("</g>").next().expect("label group end");
    let content = content_of(&format!("<g id=\"content\">{group}</g>"));
    for (index, left) in content.texts.iter().enumerate() {
        for right in content.texts.iter().skip(index + 1) {
            let overlap_x = left.left < right.right - 0.05 && right.left < left.right - 0.05;
            let overlap_y = left.top < right.bottom - 0.05 && right.top < left.bottom - 0.05;
            assert!(!(overlap_x && overlap_y), "labels overlap: {left:?} and {right:?}");
        }
    }
}

#[test]
fn table_columns_do_not_overlap() {
    let doc = document();
    let sheets = [
        ("placement", render_svg(&doc, "VIS").expect("placement").text),
        ("stencil", render_stencil_svg(&doc, "VIS").expect("stencil").text),
        ("setup", render_setup_svg(&doc, "VIS").expect("setup").text),
    ];
    for (name, svg) in sheets {
        let mut rest = svg.as_str();
        while let Some(position) = rest.find("<g id=\"") {
            let element = &rest[position..];
            let id_end = element.find("\">").expect("group id");
            let id = &element[7..id_end];
            let body = &element[id_end + 2..];
            let body = &body[..body.find("</g>").expect("group end")];
            if id.contains("table") || id.contains("rules") {
                let content = content_of(&format!("<g id=\"content\">{body}</g>"));
                let rows: Vec<_> = content.texts;
                for (index, left) in rows.iter().enumerate() {
                    for right in rows.iter().skip(index + 1) {
                        let same_row = (left.top - right.top).abs() < 0.1;
                        if same_row {
                            assert!(
                                left.right <= right.left + 0.05 || right.right <= left.left + 0.05,
                                "{name}: table {id} columns overlap: {left:?} and {right:?}"
                            );
                        }
                    }
                }
            }
            rest = &rest[position + id_end + 2 + body.len()..];
        }
    }
}

#[test]
fn setup_sheet_puts_the_table_directly_under_the_header() {
    let svg = render_setup_svg(&document(), "VIS").expect("setup").text;
    let table = svg.split_once("id=\"setup-table\" transform=\"translate(").expect("setup table").1;
    let table_y: f64 =
        table.split(')').next().and_then(|values| values.split_whitespace().nth(1)).expect("table y").parse().expect("table y");
    let subtitle = svg.lines().find(|line| line.contains("nozzle and feeder schedule")).expect("setup subtitle");
    let baseline: f64 =
        subtitle.split("y=\"").nth(1).and_then(|rest| rest.split('"').next()).expect("baseline").parse().expect("baseline");
    assert!(table_y - baseline <= 14.0, "the table starts {:.1} mm below the subtitle", table_y - baseline);
    let content = content_of(&svg);
    // The reference column absorbs the remaining width instead of leaving the
    // right half of the page blank.
    let right = content.shapes.iter().map(|shape| shape.bounds.2).fold(0.0f64, f64::max);
    assert!(right > SHEET_WIDTH * 0.9, "the setup table only reaches {right:.1} mm across the page");
    let slot = content.texts.iter().find(|text| text.text == "SLOT").expect("slot header");
    let reference = content.texts.iter().find(|text| text.text == "REFERENCE DESIGNATORS").expect("reference header");
    assert!(reference.left > slot.left + 100.0, "the reference column starts at {:.1} mm", reference.left);
}

#[test]
fn png_background_is_explicitly_white() {
    for (name, svg) in drawings() {
        let first_rect = svg.split_once("<rect").expect("background rect").1;
        assert!(first_rect[..first_rect.find("/>").expect("rect end")].contains("fill=\"#ffffff\""), "{name} background");
        assert!(!svg.contains("fill=\"transparent\""), "{name} has a transparent fill");
    }
}

#[test]
fn new_formats_parse_and_render_deterministically() {
    assert_eq!(
        "pick-cycle-svg".parse::<smt_model::ExportFormat>().expect("pick-cycle-svg"),
        smt_model::ExportFormat::PickCycleSvg
    );
    assert_eq!(
        "consumption-svg".parse::<smt_model::ExportFormat>().expect("consumption-svg"),
        smt_model::ExportFormat::ConsumptionSvg
    );
    assert_eq!(smt_model::ExportFormat::PickCycleSvg.as_str(), "pick-cycle-svg");
    assert_eq!(smt_model::ExportFormat::ConsumptionSvg.as_str(), "consumption-svg");
    let doc = document();
    let first = render_pick_cycle_svg(&doc, "VIS").expect("rendered");
    let second = render_pick_cycle_svg(&doc, "VIS").expect("rendered");
    assert_eq!(first, second);
    let first = render_consumption_svg(&doc, "VIS").expect("rendered");
    let second = render_consumption_svg(&doc, "VIS").expect("rendered");
    assert_eq!(first, second);
}

#[test]
fn every_sheet_renders_a_white_pdf_background() {
    let pdf = render_pdf(&document(), "VIS").expect("placement pdf").text;
    assert!(pdf.contains("1 1 1 rg"));
}

#[test]
fn pick_cycle_sheet_draws_every_cycle_and_greys_unused_slots() {
    let doc = document();
    let report = smt_model::pick_cycle_plan(&doc, smt_model::DEFAULT_PICK_CYCLE_HEADS).expect("planned");
    let drawing = render_pick_cycle_svg(&doc, "VIS").expect("pick cycle sheet");
    assert!(drawing.text.contains("PICK CYCLE MATRIX"));
    for cycle in 1..=report.cycles {
        assert!(drawing.text.contains(&format!(">C{cycle}</text>")), "cycle C{cycle} row");
    }
    assert_eq!(
        drawing.text.matches("stroke=\"#d5dbe4\"").count(),
        report.wasted_slots as usize + 1,
        "one grey slot per unused head plus one legend swatch"
    );
    assert_eq!(drawing.summary.cycles, report.cycles);
    assert_eq!(drawing.summary.head_slots, report.head_slots);
    assert!((drawing.summary.utilization - report.utilization).abs() < 1e-9);
}

#[test]
fn consumption_sheet_draws_bars_thresholds_and_a_changeover_timeline() {
    let doc = document();
    let drawing = render_consumption_svg(&doc, "VIS").expect("consumption sheet");
    assert!(drawing.text.contains("FEEDER CONSUMPTION"));
    assert!(drawing.text.contains("REEL CHANGE TIMELINE"));
    assert!(drawing.text.contains("first-reel threshold"));
    assert!(drawing.text.contains("reel change"));
    assert_eq!(drawing.summary.boards, 100);
    assert_eq!(drawing.summary.lanes, doc.feeders.len() as u32);
    assert!(drawing.text.contains(">F1</text>"));
    assert!(drawing.text.contains(">F4</text>"));
}

#[test]
fn consumption_timeline_marks_reel_changes_when_supply_is_short() {
    let mut doc = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    doc.feeders.push(Feeder::new("F1", "0402", 2.0, 100));
    for index in 0..20 {
        doc.components.push(Component::new(format!("R{}", index + 1), "0402", 10.0 + index as f64 * 4.0, 20.0));
    }
    let drawing = render_consumption_svg(&doc, "VIS").expect("consumption sheet");
    assert_eq!(drawing.summary.short_lanes, 1, "20 picks per board over a 100-part supply");
    // Twenty reels over the run produce nineteen changeover ticks and a
    // first-reel threshold inside the chart.
    assert!(drawing.text.matches("stroke-width=\"0.8\"").count() >= 10);
    assert!(drawing.text.contains("stroke-dasharray"));
}
