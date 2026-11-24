//! Temporary driver: builds a demo panel and exports every supported format.

use std::path::Path;

use smt_model::{Board, Component, ExportFormat, Feeder, OpenSmt, Panel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "export-all".into());
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;

    let path = out.join("demo.osmt");
    let _ = std::fs::remove_file(&path);
    let mut session = OpenSmt::new();
    session.create(&path)?;
    session.set_board(Board::new(120.0, 90.0, 1.6))?;
    session.set_panel(Panel::new(2, 2, 5.0, 5.0))?;
    session.add_component(Component::new("U1", "SOIC-8", 40.0, 30.0))?;
    session.add_component(Component::new("R1", "0603", 10.0, 10.0))?;
    session.add_component(Component::new("R2", "0603", 20.0, 60.0))?;
    session.add_component(Component::new("C1", "0603", 70.0, 20.0))?;
    session.add_component(Component::new("D1", "0805", 90.0, 70.0))?;
    session.add_feeder(Feeder::new("F1", "0603", 4.0, 5000))?;
    session.add_feeder(Feeder::new("F2", "SOIC-8", 4.0, 500))?;
    session.add_feeder(Feeder::new("F3", "0805", 4.0, 3000))?;

    for format in [ExportFormat::Program, ExportFormat::Feeders, ExportFormat::FirstArticle, ExportFormat::Json, ExportFormat::Md] {
        let target = out.join(format!("demo.{}", format.as_str()));
        match session.export(format, &target) {
            Ok(report) => println!("OK {}: {} bytes -> {}", report.format, report.bytes, report.path),
            Err(err) => println!("ERR {}: {}", format.as_str(), err.message),
        }
    }
    Ok(())
}
