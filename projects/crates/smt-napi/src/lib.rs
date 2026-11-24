//! Thin Node-API surface for OpenSMT.
//!
//! Every export performs one orthogonal operation and returns a JSON string, so
//! the TypeScript layer owns argument parsing, help text and exit codes. All
//! analysis forwards to `smt-model`'s [`smt_model::OpenSmt`] session, the single
//! implementation behind the SDK and the `open-smt` CLI.
//!
//! Bad arguments report `session/argument-invalid`; every other failure keeps
//! the stable code of [`smt_model::SmtError`].

#![deny(missing_docs)]

use std::path::Path;

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use smt_model::{
    Board, Component, ComponentUpdate, DEFAULT_FEEDER_CHANGE_SECONDS, DEFAULT_SLOT_PITCH, DualLaneStrategy, ExportFormat,
    Feeder, FeederUpdate, OpenSmt, Panel, ReflowProfile, ReflowWindowLimits, Side, SmtError, Splice, TaktOptions, Zone,
};
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

fn optional_text_field(request: &Value, name: &str) -> Result<Option<String>> {
    match request.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => match value.as_str() {
            Some(text) if !text.is_empty() => Ok(Some(text.to_string())),
            _ => Err(argument_error(&format!("`{name}` must be a non-empty string"))),
        },
    }
}

