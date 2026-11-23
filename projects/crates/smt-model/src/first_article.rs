//! Read-only first-article inspection sheet.
//!
//! The sheet runs the same feeder allocation and placement sequence as the
//! placement program and turns them into an inspection record: one row per
//! program step with the coordinates, rotation, lane and tape pitch to verify,
//! plus one material block per package with the lanes that supply it. The
//! checklist names the four generic checks the sheet supports. The report is
//! deterministic, records no inspection result and never writes the document.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    SmtError,
    model::{Board, Panel, Side, SmtDocument},
    place,
    validate::package_counts,
};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-first-article";

/// Generic checks the sheet tells an inspector to perform per step.
pub const CHECKS: [&str; 4] = ["placement", "orientation", "package", "feeder"];

/// One lane that supplies a package on the sheet.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstArticleLane {
    /// Feeder identifier.
    pub id: String,
    /// Tape pitch in millimetres.
    pub pitch: f64,
    /// Components loaded on the lane.
    pub loaded: u32,
    /// Maximum lane capacity.
    pub capacity: u32,
    /// Components the allocation assigns to the lane.
    pub assigned: u32,
}

/// Material block of one package.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstArticleMaterial {
    /// Package identifier.
    pub package: String,
    /// Components of this package on one board.
    pub components: u32,
    /// Distinct board sides, `top` before `bottom`.
    pub sides: Vec<Side>,
    /// Lanes that supply the package, in id order.
    pub lanes: Vec<FirstArticleLane>,
}

/// One row of the inspection sheet.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstArticleStep {
    /// One-based row number in program order.
    pub step: u32,
    /// Reference designator.
    pub refdes: String,
    /// Package identifier.
    pub package: String,
    /// Board side.
    pub side: Side,
    /// Placement X coordinate on one board.
    pub x: f64,
    /// Placement Y coordinate on one board.
    pub y: f64,
    /// Rotation in degrees.
    pub rotation: f64,
    /// Feeder lane that supplies the component.
    pub feeder: String,
    /// Tape pitch of that lane, in millimetres.
    pub pitch: f64,
}

/// First-article inspection sheet.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstArticleReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Document revision the sheet was produced from.
    pub revision: u32,
    /// Board geometry.
    pub board: Board,
    /// Panel array.
    pub panel: Panel,
    /// Number of boards on the panel.
    pub board_count: u32,
    /// Panel width including column gaps.
    pub panel_width: f64,
    /// Panel depth including row gaps.
    pub panel_depth: f64,
    /// Components on one board.
    pub components: u32,
    /// Components on the top side.
    pub top_components: u32,
    /// Components on the bottom side.
    pub bottom_components: u32,
    /// Distinct packages.
    pub packages: u32,
    /// Declared feeder lanes.
    pub feeders: u32,
    /// Generic checks the sheet supports, in fixed order.
    pub checklist: Vec<String>,
    /// One material block per package, in package order.
    pub materials: Vec<FirstArticleMaterial>,
    /// One inspection row per placement step, in program order.
    pub steps: Vec<FirstArticleStep>,
}

/// Builds the first-article inspection sheet of the open document.
///
/// The allocation and the sequence run exactly like the placement program, so a
/// package without a lane reports `feeder/missing` and an undersized setup
/// reports `feeder/capacity`. Steps keep program order; materials keep package
/// order and their lanes keep id order. The document is not modified.
pub fn sheet(document: &SmtDocument) -> Result<FirstArticleReport, SmtError> {
    let allocation = place::allocate(document)?;
    let placement = place::sequence(document, &allocation)?;

    let pitch_of: BTreeMap<&str, f64> = document.feeders.iter().map(|feeder| (feeder.id.as_str(), feeder.pitch)).collect();
    let assigned_of: BTreeMap<&str, u32> = allocation.feeders.iter().map(|lane| (lane.id.as_str(), lane.assigned)).collect();

    let steps = placement
        .entries
        .iter()
        .map(|entry| {
            let id = entry.feeder.as_deref().unwrap_or_default();
            FirstArticleStep {
                step: entry.index.saturating_add(1),
                refdes: entry.refdes.clone(),
                package: entry.package.clone(),
                side: entry.side,
                x: entry.x,
                y: entry.y,
                rotation: entry.rotation,
                feeder: id.to_string(),
                pitch: pitch_of.get(id).copied().unwrap_or(0.0),
            }
        })
        .collect();

    let materials = package_counts(document)
        .into_iter()
        .map(|(package, components)| {
            let mut top = false;
            let mut bottom = false;
            for component in &document.components {
                if component.package != package {
                    continue;
                }
                match component.side {
                    Side::Top => top = true,
                    Side::Bottom => bottom = true,
                }
            }
            let mut sides = Vec::with_capacity(2);
            if top {
                sides.push(Side::Top);
            }
            if bottom {
                sides.push(Side::Bottom);
            }
            let lanes = document
                .feeders
                .iter()
                .filter(|feeder| feeder.package == package)
                .map(|feeder| FirstArticleLane {
                    id: feeder.id.clone(),
                    pitch: feeder.pitch,
                    loaded: feeder.loaded,
                    capacity: feeder.capacity,
                    assigned: assigned_of.get(feeder.id.as_str()).copied().unwrap_or(0),
                })
                .collect();
            FirstArticleMaterial { package, components, sides, lanes }
        })
        .collect::<Vec<_>>();

    let top_components = document.components.iter().filter(|component| component.side == Side::Top).count() as u32;
    let bottom_components = document.components.iter().filter(|component| component.side == Side::Bottom).count() as u32;

    Ok(FirstArticleReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        revision: document.revision,
        board: document.board,
        panel: document.panel,
        board_count: placement.board_count,
        panel_width: placement.panel_width,
        panel_depth: placement.panel_depth,
        components: document.components.len() as u32,
        top_components,
        bottom_components,
        packages: materials.len() as u32,
        feeders: document.feeders.len() as u32,
        checklist: CHECKS.iter().map(|check| (*check).to_string()).collect(),
        materials,
        steps,
    })
}
