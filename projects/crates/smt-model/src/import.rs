//! KiCad PCB reader.
//!
//! The reader maps the small subset of a `.kicad_pcb` document that a placement
//! job needs: the rectangular `Edge.Cuts` outline and one component per
//! footprint. Everything else is skipped rather than approximated, and every
//! skipped construct is counted in a loss ledger with a stable code.
//!
//! KiCad coordinates are absolute millimetres on an arbitrary sheet origin. The
//! board is translated so its outline starts at `(0, 0)`, because an OpenSMT
//! document puts the board origin at a corner. Rotation is normalised to
//! `[0, 360)`. The subset carries no board thickness and no component height,
//! so the thickness falls back to the OpenSMT default and the height stays
//! unknown.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::{
    SmtError,
    model::{Board, Component, Side},
};

/// Board thickness used when a KiCad file does not state one. This subset of the
/// format does not read stackup thickness.
pub const DEFAULT_THICKNESS: f64 = 1.6;

/// Format identifier returned in an import report.
pub const FORMAT: &str = "kicad_pcb";

/// One aggregated entry of the loss ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportLoss {
    /// Stable code describing the construct that was skipped.
    pub code: String,
    /// Number of source constructs counted under `code`.
    pub count: u32,
}

/// Report returned by an import.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// Source format identifier; always [`FORMAT`].
    pub format: String,
    /// Aggregated losses in code order.
    pub losses: Vec<ImportLoss>,
    /// Number of components written to the document.
    pub component_count: usize,
    /// Board derived from the `Edge.Cuts` outline.
    pub board: Board,
}

/// A parsed board and its components.
pub(crate) struct Imported {
    pub(crate) board: Board,
    pub(crate) components: Vec<Component>,
    pub(crate) losses: BTreeMap<&'static str, u32>,
}

impl Imported {
    /// Collapses the ledger into its public, code-ordered form.
    pub(crate) fn loss_entries(&self) -> Vec<ImportLoss> {
        self.losses.iter().map(|(code, count)| ImportLoss { code: code.to_string(), count: *count }).collect()
    }
}

/// Parses one KiCad PCB document.
pub(crate) fn parse(source: &str) -> Result<Imported, SmtError> {
    let root = Parser::run(source)?;
    if tag(&root) != Some("kicad_pcb") {
        return Err(parse_error("the document does not start with `(kicad_pcb`"));
    }
    let items = body(&root).ok_or_else(|| parse_error("the KiCad document is not a list"))?;

    let mut losses = BTreeMap::new();
    let mut edges = Vec::new();
    let mut footprints = Vec::new();

    for item in items {
        let Some(kind) = tag(item)
        else {
            continue;
        };
        match kind {
            "version" | "generator" | "generator_version" | "general" | "paper" | "title_block" | "setup" | "layers"
            | "net" => {}
            "footprint" | "module" => {
                if on_edge_cuts(item) {
                    return Err(outline_error("a footprint is drawn on `Edge.Cuts`"));
                }
                footprints.push(item);
            }
            "gr_line" => {
                if on_edge_cuts(item) {
                    edges.push(segment(item)?);
                }
                else {
                    loss(&mut losses, "kicad/construct-not-imported");
                }
            }
            "zone" => loss(&mut losses, "kicad/zone-not-imported"),
            other => {
                if on_edge_cuts(item) {
                    return Err(outline_error(format!("`{other}` on `Edge.Cuts` is not a straight line")));
                }
                loss(&mut losses, "kicad/construct-not-imported");
            }
        }
    }

    let (min_x, min_y, board) = outline(&edges)?;

    let mut components = Vec::new();
    let mut references = HashSet::new();
    for item in footprints {
        let Some(component) = component(item, min_x, min_y, &mut losses)?
        else {
            continue;
        };
        if !references.insert(component.refdes.clone()) {
            loss(&mut losses, "kicad/footprint-duplicate-reference");
            continue;
        }
        components.push(component);
    }

    Ok(Imported { board, components, losses })
}

