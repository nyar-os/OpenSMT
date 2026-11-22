//! Thin Node-API surface for OpenSMT.
//!
//! Every export performs one orthogonal operation and returns a JSON string, so
//! the TypeScript layer owns argument parsing, help text and exit codes. All
//! analysis forwards to `smt-model`'s [`smt_model::OpenSmt`] session, which the
//! `osmt` binary also calls.
//!
//! Bad arguments report `session/argument-invalid`; every other failure keeps
//! the stable code of [`smt_model::SmtError`].

#![deny(missing_docs)]

use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use smt_model::{Board, Component, ExportFormat, Feeder, OpenSmt, Panel, ReflowProfile, Side, SmtError, Zone};
use wae::{CallContext, Error, JsValue, Result, function, register_module};

fn argument_error(message: &str) -> Error {
    Error::new(format!("session/argument-invalid: {message}"))
}

fn argument(ctx: &CallContext, index: usize) -> Result<JsValue> {
    ctx.args().get(index).copied().ok_or_else(|| argument_error("a required argument is missing"))
}

fn text(ctx: &CallContext, index: usize) -> Result<String> {
    argument(ctx, index)?.as_string()
}

fn optional_text(ctx: &CallContext, index: usize) -> Result<Option<String>> {
    match ctx.args().get(index) {
        Some(value) => value.as_string().map(Some),
        None => Ok(None),
    }
}

fn optional_bool(ctx: &CallContext, index: usize) -> Result<bool> {
    match ctx.args().get(index) {
        Some(value) => value.as_bool(),
        None => Ok(false),
    }
}

fn object(ctx: &CallContext, index: usize) -> Result<Value> {
    let raw = text(ctx, index)?;
    serde_json::from_str(&raw).map_err(|error| argument_error(&format!("expected a JSON object: {error}")))
}

fn number_field(request: &Value, name: &str) -> Result<Option<f64>> {
    match request.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_f64().map(Some).ok_or_else(|| argument_error(&format!("`{name}` must be a number"))),
    }
}

fn required_number_field(request: &Value, name: &str) -> Result<f64> {
    number_field(request, name)?.ok_or_else(|| argument_error(&format!("`{name}` is required")))
}

fn text_field(request: &Value, name: &str) -> Result<String> {
    match request.get(name).and_then(Value::as_str) {
        Some(value) if !value.is_empty() => Ok(value.to_string()),
        _ => Err(argument_error(&format!("`{name}` must be a non-empty string"))),
    }
}

fn optional_u32(request: &Value, name: &str) -> Result<Option<u32>> {
    match request.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|number| u32::try_from(number).ok())
            .map(Some)
            .ok_or_else(|| argument_error(&format!("`{name}` must be a non-negative integer"))),
    }
}

fn fail(error: SmtError) -> Error {
    Error::new(format!("{}: {}", error.code, error.message))
}

fn respond<T: Serialize>(ctx: &CallContext, value: &T) -> Result<JsValue> {
    let text = serde_json::to_string(value).map_err(|error| Error::new(format!("document/serialize: {error}")))?;
    ctx.env().string(&text)
}

/// Library version of the native binding, as a JSON string.
#[function]
fn version(ctx: CallContext) -> Result<JsValue> {
    respond(&ctx, &env!("CARGO_PKG_VERSION"))
}

/// Capability matrix of the engine, as a JSON string.
#[function]
fn capabilities(ctx: CallContext) -> Result<JsValue> {
    ctx.env().string(OpenSmt::new().capabilities())
}

