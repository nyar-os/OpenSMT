//! OpenSMT: a deterministic SMT assembly planner.
//!
//! This crate implements a deliberately small assembly kernel:
//!
//! * one rectangular board, an optional panel array and a component list;
//! * a feeder setup with package, pitch, capacity and loaded quantity;
//! * a deterministic placement program: feeders are allocated in id order and
//!   the head visits the nearest remaining component, top side first; an
//!   optional 2-opt pass improves each side's route;
//! * a multi-machine line balance that assigns components in reference
//!   designator order to the machine with the smallest estimated completion
//!   time;
//! * a multi-lane schedule (2 to 16 lanes) that assigns placement tasks to the
//!   lane with the smaller accumulated time, in program order (greedy) or
//!   longest-first (LPT), with an optional pairwise-swap improvement pass;
//! * a reflow profile evaluator that integrates time above liquidus over linear
//!   zone ramps;
//! * a read-only BOM cross-check against a BOM CSV;
//! * a read-only feeder station assignment along the tape bank;
//! * a read-only feeder changeover sequence between two documents;
//! * a read-only reflow process-window margin report;
//! * a read-only feeder consumption plan for a build quantity;
//! * CSV program and feeder sheets, plus JSON and Markdown reports.
//!
//! Paste-aperture generation and vendor machine programs are **not**
//! implemented. See [`CAPABILITIES_JSON`].

#![deny(missing_docs)]

mod balance;
mod bom;
mod changeover;
mod consumption;
mod csv_import;
mod dual;
mod error;
mod export;
mod feeder;
mod import;
mod model;
mod panel_fit;
mod place;
mod reflow;
mod reflow_window;
mod session;
mod station;
mod takt;
pub mod validate;

pub use crate::{
    balance::{BalanceReport, MAX_MACHINES, MIN_MACHINES, MachineFeeder, MachineReport, balance, balance_with},
    bom::{BomCheckReport, BomFinding, BomQuantityMismatch, check as bom_check},
    changeover::{ChangeoverAction, ChangeoverKind, ChangeoverReport, DEFAULT_FEEDER_CHANGE_SECONDS, plan as plan_changeover},
    consumption::{ConsumptionReport, FeederConsumption, PackageConsumption, plan as plan_consumption},
    dual::{
        DualLaneReport, DualLaneStrategy, DualLaneTask, LANES, LaneSchedule, LanesReport, MAX_LANES, MIN_LANES,
        plan_document_lanes, plan_dual_lane, plan_dual_lane_tasks, plan_dual_lane_tasks_with_strategy,
        plan_dual_lane_with_strategy, plan_lanes,
    },
    error::{SmtError, not_implemented},
    export::{feeder_csv, markdown, program_csv, write_atomic},
    feeder::{FeederLoadingReport, FeederSlot, report as feeder_loading_report},
    import::{ImportLoss, ImportReport},
    model::{Board, Component, FORMAT, Feeder, Panel, ReflowProfile, Side, SmtDocument, UNITS, VERSION, Zone},
    panel_fit::{PanelFitReport, fit as panel_fit},
    place::{
        FeederReport, FeederUsage, HEAD_SPEED, PLACE_SECONDS, PackageUsage, PlacementEntry, PlacementReport,
        allocate as allocate_feeders, sequence as sequence_placement, sequence_with as sequence_placement_with,
    },
    reflow::{AMBIENT, ReflowReport, ZoneReport, evaluate as evaluate_reflow},
    reflow_window::{ReflowWindowLimits, ReflowWindowReport, WindowCheck, WindowZone, evaluate as evaluate_reflow_window},
    session::{ComponentUpdate, DEFAULT_BOARD, DocumentSummary, ExportFormat, ExportReport, FeederUpdate, OpenSmt, Report},
    station::{DEFAULT_SLOT_PITCH, Station, StationReport, assign as assign_stations},
    takt::{TaktOptions, TaktReport, estimate as takt_estimate},
    validate::{Issue, Severity, ValidationReport},
};

/// Stable machine-readable capability matrix of this build.
pub const CAPABILITIES_JSON: &str = r#"{
  "analysis": {
    "placementSequence": "supported",
    "placementOptimization": "supported",
    "feederAllocation": "supported",
    "feederLoadingReport": "supported",
    "bomCheck": "supported",
    "panelization": "supported",
    "panelFit": "supported",
    "reflowProfile": "supported",
    "placementProgram": "supported",
    "pasteAperture": "unsupported",
    "machineProgram": "unsupported",
    "lineBalance": "supported",
    "dualLane": "supported",
    "multiLane": "supported",
    "cycleTimeEstimate": "supported",
    "feederStationAssignment": "supported",
    "changeoverSequence": "supported",
    "reflowWindow": "supported",
    "feederConsumption": "supported"
  },
  "io": {
    "osmt": "supported",
    "programCsv": "supported",
    "feedersCsv": "supported",
    "json": "supported",
    "md": "supported",
    "kicad": "unsupported",
    "altium": "unsupported",
    "odb": "unsupported"
  },
  "imports": {
    "kicad_pcb": "supported",
    "csv": "supported"
  },
  "limitations": {
    "dualLane": "greedy or LPT heuristic with an optional single-swap pass, not an optimal schedule",
    "multiLane": "greedy or LPT heuristic on 2 to 16 lanes with an optional pairwise-swap pass, not an optimal schedule",
    "feederLoadingReport": "slot numbers are suggestions only; no machine slot, tape direction or pitch constraint is modelled",
    "bomCheck": "packages are matched by exact name; refdes-level findings only run for packages whose BOM rows enumerate refdes",
    "panelFit": "axis-aligned grid of whole boards only; no rotation, edge rail, fiducial or panel outline is modelled",
    "cycleTimeEstimate": "analytical estimate over the placement program with a fixed machine model; no acceleration, tape advance or nozzle physics is modelled",
    "feederStationAssignment": "heuristic lane order by mean placement X over the nearest-neighbour program; models only tape-bank travel along X, not slot capacity, tape direction or nozzle constraints",
    "changeoverSequence": "lanes are matched by id and compared by package and exact pitch; the downtime is a fixed time per reel change, not a measured rate",
    "reflowWindow": "metrics and ramp rates come from zone set-points, not measured thermocouple data; the comparison is against caller-supplied limits",
    "feederConsumption": "reels are derived from lane capacity and the per-board feeder allocation; splices, reel changes, scrap and moisture exposure are not modelled",
    "csv": "placement rows only, board sized to fit the coordinates with no outline, height stays null"
  }
}"#;
