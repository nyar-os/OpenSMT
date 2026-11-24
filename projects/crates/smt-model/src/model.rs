//! Document model of an SMT assembly job.
//!
//! A document is one `.osmt` JSON file: a board, an optional panel array, a
//! component list, a feeder setup and a reflow profile. Placement order, feeder
//! allocation and reflow metrics are derived on demand, so the file never
//! contains stale machine output.

use serde::{Deserialize, Serialize};

/// Document format identifier written to every file.
pub const FORMAT: &str = "open-smt";

/// Document schema version written to every file.
pub const VERSION: u32 = 1;

/// The only unit system this build accepts. Board dimensions are millimetres,
/// feeder pitch is millimetres and conveyor speed is millimetres per minute.
pub const UNITS: &str = "mm";

/// One SMT assembly job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmtDocument {
    /// Format identifier; always [`FORMAT`] for a document this crate wrote.
    pub format: String,
    /// Schema version; always [`VERSION`] for a document this crate wrote.
    pub version: u32,
    /// Monotonically increasing edit counter.
    pub revision: u32,
    /// Declared unit system; must be [`UNITS`] to analyse.
    pub units: String,
    /// Single board geometry.
    pub board: Board,
    /// Panel array; `1 × 1` means a single board.
    pub panel: Panel,
    /// Components in declaration order.
    pub components: Vec<Component>,
    /// Feeder setup.
    pub feeders: Vec<Feeder>,
    /// Reel splice events, one per joint between the remaining tape and a new reel.
    #[serde(default)]
    pub splices: Vec<Splice>,
    /// Reflow profile.
    pub profile: ReflowProfile,
}

impl SmtDocument {
    /// Creates a document with the supplied board and a single-board panel.
    pub fn new(board: Board) -> Self {
        Self {
            format: FORMAT.to_string(),
            version: VERSION,
            revision: 0,
            units: UNITS.to_string(),
            board,
            panel: Panel::new(1, 1, 0.0, 0.0),
            components: Vec::new(),
            feeders: Vec::new(),
            splices: Vec::new(),
            profile: ReflowProfile::default(),
        }
    }
}

/// One rectangular board; the origin is a corner and Z is thickness.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    /// Size along X.
    pub width: f64,
    /// Size along Y.
    pub depth: f64,
    /// Board thickness.
    pub thickness: f64,
}

impl Board {
    /// Creates a board from its sizes.
    pub fn new(width: f64, depth: f64, thickness: f64) -> Self {
        Self { width, depth, thickness }
    }
}

/// A panel array of boards with gaps between them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Panel {
    /// Number of board rows.
    pub rows: u32,
    /// Number of board columns.
    pub columns: u32,
    /// Gap between columns.
    pub gap_x: f64,
    /// Gap between rows.
    pub gap_y: f64,
}

impl Panel {
    /// Creates a panel array.
    pub const fn new(rows: u32, columns: u32, gap_x: f64, gap_y: f64) -> Self {
        Self { rows, columns, gap_x, gap_y }
    }

    /// Number of boards on the panel.
    pub const fn board_count(&self) -> u32 {
        self.rows * self.columns
    }
}

/// Board side a component is placed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    /// Primary side, processed first.
    Top,
    /// Secondary side.
    Bottom,
}

impl Side {
    /// Short identifier used by the CLI and by reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

impl std::str::FromStr for Side {
    type Err = crate::SmtError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            other => Err(crate::SmtError::new(
                "component/unknown-side",
                format!("unknown side `{other}`; only top or bottom is supported"),
            )),
        }
    }
}

/// One placed component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// Unique reference designator such as `R1` or `U3`.
    pub refdes: String,
    /// Package identifier such as `0603` or `QFN-32`.
    pub package: String,
    /// Board side.
    pub side: Side,
    /// Placement X coordinate on a single board.
    pub x: f64,
    /// Placement Y coordinate on a single board.
    pub y: f64,
    /// Rotation in degrees, clockwise, `0 … 360`.
    pub rotation: f64,
    /// Component height; `null` when unknown.
    #[serde(default)]
    pub height: Option<f64>,
}

