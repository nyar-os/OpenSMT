//! Visual-quality guards for every drawing: text stays on the page, labels do
//! not overlap, tables keep their columns apart and the drawn content fills the
//! sheet instead of leaving most of it blank.

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

/// Text bounding boxes `(left, top, right, bottom)` of the whole sheet.
fn text_boxes(svg: &str) -> Vec<(f64, f64, f64, f64)> {
    text_elements(svg).into_iter().map(|(_, box_)| box_).collect()
}

/// Text bounding boxes together with their text content, in sheet millimetres.
fn text_elements(svg: &str) -> Vec<(String, (f64, f64, f64, f64))> {
    let mut boxes = Vec::new();
    for (offset_x, offset_y, element) in absolute_elements(svg) {
        if !element.starts_with("<text") {
            continue;
        }
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
        boxes.push((body.to_string(), (left + offset_x, y - size + offset_y, right + offset_x, y + offset_y)));
    }
    boxes
}

/// Elements with the cumulative `translate` of their group ancestors applied.
fn absolute_elements(svg: &str) -> Vec<(f64, f64, &str)> {
    let mut elements = Vec::new();
    let mut stack: Vec<(f64, f64)> = Vec::new();
    let mut rest = svg;
    while let Some(position) = rest.find('<') {
        let element = &rest[position..];
        let (offset_x, offset_y) = stack.last().copied().unwrap_or((0.0, 0.0));
        if element.starts_with("<g") {
            let end = element.find('>').expect("group start") + 1;
            let tag = &element[..end];
            let translation = tag.split_once("transform=\"translate(").and_then(|(_, value)| {
                let values = value.split(')').next()?;
                let mut parts = values.split_whitespace();
                Some((parts.next()?.parse::<f64>().ok()?, parts.next()?.parse::<f64>().ok()?))
            });
            let (dx, dy) = translation.unwrap_or((0.0, 0.0));
            stack.push((offset_x + dx, offset_y + dy));
            rest = &rest[position + end..];
        }
        else if element.starts_with("</g") {
            stack.pop();
            let end = element.find('>').expect("group end") + 1;
            rest = &rest[position + end..];
        }
        else if element.starts_with("<text") {
            let end = element.find("</text>").expect("closed text") + "</text>".len();
            elements.push((offset_x, offset_y, &element[..end]));
            rest = &rest[position + end..];
        }
        else if element.starts_with("<rect") || element.starts_with("<circle") || element.starts_with("<polyline") {
            let end = element.find("/>").expect("closed shape") + 2;
            elements.push((offset_x, offset_y, &element[..end]));
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
    elements
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

#[test]
fn text_stays_inside_every_sheet() {
    for (name, svg) in drawings() {
        let (width, height) = view_box(&svg);
        for (text, (left, top, right, bottom)) in text_elements(&svg) {
            assert!(left >= -0.05, "{name}: `{text}` starts at {left:.2} mm, left of the page");
            assert!(right <= width + 0.05, "{name}: `{text}` ends at {right:.2} mm, past {width:.2} mm");
            assert!(top >= -0.05, "{name}: `{text}` top {top:.2} mm above the page");
            assert!(bottom <= height + 0.05, "{name}: `{text}` baseline {bottom:.2} mm below {height:.2} mm");
        }
    }
}

#[test]
fn text_is_not_left_clipped_by_a_negative_coordinate() {
    for (name, svg) in drawings() {
        assert!(!svg.contains("x=\"-"), "{name} has a negative x");
        assert!(!svg.contains("y=\"-"), "{name} has a negative y");
    }
}

#[test]
fn content_covers_at_least_sixty_percent_of_the_sheet() {
    for (name, svg) in drawings() {
        let (width, height) = view_box(&svg);
        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0.0f64;
        let mut max_y = 0.0f64;
        for (offset_x, offset_y, element) in absolute_elements(&svg) {
            if !element.starts_with("<rect") {
                continue;
            }
            let (Some(x), Some(y), Some(w), Some(h)) =
                (attribute(element, "x"), attribute(element, "y"), attribute(element, "width"), attribute(element, "height"))
            else {
                continue;
            };
            // The page background is not content.
            if w >= width - 0.5 && h >= height - 0.5 {
                continue;
            }
            min_x = min_x.min(x + offset_x);
            min_y = min_y.min(y + offset_y);
            max_x = max_x.max(x + w + offset_x);
            max_y = max_y.max(y + h + offset_y);
        }
        for (_, (left, top, right, bottom)) in text_elements(&svg) {
            min_x = min_x.min(left);
            min_y = min_y.min(top);
            max_x = max_x.max(right);
            max_y = max_y.max(bottom);
        }
        let coverage = (max_x - min_x) * (max_y - min_y) / (width * height);
        assert!(
            coverage >= 0.6,
            "{name}: content covers only {:.0}% of the sheet (x {min_x:.1}..{max_x:.1} of {width:.1}, y {min_y:.1}..{max_y:.1} of {height:.1})",
            coverage * 100.0
        );
    }
}

#[test]
fn shapes_stay_inside_every_sheet() {
    for (name, svg) in drawings() {
        let (width, height) = view_box(&svg);
        for (offset_x, offset_y, element) in absolute_elements(&svg) {
            if element.starts_with("<rect") {
                let (Some(x), Some(y), Some(w), Some(h)) = (
                    attribute(element, "x"),
                    attribute(element, "y"),
                    attribute(element, "width"),
                    attribute(element, "height"),
                )
                else {
                    continue;
                };
                if w >= width - 0.5 && h >= height - 0.5 {
                    continue;
                }
                assert!(x + offset_x >= -0.05, "{name}: rect starts at {:.2} mm", x + offset_x);
                assert!(y + offset_y >= -0.05, "{name}: rect starts at {:.2} mm", y + offset_y);
                assert!(x + w + offset_x <= width + 0.05, "{name}: rect ends at {:.2} mm", x + w + offset_x);
                assert!(y + h + offset_y <= height + 0.05, "{name}: rect ends at {:.2} mm", y + h + offset_y);
            }
            else if element.starts_with("<circle") {
                let (Some(cx), Some(cy), Some(r)) =
                    (attribute(element, "cx"), attribute(element, "cy"), attribute(element, "r"))
                else {
                    continue;
                };
                assert!(cx + offset_x - r >= -0.05, "{name}: circle leaves the page");
                assert!(cy + offset_y - r >= -0.05, "{name}: circle leaves the page");
                assert!(cx + offset_x + r <= width + 0.05, "{name}: circle leaves the page");
                assert!(cy + offset_y + r <= height + 0.05, "{name}: circle leaves the page");
            }
            else if element.starts_with("<polyline") {
                let Some(points) = element.split_once("points=\"").map(|(_, rest)| rest.split('"').next().expect("points"))
                else {
                    continue;
                };
                for point in points.split_whitespace() {
                    let mut values = point.split(',');
                    let (Some(x), Some(y)) = (values.next(), values.next())
                    else {
                        continue;
                    };
                    let x: f64 = x.parse().expect("polyline x");
                    let y: f64 = y.parse().expect("polyline y");
                    assert!(x + offset_x >= -0.05 && x + offset_x <= width + 0.05, "{name}: polyline x {x}");
                    assert!(y + offset_y >= -0.05 && y + offset_y <= height + 0.05, "{name}: polyline y {y}");
                }
            }
        }
    }
}

#[test]
fn text_boxes_never_overlap_on_a_sheet() {
    for (name, svg) in drawings() {
        let boxes: Vec<_> = text_elements(&svg).into_iter().map(|(_, box_)| box_).collect();
        for (index, left) in boxes.iter().enumerate() {
            for right in boxes.iter().skip(index + 1) {
                let overlap_x = left.0 < right.2 - 0.05 && right.0 < left.2 - 0.05;
                let overlap_y = left.1 < right.3 - 0.05 && right.1 < left.3 - 0.05;
                assert!(!(overlap_x && overlap_y), "{name}: text overlaps: {left:?} and {right:?}");
            }
        }
    }
}

#[test]
fn placement_labels_do_not_overlap() {
    let svg = render_svg(&document(), "VIS").expect("placement").text;
    let group = svg.split_once("id=\"labels\">").expect("label group").1;
    let group = group.split("</g>").next().expect("label group end");
    let labels = text_boxes(group);
    for (index, left) in labels.iter().enumerate() {
        for right in labels.iter().skip(index + 1) {
            let overlap_x = left.0 < right.2 - 0.05 && right.0 < left.2 - 0.05;
            let overlap_y = left.1 < right.3 - 0.05 && right.1 < left.3 - 0.05;
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
                let rows = text_boxes(body);
                for (index, left) in rows.iter().enumerate() {
                    for right in rows.iter().skip(index + 1) {
                        let same_row = (left.1 - right.1).abs() < 0.1;
                        if same_row {
                            assert!(
                                left.2 <= right.0 + 0.05 || right.2 <= left.0 + 0.05,
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