/// Creates an empty document and returns its summary.
#[function]
fn create(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let summary = OpenSmt::new().create(Path::new(&path)).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Opens a document and returns its summary.
#[function]
fn open(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let summary = OpenSmt::new().open(Path::new(&path)).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Imports a KiCad PCB into a new document and returns the import report.
#[function]
fn import(ctx: CallContext) -> Result<JsValue> {
    let source = text(&ctx, 0)?;
    let target = text(&ctx, 1)?;
    let mut session = OpenSmt::new();
    let report = session.import(Path::new(&source), Path::new(&target)).map_err(fail)?;
    respond(&ctx, &report)
}

/// Writes the open document to `target`, or in place, and returns its summary.
#[function]
fn save(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let target = optional_text(&ctx, 1)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.save(target.as_deref().map(Path::new)).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Replaces the board and saves the document.
#[function]
fn board_set(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let board = Board::new(
        required_number_field(&request, "width")?,
        required_number_field(&request, "depth")?,
        required_number_field(&request, "thickness")?,
    );
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.set_board(board).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Replaces the panel array and saves the document.
#[function]
fn panel_set(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let panel = Panel::new(
        optional_u32(&request, "rows")?.unwrap_or(1),
        optional_u32(&request, "columns")?.unwrap_or(1),
        number_field(&request, "gapX")?.unwrap_or(0.0),
        number_field(&request, "gapY")?.unwrap_or(0.0),
    );
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.set_panel(panel).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Adds a component and saves the document.
#[function]
fn component_add(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let mut component = Component::new(
        text_field(&request, "refdes")?,
        text_field(&request, "package")?,
        required_number_field(&request, "x")?,
        required_number_field(&request, "y")?,
    );
    if let Some(side) = request.get("side").and_then(Value::as_str) {
        component.side = side.parse::<Side>().map_err(fail)?;
    }
    component.rotation = number_field(&request, "rotation")?.unwrap_or(0.0);
    component.height = number_field(&request, "height")?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.add_component(component).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Removes a component and saves the document.
#[function]
fn component_remove(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let refdes = text_field(&request, "refdes")?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.remove_component(&refdes).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Adds a feeder and saves the document.
#[function]
fn feeder_add(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let capacity = optional_u32(&request, "capacity")?.unwrap_or(0);
    let mut feeder = Feeder::new(
        text_field(&request, "id")?,
        text_field(&request, "package")?,
        required_number_field(&request, "pitch")?,
        capacity,
    );
    if let Some(loaded) = optional_u32(&request, "loaded")? {
        feeder.loaded = loaded;
    }
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.add_feeder(feeder).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Removes a feeder and saves the document.
#[function]
fn feeder_remove(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let id = text_field(&request, "id")?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.remove_feeder(&id).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Replaces the reflow profile and saves the document.
#[function]
fn profile_set(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let zones = match request.get("zones") {
        Some(Value::Array(zones)) if !zones.is_empty() => {
            let mut parsed = Vec::with_capacity(zones.len());
            for zone in zones {
                parsed.push(Zone::new(
                    text_field(zone, "name")?,
                    required_number_field(zone, "length")?,
                    required_number_field(zone, "temperature")?,
                ));
            }
            parsed
        }
        _ => return Err(argument_error("`zones` must be a non-empty array")),
    };
    let profile = ReflowProfile {
        conveyor_speed: required_number_field(&request, "conveyorSpeed")?,
        liquidus: required_number_field(&request, "liquidus")?,
        zones,
    };
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.set_profile(profile).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Validates a document.
#[function]
fn validate(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.validate().map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the placement program; an optional second argument enables 2-opt.
#[function]
fn placement(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let optimize = optional_bool(&ctx, 1)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.placement_with(optimize).map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the feeder setup.
#[function]
fn feeders(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.feeders().map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the multi-machine line balance; an optional third argument enables
/// per-machine 2-opt.
#[function]
fn balance(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let machines = argument(&ctx, 1)?.as_i64()?;
    let machines = u32::try_from(machines).unwrap_or(0);
    let optimize = optional_bool(&ctx, 2)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.balance_with(machines, optimize).map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the dual-lane schedule.
#[function]
fn dual_lane(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.dual_lane().map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the reflow evaluation.
#[function]
fn reflow(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.reflow().map_err(fail)?;
    respond(&ctx, &report)
}

/// Exports a document as `program`, `feeders`, `json` or `md`.
#[function]
fn export(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let format = text(&ctx, 1)?.parse::<ExportFormat>().map_err(fail)?;
    let target = text(&ctx, 2)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.export(format, Path::new(&target)).map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the full report.
#[function]
fn report(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.report().map_err(fail)?;
    respond(&ctx, &report)
}

register_module![
    version,
    capabilities,
    create,
    open,
    import,
    save,
    board_set,
    panel_set,
    component_add,
    component_remove,
    feeder_add,
    feeder_remove,
    profile_set,
    validate,
    placement,
    feeders,
    balance,
    dual_lane,
    reflow,
    export,
    report
];
