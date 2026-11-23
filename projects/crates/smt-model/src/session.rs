//! Path-backed editing session.
//!
//! [`OpenSmt`] owns at most one document. Every editing call validates the
//! candidate document, writes it atomically and returns a fresh summary, so the
//! CLI, the native addon and the TypeScript SDK cannot drift apart.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::Serialize;

use crate::{
    CAPABILITIES_JSON, SmtError,
    balance::{self, BalanceReport},
    bom::{self, BomCheckReport},
    changeover::{self, ChangeoverReport},
    consumption::{self, ConsumptionReport},
    dual::{self, DualLaneReport, DualLaneStrategy, LanesReport},
    export::{feeder_csv, first_article_csv, markdown, program_csv, write_atomic},
    feeder::{self, FeederLoadingReport},
    first_article::{self, FirstArticleReport},
    heatmap::{self, HeatmapReport},
    import::ImportReport,
    machine_program::{self, MachineProgramExport, MachineProgramVerify},
    model::{Board, Component, Feeder, Panel, ReflowProfile, Side, SmtDocument, UNITS},
    panel_fit::{self, PanelFitReport},
    panel_map::{self, PanelMapReport},
    place::{self, FeederReport, PlacementReport},
    reflow::{self, ReflowReport},
    reflow_window::{self, ReflowWindowLimits, ReflowWindowReport},
    schedule::{self, FeederScheduleReport},
    station::{self, StationReport},
    takt::{self, TaktOptions, TaktReport},
    validate::{self, ValidationReport},
};

/// Board used by [`OpenSmt::create`] when the caller sets none.
pub const DEFAULT_BOARD: Board = Board { width: 100.0, depth: 80.0, thickness: 1.6 };

/// Summary of the document a session currently owns.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    /// Opaque document identity, derived from the file stem.
    pub id: String,
    /// Path of the document file.
    pub path: String,
    /// Declared unit system.
    pub units: String,
    /// Monotonically increasing edit counter.
    pub revision: u32,
    /// Current board.
    pub board: Board,
    /// Current panel array.
    pub panel: Panel,
    /// Number of components.
    pub components: usize,
    /// Number of feeders.
    pub feeders: usize,
}

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// The placement program as CSV.
    Program,
    /// The feeder setup sheet as CSV.
    Feeders,
    /// The first-article inspection sheet as CSV.
    FirstArticle,
    /// The full report as JSON.
    Json,
    /// The full report as Markdown.
    Md,
}

impl ExportFormat {
    /// Returns the short identifier used by the CLI and by reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Program => "program",
            Self::Feeders => "feeders",
            Self::FirstArticle => "fai",
            Self::Json => "json",
            Self::Md => "md",
        }
    }
}

impl FromStr for ExportFormat {
    type Err = SmtError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "program" | "csv" => Ok(Self::Program),
            "feeders" | "setup" => Ok(Self::Feeders),
            "fai" | "first-article" => Ok(Self::FirstArticle),
            "json" => Ok(Self::Json),
            "md" | "markdown" => Ok(Self::Md),
            "kicad" | "altium" | "odb" => Err(crate::not_implemented(&format!("`{value}` export"))),
            other => Err(SmtError::new(
                "cli/unknown-format",
                format!("unknown export format `{other}`; only program, feeders, fai, json or md is supported"),
            )),
        }
    }
}

/// Report returned by [`OpenSmt::export`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    /// Format that was written.
    pub format: String,
    /// Target path exactly as the caller supplied it.
    pub path: String,
    /// Concrete files created by the export.
    pub files: Vec<String>,
    /// Number of bytes written.
    pub bytes: usize,
}

/// Full report returned by [`OpenSmt::report`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Document summary.
    pub document: DocumentSummary,
    /// Validation findings.
    pub validation: ValidationReport,
    /// Placement program.
    pub placement: PlacementReport,
    /// Feeder setup.
    pub feeders: FeederReport,
    /// Reflow evaluation.
    pub reflow: ReflowReport,
    /// Number of rows in the placement program CSV, including the header.
    pub program_lines: usize,
}

