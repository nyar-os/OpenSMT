//! OpenSMT: a deterministic SMT assembly planner.
//!
//! This crate implements a deliberately small assembly kernel:
//!
//! * one rectangular board, an optional panel array and a component list;
//! * a feeder setup with package, pitch, capacity, loaded quantity, per-reel
//!   leader/trailer waste, manual scrap and placement-step reel splices;
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
//! * a read-only feeder consumption plan that separates net picks, gross
//!   consumption and leader/trailer/manual scrap;
//! * a read-only feeder replenishment schedule that accounts for splices, scrap
//!   and per-reel leader/trailer waste;
//! * a read-only takt bottleneck heat map by step, package and side;
//! * a read-only stencil aperture report and sheet from the package rules table;
//! * a read-only solder paste volume report estimating run and waste usage from
//!   the stencil apertures, the foil thickness and the build quantity;
//! * a read-only feeder setup drawing with lane slots, load bars and the
//!   component-to-lane mapping;
//! * a read-only pick-and-place sequence drawing with one numbered route per
//!   board side;
//! * a read-only component height and clearance check;
//! * a neutral machine program interchange with a readback check;
//! * CSV program and feeder sheets, plus JSON and Markdown reports.
//!
//! Vendor machine programs are **not** implemented. See [`CAPABILITIES_JSON`].

#![deny(missing_docs)]

mod balance;
mod bom;
mod changeover;
mod clearance;
mod component_list;
mod consumption;
mod csv_import;
mod drawing;
mod dual;
mod error;
mod export;
mod feeder;
mod feeder_layout;
mod first_article;
mod heatmap;
mod import;
mod line_balance;
mod machine_program;
mod model;
mod packages;
mod panel_fit;
mod panel_map;
mod paste;
mod place;
mod reflow;
mod reflow_window;
mod schedule;
mod sequence_drawing;
mod session;
mod setup_sheet;
mod station;
mod stencil;
mod takt;
pub mod validate;

pub use crate::{
    balance::{BalanceReport, MAX_MACHINES, MIN_MACHINES, MachineFeeder, MachineReport, balance, balance_with},
    bom::{BomCheckReport, BomFinding, BomQuantityMismatch, check as bom_check},
    changeover::{ChangeoverAction, ChangeoverKind, ChangeoverReport, DEFAULT_FEEDER_CHANGE_SECONDS, plan as plan_changeover},
    clearance::{
        ClearanceFinding, ClearanceLimits, ClearanceReport, DEFAULT_MAX_HEIGHT_MM, DEFAULT_MIN_GAP_MM, check as clearance_check,
    },
    component_list::{ComponentListReport, ComponentPackage, ListedComponent, report as component_list_report},
    consumption::{ConsumptionReport, FeederConsumption, PackageConsumption, plan as plan_consumption},
    drawing::{
        DEFAULT_COMPONENT_HEIGHT_MM, DEFAULT_COMPONENT_WIDTH_MM, Drawing, DrawingLoss, DrawingSummary, PT_PER_MM,
        SVG_PX_PER_MM, render_pdf, render_svg,
    },
    dual::{
        DualLaneReport, DualLaneStrategy, DualLaneTask, LANES, LaneSchedule, LanesReport, MAX_LANES, MIN_LANES,
        plan_document_lanes, plan_dual_lane, plan_dual_lane_tasks, plan_dual_lane_tasks_with_strategy,
        plan_dual_lane_with_strategy, plan_lanes,
    },
    error::{SmtError, not_implemented},
    export::{feeder_csv, first_article_csv, markdown, program_csv, write_atomic},
    feeder::{FeederLoadingReport, FeederSlot, report as feeder_loading_report},
    feeder_layout::{FeederDrawing, FeederDrawingSummary, render_svg as render_feeder_svg},
    first_article::{
        FirstArticleLane, FirstArticleMaterial, FirstArticleReport, FirstArticleStep, sheet as first_article_sheet,
    },
    heatmap::{
        HeatPackage, HeatSide, HeatStep, HeatmapDrawing, HeatmapDrawingSummary, HeatmapReport, heatmap as takt_heatmap,
        render_svg as render_heatmap_svg,
    },
    import::{ImportLoss, ImportReport},
    line_balance::{
        LineBalanceDrawing, LineBalanceSummary, MACHINES as LINE_BALANCE_MACHINES, render_svg as render_line_balance_svg,
    },
    machine_program::{
        MachineProgram, MachineProgramExport, MachineProgramVerify, ProgramMismatch, build as build_machine_program,
        parse as parse_machine_program, render as render_machine_program, verify as verify_machine_program,
    },
    model::{Board, Component, FORMAT, Feeder, Panel, ReflowProfile, Side, SmtDocument, Splice, UNITS, VERSION, Zone},
    panel_fit::{PanelFitReport, fit as panel_fit},
    panel_map::{
        MappedComponent, PanelInstance, PanelMapDrawing, PanelMapDrawingSummary, PanelMapReport, PanelPlacement,
        map as panel_map, render_svg as render_panel_map_svg,
    },
    paste::{DEFAULT_FOIL_THICKNESS_MM, PasteOptions, PastePackage, PasteReport, csv as paste_csv, report as paste_report},
    place::{
        FeederReport, FeederUsage, HEAD_SPEED, PLACE_SECONDS, PackageUsage, PlacementEntry, PlacementReport,
        allocate as allocate_feeders, sequence as sequence_placement, sequence_with as sequence_placement_with,
    },
    reflow::{
        AMBIENT, ReflowDrawing, ReflowDrawingSummary, ReflowReport, ZoneReport, evaluate as evaluate_reflow,
        render_svg as render_reflow_svg,
    },
    reflow_window::{ReflowWindowLimits, ReflowWindowReport, WindowCheck, WindowZone, evaluate as evaluate_reflow_window},
    schedule::{
        FeederScheduleReport, LaneSplice, ReelChange, ScheduleLane, plan as plan_feeder_schedule, plan_lane,
        report as feeder_schedule_report,
    },
    sequence_drawing::{
        SequenceDrawing, SequenceDrawingSummary, render_pdf as render_sequence_pdf, render_svg as render_sequence_svg,
    },
    session::{ComponentUpdate, DEFAULT_BOARD, DocumentSummary, ExportFormat, ExportReport, FeederUpdate, OpenSmt, Report},
    setup_sheet::{
        PAGE_HEIGHT_MM as SETUP_PAGE_HEIGHT_MM, PAGE_WIDTH_MM as SETUP_PAGE_WIDTH_MM, ROWS_PER_PAGE as SETUP_ROWS_PER_PAGE,
        SetupSheet, SetupSheetSummary, render_pdf as render_setup_pdf, render_svg as render_setup_svg,
    },
    station::{DEFAULT_SLOT_PITCH, Station, StationReport, assign as assign_stations},
    stencil::{
        StencilAperture, StencilDrawing, StencilDrawingSummary, StencilReport, StencilRule, csv as stencil_csv,
        render_pdf as render_stencil_pdf, render_svg as render_stencil_svg, report as stencil_report,
    },
    takt::{TaktOptions, TaktReport, estimate as takt_estimate},
    validate::{Issue, Severity, ValidationReport},
};

