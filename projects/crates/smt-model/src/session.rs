//! Path-backed editing session.
//!
//! [`OpenSmt`] owns at most one document. Every editing call validates the
//! candidate document, writes it atomically and returns a fresh summary, so the
//! CLI, the native addon and the TypeScript SDK cannot drift apart.

use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::Serialize;

use crate::{
    CAPABILITIES_JSON, SmtError,
    balance::{self, BalanceReport},
    dual::{self, DualLaneReport, DualLaneStrategy, LanesReport},
    export::{feeder_csv, markdown, program_csv, write_atomic},
    import::ImportReport,
    model::{Board, Component, Feeder, Panel, ReflowProfile, Side, SmtDocument, UNITS},
    place::{self, FeederReport, PlacementReport},
    reflow::{self, ReflowReport},
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
            "json" => Ok(Self::Json),
            "md" | "markdown" => Ok(Self::Md),
            "kicad" | "altium" | "odb" => Err(crate::not_implemented(&format!("`{value}` export"))),
            other => Err(SmtError::new(
                "cli/unknown-format",
                format!("unknown export format `{other}`; only program, feeders, json or md is supported"),
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

    /// Exports the open document and returns the created files.
    pub fn export(&self, format: ExportFormat, target: &Path) -> Result<ExportReport, SmtError> {
        let document = self.document()?;
        let text = match format {
            ExportFormat::Program => {
                let feeders = place::allocate(document)?;
                program_csv(&place::sequence(document, &feeders)?)
            }
            ExportFormat::Feeders => feeder_csv(&place::allocate(document)?),
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