/// Partial in-place edit of one placed component.
///
/// Every supplied field replaces the stored value; omitted fields keep theirs.
/// The reference designator is never part of the request because it identifies
/// the component. Supplying `rotation` re-normalises it to `[0, 360)`; omitting
/// it keeps the stored value unchanged. `clear_height` sets the height back to
/// `None` and conflicts with `height`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComponentUpdate {
    /// Replacement package identifier.
    pub package: Option<String>,
    /// Replacement board side.
    pub side: Option<Side>,
    /// Replacement X coordinate.
    pub x: Option<f64>,
    /// Replacement Y coordinate.
    pub y: Option<f64>,
    /// Replacement rotation in degrees, normalised to `[0, 360)`.
    pub rotation: Option<f64>,
    /// Replacement component height.
    pub height: Option<f64>,
    /// Clears the component height.
    pub clear_height: bool,
}

impl ComponentUpdate {
    /// Returns whether the request names no field at all.
    pub const fn is_empty(&self) -> bool {
        self.package.is_none()
            && self.side.is_none()
            && self.x.is_none()
            && self.y.is_none()
            && self.rotation.is_none()
            && self.height.is_none()
            && !self.clear_height
    }
}

/// Partial in-place edit of one feeder.
///
/// Every supplied field replaces the stored value; omitted fields keep theirs.
/// The feeder id is never part of the request because it identifies the feeder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FeederUpdate {
    /// Replacement package identifier.
    pub package: Option<String>,
    /// Replacement tape pitch in millimetres.
    pub pitch: Option<f64>,
    /// Replacement maximum lane capacity.
    pub capacity: Option<u32>,
    /// Replacement loaded quantity.
    pub loaded: Option<u32>,
}

impl FeederUpdate {
    /// Returns whether the request names no field at all.
    pub const fn is_empty(&self) -> bool {
        self.package.is_none() && self.pitch.is_none() && self.capacity.is_none() && self.loaded.is_none()
    }
}

/// One document session.
#[derive(Debug, Default)]
pub struct OpenSmt {
    path: Option<PathBuf>,
    document: Option<SmtDocument>,
}

