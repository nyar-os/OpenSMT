//! Generic CSV placement reader.
//!
//! A placement CSV starts with a header naming `refdes`, `package`, `x` and `y`;
//! `rotation` and `side` are optional. Column names are case-insensitive and may
//! appear in any order, fields may be wrapped in double quotes, and empty lines
//! are skipped. Coordinates are millimetres and are never converted or
//! translated.
//!
//! The reader is strict: a missing column, an unknown column, a wrong field
//! count, a non-numeric coordinate, an unknown side or a repeated reference
//! designator fails with an `import/...` code and names the physical line. The
//! component list keeps the row order of the source.

use std::collections::HashSet;

use crate::{
    SmtError,
    model::{Board, Component, Side},
    session::DEFAULT_BOARD,
};

/// Format identifier returned in an import report.
pub const FORMAT: &str = "csv";

/// Columns every header must declare.
const REQUIRED: [&str; 4] = ["refdes", "package", "x", "y"];

/// Columns a header may also declare.
const OPTIONAL: [&str; 2] = ["rotation", "side"];

/// A parsed board and its components, in source row order.
pub(crate) struct ImportedCsv {
    pub(crate) board: Board,
    pub(crate) components: Vec<Component>,
}

/// Parses one CSV placement file.
pub(crate) fn parse(source: &str, default_side: Side) -> Result<ImportedCsv, SmtError> {
    let records = read_records(source)?;
    let Some(header) = records.first()
    else {
        return Err(SmtError::new("import/parse", "the CSV file is empty"));
    };
    let columns = columns(header)?;

    let mut components = Vec::new();
    let mut references = HashSet::new();
    for record in records.iter().skip(1) {
        let component = row(record, &columns, default_side)?;
        if !references.insert(component.refdes.clone()) {
            return Err(SmtError::new(
                "import/duplicate-refdes",
                format!("line {}: component `{}` is declared twice", record.line, component.refdes),
            ));
        }
        components.push(component);
    }

    let board = board_for(&components);
    Ok(ImportedCsv { board, components })
}

/// Maps every header name to its column index.
fn columns(header: &Record) -> Result<Vec<(String, usize)>, SmtError> {
    let mut columns: Vec<(String, usize)> = Vec::with_capacity(header.fields.len());
    for (index, raw) in header.fields.iter().enumerate() {
        let name = raw.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err(SmtError::new(
                "import/invalid-header",
                format!("line {}: column {} has an empty name", header.line, index + 1),
            ));
        }
        if !REQUIRED.contains(&name.as_str()) && !OPTIONAL.contains(&name.as_str()) {
            return Err(SmtError::new(
                "import/unknown-column",
                format!("line {}: unknown column `{name}`; expected refdes, package, x, y, rotation or side", header.line),
            ));
        }
        if columns.iter().any(|(existing, _)| existing.as_str() == name) {
            return Err(SmtError::new(
                "import/duplicate-column",
                format!("line {}: column `{name}` is declared twice", header.line),
            ));
        }
        columns.push((name, index));
    }
    for name in REQUIRED {
        if !columns.iter().any(|(existing, _)| existing == name) {
            return Err(SmtError::new(
                "import/missing-column",
                format!("line {}: the header needs a `{name}` column", header.line),
            ));
        }
    }
    Ok(columns)
}

