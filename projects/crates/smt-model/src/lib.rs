//! OpenSMT: a deterministic SMT assembly planner.
//!
//! This crate implements a deliberately small assembly kernel:
//!
//! * one rectangular board, an optional panel array and a component list;
//! * a feeder setup with package, pitch, capacity and loaded quantity;
//! * a deterministic placement program: feeders are allocated in id order and
//!   the head visits the nearest remaining component, top side first;
//! * a multi-machine line balance that assigns components in reference
//!   designator order to the machine with the smallest estimated completion
//!   time;
//! * a reflow profile evaluator that integrates time above liquidus over linear
//!   zone ramps;
//! * CSV program and feeder sheets, plus JSON and Markdown reports.
//!
//! Paste-aperture generation, vendor machine programs and dual-lane scheduling
//! are **not** implemented. See [`CAPABILITIES_JSON`].

#![deny(missing_docs)]

mod balance;
mod error;
mod export;
mod model;
mod place;
mod reflow;
mod session;
pub mod validate;

pub use crate::{
    balance::{BalanceReport, MAX_MACHINES, MIN_MACHINES, MachineFeeder, MachineReport, balance},
    error::{SmtError, not_implemented},
    export::{feeder_csv, markdown, program_csv, write_atomic},
    model::{Board, Component, FORMAT, Feeder, Panel, ReflowProfile, Side, SmtDocument, UNITS, VERSION, Zone},
    place::{
        FeederReport, FeederUsage, HEAD_SPEED, PLACE_SECONDS, PackageUsage, PlacementEntry, PlacementReport,
        allocate as allocate_feeders, sequence as sequence_placement,
    },
    reflow::{AMBIENT, ReflowReport, ZoneReport, evaluate as evaluate_reflow},
    session::{DEFAULT_BOARD, DocumentSummary, ExportFormat, ExportReport, OpenSmt, Report},
    validate::{Issue, Severity, ValidationReport},
};

/// Stable machine-readable capability matrix of this build.
pub const CAPABILITIES_JSON: &str = r#"{
  "analysis": {
    "placementSequence": "supported",
    "feederAllocation": "supported",
    "panelization": "supported",
    "reflowProfile": "supported",
    "placementProgram": "supported",
    "pasteAperture": "unsupported",
    "machineProgram": "unsupported",
    "lineBalance": "supported",
    "dualLane": "unsupported"
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
  }
}"#;