/// Stable machine-readable capability matrix of this build.
pub const CAPABILITIES_JSON: &str = r#"{
  "analysis": {
    "placementSequence": "supported",
    "placementOptimization": "supported",
    "feederAllocation": "supported",
    "componentList": "supported",
    "feederLoadingReport": "supported",
    "bomCheck": "supported",
    "panelization": "supported",
    "panelFit": "supported",
    "reflowProfile": "supported",
    "placementProgram": "supported",
    "pasteAperture": "supported",
    "machineProgram": "supported",
    "lineBalance": "supported",
    "dualLane": "supported",
    "multiLane": "supported",
    "cycleTimeEstimate": "supported",
    "feederStationAssignment": "supported",
    "changeoverSequence": "supported",
    "reflowWindow": "supported",
    "feederConsumption": "supported",
    "feederSchedule": "supported",
    "panelMap": "supported",
    "firstArticle": "supported",
    "taktHeatmap": "supported",
    "clearanceCheck": "supported",
    "pasteVolume": "supported"
  },
  "io": {
    "osmt": "supported",
    "programCsv": "supported",
    "feedersCsv": "supported",
    "firstArticleCsv": "supported",
    "machineProgramCsv": "supported",
    "json": "supported",
    "md": "supported",
    "svg": "supported",
    "pdf": "supported",
    "feederSvg": "supported",
    "pasteCsv": "supported",
    "sequenceSvg": "supported",
    "sequencePdf": "supported",
    "reflowSvg": "supported",
    "panelMapSvg": "supported",
    "heatmapSvg": "supported",
    "balanceSvg": "supported",
    "setupSheetSvg": "supported",
    "setupSheetPdf": "supported",
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
    "componentList": "reads the same feeder allocation as the feeders report; the feeder column is null for a component with no loaded lane and no validation runs",
    "feederLoadingReport": "slot numbers are suggestions only; no machine slot, tape direction or pitch constraint is modelled",
    "bomCheck": "packages are matched by exact name; refdes-level findings only run for packages whose BOM rows enumerate refdes",
    "panelFit": "axis-aligned grid of whole boards only; no rotation, edge rail, fiducial or panel outline is modelled",
    "cycleTimeEstimate": "analytical estimate over the placement program with a fixed machine model; no acceleration, tape advance or nozzle physics is modelled",
    "feederStationAssignment": "heuristic lane order by mean placement X over the nearest-neighbour program; models only tape-bank travel along X, not slot capacity, tape direction or nozzle constraints",
    "changeoverSequence": "lanes are matched by id and compared by package and exact pitch; the downtime is a fixed time per reel change, not a measured rate",
    "reflowWindow": "metrics and ramp rates come from zone set-points, not measured thermocouple data; the comparison is against caller-supplied limits",
    "feederConsumption": "separates net picks, gross consumption and scrap; reels and splice stock follow lane capacity, but partial reels and moisture exposure are not modelled",
    "feederSchedule": "splices and per-reel leader/trailer or manual scrap are applied at board granularity; partial reels, moisture exposure and changeover time are not modelled",
    "panelMap": "row-major instances of the stored panel array; no board rotation, edge rail, fiducial, tooling area or panel outline is modelled",
    "firstArticle": "repeats the placement program with the allocated lane pitch and a fixed four-item checklist; it records no inspection result and models no component polarity",
    "pasteAperture": "apertures come from a fixed package rules table keyed by package name, not from footprint data; the reduction is one area factor per family, QFN thermal pads are not modelled and the sheet draws one board, not a panel",
    "pasteVolume": "paste volumes come from the generated stencil apertures times the foil thickness; the sheet models no squeegee transfer loss, board-to-board variation or reel waste",
    "taktHeatmap": "attributes the analytical step time of the default machine model to each step, package and side; it models no acceleration, tape advance, vision or nozzle physics",
    "clearanceCheck": "body outlines come from the fixed package rules table, not footprint data; same-side bodies are compared as rotated bounding boxes, findings are warnings only and height checks apply only to components that store a height",
    "machineProgram": "neutral CSV interchange of the placement program with a readback check; it is not a Fuji, Yamaha, Juki or ASM dialect and takes nozzle ids only from a caller-supplied package map",
    "svg": "one millimetre placement sheet at 8 px/mm; component bodies come from the package rules table with a 2.0 x 1.2 mm generic fallback, labels are ASCII-only, collision-free and drawn on board 1 only",
    "pdf": "one A4 landscape page fitted and centred from the same millimetre layout with Helvetica; the same rules-table body sizing, ASCII-only collision-free labels and board-1 labels apply",
    "feederSvg": "the feeder setup drawing is an SVG tape-bank sheet with one slot per lane in feeder id order, a load bar for the loaded share of the capacity and the assigned reference designators; it reads the same allocation as the feeders report and models no machine slot, tape direction or pitch constraint",
    "sequenceSvg": "the pick-and-place sequence drawing is a millimetre SVG sheet at 8 px/mm with one numbered route per board side over the default nearest-neighbour program; it models no acceleration, tape advance, vision or nozzle physics",
    "sequencePdf": "the pick-and-place sequence drawing is a one-page A4 landscape PDF fitted from the same millimetre layout with Helvetica; it models no acceleration, tape advance, vision or nozzle physics",
    "reflowSvg": "the reflow drawing is a millimetre SVG time-temperature plot of the linear-ramp zone model with the liquidus and peak annotated; it plots set-points, not measured thermocouple data, and models no thermal mass or soak dynamics",
    "panelMapSvg": "the panel map drawing is a millimetre SVG of the stored row-major panel array with board numbers, side-coloured component dots and a panel origin cross; it models no board rotation, edge rail, fiducial or panel outline",
    "heatmapSvg": "the takt heat map drawing is a millimetre SVG of one board with a heat dot per program step plus a package Pareto column; it colours the analytical step time of the default machine model and models no acceleration, tape advance, vision or nozzle physics",
    "balanceSvg": "the line balance drawing is a millimetre SVG bar chart of one station per machine with the slowest station highlighted and the average as a dashed target line; export formats carry no options, so the sheet always balances across three machines and it models no changeover, staffing or machine-specific accelerations",
    "setupSheet": "the setup table lists slot, lane, part number, nozzle, tape width, quantity and assigned reference designators; the document has no nozzle or tape-width field, so both are derived from the fixed package rules (family and body length) and no tape direction, feeder pitch limit or machine slot is modelled; long tables continue on further pages",
    "csv": "placement rows only, board sized to fit the coordinates with no outline, height stays null"
  }
}"#;
