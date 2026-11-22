//! Read-only feeder loading report.
//!
//! The report aggregates the placement list by package and numbers one
//! suggested tape slot per package. Slot numbers follow package order and are
//! suggestions only: they are neither a physical machine slot map nor a feeder
//! setup.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::model::SmtDocument;

/// Report format identifier.
pub const FORMAT: &str = "open-smt-feeder-report";

/// Suggested tape slot of one package.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeederSlot {
    /// Suggested slot number, starting at 1 in package order.
    pub slot: u32,
    /// Package identifier.
    pub package: String,
    /// Number of placements of this package.
    pub count: u32,
    /// Reference designators in document order.
    pub refdes: Vec<String>,
    /// Distinct board sides, sorted.
    pub sides: Vec<String>,
}

/// Read-only feeder loading report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeederLoadingReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Number of placements across every package.
    pub total_placements: u32,
    /// Number of distinct packages.
    pub unique_packages: u32,
    /// One suggested slot per package, in package order.
    pub slots: Vec<FeederSlot>,
}

/// Aggregates the placements by package and suggests one tape slot per package.
///
/// Packages are visited in ascending name order and receive slots `1..n`. The
/// reference designators of a slot keep document order and its sides are
/// deduplicated and sorted. No validation runs: the report is meant to be read
/// before a feeder setup exists, and the slot numbers are suggestions only.
pub fn report(document: &SmtDocument) -> FeederLoadingReport {
    let mut packages: BTreeMap<&str, (u32, Vec<String>, BTreeSet<&str>)> = BTreeMap::new();
    for component in &document.components {
        let entry = packages.entry(component.package.as_str()).or_default();
        entry.0 += 1;
        entry.1.push(component.refdes.clone());
        entry.2.insert(component.side.as_str());
    }
    let slots: Vec<FeederSlot> = packages
        .into_iter()
        .enumerate()
        .map(|(index, (package, (count, refdes, sides)))| FeederSlot {
            slot: index as u32 + 1,
            package: package.to_string(),
            count,
            refdes,
            sides: sides.into_iter().map(str::to_string).collect(),
        })
        .collect();
    FeederLoadingReport {
        format: FORMAT.to_string(),
        version: 1,
        total_placements: document.components.len() as u32,
        unique_packages: slots.len() as u32,
        slots,
    }
}
