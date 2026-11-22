//! Multi-lane placement scheduling.
//!
//! The tasks of a schedule are the steps of the existing placement program.
//! Each task carries a fixed time: the placement overhead plus the travel from
//! the previous step of the same side, so the task times add up to the
//! single-lane placement estimate. Two deterministic strategies assign the
//! tasks: `greedy` visits them in program order, `lpt` visits them by
//! decreasing task time (ties by program index). Both give every task to the
//! lane with the smaller accumulated time; a tie stays on the lowest lane
//! index. Relative order inside a lane is always the program order. An optional
//! swap pass exchanges one task of each lane pair while that lowers the slower
//! lane. `plan_dual_lane*` is the two-lane special case and keeps its report
//! shape. The report is a load estimate and does not model conveyor transfer,
//! lane hardware or re-sequencing inside a lane.

use std::str::FromStr;

use serde::Serialize;

use crate::{
    SmtError,
    model::{Component, SmtDocument},
    place::{HEAD_SPEED, PLACE_SECONDS, distance, order_with_travel},
    validate::{Severity, validate},
};

/// Number of parallel lanes a dual-lane schedule uses.
pub const LANES: usize = 2;

/// Smallest number of lanes a multi-lane schedule accepts.
pub const MIN_LANES: usize = 2;

/// Largest number of lanes a multi-lane schedule accepts.
pub const MAX_LANES: usize = 16;

/// Difference in accumulated seconds at or below which two lanes count as tied.
const TIE_EPSILON: f64 = 1e-9;

/// Highest number of accepted swaps a single-improvement pass may perform.
const MAX_SWAP_ROUNDS: usize = 64;

/// Assignment strategy of a dual-lane schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DualLaneStrategy {
    /// Visit tasks in program order and give each to the lighter lane.
    Greedy,
    /// Visit tasks by decreasing time (ties by program index) and give each to
    /// the lighter lane.
    Lpt,
}

impl DualLaneStrategy {
    /// Returns the short identifier used by the CLI and by reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Greedy => "greedy",
            Self::Lpt => "lpt",
        }
    }
}

impl FromStr for DualLaneStrategy {
    type Err = SmtError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "greedy" => Ok(Self::Greedy),
            "lpt" => Ok(Self::Lpt),
            other => Err(SmtError::new(
                "schedule/invalid-strategy",
                format!("unknown dual-lane strategy `{other}`; only greedy or lpt is supported"),
            )),
        }
    }
}

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
    /// Assignment strategy that produced the report.
    pub strategy: DualLaneStrategy,
    /// One entry per lane, in index order.
    pub lanes: Vec<LaneSchedule>,
    /// Single-lane program time, in seconds; the sum of every task time.
    pub single_lane_seconds: f64,
    /// Slowest lane's total task time, in seconds.
    pub makespan: f64,
    /// `makespan / singleLaneSeconds`, or zero when every task takes zero seconds.
    pub balance_ratio: f64,
}

/// Multi-lane schedule report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanesReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Assignment strategy that produced the report.
    pub strategy: DualLaneStrategy,
    /// Number of lanes the schedule uses, between 2 and 16.
    pub lane_count: u32,
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
    plan_dual_lane_with_strategy(document, DualLaneStrategy::Greedy, false)
}

/// Schedules the placement tasks of `document` with an explicit strategy and an
/// optional single-swap improvement pass.
///
/// Like [`plan_dual_lane`], only structural errors block the schedule.
pub fn plan_dual_lane_with_strategy(
    document: &SmtDocument,
    strategy: DualLaneStrategy,
    improve: bool,
) -> Result<DualLaneReport, SmtError> {
    check_structure(document)?;
    Ok(dual_report(schedule_lanes(&document_tasks(document), LANES, strategy, improve)))
}

