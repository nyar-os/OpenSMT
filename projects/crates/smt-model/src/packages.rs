//! Shared package geometry rules.
//!
//! The document model stores no body dimensions, so the stencil generator and
//! the clearance check derive them from the package name through this table.
//! Names are matched case-insensitively: a leading letter (`R0603`) keeps the
//! chip code and a trailing number (`SOIC-8`, `QFP-44`) names the lead count.
//! Every family is a simplification of the real footprint; an unrecognised
//! package falls back to the generic rule, which reuses the default drawing
//! footprint of `2.0 × 1.2 mm`.

/// Family a package name was recognised as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    /// Two-terminal chip case, e.g. `0402`, `0603`, `0805`, `1206`.
    Chip,
    /// Three-lead SOT-23.
    Sot23,
    /// Small-outline package with two lead rows, e.g. `SOIC-8`.
    Soic,
    /// Quad flat pack with four lead rows, e.g. `TQFP-100`.
    Qfp,
    /// Quad flat no-leads with four side terminals, e.g. `QFN-32`.
    Qfn,
    /// Land-grid, chip-scale or ball-grid array of round terminals.
    Lga,
    /// Unrecognised package.
    Generic,
}

impl Family {
    /// Short identifier used by reports.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Chip => "chip",
            Self::Sot23 => "sot23",
            Self::Soic => "soic",
            Self::Qfp => "qfp",
            Self::Qfn => "qfn",
            Self::Lga => "lga",
            Self::Generic => "generic",
        }
    }
}

/// Shape of one terminal or aperture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// Rectangle.
    Rect,
    /// Circle.
    Circle,
}

impl Shape {
    /// Short identifier used by reports.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Rect => "rect",
            Self::Circle => "circle",
        }
    }
}

/// Simplified body and terminal geometry of one package.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Geometry {
    /// Recognised family.
    pub(crate) family: Family,
    /// Terminal shape.
    pub(crate) shape: Shape,
    /// Lead count taken from the name, or the family default.
    pub(crate) pins: u32,
    /// Lead pitch in millimetres.
    pub(crate) pitch: f64,
    /// Body size along the component long axis in millimetres.
    pub(crate) body_length: f64,
    /// Body size across the component short axis in millimetres.
    pub(crate) body_width: f64,
    /// Nominal terminal width along X in millimetres, before reduction.
    pub(crate) aperture_width: f64,
    /// Nominal terminal height along Y in millimetres, before reduction.
    pub(crate) aperture_height: f64,
    /// Paste area reduction factor, e.g. `0.9` for a 10 % area reduction.
    pub(crate) reduction: f64,
}

/// One chip rule: code, body length, body width, terminal width, terminal
/// height and paste area reduction.
const CHIPS: [(&str, f64, f64, f64, f64, f64); 8] = [
    ("0201", 0.6, 0.3, 0.22, 0.30, 1.00),
    ("0402", 1.0, 0.5, 0.40, 0.50, 0.90),
    ("0603", 1.6, 0.8, 0.65, 0.80, 0.90),
    ("0805", 2.0, 1.25, 0.85, 1.25, 0.90),
    ("1206", 3.2, 1.6, 1.20, 1.60, 0.90),
    ("1210", 3.2, 2.5, 1.20, 2.50, 0.90),
    ("1812", 4.5, 3.2, 1.60, 3.20, 0.90),
    ("2010", 5.0, 2.5, 1.80, 2.50, 0.90),
];

/// Returns the geometry of one package name.
pub(crate) fn geometry(package: &str) -> Geometry {
    let name = package.to_ascii_uppercase();
    if name.contains("SOT-23") || name.contains("SOT23") {
        return Geometry {
            family: Family::Sot23,
            shape: Shape::Rect,
            pins: 3,
            pitch: 0.95,
            body_length: 2.9,
            body_width: 1.6,
            aperture_width: 0.55,
            aperture_height: 0.65,
            reduction: 0.85,
        };
    }
    let groups = digit_groups(&name);
    if let Some((_, length, width, aperture_width, aperture_height, reduction)) =
        CHIPS.iter().find(|(code, ..)| groups.contains(code))
    {
        return Geometry {
            family: Family::Chip,
            shape: Shape::Rect,
            pins: 2,
            pitch: length - aperture_width,
            body_length: *length,
            body_width: *width,
            aperture_width: *aperture_width,
            aperture_height: *aperture_height,
            reduction: *reduction,
        };
    }
    let pins = groups.iter().filter_map(|group| group.parse::<u32>().ok()).find(|value| (2..=1024).contains(value));
    if name.contains("QFN") || name.contains("DFN") {
        return quad_rule(Family::Qfn, pins.unwrap_or(32));
    }
    if name.contains("QFP") {
        return quad_rule(Family::Qfp, pins.unwrap_or(44));
    }
    if name.contains("SOIC") || name.contains("SOP") || name.contains("SOJ") {
        let pins = pins.unwrap_or(8);
        let rows = (pins / 2).max(1);
        return Geometry {
            family: Family::Soic,
            shape: Shape::Rect,
            pins,
            pitch: 1.27,
            body_length: (rows - 1) as f64 * 1.27 + 1.0,
            body_width: 3.7,
            aperture_width: 0.60,
            aperture_height: 0.70,
            reduction: 0.85,
        };
    }
    if name.contains("LGA") || name.contains("CSP") || name.contains("BGA") {
        let pins = pins.unwrap_or(16);
        let grid = (pins as f64).sqrt().ceil() as u32;
        let body = (grid - 1) as f64 * 0.5 + 0.6;
        return Geometry {
            family: Family::Lga,
            shape: Shape::Circle,
            pins,
            pitch: 0.5,
            body_length: body,
            body_width: body,
            aperture_width: 0.30,
            aperture_height: 0.30,
            reduction: 0.85,
        };
    }
    Geometry {
        family: Family::Generic,
        shape: Shape::Rect,
        pins: 0,
        pitch: 0.0,
        body_length: 2.0,
        body_width: 1.2,
        aperture_width: 1.60,
        aperture_height: 1.00,
        reduction: 0.90,
    }
}