/// Reads the footprint subset at `item` and translates it to the board corner.
fn component(
    item: &Node,
    min_x: f64,
    min_y: f64,
    losses: &mut BTreeMap<&'static str, u32>,
) -> Result<Option<Component>, SmtError> {
    let values = body(item).ok_or_else(|| parse_error("a footprint is not a list"))?;
    let package = values.first().and_then(text).unwrap_or("");
    if package.trim().is_empty() {
        loss(losses, "kicad/footprint-invalid");
        return Ok(None);
    }

    let Some(layer) = child(values, "layer").and_then(|node| body(node)).and_then(|values| values.first()).and_then(text)
    else {
        loss(losses, "kicad/footprint-layer-missing");
        return Ok(None);
    };
    let side = match layer {
        "F.Cu" => Side::Top,
        "B.Cu" => Side::Bottom,
        _ => {
            loss(losses, "kicad/footprint-layer-unsupported");
            return Ok(None);
        }
    };

    let Some(at) = child(values, "at").and_then(|node| body(node))
    else {
        loss(losses, "kicad/footprint-invalid");
        return Ok(None);
    };
    let numbers: Vec<f64> = at.iter().filter_map(text).filter_map(|value| value.parse::<f64>().ok()).collect();
    let (Some(x), Some(y)) = (numbers.first().copied(), numbers.get(1).copied())
    else {
        loss(losses, "kicad/footprint-invalid");
        return Ok(None);
    };
    let rotation = numbers.get(2).copied().unwrap_or(0.0).rem_euclid(360.0);

    let reference = reference(values);
    let Some(reference) = reference.filter(|value| !value.trim().is_empty())
    else {
        loss(losses, "kicad/footprint-without-reference");
        return Ok(None);
    };

    for entry in values {
        let Some(kind) = tag(entry)
        else {
            continue;
        };
        match kind {
            "layer"
            | "at"
            | "attr"
            | "path"
            | "sheetname"
            | "sheetfile"
            | "descr"
            | "tags"
            | "tedit"
            | "tstamp"
            | "uuid"
            | "locked"
            | "autoplace_cost90"
            | "autoplace_cost180"
            | "solder_mask_margin"
            | "solder_paste_margin"
            | "solder_paste_ratio"
            | "clearance"
            | "private_layers"
            | "net_tie_pad_groups" => {}
            "property" => {
                if property_key(entry) != Some("Reference") {
                    loss(losses, "kicad/footprint-property");
                }
            }
            "fp_text" => {
                if text_kind(entry) != Some("reference") {
                    loss(losses, "kicad/footprint-text");
                }
            }
            "pad" => loss(losses, "kicad/pad-not-imported"),
            "model" => loss(losses, "kicad/footprint-model"),
            "zone" => loss(losses, "kicad/zone-not-imported"),
            "fp_line" | "fp_arc" | "fp_curve" | "fp_poly" | "fp_rect" | "fp_circle" => loss(losses, "kicad/footprint-graphics"),
            _ => loss(losses, "kicad/construct-not-imported"),
        }
    }

    Ok(Some(Component {
        refdes: reference,
        package: package.to_string(),
        side,
        x: x - min_x,
        y: y - min_y,
        rotation,
        height: None,
    }))
}