/// Schedules the placement tasks of `document` on `lane_count` parallel lanes.
///
/// The lane count must be between [`MIN_LANES`] and [`MAX_LANES`]; any other
/// value reports `schedule/invalid-lanes`. Task times come from the same
/// nearest-neighbour program and cycle estimate as [`crate::sequence_placement`].
/// Structural validation errors are returned; feeder coverage does not block the
/// schedule, exactly like a line balance.
pub fn plan_document_lanes(
    document: &SmtDocument,
    lane_count: usize,
    strategy: DualLaneStrategy,
    improve: bool,
) -> Result<LanesReport, SmtError> {
    check_lane_count(lane_count)?;
    check_structure(document)?;
    Ok(schedule_lanes(&document_tasks(document), lane_count, strategy, improve))
}

/// Assigns `tasks` to `lane_count` lanes with an explicit strategy and an
/// optional pairwise-swap improvement pass.
///
/// The lane count must be between [`MIN_LANES`] and [`MAX_LANES`]; any other
/// value reports `schedule/invalid-lanes`. The greedy strategy visits tasks in
/// slice order; the LPT strategy visits them by decreasing task time, ties
/// keeping the slice order. Both give each task to the lane with the smaller
/// accumulated time, a tie going to the lowest lane index. With `improve`, one
/// task of each lane pair is exchanged while that lowers the slowest lane, up to
/// a fixed number of rounds. Lane contents are always reported in slice order.
/// Task times are assumed to be finite and non-negative.
pub fn plan_lanes(
    tasks: &[DualLaneTask],
    lane_count: usize,
    strategy: DualLaneStrategy,
    improve: bool,
) -> Result<LanesReport, SmtError> {
    check_lane_count(lane_count)?;
    Ok(schedule_lanes(tasks, lane_count, strategy, improve))
}

/// Assigns `tasks` to two lanes by cumulative time with the greedy strategy.
///
/// Tasks are visited in slice order and each one joins the lane with the smaller
/// accumulated time; a tie goes to the first lane. Task times are assumed to be
/// finite and non-negative.
pub fn plan_dual_lane_tasks(tasks: &[DualLaneTask]) -> DualLaneReport {
    plan_dual_lane_tasks_with_strategy(tasks, DualLaneStrategy::Greedy, false)
}

/// Assigns `tasks` to two lanes with an explicit strategy and an optional
/// single-swap improvement pass.
///
/// The greedy strategy visits tasks in slice order; the LPT strategy visits them
/// by decreasing task time, ties keeping the slice order. Both give each task to
/// the lane with the smaller accumulated time, a tie going to the first lane.
/// With `improve`, one task of each lane is exchanged while that lowers the
/// slower lane, up to a fixed number of rounds. Lane contents are always
/// reported in slice order. Task times are assumed to be finite and
/// non-negative.
pub fn plan_dual_lane_tasks_with_strategy(tasks: &[DualLaneTask], strategy: DualLaneStrategy, improve: bool) -> DualLaneReport {
    dual_report(schedule_lanes(tasks, LANES, strategy, improve))
}

fn dual_report(report: LanesReport) -> DualLaneReport {
    let LanesReport { strategy, lanes, single_lane_seconds, makespan, balance_ratio, .. } = report;
    DualLaneReport {
        format: "open-smt-dual-lane".to_string(),
        version: 1,
        strategy,
        lanes,
        single_lane_seconds,
        makespan,
        balance_ratio,
    }
}

fn check_structure(document: &SmtDocument) -> Result<(), SmtError> {
    let report = validate(document);
    // Lane scheduling does not read feeders, so an incomplete or short setup
    // must not block it. Structural errors still do.
    if let Some(issue) = report
        .issues
        .iter()
        .find(|issue| issue.severity == Severity::Error && issue.code != "feeder/missing" && issue.code != "feeder/capacity")
    {
        return Err(SmtError::new(issue.code.clone(), issue.message.clone()));
    }
    Ok(())
}

fn check_lane_count(lane_count: usize) -> Result<(), SmtError> {
    if (MIN_LANES..=MAX_LANES).contains(&lane_count) {
        Ok(())
    } else {
        Err(SmtError::new(
            "schedule/invalid-lanes",
            format!("lane count {lane_count} must be between {MIN_LANES} and {MAX_LANES}"),
        ))
    }
}

