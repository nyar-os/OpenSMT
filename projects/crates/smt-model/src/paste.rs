//! Read-only solder paste volume usage report.
//!
//! The report multiplies each generated stencil aperture by the stencil foil
//! thickness, scales the per-board volume by the build quantity and adds the
//! waste percentage, so a planner can size paste jars and reel changeovers.
//! Volumes are rounded to three decimals at every reported level, the sheet
//! is deterministic and byte-identical on repeat, and it never modifies the
//! document. Invalid options fail with `paste/invalid`.

use serde::{Deserialize, Serialize};

use crate::{
    SmtError,
    export::{escape, number},
    model::{Side, SmtDocument},
    stencil,
};

pub const FORMAT: &str = "open-smt-paste-volume";
pub const VERSION: u32 = 1;
/// The foil thickness assumed when the request omits one.
pub const DEFAULT_FOIL_THICKNESS_MM: f64 = 0.12;

/// The paste volume request. Omitted fields keep their defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PasteOptions {
    /// Stencil foil thickness in millimetres; defaults to `0.12`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foil_thickness_mm: Option<f64>,
    /// Number of boards the run covers; defaults to `1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boards: Option<u32>,
    /// Paste waste percentage added on top of the run volume; defaults to `0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waste_percent: Option<f64>,
}

impl PasteOptions {
    /// Resolves the options to `(foil thickness, boards, waste percent)`.
    ///
    /// The foil thickness must be a positive finite number, the board count
    /// at least one and the waste percentage finite between 0 and 100;
    /// anything else fails with `paste/invalid`.
    pub fn resolve(&self) -> Result<(f64, u32, f64), SmtError> {
        let foil = self.foil_thickness_mm.unwrap_or(DEFAULT_FOIL_THICKNESS_MM);
        let boards = self.boards.unwrap_or(1);
        let waste = self.waste_percent.unwrap_or(0.0);
        if !foil.is_finite() || foil <= 0.0 {
            return Err(SmtError::new(
                "paste/invalid",
                format!("foil thickness `{foil}` must be a positive finite number of millimetres"),
            ));
        }
        if boards < 1 {
            return Err(SmtError::new("paste/invalid", "board count must be a positive integer"));
        }
        if !waste.is_finite() || waste < 0.0 || waste > 100.0 {
            return Err(SmtError::new(
                "paste/invalid",
                format!("waste percentage `{waste}` must be a finite number from 0 to 100"),
            ));
        }
        Ok((round3(foil), boards, round3(waste)))
    }
}

/// Paste volume of one package, in first-aperture order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PastePackage {
    /// The package identifier as written on the placement.
    pub package: String,
    /// Number of apertures this package contributes per board.
    pub apertures_per_board: u32,
    /// Volume of a single aperture at the resolved foil thickness; identical
    /// for every aperture of the package because the geometry is per package.
    pub volume_per_aperture_mm3: f64,
    /// Sum of the package apertures for one board.
    pub volume_per_board_mm3: f64,
    /// `volume_per_board_mm3` scaled by the board count, before waste.
    pub volume_run_mm3: f64,
}

/// Read-only solder paste volume report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteReport {
    /// Always `open-smt-paste-volume`.
    pub format: String,
    /// Report version, currently `1`.
    pub version: u32,
    /// Always `mm`; volumes are cubic millimetres.
    pub units: String,
    /// The resolved foil thickness in millimetres.
    pub foil_thickness_mm: f64,
    /// The resolved board count.
    pub boards: u32,
    /// The resolved waste percentage.
    pub waste_percent: f64,
    /// Total apertures per board across all packages.
    pub apertures_per_board: u32,
    /// Paste volume deposited on one board.
    pub volume_per_board_mm3: f64,
    /// `volume_per_board_mm3` scaled by the board count, before waste.
    pub volume_run_mm3: f64,
    /// Paste volume lost to the waste percentage over the run.
    pub waste_volume_mm3: f64,
    /// `volume_run_mm3 + waste_volume_mm3`.
    pub total_volume_mm3: f64,
    /// Top-side paste volume per board.
    pub top_volume_per_board_mm3: f64,
    /// Bottom-side paste volume per board.
    pub bottom_volume_per_board_mm3: f64,
    /// Per-package volumes in first-aperture order.
    pub packages: Vec<PastePackage>,
}