impl OpenSmt {
    /// Creates an empty session. Constructing a session opens no file.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the stable capability matrix of this build.
    pub fn capabilities(&self) -> &'static str {
        CAPABILITIES_JSON
    }

    /// Creates a document at `path` with the default board and profile.
    pub fn create(&mut self, path: &Path) -> Result<DocumentSummary, SmtError> {
        if path.exists() {
            return Err(SmtError::new("file/exists", format!("`{}` already exists", path.display())));
        }
        let document = SmtDocument::new(DEFAULT_BOARD);
        let text =
            serde_json::to_string_pretty(&document).map_err(|error| SmtError::new("document/serialize", error.to_string()))?;
        write_atomic(path, &text)?;
        self.path = Some(path.to_path_buf());
        self.document = Some(document);
        self.summary()
    }

    /// Opens the document at `path`.
    pub fn open(&mut self, path: &Path) -> Result<DocumentSummary, SmtError> {
        let text = fs::read_to_string(path)
            .map_err(|error| SmtError::new("io/read", format!("cannot read `{}`: {error}", path.display())))?;
        let document: SmtDocument = serde_json::from_str(&text)
            .map_err(|error| SmtError::new("document/parse", format!("cannot parse `{}`: {error}", path.display())))?;
        self.path = Some(path.to_path_buf());
        self.document = Some(document);
        self.summary()
    }

    /// Imports a KiCad PCB into a new document at `target`.
    ///
    /// Only the rectangular `Edge.Cuts` outline and one component per footprint
    /// are imported; every other construct is counted in the returned loss
    /// ledger. The target must not exist yet.
    pub fn import(&mut self, source: &Path, target: &Path) -> Result<ImportReport, SmtError> {
        if target.exists() {
            return Err(SmtError::new("file/exists", format!("`{}` already exists", target.display())));
        }
        let source_text = fs::read_to_string(source)
            .map_err(|error| SmtError::new("io/read", format!("cannot read `{}`: {error}", source.display())))?;
        let imported = crate::import::parse(&source_text)?;
        let losses = imported.loss_entries();
        let crate::import::Imported { board, components, .. } = imported;

        let mut document = SmtDocument::new(board);
        document.components = components;
        let report = validate::validate(&document);
        // Imported documents carry no feeders yet; those coverage errors are
        // reported by `validate` once the setup exists and must not block the
        // import, exactly like an edit that adds components first.
        if let Some(issue) = report.issues.iter().find(|issue| {
            issue.severity == crate::Severity::Error && issue.code != "feeder/missing" && issue.code != "feeder/capacity"
        }) {
            return Err(SmtError::new(issue.code.clone(), issue.message.clone()));
        }

        let text =
            serde_json::to_string_pretty(&document).map_err(|error| SmtError::new("document/serialize", error.to_string()))?;
        write_atomic(target, &text)?;
        let component_count = document.components.len();
        self.path = Some(target.to_path_buf());
        self.document = Some(document);
        Ok(ImportReport { format: crate::import::FORMAT.to_string(), losses, component_count, board })
    }

    /// Imports a generic CSV placement file into a new document at `target`.
    ///
    /// The header must declare `refdes`, `package`, `x` and `y`; `rotation` and
    /// `side` are optional. Rows keep their source order, empty lines are
    /// skipped and `default_side` applies when a row states no side. Coordinates
    /// are millimetres and are neither converted nor translated; the board is
    /// sized to the component extents. The import is one edit, so the written
    /// document starts at revision 1.
    pub fn import_csv(&mut self, source: &Path, target: &Path, default_side: Side) -> Result<ImportReport, SmtError> {
        if target.exists() {
            return Err(SmtError::new("file/exists", format!("`{}` already exists", target.display())));
        }
        let source_text = fs::read_to_string(source)
            .map_err(|error| SmtError::new("io/read", format!("cannot read `{}`: {error}", source.display())))?;
        let crate::csv_import::ImportedCsv { board, components } = crate::csv_import::parse(&source_text, default_side)?;

        let mut document = SmtDocument::new(board);
        document.components = components;
        document.revision = document.revision.saturating_add(1);
        let report = validate::validate(&document);
        if let Some(issue) = report.issues.iter().find(|issue| {
            issue.severity == crate::Severity::Error && issue.code != "feeder/missing" && issue.code != "feeder/capacity"
        }) {
            return Err(SmtError::new(issue.code.clone(), issue.message.clone()));
        }

        let text =
            serde_json::to_string_pretty(&document).map_err(|error| SmtError::new("document/serialize", error.to_string()))?;
        write_atomic(target, &text)?;
        let component_count = document.components.len();
        self.path = Some(target.to_path_buf());
        self.document = Some(document);
        Ok(ImportReport { format: crate::csv_import::FORMAT.to_string(), losses: Vec::new(), component_count, board })
    }

    /// Writes the open document in place, or to `target` when supplied.
    pub fn save(&mut self, target: Option<&Path>) -> Result<DocumentSummary, SmtError> {
        let document = self.document()?.clone();
        let target = target.map(Path::to_path_buf).or_else(|| self.path.clone()).ok_or_else(no_document)?;
        let text =
            serde_json::to_string_pretty(&document).map_err(|error| SmtError::new("document/serialize", error.to_string()))?;
        write_atomic(&target, &text)?;
        self.path = Some(target);
        self.summary()
    }

    /// Replaces the board and saves the document.
    pub fn set_board(&mut self, board: Board) -> Result<DocumentSummary, SmtError> {
        let mut document = self.document()?.clone();
        document.board = board;
        self.persist(document)
    }

    /// Replaces the panel array and saves the document.
    pub fn set_panel(&mut self, panel: Panel) -> Result<DocumentSummary, SmtError> {
        let mut document = self.document()?.clone();
        document.panel = panel;
        self.persist(document)
    }

    /// Adds a component and saves the document.
    pub fn add_component(&mut self, component: Component) -> Result<DocumentSummary, SmtError> {
        let mut document = self.document()?.clone();
        if document.components.iter().any(|entry| entry.refdes == component.refdes) {
            return Err(SmtError::new(
                "component/duplicate-refdes",
                format!("component `{}` already exists", component.refdes),
            ));
        }
        document.components.push(component);
        self.persist(document)
    }

    /// Applies a partial edit to a placed component and saves the document.
    ///
    /// `refdes` selects the component and never changes. The request must name
    /// at least one field and cannot set and clear the height at once; both
    /// report `component/invalid`. An unknown reference reports
    /// `component/unknown-refdes`. The edited document is validated with the
    /// same rules as [`OpenSmt::add_component`] before it is written.
    pub fn update_component(&mut self, refdes: &str, request: ComponentUpdate) -> Result<DocumentSummary, SmtError> {
        let mut document = self.document()?.clone();
        if request.is_empty() {
            return Err(SmtError::new("component/invalid", "component update needs at least one field"));
        }
        if request.clear_height && request.height.is_some() {
            return Err(SmtError::new("component/invalid", "component update cannot set and clear the height at once"));
        }
        let Some(component) = document.components.iter_mut().find(|component| component.refdes == refdes)
        else {
            return Err(SmtError::new("component/unknown-refdes", format!("component `{refdes}` does not exist")));
        };
        if let Some(package) = request.package {
            component.package = package;
        }
        if let Some(side) = request.side {
            component.side = side;
        }
        if let Some(x) = request.x {
            component.x = x;
        }
        if let Some(y) = request.y {
            component.y = y;
        }
        if let Some(rotation) = request.rotation {
            component.rotation = rotation.rem_euclid(360.0);
        }
        if request.clear_height {
            component.height = None;
        }
        else if let Some(height) = request.height {
            component.height = Some(height);
        }
        self.persist(document)
    }

    /// Removes a component and saves the document.
    pub fn remove_component(&mut self, refdes: &str) -> Result<DocumentSummary, SmtError> {
        let mut document = self.document()?.clone();
        if !document.components.iter().any(|component| component.refdes == refdes) {
            return Err(SmtError::new("component/unknown-refdes", format!("component `{refdes}` does not exist")));
        }
        document.components.retain(|component| component.refdes != refdes);
        self.persist(document)
    }

    /// Adds a feeder and saves the document.
    pub fn add_feeder(&mut self, feeder: Feeder) -> Result<DocumentSummary, SmtError> {
        let mut document = self.document()?.clone();
        if document.feeders.iter().any(|entry| entry.id == feeder.id) {
            return Err(SmtError::new("feeder/duplicate-id", format!("feeder `{}` already exists", feeder.id)));
        }
        document.feeders.push(feeder);
        self.persist(document)
    }

    /// Applies a partial edit to a feeder and saves the document.
    ///
    /// `id` selects the feeder and never changes. The request must name at
    /// least one field; an empty request reports `feeder/invalid`. An unknown
    /// id reports `feeder/unknown-id`. The edited document is validated with
    /// the same feeder rules as [`OpenSmt::add_feeder`], so an empty package, a
    /// non-positive or non-finite pitch, a zero capacity or a loaded quantity
    /// above the capacity reports `feeder/invalid` and nothing is written.
    pub fn update_feeder(&mut self, id: &str, request: FeederUpdate) -> Result<DocumentSummary, SmtError> {
        let mut document = self.document()?.clone();
        if request.is_empty() {
            return Err(SmtError::new("feeder/invalid", "feeder update needs at least one field"));
        }
        let Some(feeder) = document.feeders.iter_mut().find(|feeder| feeder.id == id)
        else {
            return Err(SmtError::new("feeder/unknown-id", format!("feeder `{id}` does not exist")));
        };
        if let Some(package) = request.package {
            feeder.package = package;
        }
        if let Some(pitch) = request.pitch {
            feeder.pitch = pitch;
        }
        if let Some(capacity) = request.capacity {
            feeder.capacity = capacity;
        }
        if let Some(loaded) = request.loaded {
            feeder.loaded = loaded;
        }
        self.persist(document)
    }

    /// Removes a feeder and saves the document.
    pub fn remove_feeder(&mut self, id: &str) -> Result<DocumentSummary, SmtError> {
        let mut document = self.document()?.clone();
        let Some(feeder) = document.feeders.iter().find(|feeder| feeder.id == id)
        else {
            return Err(SmtError::new("feeder/unknown-id", format!("feeder `{id}` does not exist")));
        };
        let used = document.components.iter().any(|component| component.package == feeder.package);
        let suppliers = document.feeders.iter().filter(|entry| entry.package == feeder.package).count();
        if used && suppliers <= 1 {
            return Err(SmtError::new(
                "feeder/in-use",
                format!("feeder `{id}` is the only supplier of package `{}`", feeder.package),
            ));
        }
        document.feeders.retain(|entry| entry.id != id);
        self.persist(document)
    }

    /// Replaces the reflow profile and saves the document.
    pub fn set_profile(&mut self, profile: ReflowProfile) -> Result<DocumentSummary, SmtError> {
        let mut document = self.document()?.clone();
        document.profile = profile;
        self.persist(document)
    }

    /// Validates the open document.
    pub fn validate(&self) -> Result<ValidationReport, SmtError> {
        Ok(validate::validate(self.document()?))
    }

    /// Returns the feeder allocation of the open document.
    pub fn feeders(&self) -> Result<FeederReport, SmtError> {
        place::allocate(self.document()?)
    }

    /// Returns the read-only feeder loading report of the open document.
    ///
    /// The report aggregates placements by package and suggests one tape slot
    /// per package; it does not need a feeder setup and never writes.
    pub fn feeder_report(&self) -> Result<FeederLoadingReport, SmtError> {
        Ok(feeder::report(self.document()?))
    }

    /// Returns the read-only panel fit report for a maximum panel size.
    ///
    /// `gap_x` and `gap_y` default to the stored panel gaps. The document is
    /// never modified.
    pub fn panel_fit(
        &self,
        max_width: f64,
        max_depth: f64,
        gap_x: Option<f64>,
        gap_y: Option<f64>,
    ) -> Result<PanelFitReport, SmtError> {
        panel_fit::fit(self.document()?, max_width, max_depth, gap_x, gap_y)
    }

    /// Returns the read-only single-board to panel coordinate map.
    ///
    /// Every component lists its placement on every board instance of the stored
    /// panel array, and the report counts components outside the single board.
    /// The document is never modified.
    pub fn panel_map(&self) -> Result<PanelMapReport, SmtError> {
        panel_map::map(self.document()?)
    }

    /// Returns the read-only first-article inspection sheet.
    ///
    /// The sheet runs the same allocation and sequence as [`OpenSmt::placement`],
    /// so feeder coverage errors propagate unchanged. The document is never
    /// modified.
    pub fn first_article(&self) -> Result<FirstArticleReport, SmtError> {
        first_article::sheet(self.document()?)
    }

    /// Returns the read-only BOM cross-check against the BOM CSV at `bom`.
    ///
    /// The report compares the placement list with the BOM and names missing
    /// and extra reference designators plus per-package quantity mismatches.
    /// Neither the document nor the BOM file is modified.
    pub fn bom_check(&self, bom: &Path) -> Result<BomCheckReport, SmtError> {
        let document = self.document()?;
        let text = fs::read_to_string(bom)
            .map_err(|error| SmtError::new("io/read", format!("cannot read `{}`: {error}", bom.display())))?;
        bom::check(document, &text)
    }

    /// Returns the placement program of the open document.
    pub fn placement(&self) -> Result<PlacementReport, SmtError> {
        self.placement_with(false)
    }

    /// Returns the placement program, optionally improved with 2-opt.
    pub fn placement_with(&self, optimize: bool) -> Result<PlacementReport, SmtError> {
        let document = self.document()?;
        let feeders = place::allocate(document)?;
        place::sequence_with(document, &feeders, optimize)
    }

    /// Returns the multi-machine line balance of the open document.
    pub fn balance(&self, machine_count: u32) -> Result<BalanceReport, SmtError> {
        self.balance_with(machine_count, false)
    }

    /// Returns the multi-machine line balance, optionally improved with 2-opt
    /// per machine.
    pub fn balance_with(&self, machine_count: u32, optimize: bool) -> Result<BalanceReport, SmtError> {
        balance::balance_with(self.document()?, machine_count, optimize)
    }

    /// Returns the dual-lane schedule of the open document.
    pub fn dual_lane(&self) -> Result<DualLaneReport, SmtError> {
        self.dual_lane_with(DualLaneStrategy::Greedy, false)
    }

    /// Returns the dual-lane schedule with an explicit strategy and an optional
    /// single-swap improvement pass.
    pub fn dual_lane_with(&self, strategy: DualLaneStrategy, improve: bool) -> Result<DualLaneReport, SmtError> {
        dual::plan_dual_lane_with_strategy(self.document()?, strategy, improve)
    }

    /// Returns the multi-lane schedule of the open document.
    ///
    /// The lane count must be between 2 and 16; any other value reports
    /// `schedule/invalid-lanes`.
    pub fn lanes(&self, lane_count: u32, strategy: DualLaneStrategy, improve: bool) -> Result<LanesReport, SmtError> {
        let lane_count = usize::try_from(lane_count).unwrap_or(0);
        dual::plan_document_lanes(self.document()?, lane_count, strategy, improve)
    }

    /// Returns the reflow evaluation of the open document.
    pub fn reflow(&self) -> Result<ReflowReport, SmtError> {
        reflow::evaluate(self.document()?)
    }

    /// Returns the read-only reflow process-window report of the open document.
    ///
    /// Every supplied limit produces one check with its margin; an invalid limit
    /// reports `reflow/invalid`. The document is never modified.
    pub fn reflow_window(&self, limits: &ReflowWindowLimits) -> Result<ReflowWindowReport, SmtError> {
        reflow_window::evaluate(self.document()?, limits)
    }

    /// Returns the read-only feeder consumption plan for `boards` boards.
    ///
    /// A zero board count reports `consumption/invalid`; a missing or undersized
    /// feeder fails like the `feeders` report. The document is never modified.
    pub fn consumption(&self, boards: u32) -> Result<ConsumptionReport, SmtError> {
        consumption::plan(self.document()?, boards)
    }

    /// Returns the read-only feeder replenishment schedule for `boards` boards.
    ///
    /// A zero board count reports `schedule/invalid`; a missing or undersized
    /// feeder fails like the `feeders` report. A lane that cannot cover a board
    /// is reported as short instead of failing, and the document is never
    /// modified.
    pub fn schedule(&self, boards: u32) -> Result<FeederScheduleReport, SmtError> {
        schedule::plan(self.document()?, boards)
    }

    /// Returns the read-only cycle time and throughput estimate of the open
    /// document.
    ///
    /// The estimate runs the same allocation and sequence as
    /// [`OpenSmt::placement`] and applies the machine model in `options`. The
    /// document is never modified.
    pub fn takt(&self, options: &TaktOptions) -> Result<TaktReport, SmtError> {
        takt::estimate(self.document()?, options)
    }

    /// Returns the read-only takt bottleneck heat map of the open document.
    ///
    /// The map runs the same allocation and nearest-neighbour sequence as the
    /// default [`OpenSmt::takt`] estimate, attributes the step time to each
    /// step, package and side, and reports shares and cumulative package
    /// shares. The document is never modified.
    pub fn heatmap(&self) -> Result<HeatmapReport, SmtError> {
        heatmap::heatmap(self.document()?)
    }

    /// Writes the neutral machine program and reads it back for verification.
    ///
    /// `nozzles` maps a package name to the nozzle id the program should carry;
    /// a package without an entry exports an empty nozzle. The report states
    /// whether the written file parsed back to the same program. A malformed
    /// nozzle map reports `program/invalid`, and anything the allocation or the
    /// sequence rejects fails with its own code. The document is never modified.
    pub fn export_machine_program(
        &self,
        target: &Path,
        nozzles: &BTreeMap<String, String>,
    ) -> Result<MachineProgramExport, SmtError> {
        let program = machine_program::build(self.document()?, nozzles)?;
        let text = machine_program::render(&program);
        write_atomic(target, &text)?;
        let readback = machine_program::parse(&text)?;
        if !machine_program::verify(&program, &readback).is_empty() {
            return Err(SmtError::new("program/invalid", "the written machine program did not read back identically"));
        }
        Ok(MachineProgramExport {
            format: machine_program::FORMAT.to_string(),
            version: 1,
            units: program.units.clone(),
            path: target.display().to_string(),
            steps: program.components,
            bytes: text.len(),
            verified: true,
        })
    }

    /// Checks a machine program file against the open document.
    ///
    /// The document program is rebuilt with the same `nozzles` map and compared
    /// field by field; the report lists every difference and is `ok` only when
    /// there is none. An unreadable file reports `io/read`, a malformed program
    /// reports `program/invalid`. Neither file is modified.
    pub fn verify_machine_program(
        &self,
        source: &Path,
        nozzles: &BTreeMap<String, String>,
    ) -> Result<MachineProgramVerify, SmtError> {
        let expected = machine_program::build(self.document()?, nozzles)?;
        let text = fs::read_to_string(source)
            .map_err(|error| SmtError::new("io/read", format!("cannot read `{}`: {error}", source.display())))?;
        let actual = machine_program::parse(&text)?;
        let mismatches = machine_program::verify(&expected, &actual);
        Ok(MachineProgramVerify {
            format: machine_program::FORMAT.to_string(),
            version: 1,
            units: expected.units.clone(),
            path: source.display().to_string(),
            steps: expected.components,
            ok: mismatches.is_empty(),
            mismatches,
        })
    }

    /// Returns the read-only feeder station assignment of the open document.
    ///
    /// The report sequences the same placement program as [`OpenSmt::placement`]
    /// and orders the feeder lanes along the tape bank. `slot_pitch` is the
    /// distance between neighbouring slots in millimetres; a non-finite or
    /// non-positive value reports `station/invalid`. The document is never
    /// modified.
    pub fn stations(&self, slot_pitch: f64) -> Result<StationReport, SmtError> {
        station::assign(self.document()?, slot_pitch)
    }

    /// Returns the read-only feeder changeover sequence from the open document
    /// to the document at `other`.
    ///
    /// `feeder_change_seconds` is the time assumed for one reel change; a
    /// non-finite or non-positive value reports `changeover/invalid`. Neither
    /// document is modified.
    pub fn changeover(&self, other: &Path, feeder_change_seconds: f64) -> Result<ChangeoverReport, SmtError> {
        let document = self.document()?;
        let text = fs::read_to_string(other)
            .map_err(|error| SmtError::new("io/read", format!("cannot read `{}`: {error}", other.display())))?;
        let next: SmtDocument = serde_json::from_str(&text)
            .map_err(|error| SmtError::new("document/parse", format!("cannot parse `{}`: {error}", other.display())))?;
        changeover::plan(document, &next, feeder_change_seconds)
    }

    /// Exports the open document and returns the created files.
    pub fn export(&self, format: ExportFormat, target: &Path) -> Result<ExportReport, SmtError> {
        let document = self.document()?;
        let text = match format {
            ExportFormat::Program => {
                let feeders = place::allocate(document)?;
                program_csv(&place::sequence(document, &feeders)?)
            }
            ExportFormat::Feeders => feeder_csv(&place::allocate(document)?),
            ExportFormat::FirstArticle => first_article_csv(&first_article::sheet(document)?),
            ExportFormat::Json => {
                let report = self.build_report()?;
                serde_json::to_string_pretty(&report).map_err(|error| SmtError::new("document/serialize", error.to_string()))?
            }
            ExportFormat::Md => {
                let feeders = place::allocate(document)?;
                let placement = place::sequence(document, &feeders)?;
                let reflow = reflow::evaluate(document)?;
                markdown(document, &placement, &feeders, &reflow)
            }
        };
        write_atomic(target, &text)?;
        Ok(ExportReport {
            format: format.as_str().to_string(),
            path: target.display().to_string(),
            files: vec![target.display().to_string()],
            bytes: text.len(),
        })
    }

    /// Returns the summary, validation, placement, feeders and reflow of the document.
    pub fn report(&self) -> Result<Report, SmtError> {
        self.build_report()
    }

    /// Returns the summary of the open document.
    pub fn summary(&self) -> Result<DocumentSummary, SmtError> {
        let document = self.document()?;
        let path = self.path.as_ref().ok_or_else(no_document)?;
        let id = path.file_stem().and_then(|value| value.to_str()).unwrap_or("document").to_string();
        Ok(DocumentSummary {
            id,
            path: path.display().to_string(),
            units: document.units.clone(),
            revision: document.revision,
            board: document.board,
            panel: document.panel,
            components: document.components.len(),
            feeders: document.feeders.len(),
        })
    }

    fn build_report(&self) -> Result<Report, SmtError> {
        let document = self.document()?;
        let summary = self.summary()?;
        let validation = validate::validate(document);
        let feeders = place::allocate(document)?;
        let placement = place::sequence(document, &feeders)?;
        let reflow = reflow::evaluate(document)?;
        let program_lines = placement.entries.len() + 1;
        Ok(Report {
            format: "open-smt-report".to_string(),
            version: 1,
            units: document.units.clone(),
            document: summary,
            validation,
            placement,
            feeders,
            reflow,
            program_lines,
        })
    }

    fn document(&self) -> Result<&SmtDocument, SmtError> {
        self.document.as_ref().ok_or_else(no_document)
    }

    fn persist(&mut self, mut document: SmtDocument) -> Result<DocumentSummary, SmtError> {
        let report = validate::validate(&document);
        // Feeder coverage depends on edit order: a component list is normally
        // entered before the feeder setup. Structural errors still block the
        // write, but an incomplete setup is allowed to be saved and is reported
        // by `validate`, `placement` and `report` until the feeders cover it.
        if let Some(issue) = report.issues.iter().find(|issue| {
            issue.severity == crate::Severity::Error && issue.code != "feeder/missing" && issue.code != "feeder/capacity"
        }) {
            return Err(SmtError::new(issue.code.clone(), issue.message.clone()));
        }
        let path = self.path.as_ref().ok_or_else(no_document)?.clone();
        document.revision = document.revision.saturating_add(1);
        document.units = UNITS.to_string();
        let text =
            serde_json::to_string_pretty(&document).map_err(|error| SmtError::new("document/serialize", error.to_string()))?;
        write_atomic(&path, &text)?;
        self.document = Some(document);
        self.summary()
    }
}

fn no_document() -> SmtError {
    SmtError::new("session/no-document", "no document is open; call `create` or `open` first")
}