/// Reads the reference designator from `(property "Reference" ...)` first, then
/// from `(fp_text reference ...)`.
fn reference(values: &[Node]) -> Option<String> {
    for entry in values {
        if tag(entry) == Some("property") && property_key(entry) == Some("Reference") {
            if let Some(value) = property_value(entry) {
                return Some(value.to_string());
            }
        }
    }
    for entry in values {
        if tag(entry) == Some("fp_text") && text_kind(entry) == Some("reference") {
            if let Some(value) = body(entry).and_then(|values| values.get(1)).and_then(text) {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn property_key(node: &Node) -> Option<&str> {
    body(node)?.first().and_then(text)
}

fn property_value(node: &Node) -> Option<&str> {
    body(node)?.get(1).and_then(text)
}

fn text_kind(node: &Node) -> Option<&str> {
    body(node)?.first().and_then(text)
}

/// Reads one `(gr_line (start x y) (end x y) (layer "Edge.Cuts"))`.
fn segment(node: &Node) -> Result<[f64; 4], SmtError> {
    let values = body(node).ok_or_else(|| outline_error("an `Edge.Cuts` line is not a list"))?;
    let start = child(values, "start")
        .and_then(|node| body(node))
        .and_then(pair)
        .ok_or_else(|| outline_error("an `Edge.Cuts` line has no start point"))?;
    let end = child(values, "end")
        .and_then(|node| body(node))
        .and_then(pair)
        .ok_or_else(|| outline_error("an `Edge.Cuts` line has no end point"))?;
    Ok([start[0], start[1], end[0], end[1]])
}

/// Confirms the `Edge.Cuts` segments cover exactly one axis-aligned rectangle
/// and returns its corner and size.
fn outline(segments: &[[f64; 4]]) -> Result<(f64, f64, Board), SmtError> {
    if segments.is_empty() {
        return Err(outline_error("no `Edge.Cuts` lines form an outline"));
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for [x1, y1, x2, y2] in segments {
        for value in [x1, x2] {
            min_x = min_x.min(*value);
            max_x = max_x.max(*value);
        }
        for value in [y1, y2] {
            min_y = min_y.min(*value);
            max_y = max_y.max(*value);
        }
    }
    let width = max_x - min_x;
    let depth = max_y - min_y;
    if !width.is_finite() || width <= 0.0 || !depth.is_finite() || depth <= 0.0 {
        return Err(outline_error("the `Edge.Cuts` outline has no area"));
    }

    const EPSILON: f64 = 1e-6;
    let mut bottom = Vec::new();
    let mut top = Vec::new();
    let mut left = Vec::new();
    let mut right = Vec::new();
    for [x1, y1, x2, y2] in segments {
        if (y1 - y2).abs() <= EPSILON {
            let y = (y1 + y2) / 2.0;
            let interval = (x1.min(*x2), x1.max(*x2));
            if (y - min_y).abs() <= EPSILON {
                bottom.push(interval);
            }
            else if (y - max_y).abs() <= EPSILON {
                top.push(interval);
            }
            else {
                return Err(outline_error("a horizontal `Edge.Cuts` line is not on the outline"));
            }
        }
        else if (x1 - x2).abs() <= EPSILON {
            let x = (x1 + x2) / 2.0;
            let interval = (y1.min(*y2), y1.max(*y2));
            if (x - min_x).abs() <= EPSILON {
                left.push(interval);
            }
            else if (x - max_x).abs() <= EPSILON {
                right.push(interval);
            }
            else {
                return Err(outline_error("a vertical `Edge.Cuts` line is not on the outline"));
            }
        }
        else {
            return Err(outline_error("an `Edge.Cuts` line is not axis aligned"));
        }
    }

    if !covers(&mut bottom, min_x, max_x)
        || !covers(&mut top, min_x, max_x)
        || !covers(&mut left, min_y, max_y)
        || !covers(&mut right, min_y, max_y)
    {
        return Err(outline_error("the `Edge.Cuts` lines do not close one rectangle"));
    }

    Ok((min_x, min_y, Board::new(width, depth, DEFAULT_THICKNESS)))
}

/// Returns `true` when the intervals cover `[low, high]` without gaps.
fn covers(intervals: &mut [(f64, f64)], low: f64, high: f64) -> bool {
    intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut reach = low;
    for (start, end) in intervals.iter() {
        if *start > reach + 1e-6 {
            return false;
        }
        reach = reach.max(*end);
    }
    reach >= high - 1e-6
}

fn on_edge_cuts(node: &Node) -> bool {
    child(body(node).unwrap_or(&[]), "layer").and_then(|node| body(node)).and_then(|values| values.first()).and_then(text)
        == Some("Edge.Cuts")
}

fn pair(values: &[Node]) -> Option<[f64; 2]> {
    let mut numbers = values.iter().filter_map(text).filter_map(|value| value.parse::<f64>().ok());
    Some([numbers.next()?, numbers.next()?])
}

fn loss(losses: &mut BTreeMap<&'static str, u32>, code: &'static str) {
    *losses.entry(code).or_insert(0) += 1;
}

fn parse_error(message: impl Into<String>) -> SmtError {
    SmtError::new("import/parse", message)
}

fn outline_error(message: impl Into<String>) -> SmtError {
    SmtError::new("import/outline-unsupported", message)
}

/// One S-expression node.
#[derive(Debug, Clone, PartialEq)]
enum Node {
    Atom(String),
    Str(String),
    List(Vec<Node>),
}

fn tag(node: &Node) -> Option<&str> {
    match node {
        Node::List(items) => match items.first()? {
            Node::Atom(value) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

fn body(node: &Node) -> Option<&[Node]> {
    match node {
        Node::List(items) => items.get(1..),
        _ => None,
    }
}

fn child<'a>(items: &'a [Node], name: &str) -> Option<&'a Node> {
    items.iter().find(|item| tag(item) == Some(name))
}

fn text(node: &Node) -> Option<&str> {
    match node {
        Node::Atom(value) | Node::Str(value) => Some(value),
        Node::List(_) => None,
    }
}

/// Recursive-descent reader for the KiCad S-expression dialect.
struct Parser<'a> {
    input: &'a [u8],
    index: usize,
}

impl<'a> Parser<'a> {
    fn run(text: &'a str) -> Result<Node, SmtError> {
        let mut parser = Parser { input: text.as_bytes(), index: 0 };
        parser.skip();
        if parser.index == parser.input.len() {
            return Err(parse_error("the KiCad file is empty"));
        }
        let node = parser.node()?;
        parser.skip();
        if parser.index != parser.input.len() {
            return Err(parse_error(format!("unexpected content at byte {}", parser.index)));
        }
        Ok(node)
    }

    fn node(&mut self) -> Result<Node, SmtError> {
        match self.input[self.index] {
            b'(' => self.list(),
            b')' => Err(parse_error(format!("unexpected `)` at byte {}", self.index))),
            b'"' => Ok(Node::Str(self.string()?)),
            _ => self.atom(),
        }
    }

    fn list(&mut self) -> Result<Node, SmtError> {
        let start = self.index;
        self.index += 1;
        let mut items = Vec::new();
        loop {
            self.skip();
            if self.index >= self.input.len() {
                return Err(parse_error(format!("unterminated list starting at byte {start}")));
            }
            if self.input[self.index] == b')' {
                self.index += 1;
                return Ok(Node::List(items));
            }
            items.push(self.node()?);
        }
    }

    fn string(&mut self) -> Result<String, SmtError> {
        let start = self.index;
        self.index += 1;
        let mut out = String::new();
        loop {
            if self.index >= self.input.len() {
                return Err(parse_error(format!("unterminated string starting at byte {start}")));
            }
            let character = self.next_char();
            match character {
                '"' => return Ok(out),
                '\\' => {
                    if self.index >= self.input.len() {
                        return Err(parse_error(format!("unterminated escape starting at byte {start}")));
                    }
                    match self.next_char() {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        '0' => out.push('\0'),
                        '\\' => out.push('\\'),
                        '"' => out.push('"'),
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn atom(&mut self) -> Result<Node, SmtError> {
        let start = self.index;
        while self.index < self.input.len() {
            match self.input[self.index] {
                b'(' | b')' | b'"' | b' ' | b'\t' | b'\r' | b'\n' | 0x0c => break,
                _ => self.index += 1,
            }
        }
        if self.index == start {
            return Err(parse_error(format!("unexpected byte at {}", self.index)));
        }
        let value = std::str::from_utf8(&self.input[start..self.index])
            .map_err(|_| parse_error(format!("invalid UTF-8 at byte {start}")))?;
        Ok(Node::Atom(value.to_string()))
    }

    fn skip(&mut self) {
        while self.index < self.input.len() && matches!(self.input[self.index], b' ' | b'\t' | b'\r' | b'\n' | 0x0c) {
            self.index += 1;
        }
    }

    fn next_char(&mut self) -> char {
        let rest = std::str::from_utf8(&self.input[self.index..]).unwrap_or("");
        let character = rest.chars().next().unwrap_or('\u{fffd}');
        self.index += character.len_utf8();
        character
    }
}