/// Volume of one aperture at the foil thickness, rounded to three decimals.
fn aperture_volume(shape: &str, width: f64, height: f64, foil: f64) -> f64 {
    let area = if shape == "circle" { std::f64::consts::PI * (width / 2.0) * (width / 2.0) } else { width * height };
    round3(area * foil)
}

/// Builds the paste volume report from the generated stencil apertures.
pub fn report(document: &SmtDocument, options: &PasteOptions) -> Result<PasteReport, SmtError> {
    let (foil, boards, waste) = options.resolve()?;
    let stencil = stencil::report(document);
    let mut packages: Vec<PastePackage> = Vec::new();
    let mut apertures = 0u32;
    let mut per_board = 0.0;
    let mut top = 0.0;
    let mut bottom = 0.0;
    for entry in &stencil.entries {
        let volume = aperture_volume(&entry.shape, entry.width, entry.height, foil);
        apertures += 1;
        per_board += volume;
        match entry.side {
            Side::Top => top += volume,
            Side::Bottom => bottom += volume,
        }
        if let Some(package) = packages.iter_mut().find(|package| package.package == entry.package) {
            package.apertures_per_board += 1;
            package.volume_per_board_mm3 += volume;
        }
        else {
            packages.push(PastePackage {
                package: entry.package.clone(),
                apertures_per_board: 1,
                volume_per_aperture_mm3: volume,
                volume_per_board_mm3: volume,
                volume_run_mm3: 0.0,
            });
        }
    }
    let scale = f64::from(boards);
    for package in &mut packages {
        package.volume_per_board_mm3 = round3(package.volume_per_board_mm3);
        package.volume_run_mm3 = round3(package.volume_per_board_mm3 * scale);
    }
    let per_board = round3(per_board);
    let volume_run = round3(per_board * scale);
    let waste_volume = round3(volume_run * waste / 100.0);
    Ok(PasteReport {
        format: FORMAT.to_string(),
        version: VERSION,
        units: stencil.units.clone(),
        foil_thickness_mm: foil,
        boards,
        waste_percent: waste,
        apertures_per_board: apertures,
        volume_per_board_mm3: per_board,
        volume_run_mm3: volume_run,
        waste_volume_mm3: waste_volume,
        total_volume_mm3: round3(volume_run + waste_volume),
        top_volume_per_board_mm3: round3(top),
        bottom_volume_per_board_mm3: round3(bottom),
        packages,
    })
}

/// Renders the report as a deterministic CSV sheet.
///
/// One row per package in first-aperture order, followed by a `TOTAL` row
/// whose per-aperture volume column is empty because the total mixes
/// packages. Waste and the total with waste stay in the JSON report.
pub fn csv(report: &PasteReport) -> String {
    let mut text = String::from("package,apertures_per_board,volume_per_aperture_mm3,volume_per_board_mm3,volume_run_mm3\n");
    for package in &report.packages {
        text.push_str(&format!(
            "{},{},{},{},{}\n",
            escape(&package.package),
            package.apertures_per_board,
            number(package.volume_per_aperture_mm3),
            number(package.volume_per_board_mm3),
            number(package.volume_run_mm3),
        ));
    }
    text.push_str(&format!(
        "TOTAL,{apertures},,{per_board},{run}\n",
        apertures = report.apertures_per_board,
        per_board = number(report.volume_per_board_mm3),
        run = number(report.volume_run_mm3),
    ));
    text
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