fn bool_field(request: &Value, name: &str) -> Result<bool> {
    match request.get(name) {
        None | Some(Value::Null) => Ok(false),
        Some(value) => value.as_bool().ok_or_else(|| argument_error(&format!("`{name}` must be a boolean"))),
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

/// Reads an optional `{ "nozzles": { "0603": "N1" } }` request.
fn nozzle_map(ctx: &CallContext, index: usize) -> Result<BTreeMap<String, String>> {
    let Some(_) = ctx.args().get(index) else {
        return Ok(BTreeMap::new());
    };
    let request = object(ctx, index)?;
    let mut nozzles = BTreeMap::new();
    match request.get("nozzles") {
        None | Some(Value::Null) => {}
        Some(Value::Object(entries)) => {
            for (package, nozzle) in entries {
                let nozzle = nozzle.as_str().ok_or_else(|| argument_error(&format!("`nozzles.{package}` must be a string")))?;
                nozzles.insert(package.clone(), nozzle.to_string());
            }
        }
        Some(_) => return Err(argument_error("`nozzles` must be an object mapping packages to nozzle ids")),
    }
    Ok(nozzles)
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

/// Imports a generic CSV placement file into a new document and returns the
/// import report. An optional third argument is the default side (`top`).
#[function]
fn import_csv(ctx: CallContext) -> Result<JsValue> {
    let source = text(&ctx, 0)?;
    let target = text(&ctx, 1)?;
    let default_side = match optional_text(&ctx, 2)? {
        Some(raw) => raw.parse::<Side>().map_err(fail)?,
        None => Side::Top,
    };
    let mut session = OpenSmt::new();
    let report = session.import_csv(Path::new(&source), Path::new(&target), default_side).map_err(fail)?;
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

/// Applies a partial edit to a placed component and saves the document.
///
/// The request carries `refdes` plus any of `package`, `side`, `x`, `y`,
/// `rotation`, `height` and `clearHeight`.
#[function]
fn component_update(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let refdes = text_field(&request, "refdes")?;
    let side = optional_text_field(&request, "side")?.map(|raw| raw.parse::<Side>()).transpose().map_err(fail)?;
    let update = ComponentUpdate {
        package: optional_text_field(&request, "package")?,
        side,
        x: number_field(&request, "x")?,
        y: number_field(&request, "y")?,
        rotation: number_field(&request, "rotation")?,
        height: number_field(&request, "height")?,
        clear_height: bool_field(&request, "clearHeight")?,
    };
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.update_component(&refdes, update).map_err(fail)?;
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
    feeder.leader_waste = optional_u32(&request, "leaderWaste")?.unwrap_or(0);
    feeder.trailer_waste = optional_u32(&request, "trailerWaste")?.unwrap_or(0);
    feeder.scrap = optional_u32(&request, "scrap")?.unwrap_or(0);
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.add_feeder(feeder).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Applies a partial edit to a feeder and saves the document.
///
/// The request carries `id` plus any of `package`, `pitch`, `capacity`,
/// `loaded`, `leaderWaste`, `trailerWaste` and `scrap`.
#[function]
fn feeder_update(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let id = text_field(&request, "id")?;
    let update = FeederUpdate {
        package: optional_text_field(&request, "package")?,
        pitch: number_field(&request, "pitch")?,
        capacity: optional_u32(&request, "capacity")?,
        loaded: optional_u32(&request, "loaded")?,
        leader_waste: optional_u32(&request, "leaderWaste")?,
        trailer_waste: optional_u32(&request, "trailerWaste")?,
        scrap: optional_u32(&request, "scrap")?,
    };
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.update_feeder(&id, update).map_err(fail)?;
    respond(&ctx, &summary)
}

/// Records a reel splice and saves the document.
///
/// The request carries `id`, `sequence` and optionally `leaderWaste`,
/// `trailerWaste` and `scrap`.
#[function]
fn feeder_splice(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let id = text_field(&request, "id")?;
    let sequence = optional_u32(&request, "sequence")?.ok_or_else(|| argument_error("`sequence` is required"))?;
    let mut splice = Splice::new(id, sequence);
    splice.leader_waste = optional_u32(&request, "leaderWaste")?.unwrap_or(0);
    splice.trailer_waste = optional_u32(&request, "trailerWaste")?.unwrap_or(0);
    splice.scrap = optional_u32(&request, "scrap")?.unwrap_or(0);
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let summary = session.add_splice(splice).map_err(fail)?;
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

/// Returns the read-only component list report.
#[function]
fn component_list(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.component_list().map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the read-only feeder loading report.
#[function]
fn feeder_report(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.feeder_report().map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the read-only panel coordinate map.
#[function]
fn panel_map(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.panel_map().map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the read-only first-article inspection sheet.
#[function]
fn first_article(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.first_article().map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the read-only panel fit report for a maximum panel size.
///
/// The request carries `maxWidth` and `maxDepth`; `gapX` and `gapY` default to
/// the stored panel gaps.
#[function]
fn panel_fit(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let max_width = required_number_field(&request, "maxWidth")?;
    let max_depth = required_number_field(&request, "maxDepth")?;
    let gap_x = number_field(&request, "gapX")?;
    let gap_y = number_field(&request, "gapY")?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.panel_fit(max_width, max_depth, gap_x, gap_y).map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the read-only BOM cross-check against a BOM CSV.
#[function]
fn bom_check(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let bom = text(&ctx, 1)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.bom_check(Path::new(&bom)).map_err(fail)?;
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
///
/// An optional second argument selects the strategy (`greedy`, the default, or
/// `lpt`); an optional third argument enables the single-swap improvement pass.
#[function]
fn dual_lane(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let strategy = match optional_text(&ctx, 1)? {
        Some(raw) => raw.parse::<DualLaneStrategy>().map_err(fail)?,
        None => DualLaneStrategy::Greedy,
    };
    let improve = optional_bool(&ctx, 2)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.dual_lane_with(strategy, improve).map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the multi-lane schedule.
///
/// A second argument is the lane count (2 to 16); an optional third argument
/// selects the strategy (`greedy`, the default, or `lpt`) and an optional fourth
/// argument enables the pairwise-swap improvement pass.
#[function]
fn schedule_lanes(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let lanes = argument(&ctx, 1)?.as_i64()?;
    let lanes = u32::try_from(lanes).unwrap_or(0);
    let strategy = match optional_text(&ctx, 2)? {
        Some(raw) => raw.parse::<DualLaneStrategy>().map_err(fail)?,
        None => DualLaneStrategy::Greedy,
    };
    let improve = optional_bool(&ctx, 3)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.lanes(lanes, strategy, improve).map_err(fail)?;
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

/// Returns the read-only reflow process-window report.
///
/// An optional second argument is a JSON object of limits (`peakMin`,
/// `peakMax`, `timeAboveLiquidusMin`, `timeAboveLiquidusMax`,
/// `maxHeatingRate`, `maxCoolingRate`); omitted limits produce no check.
#[function]
fn reflow_window(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let limits = match ctx.args().get(1) {
        Some(_) => serde_json::from_value::<ReflowWindowLimits>(object(&ctx, 1)?)
            .map_err(|error| argument_error(&format!("invalid reflow window limits: {error}")))?,
        None => ReflowWindowLimits::default(),
    };
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.reflow_window(&limits).map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the read-only feeder consumption plan for a build quantity.
///
/// The first argument is the document path and the second a JSON object whose
/// `boards` field is the number of boards the plan covers.
#[function]
fn consumption(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let boards = optional_u32(&request, "boards")?.ok_or_else(|| argument_error("`boards` is required"))?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.consumption(boards).map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the read-only feeder replenishment schedule for a build quantity.
///
/// The first argument is the document path and the second a JSON object whose
/// `boards` field is the number of boards the schedule covers.
#[function]
fn feeder_schedule(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let request = object(&ctx, 1)?;
    let boards = optional_u32(&request, "boards")?.ok_or_else(|| argument_error("`boards` is required"))?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.schedule(boards).map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the read-only cycle time and throughput estimate.
///
/// An optional second argument is a JSON object of machine-model options;
/// omitted fields keep their defaults.
#[function]
fn takt(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let options = match ctx.args().get(1) {
        Some(_) => {
            let request = object(&ctx, 1)?;
            serde_json::from_value::<TaktOptions>(request)
                .map_err(|error| argument_error(&format!("invalid takt options: {error}")))?
        }
        None => TaktOptions::default(),
    };
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.takt(&options).map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the read-only takt bottleneck heat map.
#[function]
fn takt_heatmap(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.heatmap().map_err(fail)?;
    respond(&ctx, &report)
}

/// Writes the neutral machine program and verifies the readback.
///
/// The first path is the document and the second the target CSV; an optional
/// third argument is a JSON object whose `nozzles` field maps packages to
/// nozzle ids.
#[function]
fn machine_export(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let target = text(&ctx, 1)?;
    let nozzles = nozzle_map(&ctx, 2)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.export_machine_program(Path::new(&target), &nozzles).map_err(fail)?;
    respond(&ctx, &report)
}

/// Checks a machine program file against a document.
///
/// The first path is the document and the second the program CSV; an optional
/// third argument is a JSON object whose `nozzles` field maps packages to
/// nozzle ids.
#[function]
fn machine_verify(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let source = text(&ctx, 1)?;
    let nozzles = nozzle_map(&ctx, 2)?;
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.verify_machine_program(Path::new(&source), &nozzles).map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the read-only feeder station assignment.
///
/// An optional second argument is a JSON object; its `slotPitch` field
/// overrides the default distance between neighbouring slots.
#[function]
fn feeder_optimize(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let slot_pitch = match ctx.args().get(1) {
        Some(_) => number_field(&object(&ctx, 1)?, "slotPitch")?.unwrap_or(DEFAULT_SLOT_PITCH),
        None => DEFAULT_SLOT_PITCH,
    };
    let mut session = OpenSmt::new();
    session.open(Path::new(&path)).map_err(fail)?;
    let report = session.stations(slot_pitch).map_err(fail)?;
    respond(&ctx, &report)
}

/// Returns the read-only feeder changeover sequence between two documents.
///
/// The first path is the current document and the second the next one; an
/// optional third argument is a JSON object whose `feederChangeSeconds` field
/// overrides the default time per reel change.
#[function]
fn changeover(ctx: CallContext) -> Result<JsValue> {
    let current = text(&ctx, 0)?;
    let next = text(&ctx, 1)?;
    let feeder_change_seconds = match ctx.args().get(2) {
        Some(_) => number_field(&object(&ctx, 2)?, "feederChangeSeconds")?.unwrap_or(DEFAULT_FEEDER_CHANGE_SECONDS),
        None => DEFAULT_FEEDER_CHANGE_SECONDS,
    };
    let mut session = OpenSmt::new();
    session.open(Path::new(&current)).map_err(fail)?;
    let report = session.changeover(Path::new(&next), feeder_change_seconds).map_err(fail)?;
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
    import_csv,
    save,
    board_set,
    panel_set,
    component_add,
    component_update,
    component_remove,
    feeder_add,
    feeder_update,
    feeder_splice,
    feeder_remove,
    profile_set,
    validate,
    placement,
    feeders,
    component_list,
    feeder_report,
    panel_map,
    first_article,
    panel_fit,
    bom_check,
    balance,
    dual_lane,
    schedule_lanes,
    reflow,
    reflow_window,
    consumption,
    feeder_schedule,
    takt,
    takt_heatmap,
    machine_export,
    machine_verify,
    feeder_optimize,
    changeover,
    export,
    report
];