fn schedule_lanes(tasks: &[DualLaneTask], lane_count: usize, strategy: DualLaneStrategy, improve: bool) -> LanesReport {
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); lane_count];
    let mut seconds = vec![0.0f64; lane_count];

    match strategy {
        DualLaneStrategy::Greedy => {
            for (index, task) in tasks.iter().enumerate() {
                let lane = lighter_lane(&seconds);
                seconds[lane] += task.seconds;
                members[lane].push(index);
            }
        }
        DualLaneStrategy::Lpt => {
            let mut order: Vec<usize> = (0..tasks.len()).collect();
            order.sort_by(|&left, &right| tasks[right].seconds.total_cmp(&tasks[left].seconds).then(left.cmp(&right)));
            for index in order {
                let lane = lighter_lane(&seconds);
                seconds[lane] += tasks[index].seconds;
                members[lane].push(index);
            }
        }
    }

    if improve {
        improve_swaps(tasks, &mut members, &mut seconds);
    }

    let single_lane_seconds = seconds.iter().sum();
    let makespan = seconds.iter().copied().fold(0.0, f64::max);
    let balance_ratio = if single_lane_seconds == 0.0 { 0.0 } else { makespan / single_lane_seconds };
    let lanes = members
        .into_iter()
        .enumerate()
        .map(|(index, mut lane)| {
            // LPT and the swap pass append out of order; the report always
            // keeps the program order inside a lane.
            lane.sort_unstable();
            LaneSchedule {
                index: index as u32,
                tasks: lane.into_iter().map(|task| tasks[task].id.clone()).collect(),
                seconds: seconds[index],
            }
        })
        .collect();

    LanesReport {
        format: "open-smt-lanes".to_string(),
        version: 1,
        strategy,
        lane_count: lane_count as u32,
        lanes,
        single_lane_seconds,
        makespan,
        balance_ratio,
    }
}

fn lighter_lane(seconds: &[f64]) -> usize {
    let mut lane = 0;
    for candidate in 1..seconds.len() {
        if seconds[candidate] < seconds[lane] - TIE_EPSILON {
            lane = candidate;
        }
    }
    lane
}

fn improve_swaps(tasks: &[DualLaneTask], members: &mut [Vec<usize>], seconds: &mut [f64]) {
    for _ in 0..MAX_SWAP_ROUNDS {
        let current = seconds.iter().copied().fold(0.0, f64::max);
        let mut swap = None;
        'search: for (left_lane, left_members) in members.iter().enumerate() {
            for (right_lane, right_members) in members.iter().enumerate().skip(left_lane + 1) {
                for (left, &left_task) in left_members.iter().enumerate() {
                    for (right, &right_task) in right_members.iter().enumerate() {
                        let left_seconds = tasks[left_task].seconds;
                        let right_seconds = tasks[right_task].seconds;
                        let candidate_left = seconds[left_lane] - left_seconds + right_seconds;
                        let candidate_right = seconds[right_lane] - right_seconds + left_seconds;
                        let mut candidate = candidate_left.max(candidate_right);
                        for (lane, lane_seconds) in seconds.iter().enumerate() {
                            if lane != left_lane && lane != right_lane {
                                candidate = candidate.max(*lane_seconds);
                            }
                        }
                        if candidate < current - TIE_EPSILON {
                            swap = Some((left_lane, right_lane, left, right, candidate_left, candidate_right));
                            break 'search;
                        }
                    }
                }
            }
        }
        let Some((left_lane, right_lane, left, right, candidate_left, candidate_right)) = swap else {
            break;
        };
        let (before, after) = members.split_at_mut(right_lane);
        std::mem::swap(&mut before[left_lane][left], &mut after[0][right]);
        seconds[left_lane] = candidate_left;
        seconds[right_lane] = candidate_right;
    }
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