/// Reads one data row.
fn row(record: &Record, columns: &[(String, usize)], default_side: Side) -> Result<Component, SmtError> {
    let expected = columns.len();
    if record.fields.len() != expected {
        return Err(SmtError::new(
            "import/invalid-row",
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

    let refdes = field("refdes");
    if refdes.is_empty() {
        return Err(SmtError::new("import/invalid-value", format!("line {}: `refdes` must be non-empty", record.line)));
    }
    let package = field("package");
    if package.is_empty() {
        return Err(SmtError::new(
            "import/invalid-value",
            format!("line {}: `package` must be non-empty for component `{refdes}`", record.line),
        ));
    }

    let x = number(record, "x", field("x"))?;
    let y = number(record, "y", field("y"))?;
    if x < 0.0 || y < 0.0 {
        return Err(SmtError::new(
            "import/outside-board",
            format!(
                "line {}: component `{refdes}` at ({x}, {y}) has a negative coordinate; the board origin is a corner",
                record.line
            ),
        ));
    }

    let rotation = match field("rotation") {
        "" => 0.0,
        raw => number(record, "rotation", raw)?.rem_euclid(360.0),
    };

    let side = match field("side") {
        "" => default_side,
        raw => match raw.to_ascii_lowercase().as_str() {
            "top" => Side::Top,
            "bottom" => Side::Bottom,
            _ => {
                return Err(SmtError::new(
                    "import/invalid-side",
                    format!("line {}: unknown side `{raw}`; only top or bottom is supported", record.line),
                ));
            }
        },
    };

    Ok(Component { refdes: refdes.to_string(), package: package.to_string(), side, x, y, rotation, height: None })
}

/// Reads one required numeric cell, naming the physical line on failure.
fn number(record: &Record, column: &str, raw: &str) -> Result<f64, SmtError> {
    match raw.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        _ => Err(SmtError::new("import/invalid-number", format!("line {}: `{column}` is not a number: `{raw}`", record.line))),
    }
}

/// Sizes the board to the component extents, never below the default board.
fn board_for(components: &[Component]) -> Board {
    let width = components.iter().map(|component| component.x).fold(DEFAULT_BOARD.width, f64::max);
    let depth = components.iter().map(|component| component.y).fold(DEFAULT_BOARD.depth, f64::max);
    Board::new(width, depth, DEFAULT_BOARD.thickness)
}

/// One physical CSV record and the line it started on.
pub(crate) struct Record {
    pub(crate) line: usize,
    pub(crate) fields: Vec<String>,
}

/// Splits the source into records, honouring quoted fields and skipping empty
/// lines.
pub(crate) fn read_records(source: &str) -> Result<Vec<Record>, SmtError> {
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut line = 1_usize;
    let mut start = 1_usize;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut characters = source.chars().peekable();

    while let Some(character) = characters.next() {
        if in_quotes {
            match character {
                '"' if characters.peek() == Some(&'"') => {
                    characters.next();
                    field.push('"');
                }
                '"' => {
                    in_quotes = false;
                    after_quote = true;
                }
                '\n' => {
                    field.push('\n');
                    line += 1;
                }
                other => field.push(other),
            }
            continue;
        }
        if after_quote && !matches!(character, ',' | '\r' | '\n') {
            return Err(SmtError::new("import/parse", format!("line {line}: unexpected content after a closing quote")));
        }
        match character {
            '"' => {
                if !field.is_empty() {
                    return Err(SmtError::new(
                        "import/parse",
                        format!("line {line}: unexpected quote inside an unquoted field"),
                    ));
                }
                in_quotes = true;
            }
            ',' => {
                fields.push(std::mem::take(&mut field));
                after_quote = false;
            }
            '\r' => {}
            '\n' => {
                fields.push(std::mem::take(&mut field));
                push_record(&mut records, start, std::mem::take(&mut fields));
                line += 1;
                start = line;
                after_quote = false;
            }
            other => field.push(other),
        }
    }
    if in_quotes {
        return Err(SmtError::new("import/parse", format!("line {start}: a quoted field is not closed")));
    }
    if !field.is_empty() || after_quote || !fields.is_empty() {
        fields.push(field);
        push_record(&mut records, start, fields);
    }
    Ok(records)
}

/// Keeps a record unless every field is blank.
fn push_record(records: &mut Vec<Record>, line: usize, fields: Vec<String>) {
    if fields.iter().any(|field| !field.trim().is_empty()) {
        records.push(Record { line, fields });
    }
}
