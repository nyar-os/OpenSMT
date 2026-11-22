//! File writers.
//!
//! Every writer produces deterministic text and every write goes through
//! [`write_atomic`], so a reader never observes a half-written program.

use std::{fs, path::Path};

use crate::{
    SmtError,
    model::SmtDocument,
    place::{FeederReport, PlacementReport},
    reflow::ReflowReport,
};

/// Writes `contents` to `path` through a temporary file and a rename.
pub fn write_atomic(path: &Path, contents: &str) -> Result<(), SmtError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|error| SmtError::new("io/write", format!("cannot create `{}`: {error}", parent.display())))?;
        }
    }
    let temporary = path.with_extension(format!("{}.tmp", path.extension().and_then(|value| value.to_str()).unwrap_or("tmp")));
    fs::write(&temporary, contents)
        .map_err(|error| SmtError::new("io/write", format!("cannot write `{}`: {error}", temporary.display())))?;
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| SmtError::new("io/write", format!("cannot replace `{}`: {error}", path.display())))?;
    }
    fs::rename(&temporary, path)
        .map_err(|error| SmtError::new("io/write", format!("cannot replace `{}`: {error}", path.display())))?;
    Ok(())
}

/// Renders the placement program as CSV, one row per step.
pub fn program_csv(placement: &PlacementReport) -> String {
    let mut text = String::from("index,refdes,package,side,x,y,rotation,feeder\n");
    for entry in &placement.entries {
        text.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            entry.index,
            escape(&entry.refdes),
            escape(&entry.package),
            entry.side.as_str(),
            number(entry.x),
            number(entry.y),
            number(entry.rotation),
            escape(entry.feeder.as_deref().unwrap_or(""))
        ));
    }
    text
}

/// Renders the feeder setup sheet as CSV, one row per lane.
pub fn feeder_csv(feeders: &FeederReport) -> String {
    let mut text = String::from("id,package,loaded,capacity,assigned,remaining,refdes\n");
    for feeder in &feeders.feeders {
        text.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            escape(&feeder.id),
            escape(&feeder.package),
            feeder.loaded,
            feeder.capacity,
            feeder.assigned,
            feeder.remaining,
            escape(&feeder.refdes.join("|"))
        ));
    }
    text
}

/// Renders a Markdown summary of the whole job.
pub fn markdown(document: &SmtDocument, placement: &PlacementReport, feeders: &FeederReport, reflow: &ReflowReport) -> String {
    let mut text = String::new();
    text.push_str("# OpenSMT assembly report\n\n");

    text.push_str("## Board and panel\n\n");
    text.push_str("| Width | Depth | Thickness | Rows | Columns | Gap X | Gap Y | Boards |\n");
    text.push_str("|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    text.push_str(&format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} |\n\n",
        number(document.board.width),
        number(document.board.depth),
        number(document.board.thickness),
        document.panel.rows,
        document.panel.columns,
        number(document.panel.gap_x),
        number(document.panel.gap_y),
        placement.board_count
    ));
    text.push_str(&format!(
        "Panel size {} × {} mm. Head travel {} mm; estimated placement time {} s for {} components.\n\n",
        number(placement.panel_width),
        number(placement.panel_depth),
        number(placement.travel_length),
        number(placement.placement_seconds),
        placement.entries.len()
    ));

    text.push_str("## Feeders\n\n");
    text.push_str("| Id | Package | Loaded | Capacity | Assigned | Remaining |\n");
    text.push_str("|---|---|---:|---:|---:|---:|\n");
    for feeder in &feeders.feeders {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            escape(&feeder.id),
            escape(&feeder.package),
            feeder.loaded,
            feeder.capacity,
            feeder.assigned,
            feeder.remaining
        ));
    }
    text.push('\n');

    text.push_str("## Reflow\n\n");
    text.push_str("| Zone | Length | Entry °C | Exit °C | Seconds | Above liquidus |\n");
    text.push_str("|---|---:|---:|---:|---:|---:|\n");
    for zone in &reflow.zones {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            escape(&zone.name),
            number(zone.length),
            number(zone.entry_temperature),
            number(zone.exit_temperature),
            number(zone.seconds),
            number(zone.seconds_above_liquidus)
        ));
    }
    text.push_str(&format!(
        "\nPeak {} °C, liquidus {} °C, time above liquidus {} s.\n\n",
        number(reflow.peak_temperature),
        number(reflow.liquidus),
        number(reflow.time_above_liquidus)
    ));

    text.push_str("## Placement program\n\n");
    text.push_str("| # | Refdes | Package | Side | X | Y | Rotation | Feeder |\n");
    text.push_str("|---:|---|---|---|---:|---:|---:|---|\n");
    for entry in &placement.entries {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            entry.index,
            escape(&entry.refdes),
            escape(&entry.package),
            entry.side.as_str(),
            number(entry.x),
            number(entry.y),
            number(entry.rotation),
            escape(entry.feeder.as_deref().unwrap_or(""))
        ));
    }

    text
}

fn escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('|') {
        format!("\"{}\"", value.replace('"', "\"\""))
    }
    else {
        value.to_string()
    }
}

fn number(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded.abs() < 1e-9 { "0".to_string() } else { format!("{rounded}") }
}
