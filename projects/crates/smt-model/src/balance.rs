//! Multi-machine line balancing.
//!
//! Components are sorted by reference designator and assigned one at a time to
//! the machine with the smallest estimated completion time; ties go to the
//! lowest machine index. Every machine owns a full feeder copy and its own
//! origin, so its program is exactly the single-machine program over the
//! components it received. Optional per-machine 2-opt only resequences a
//! machine's own program; assignments and component counts do not change.

use serde::Serialize;

use crate::{
    SmtError,
    model::{Component, SmtDocument},
    place::{allocate_lanes_by_capacity, estimated_seconds, order_with_travel, order_with_travel_optimized},
    validate::{Severity, validate},
};

/// Lowest machine count a balance accepts.
pub const MIN_MACHINES: u32 = 1;

/// Highest machine count a balance accepts.
pub const MAX_MACHINES: u32 = 64;

/// One feeder copy on a machine.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineFeeder {
    /// Feeder identifier.
    pub id: String,
    /// Package supplied by the feeder.
    pub package: String,
    /// Components of this machine assigned to the lane.
    pub assigned: u32,
    /// Components loaded on the lane.
    pub loaded: u32,
    /// `true` when the loaded quantity covers `assigned`.
    pub ok: bool,
}

/// One machine of a line balance.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineReport {
    /// Zero-based machine index.
    pub index: u32,
    /// Reference designators in this machine's execution order.
    pub components: Vec<String>,
    /// Head travel of this machine, in millimetres.
    pub travel: f64,
    /// Estimated placement time of this machine, in seconds.
    pub seconds: f64,
    /// One entry per feeder, in id order.
    pub feeders: Vec<MachineFeeder>,
}

/// Line balance report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// One entry per machine, in index order.
    pub machines: Vec<MachineReport>,
    /// Sum of every machine's estimated placement time.
    pub total_seconds: f64,
    /// Slowest machine's estimated placement time.
    pub max_seconds: f64,
    /// Fastest machine's estimated placement time.
    pub min_seconds: f64,
    /// `(max − min) / max`, or zero when every machine takes zero seconds.
    pub imbalance: f64,
    /// `true` when every machine's feeder copies cover their assignments.
    pub ok: bool,
}

/// Balances the document across `machine_count` machines.
///
/// Feeder coverage is reported, not enforced: an incomplete feeder setup still
/// produces a report whose [`BalanceReport::ok`] is `false`.
pub fn balance(document: &SmtDocument, machine_count: u32) -> Result<BalanceReport, SmtError> {
    balance_with(document, machine_count, false)
}

/// Balances the document, optionally improving every machine's program with
/// 2-opt.
///
/// Assignments stay those of [`balance`], so component counts and the total are
/// conserved; only each machine's own travel is improved.
pub fn balance_with(document: &SmtDocument, machine_count: u32, optimize: bool) -> Result<BalanceReport, SmtError> {
    if !(MIN_MACHINES..=MAX_MACHINES).contains(&machine_count) {
        return Err(SmtError::new(
            "balance/invalid-machines",
            format!("machine count must be between {MIN_MACHINES} and {MAX_MACHINES}, got {machine_count}"),
        ));
    }

    let report = validate(document);
    // Feeder coverage is part of the balance itself, so a missing or undersized
    // lane must not block the report. Structural errors still do.
    if let Some(issue) = report
        .issues
        .iter()
        .find(|issue| issue.severity == Severity::Error && issue.code != "feeder/missing" && issue.code != "feeder/capacity")
    {
        return Err(SmtError::new(issue.code.clone(), issue.message.clone()));
    }

    let mut order: Vec<&Component> = document.components.iter().collect();
    order.sort_by(|left, right| left.refdes.cmp(&right.refdes));

    let mut assignments: Vec<Vec<&Component>> = vec![Vec::new(); machine_count as usize];
    for component in order {
        let mut best = 0;
        let mut best_seconds = candidate_seconds(&assignments[0], component);
        for (index, assigned) in assignments.iter().enumerate().skip(1) {
            let candidate = candidate_seconds(assigned, component);
            if candidate < best_seconds - 1e-9 {
                best = index;
                best_seconds = candidate;
            }
        }
        assignments[best].push(component);
    }

    let mut machines = Vec::with_capacity(assignments.len());
    let mut coverage_ok = true;
    for (index, assigned) in assignments.iter().enumerate() {
        let (ordered, travel, _) = order_with_travel_optimized(assigned, optimize);
        let (lanes, unassigned) = allocate_lanes_by_capacity(&document.feeders, assigned);
        let feeders: Vec<MachineFeeder> = lanes
            .into_iter()
            .map(|lane| MachineFeeder {
                ok: lane.assigned <= lane.loaded,
                id: lane.id,
                package: lane.package,
                assigned: lane.assigned,
                loaded: lane.loaded,
            })
            .collect();
        coverage_ok &= unassigned.is_empty() && feeders.iter().all(|feeder| feeder.ok);
        machines.push(MachineReport {
            index: index as u32,
            components: ordered.iter().map(|component| component.refdes.clone()).collect(),
            travel,
            seconds: estimated_seconds(ordered.len(), travel),
            feeders,
        });
    }

    let total_seconds = machines.iter().map(|machine| machine.seconds).sum();
    let max_seconds = machines.iter().map(|machine| machine.seconds).fold(f64::MIN, f64::max);
    let min_seconds = machines.iter().map(|machine| machine.seconds).fold(f64::MAX, f64::min);
    let imbalance = if max_seconds == 0.0 { 0.0 } else { (max_seconds - min_seconds) / max_seconds };

    Ok(BalanceReport {
        format: "open-smt-balance".to_string(),
        version: 1,
        machines,
        total_seconds,
        max_seconds,
        min_seconds,
        imbalance,
        ok: coverage_ok,
    })
}

fn candidate_seconds(assigned: &[&Component], extra: &Component) -> f64 {
    let components: Vec<&Component> = assigned.iter().copied().chain(std::iter::once(extra)).collect();
    let (_, travel) = order_with_travel(&components);
    estimated_seconds(components.len(), travel)
}