impl Component {
    /// Creates a top-side component with zero rotation.
    pub fn new(refdes: impl Into<String>, package: impl Into<String>, x: f64, y: f64) -> Self {
        Self { refdes: refdes.into(), package: package.into(), side: Side::Top, x, y, rotation: 0.0, height: None }
    }
}

/// One feeder lane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Feeder {
    /// Unique feeder identifier.
    pub id: String,
    /// Package the feeder supplies.
    pub package: String,
    /// Tape pitch in millimetres.
    pub pitch: f64,
    /// Maximum component capacity of the lane.
    pub capacity: u32,
    /// Components actually loaded; allocation never exceeds this value.
    pub loaded: u32,
    /// Leader tape components wasted when a reel is installed.
    #[serde(default)]
    pub leader_waste: u32,
    /// Trailer tape components wasted when a reel is exhausted.
    #[serde(default)]
    pub trailer_waste: u32,
    /// Extra manual scrap for this lane, in components.
    #[serde(default)]
    pub scrap: u32,
}

impl Feeder {
    /// Creates a feeder that is loaded to capacity and has no waste or scrap.
    pub fn new(id: impl Into<String>, package: impl Into<String>, pitch: f64, capacity: u32) -> Self {
        Self {
            id: id.into(),
            package: package.into(),
            pitch,
            capacity,
            loaded: capacity,
            leader_waste: 0,
            trailer_waste: 0,
            scrap: 0,
        }
    }

    /// Leader plus trailer waste charged to every reel of this lane.
    pub fn per_reel_waste(&self) -> u32 {
        self.leader_waste.saturating_add(self.trailer_waste)
    }
}

/// One splice: a new reel is joined to the remaining tape at a placement step.
///
/// `sequence` is the 1-based step in the placement program. The splice is
/// applied on the first board when that step is reached; the cumulative used
/// quantity never resets. `leader_waste`, `trailer_waste` and `scrap` are charged
/// once at the joint, on top of the lane's per-reel waste.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Splice {
    /// Feeder lane that is spliced.
    pub feeder: String,
    /// 1-based placement program step at which the new reel is joined.
    pub sequence: u32,
    /// Additional leader waste at the joint.
    #[serde(default)]
    pub leader_waste: u32,
    /// Additional trailer waste at the joint.
    #[serde(default)]
    pub trailer_waste: u32,
    /// Manual scrap at the joint.
    #[serde(default)]
    pub scrap: u32,
}

impl Splice {
    /// Creates a splice with no extra waste.
    pub fn new(feeder: impl Into<String>, sequence: u32) -> Self {
        Self { feeder: feeder.into(), sequence, leader_waste: 0, trailer_waste: 0, scrap: 0 }
    }

    /// Leader plus trailer waste charged once at the joint.
    pub fn splice_waste(&self) -> u32 {
        self.leader_waste.saturating_add(self.trailer_waste)
    }
}

/// One reflow zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Zone {
    /// Zone name such as `preheat` or `reflow`.
    pub name: String,
    /// Zone length along the conveyor.
    pub length: f64,
    /// Zone set-point temperature in degrees Celsius.
    pub temperature: f64,
}

impl Zone {
    /// Creates a zone.
    pub fn new(name: impl Into<String>, length: f64, temperature: f64) -> Self {
        Self { name: name.into(), length, temperature }
    }
}

/// Reflow profile: zone set-points, conveyor speed and the alloy liquidus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflowProfile {
    /// Conveyor speed in millimetres per minute.
    pub conveyor_speed: f64,
    /// Alloy liquidus temperature in degrees Celsius.
    pub liquidus: f64,
    /// Zones in conveyor order.
    pub zones: Vec<Zone>,
}

impl Default for ReflowProfile {
    fn default() -> Self {
        Self {
            conveyor_speed: 600.0,
            liquidus: 217.0,
            zones: vec![
                Zone::new("preheat", 300.0, 150.0),
                Zone::new("soak", 400.0, 180.0),
                Zone::new("reflow", 300.0, 245.0),
                Zone::new("cool", 400.0, 30.0),
            ],
        }
    }
}
