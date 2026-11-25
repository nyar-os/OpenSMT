//! Read-only parallel pick-cycle grouping of the placement program.
//!
//! The report runs the same feeder allocation and nearest-neighbour sequence as
//! the `placement` report and packs consecutive placements that share a nozzle
//! into one pick-and-place cycle of at most `heads` picks. The document stores
//! no nozzle field, so the package name stands in for the nozzle identity and
//! the cycle also breaks at a board-side change.
//!
//! The grouping never reorders the program: it only measures how many cycles a
//! head-count would need and how many head slots stay unused. Acceleration,
//! vision time and nozzle physics are not modelled. Nothing is written and the
//! document is unchanged.

use serde::Serialize;

use crate::{
    SmtError,
    model::{Side, SmtDocument},
    place,
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-pick-cycle";

/// Head count used when the caller names none.
pub const DEFAULT_HEADS: u32 = 4;

/// Largest head count the report accepts.
pub const MAX_HEADS: u32 = 64;

/// One pick-and-place cycle of the grouped program.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickCycle {
    /// One-based cycle number in program order.
    pub cycle: u32,
    /// Package the cycle picks; the package is the nozzle identity.
    pub package: String,
    /// Board side of every pick in the cycle.
    pub side: Side,
    /// Picks the cycle carries, never above the requested head count.
    pub picks: u32,
    /// Reference designators in program order.
    pub refdes: Vec<String>,
    /// Head slots the cycle leaves unused, `heads - picks`.
    pub unused_slots: u32,
}

/// Parallel pick-cycle report of one board.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickCycleReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Head count the grouping used.
    pub heads: u32,
    /// Number of placement points on one board.
    pub components: u32,
    /// Number of pick-and-place cycles the head count needs.
    pub cycles: u32,
    /// Total picks; equal to `components`.
    pub picks: u32,
    /// Head slots available across every cycle, `cycles × heads`.
    pub head_slots: u32,
    /// `picks / headSlots`; zero for an empty document.
    pub utilization: f64,
    /// Head slots no pick fills, `headSlots - picks`.
    pub wasted_slots: u32,
    /// Cycles a single-head program needs; equal to `components`.
    pub single_head_cycles: u32,
    /// Cycles saved against a single-head program.
    pub cycles_saved: u32,
    /// Cycles in program order.
    pub entries: Vec<PickCycle>,
}

/// Groups the placement program into cycles of at most `heads` picks.
///
/// Consecutive placements of the same package on the same side share a cycle
/// until the head count is reached; a package change or a side change starts a
/// new cycle. The nearest-neighbour program is never reordered. `heads` must be
/// between 1 and [`MAX_HEADS`], otherwise `pick-cycle/invalid`; anything the
/// allocation or the sequence rejects (a missing or undersized feeder, a broken
/// document) fails with its own code. The document is not modified.
pub fn plan(document: &SmtDocument, heads: u32) -> Result<PickCycleReport, SmtError> {
    if !(1..=MAX_HEADS).contains(&heads) {
        return Err(SmtError::new("pick-cycle/invalid", format!("head count must be between 1 and {MAX_HEADS}, got {heads}")));
    }

    let allocation = place::allocate(document)?;
    let program = place::sequence(document, &allocation)?;

    let capacity = heads as usize;
    let mut entries: Vec<PickCycle> = Vec::new();
    for step in &program.entries {
        let joins = entries
            .last()
            .is_some_and(|cycle| cycle.package == step.package && cycle.side == step.side && cycle.refdes.len() < capacity);
        if joins {
            let cycle = entries.last_mut().expect("a joinable cycle exists");
            cycle.refdes.push(step.refdes.clone());
            cycle.picks = cycle.refdes.len() as u32;
        }
        else {
            entries.push(PickCycle {
                cycle: entries.len() as u32 + 1,
                package: step.package.clone(),
                side: step.side,
                picks: 1,
                refdes: vec![step.refdes.clone()],
                unused_slots: heads.saturating_sub(1),
            });
        }
    }
    for cycle in &mut entries {
        cycle.unused_slots = heads.saturating_sub(cycle.picks);
    }

    let components = program.entries.len() as u32;
    let cycles = entries.len() as u32;
    let head_slots = cycles.saturating_mul(heads);
    let picks = components;
    let wasted_slots = head_slots.saturating_sub(picks);
    let utilization = if head_slots == 0 { 0.0 } else { f64::from(picks) / f64::from(head_slots) };

    Ok(PickCycleReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        heads,
        components,
        cycles,
        picks,
        head_slots,
        utilization,
        wasted_slots,
        single_head_cycles: components,
        cycles_saved: components.saturating_sub(cycles),
        entries,
    })
}
