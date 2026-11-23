//! Read-only component list report.
//!
//! The report lists every placed component in document order with its package,
//! side, coordinates, rotation and height, and the feeder lane the current
//! allocation assigns to it. Packages are grouped in the order they first appear
//! and keep their reference designators in document order. The feeder column
//! comes from the same allocation the `feeders` report uses, so a component
//! without a loaded lane reports `null` and is counted in `unassignedCount`.
//! Nothing is written and the document is unchanged.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    model::{Component, Side, SmtDocument},
    place,
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-component-list";

/// One package grouping of the component list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPackage {
    /// Package identifier.
    pub package: String,
    /// Number of components of this package.
    pub count: u32,
    /// Reference designators in document order.
    pub refdes: Vec<String>,
}

/// One placed component with its allocated feeder.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedComponent {
    /// Reference designator.
    pub refdes: String,
    /// Package identifier.
    pub package: String,
    /// Board side.
    pub side: Side,
    /// Placement X coordinate.
    pub x: f64,
    /// Placement Y coordinate.
    pub y: f64,
    /// Rotation in degrees.
    pub rotation: f64,
    /// Component height; omitted when unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    /// Feeder that supplies the component; `null` when unallocated.
    pub feeder: Option<String>,
}

/// Read-only component list report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentListReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Number of placed components.
    pub component_count: u32,
    /// Components the current allocation leaves without a feeder.
    pub unassigned_count: u32,
    /// One entry per package, in first-seen order.
    pub packages: Vec<ComponentPackage>,
    /// Components in document order.
    pub components: Vec<ListedComponent>,
}

/// Lists every component with its package grouping and allocated feeder.
///
/// Components keep document order and packages keep the order in which they
/// first appear. Each component is allocated with [`place`]'s lane rule, the same
/// one the `feeders` report applies, so a component without a loaded lane of its
/// package reports `null` and is counted in `unassignedCount`. No validation
/// runs: the report is meant to be read before a feeder setup exists, and it
/// never writes.
pub fn report(document: &SmtDocument) -> ComponentListReport {
    let components: Vec<&Component> = document.components.iter().collect();
    let (lanes, unassigned) = place::allocate_lanes(&document.feeders, &components);
    let mut feeder_of: BTreeMap<&str, &str> = BTreeMap::new();
    for lane in &lanes {
        for refdes in &lane.refdes {
            feeder_of.insert(refdes.as_str(), lane.id.as_str());
        }
    }

    let mut package_index: BTreeMap<&str, usize> = BTreeMap::new();
    let mut packages: Vec<ComponentPackage> = Vec::new();
    for component in &document.components {
        match package_index.get(component.package.as_str()) {
            Some(&position) => {
                let entry = &mut packages[position];
                entry.count += 1;
                entry.refdes.push(component.refdes.clone());
            }
            None => {
                package_index.insert(component.package.as_str(), packages.len());
                packages.push(ComponentPackage {
                    package: component.package.clone(),
                    count: 1,
                    refdes: vec![component.refdes.clone()],
                });
            }
        }
    }

    let rows: Vec<ListedComponent> = document
        .components
        .iter()
        .map(|component| ListedComponent {
            refdes: component.refdes.clone(),
            package: component.package.clone(),
            side: component.side,
            x: component.x,
            y: component.y,
            rotation: component.rotation,
            height: component.height,
            feeder: feeder_of.get(component.refdes.as_str()).map(|id| (*id).to_string()),
        })
        .collect();

    ComponentListReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        component_count: document.components.len() as u32,
        unassigned_count: unassigned.len() as u32,
        packages,
        components: rows,
    }
}
