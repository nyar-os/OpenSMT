//! Dual-lane placement scheduling.
//!
//! The tasks of a dual-lane schedule are the steps of the existing placement
//! program. Each task carries a fixed time: the placement overhead plus the
//! travel from the previous step of the same side, so the task times add up to
//! the single-lane placement estimate. A deterministic greedy pass assigns every
//! task, in program order, to the lane whose accumulated time is smaller; a tie
//! stays on the first lane. Relative order inside a lane is therefore the program
//! order. The report is a load estimate and does not model conveyor transfer,
//! lane hardware or re-sequencing inside a lane.

use serde::Serialize;

use crate::{
    SmtError,
    model::{Component, SmtDocument},
    place::{HEAD_SPEED, PLACE_SECONDS, distance, order_with_travel},
    validate::{Severity, validate},
};

/// Number of parallel lanes a dual-lane schedule uses.
pub const LANES: usize = 2;

/// Difference in accumulated seconds at or below which two lanes count as tied.
const TIE_EPSILON: f64 = 1e-9;

/// One placement task with its fixed estimated time.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DualLaneTask {
    /// Reference designator of the task.
    pub id: String,
    /// Estimated placement time of the task, in seconds.
    pub seconds: f64,
}

/// One lane of a dual-lane schedule.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneSchedule {
    /// Zero-based lane index.
    pub index: u32,
    /// Task identifiers in execution order.
    pub tasks: Vec<String>,
    /// Sum of the lane's task times, in seconds.
    pub seconds: f64,
}

/// Dual-lane schedule report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DualLaneReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// One entry per lane, in index order.
    pub lanes: Vec<LaneSchedule>,
    /// Single-lane program time, in seconds; the sum of every task time.
    pub single_lane_seconds: f64,
    /// Slowest lane's total task time, in seconds.
    pub makespan: f64,
    /// `makespan / singleLaneSeconds`, or zero when every task takes zero seconds.
    pub balance_ratio: f64,
}

/// Schedules the placement tasks of `document` on two parallel lanes.
///
/// The task times come from the same nearest-neighbour program and cycle estimate
/// as [`crate::sequence_placement`]. Structural validation errors are returned;
/// feeder coverage does not block the schedule, exactly like a line balance.
pub fn plan_dual_lane(document: &SmtDocument) -> Result<DualLaneReport, SmtError> {
    let report = validate(document);
    // Dual-lane scheduling does not read feeders, so an incomplete or short
    // setup must not block it. Structural errors still do.
    if let Some(issue) = report
        .issues
        .iter()
        .find(|issue| issue.severity == Severity::Error && issue.code != "feeder/missing" && issue.code != "feeder/capacity")
    {
        return Err(SmtError::new(issue.code.clone(), issue.message.clone()));
    }
    Ok(plan_dual_lane_tasks(&document_tasks(document)))
}

/// Assigns `tasks` to two lanes by cumulative time.
///
/// Tasks are visited in slice order and each one joins the lane with the smaller
/// accumulated time; a tie goes to the first lane. Task times are assumed to be
/// finite and non-negative.
pub fn plan_dual_lane_tasks(tasks: &[DualLaneTask]) -> DualLaneReport {
    let mut assigned: Vec<Vec<usize>> = vec![Vec::new(); LANES];
    let mut seconds = [0.0f64; LANES];
    for (index, task) in tasks.iter().enumerate() {
        let lane = if seconds[0] <= seconds[1] + TIE_EPSILON { 0 } else { 1 };
        seconds[lane] += task.seconds;
        assigned[lane].push(index);
    }

    let single_lane_seconds = seconds.iter().sum();
    let makespan = seconds.iter().copied().fold(0.0, f64::max);
    let balance_ratio = if single_lane_seconds == 0.0 { 0.0 } else { makespan / single_lane_seconds };
    let lanes = assigned
        .into_iter()
        .enumerate()
        .map(|(index, lane)| LaneSchedule {
            index: index as u32,
            tasks: lane.into_iter().map(|task| tasks[task].id.clone()).collect(),
            seconds: seconds[index],
        })
        .collect();

    DualLaneReport { format: "open-smt-dual-lane".to_string(), version: 1, lanes, single_lane_seconds, makespan, balance_ratio }
}

fn document_tasks(document: &SmtDocument) -> Vec<DualLaneTask> {
    let components: Vec<&Component> = document.components.iter().collect();
    let (ordered, _) = order_with_travel(&components);
    let mut tasks = Vec::with_capacity(ordered.len());
    let mut previous: Option<&Component> = None;
    for component in ordered {
        // Every side starts at the machine origin, like the placement program.
        let (from_x, from_y) = match previous {
            Some(previous) if previous.side == component.side => (previous.x, previous.y),
            _ => (0.0, 0.0),
        };
        tasks.push(DualLaneTask {
            id: component.refdes.clone(),
            seconds: PLACE_SECONDS + distance(from_x, from_y, component.x, component.y) / HEAD_SPEED,
        });
        previous = Some(component);
    }
    tasks
}
