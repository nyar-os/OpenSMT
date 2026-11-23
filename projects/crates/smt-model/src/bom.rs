//! Read-only BOM cross-check.
//!
//! The report compares the document's placement list with a BOM CSV: one row
//! per part, declaring `package` and `quantity` and optionally a `;`-separated
//! `refdes` list. It reports reference designators the BOM declares but the
//! placement table lacks (missing), reference designators the placement table
//! carries but the BOM does not declare (extra), and every package whose
//! placed count differs from the summed BOM quantity. Neither the document
//! nor the BOM file is modified.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{SmtError, csv_import::Record, model::SmtDocument};

/// Report format identifier.
pub const FORMAT: &str = "open-smt-bom-check";

/// Columns every BOM header must declare.
const REQUIRED: [&str; 2] = ["package", "quantity"];

/// Columns a BOM header may also declare.
const OPTIONAL: [&str; 1] = ["refdes"];

/// One reference designator the two sides disagree about.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BomFinding {
    /// Package the finding belongs to.
    pub package: String,
    /// The disputed reference designator.
    pub refdes: String,
}

/// One package whose placed count differs from the BOM quantity.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BomQuantityMismatch {
    /// Package identifier.
    pub package: String,
    /// Quantity the BOM declares for this package.
    pub bom_quantity: u32,
    /// Number of placements of this package.
    pub placed_quantity: u32,
    /// `placed_quantity - bom_quantity`.
    pub difference: i64,
}

/// Read-only BOM cross-check report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BomCheckReport {
    /// Report format identifier.
    pub format: String,
    /// Report version.
    pub version: u32,
    /// Unit system of the document.
    pub units: String,
    /// Whether the placement table matches the BOM.
    pub ok: bool,
    /// Number of data rows in the BOM.
    pub bom_lines: u32,
    /// Number of placements in the document.
    pub components: u32,
    /// BOM reference designators with no placement, in package then refdes order.
    pub missing: Vec<BomFinding>,
    /// Placed reference designators with no BOM entry, in package then refdes order.
    pub extra: Vec<BomFinding>,
    /// Packages whose counts disagree, in package order.
    pub quantity_mismatches: Vec<BomQuantityMismatch>,
}

/// One parsed BOM row.
struct BomRow {
    package: String,
    quantity: u32,
    refdes: Vec<String>,
}

/// Cross-checks the placement list of `document` against a BOM CSV.
///
/// The header must declare `package` and `quantity` and may declare `refdes`;
/// names are case-insensitive, fields may be double-quoted and empty lines are
/// skipped. Rows are grouped by package name and quantities are summed. A
/// package whose BOM rows enumerate no reference designators is compared by
/// quantity only. Findings are ordered by package name, then by reference
/// designator. The document is never modified.
pub fn check(document: &SmtDocument, source: &str) -> Result<BomCheckReport, SmtError> {
    let rows = parse(source)?;

    let mut bom: BTreeMap<&str, (u32, BTreeSet<&str>)> = BTreeMap::new();
    for row in &rows {
        let entry = bom.entry(row.package.as_str()).or_default();
        entry.0 = entry.0.saturating_add(row.quantity);
        for refdes in &row.refdes {
            entry.1.insert(refdes.as_str());
        }
    }

    let mut placed: BTreeMap<&str, (u32, BTreeSet<&str>)> = BTreeMap::new();
    for component in &document.components {
        let entry = placed.entry(component.package.as_str()).or_default();
        entry.0 += 1;
        entry.1.insert(component.refdes.as_str());
    }

    let empty: BTreeSet<&str> = BTreeSet::new();
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    let mut quantity_mismatches = Vec::new();
    let packages: BTreeSet<&str> = bom.keys().chain(placed.keys()).copied().collect();
    for package in packages {
        let bom_quantity = bom.get(package).map_or(0, |entry| entry.0);
        let bom_refs = bom.get(package).map(|entry| &entry.1);
        let placed_quantity = placed.get(package).map_or(0, |entry| entry.0);
        let placed_refs = placed.get(package).map_or(&empty, |entry| &entry.1);

        if bom_quantity != placed_quantity {
            quantity_mismatches.push(BomQuantityMismatch {
                package: package.to_string(),
                bom_quantity,
                placed_quantity,
                difference: i64::from(placed_quantity) - i64::from(bom_quantity),
            });
        }
        if let Some(refs) = bom_refs.filter(|refs| !refs.is_empty()) {
            for refdes in refs.difference(placed_refs) {
                missing.push(BomFinding { package: package.to_string(), refdes: (*refdes).to_string() });
            }
            for refdes in placed_refs.difference(refs) {
                extra.push(BomFinding { package: package.to_string(), refdes: (*refdes).to_string() });
            }
        }
        else if !bom.contains_key(package) {
            for refdes in placed_refs {
                extra.push(BomFinding { package: package.to_string(), refdes: (*refdes).to_string() });
            }
        }
    }

    Ok(BomCheckReport {
        format: FORMAT.to_string(),
        version: 1,
        units: document.units.clone(),
        ok: missing.is_empty() && extra.is_empty() && quantity_mismatches.is_empty(),
        bom_lines: rows.len() as u32,
        components: document.components.len() as u32,
        missing,
        extra,
        quantity_mismatches,
    })
}