/// One terminal of a package: its offset from the component centre and its
/// nominal aperture size before the area reduction is applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Terminal {
    /// Offset along the component long axis in millimetres.
    pub(crate) dx: f64,
    /// Offset across the component short axis in millimetres.
    pub(crate) dy: f64,
    /// Aperture width along X in millimetres.
    pub(crate) width: f64,
    /// Aperture height along Y in millimetres.
    pub(crate) height: f64,
}

/// Lists the terminals of one package in deterministic order.
pub(crate) fn terminals(geometry: &Geometry) -> Vec<Terminal> {
    let rect = |dx: f64, dy: f64, rotated: bool| Terminal {
        dx,
        dy,
        width: if rotated { geometry.aperture_height } else { geometry.aperture_width },
        height: if rotated { geometry.aperture_width } else { geometry.aperture_height },
    };
    match geometry.family {
        Family::Chip => {
            let offset = (geometry.body_length - geometry.aperture_width) / 2.0;
            vec![rect(-offset, 0.0, false), rect(offset, 0.0, false)]
        }
        Family::Sot23 => vec![rect(0.0, 1.0, false), rect(-0.475, -1.0, false), rect(0.475, -1.0, false)],
        Family::Soic => {
            let rows = (geometry.pins / 2).max(1);
            let mut out = Vec::with_capacity(rows as usize * 2);
            // JEDEC SO-8 (150 mil body): the gull-wing pad centres sit on a
            // 5.4 mm span, so the apertures clear the 3.7 mm body outline.
            for side in [-2.7, 2.7] {
                for row in 0..rows {
                    out.push(rect(side, (row as f64 - (rows - 1) as f64 / 2.0) * geometry.pitch, false));
                }
            }
            out
        }
        Family::Qfp | Family::Qfn => {
            let rows = (geometry.pins / 4).max(1);
            let offset = (rows - 1) as f64 * geometry.pitch / 2.0 + 0.55;
            let span = |row: u32| (row as f64 - (rows - 1) as f64 / 2.0) * geometry.pitch;
            let mut out = Vec::with_capacity(rows as usize * 4);
            for row in 0..rows {
                out.push(rect(span(row), -offset, false));
            }
            for row in 0..rows {
                out.push(rect(offset, span(row), true));
            }
            for row in 0..rows {
                out.push(rect(span(row), offset, false));
            }
            for row in 0..rows {
                out.push(rect(-offset, span(row), true));
            }
            out
        }
        Family::Lga => {
            let grid = (geometry.pins as f64).sqrt().ceil() as u32;
            (0..geometry.pins)
                .map(|index| {
                    let column = index % grid;
                    let row = index / grid;
                    Terminal {
                        dx: (column as f64 - (grid - 1) as f64 / 2.0) * geometry.pitch,
                        dy: ((grid - 1) as f64 / 2.0 - row as f64) * geometry.pitch,
                        width: geometry.aperture_width,
                        height: geometry.aperture_height,
                    }
                })
                .collect()
        }
        Family::Generic => vec![rect(0.0, 0.0, false)],
    }
}

/// Quad flat pack / no-leads rule with `pins` leads on four rows.
fn quad_rule(family: Family, pins: u32) -> Geometry {
    let rows = (pins / 4).max(1);
    let offset = (rows - 1) as f64 * 0.5 / 2.0 + 0.55;
    let body = offset * 2.0 + 0.8;
    Geometry {
        family,
        shape: Shape::Rect,
        pins,
        pitch: 0.5,
        body_length: body,
        body_width: body,
        aperture_width: 0.30,
        aperture_height: 0.80,
        reduction: 0.80,
    }
}

/// Splits a name into its maximal runs of ASCII digits.
fn digit_groups(name: &str) -> Vec<&str> {
    let mut groups = Vec::new();
    let mut start = None;
    for (index, byte) in name.bytes().enumerate() {
        match (byte.is_ascii_digit(), start) {
            (true, None) => start = Some(index),
            (false, Some(from)) => {
                groups.push(&name[from..index]);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        groups.push(&name[from..]);
    }
    groups
}
