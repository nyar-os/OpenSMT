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
//! * CSV program and feeder sheets, plus JSON and Markdown reports.
//!
//! Paste-aperture generation and vendor machine programs are **not**
//! implemented. See [`CAPABILITIES_JSON`].

#![deny(missing_docs)]

mod balance;
mod dual;
mod error;
mod export;
mod import;
mod model;
mod place;
mod reflow;
mod session;
pub mod validate;

pub use crate::{
    balance::{BalanceReport, MAX_MACHINES, MIN_MACHINES, MachineFeeder, MachineReport, balance, balance_with},
    dual::{
        DualLaneReport, DualLaneStrategy, DualLaneTask, LANES, LaneSchedule, LanesReport, MAX_LANES, MIN_LANES,
        plan_document_lanes, plan_dual_lane, plan_dual_lane_tasks, plan_dual_lane_tasks_with_strategy,
        plan_dual_lane_with_strategy, plan_lanes,
    },
    error::{SmtError, not_implemented},
    export::{feeder_csv, markdown, program_csv, write_atomic},
    import::{ImportLoss, ImportReport},
    model::{Board, Component, FORMAT, Feeder, Panel, ReflowProfile, Side, SmtDocument, UNITS, VERSION, Zone},
    place::{
        FeederReport, FeederUsage, HEAD_SPEED, PLACE_SECONDS, PackageUsage, PlacementEntry, PlacementReport,
        allocate as allocate_feeders, sequence as sequence_placement, sequence_with as sequence_placement_with,
    },
    reflow::{AMBIENT, ReflowReport, ZoneReport, evaluate as evaluate_reflow},
    session::{DEFAULT_BOARD, DocumentSummary, ExportFormat, ExportReport, OpenSmt, Report},
    validate::{Issue, Severity, ValidationReport},
};

/// Stable machine-readable capability matrix of this build.
pub const CAPABILITIES_JSON: &str = r#"{
  "analysis": {
    "placementSequence": "supported",
    "placementOptimization": "supported",
    "feederAllocation": "supported",
    "panelization": "supported",
    "reflowProfile": "supported",
    "placementProgram": "supported",
    "pasteAperture": "unsupported",
    "machineProgram": "unsupported",
    "lineBalance": "supported",
    "dualLane": "supported",
    "multiLane": "supported"
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
    "kicad_pcb": "supported"
  },
  "limitations": {
    "dualLane": "greedy or LPT heuristic with an optional single-swap pass, not an optimal schedule",
    "multiLane": "greedy or LPT heuristic on 2 to 16 lanes with an optional pairwise-swap pass, not an optimal schedule"
  }
}"#;