/// Parses the BOM CSV into rows, rejecting malformed input with `bom/...` codes.
fn parse(source: &str) -> Result<Vec<BomRow>, SmtError> {
    let records = crate::csv_import::read_records(source).map_err(|error| SmtError::new("bom/parse", error.message))?;
    let Some(header) = records.first()
    else {
        return Err(SmtError::new("bom/parse", "the BOM file is empty"));
    };
    let columns = columns(header)?;

    let mut rows = Vec::new();
    let mut references: BTreeSet<String> = BTreeSet::new();
    for record in records.iter().skip(1) {
        let row = row(record, &columns)?;
        for refdes in &row.refdes {
            if !references.insert(refdes.clone()) {
                return Err(SmtError::new(
                    "bom/duplicate-refdes",
                    format!("line {}: `{refdes}` is declared twice", record.line),
                ));
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

/// Maps every BOM header name to its column index.
fn columns(header: &Record) -> Result<Vec<(String, usize)>, SmtError> {
    let mut columns: Vec<(String, usize)> = Vec::with_capacity(header.fields.len());
    for (index, raw) in header.fields.iter().enumerate() {
        let name = raw.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err(SmtError::new(
                "bom/invalid-header",
                format!("line {}: column {} has an empty name", header.line, index + 1),
            ));
        }
        if !REQUIRED.contains(&name.as_str()) && !OPTIONAL.contains(&name.as_str()) {
            return Err(SmtError::new(
                "bom/unknown-column",
                format!("line {}: unknown column `{name}`; expected package, quantity or refdes", header.line),
            ));
        }
        if columns.iter().any(|(existing, _)| existing.as_str() == name) {
            return Err(SmtError::new(
                "bom/duplicate-column",
                format!("line {}: column `{name}` is declared twice", header.line),
            ));
        }
        columns.push((name, index));
    }
    for name in REQUIRED {
        if !columns.iter().any(|(existing, _)| existing == name) {
            return Err(SmtError::new(
                "bom/missing-column",
                format!("line {}: the header needs a `{name}` column", header.line),
            ));
        }
    }
    Ok(columns)
}

/// Reads one BOM data row.
fn row(record: &Record, columns: &[(String, usize)]) -> Result<BomRow, SmtError> {
    let expected = columns.len();
    if record.fields.len() != expected {
        return Err(SmtError::new(
            "bom/invalid-row",
            format!("line {}: found {} fields but the header declares {expected}", record.line, record.fields.len()),
        ));
    }

    let field = |name: &str| -> &str {
        columns
            .iter()
            .find(|(column, _)| column.as_str() == name)
            .and_then(|(_, index)| record.fields.get(*index))
            .map_or("", String::as_str)
            .trim()
    };

    let package = field("package");
    if package.is_empty() {
        return Err(SmtError::new("bom/invalid-value", format!("line {}: `package` must be non-empty", record.line)));
    }
    let raw_quantity = field("quantity");
    let quantity = raw_quantity.parse::<u32>().map_err(|_| {
        SmtError::new("bom/invalid-number", format!("line {}: `quantity` is not an integer: `{raw_quantity}`", record.line))
    })?;
    if quantity == 0 {
        return Err(SmtError::new(
            "bom/invalid-value",
            format!("line {}: `quantity` must be greater than zero for package `{package}`", record.line),
        ));
    }
    let refdes = field("refdes").split(';').map(str::trim).filter(|refdes| !refdes.is_empty()).map(str::to_string).collect();

    Ok(BomRow { package: package.to_string(), quantity, refdes })
}
