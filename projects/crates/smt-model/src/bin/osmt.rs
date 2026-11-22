//! Thin command line over the [`smt_model::OpenSmt`] session API.

use std::{collections::HashMap, env, path::Path, process};

use smt_model::{
    BalanceReport, Board, Component, DualLaneStrategy, ExportFormat, Feeder, OpenSmt, Panel, ReflowProfile, Side, SmtError,
    Zone,
};

const USAGE: &str = "\
osmt - OpenSMT placement, feeder and reflow planner (millimetres, degrees Celsius)

Usage:
  osmt create <file.osmt>
  osmt import <file.kicad_pcb> --out <file.osmt>
  osmt import csv <file.csv> --out <file.osmt> [--side top|bottom]
  osmt board <file> --width <w> --depth <d> --thickness <t>
  osmt panel <file> --rows <r> --columns <c> [--gap-x <g>] [--gap-y <g>]
  osmt panel fit <file> --max-width <mm> --max-depth <mm> [--gap-x <mm>] [--gap-y <mm>]
  osmt component add <file> --refdes <R1> --package <pkg> --x <x> --y <y>
                     [--side top|bottom] [--rotation <deg>] [--height <h>]
  osmt component remove <file> --refdes <R1>
  osmt feeder add <file> --id <id> --package <pkg> --pitch <mm> --capacity <n> [--loaded <n>]
  osmt feeder remove <file> --id <id>
  osmt feeder report <file>
  osmt profile set <file> --speed <mm/min> --liquidus <c> --zones \"name:length:temp;...\"
  osmt validate <file>
  osmt placement <file> [--out program.csv] [--optimize]
  osmt feeders <file> [--out setup.csv]
  osmt balance <file> --machines <n> [--optimize]
  osmt schedule dual <file> [--strategy greedy|lpt] [--improve]
  osmt schedule lanes <file> --count <n> [--strategy greedy|lpt] [--improve]
  osmt reflow <file>
  osmt export <file> --format program|feeders|json|md --out <path>
  osmt report <file>
  osmt capabilities
  osmt version

Exit codes: 0 success, 1 input/validation error, 2 capability not implemented
";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            process::exit(error.exit_code());
        }
    }
}

fn run(args: &[String]) -> Result<i32, SmtError> {
    let Some(command) = args.first().map(String::as_str)
    else {
        println!("{USAGE}");
        return Ok(0);
    };
    match command {
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        "version" => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        "capabilities" => {
            println!("{}", OpenSmt::new().capabilities());
            Ok(0)
        }
        "create" => {
            let file = positional(args, 1, "create <file>")?;
            print_json(&OpenSmt::new().create(Path::new(file))?)
        }
        "import" => import_command(args),
        "board" => {
            let file = positional(args, 1, "board <file>")?;
            let values = Options::parse(&args[2..], &["--width", "--depth", "--thickness"], &[])?;
            let board = Board::new(values.number("--width")?, values.number("--depth")?, values.number("--thickness")?);
            print_json(&session(file)?.set_board(board)?)
        }
        "panel" => panel_command(args),
        "component" => component_command(args),
        "feeder" => feeder_command(args),
        "profile" => profile_command(args),
        "validate" => {
            let file = positional(args, 1, "validate <file>")?;
            print_json(&session(file)?.validate()?)
        }
        "placement" => {
            let file = positional(args, 1, "placement <file>")?;
            let values = Options::parse(&args[2..], &["--out"], &["--optimize"])?;
            let session = session(file)?;
            let report = session.placement_with(values.flag("--optimize"))?;
            if let Some(target) = values.value("--out") {
                smt_model::write_atomic(
                    Path::new(target),
                    &serde_json::to_string_pretty(&report)
                        .map_err(|error| SmtError::new("document/serialize", error.to_string()))?,
                )?;
            }
            print_json(&report)
        }
        "feeders" => {
            let file = positional(args, 1, "feeders <file>")?;
            let values = Options::parse(&args[2..], &["--out"], &[])?;
            let session = session(file)?;
            let report = session.feeders()?;
            if let Some(target) = values.value("--out") {
                smt_model::write_atomic(Path::new(target), &smt_model::feeder_csv(&report))?;
            }
            print_json(&report)
        }
        "balance" => {
            let file = positional(args, 1, "balance <file> --machines <n>")?;
            let values = Options::parse(&args[2..], &["--machines"], &["--optimize"])?;
            let report = session(file)?.balance_with(values.integer("--machines")?, values.flag("--optimize"))?;
            print_balance(&report);
            Ok(0)
        }
        "schedule" => schedule_command(args),
        "reflow" => {
            let file = positional(args, 1, "reflow <file>")?;
            print_json(&session(file)?.reflow()?)
        }
        "export" => {
            let file = positional(args, 1, "export <file> --format <format> --out <path>")?;
            let values = Options::parse(&args[2..], &["--format", "--out"], &[])?;
            let format: ExportFormat = values.value("--format").unwrap_or("program").parse()?;
            let target = values.require("--out")?;
            print_json(&session(file)?.export(format, Path::new(target))?)
        }
        "report" => {
            let file = positional(args, 1, "report <file>")?;
            print_json(&session(file)?.report()?)
        }
        other => Err(SmtError::new("cli/unknown-command", format!("unknown command `{other}`"))),
    }
}

fn import_command(args: &[String]) -> Result<i32, SmtError> {
    if args.get(1).map(String::as_str) == Some("csv") {
        let source = positional(args, 2, "import csv <file.csv> --out <file.osmt> [--side top|bottom]")?;
        let values = Options::parse(&args[3..], &["--out", "--side"], &[])?;
        let target = values.require("--out")?;
        let default_side = match values.value("--side") {
            Some(raw) => raw.parse::<Side>()?,
            None => Side::Top,
        };
        return print_json(&OpenSmt::new().import_csv(Path::new(source), Path::new(target), default_side)?);
    }
    let file = positional(args, 1, "import <file.kicad_pcb> --out <file.osmt>")?;
    let values = Options::parse(&args[2..], &["--out"], &[])?;
    let target = values.require("--out")?;
    print_json(&OpenSmt::new().import(Path::new(file), Path::new(target))?)
}

fn panel_command(args: &[String]) -> Result<i32, SmtError> {
    if args.get(1).map(String::as_str) == Some("fit") {
        let file = positional(args, 2, "panel fit <file> --max-width <mm> --max-depth <mm> [--gap-x <mm>] [--gap-y <mm>]")?;
        let values = Options::parse(&args[3..], &["--max-width", "--max-depth", "--gap-x", "--gap-y"], &[])?;
        let report = session(file)?.panel_fit(
            values.number("--max-width")?,
            values.number("--max-depth")?,
            values.optional_number("--gap-x")?,
            values.optional_number("--gap-y")?,
        )?;
        return print_json(&report);
    }
    let file = positional(args, 1, "panel <file>")?;
    let values = Options::parse(&args[2..], &["--rows", "--columns", "--gap-x", "--gap-y"], &[])?;
    let panel = Panel::new(
        values.integer("--rows")?,
        values.integer("--columns")?,
        values.number_or("--gap-x", 0.0)?,
        values.number_or("--gap-y", 0.0)?,
    );
    print_json(&session(file)?.set_panel(panel)?)
}

fn component_command(args: &[String]) -> Result<i32, SmtError> {
    let action = args.get(1).map(String::as_str).unwrap_or("");
    match action {
        "add" => {
            let file = positional(args, 2, "component add <file>")?;
            let values =
                Options::parse(&args[3..], &["--refdes", "--package", "--x", "--y", "--side", "--rotation", "--height"], &[])?;
            let mut component = Component::new(
                values.require("--refdes")?,
                values.require("--package")?,
                values.number("--x")?,
                values.number("--y")?,
            );
            if let Some(side) = values.value("--side") {
                component.side = side.parse::<Side>()?;
            }
            component.rotation = values.number_or("--rotation", 0.0)?;
            component.height = values.optional_number("--height")?;
            print_json(&session(file)?.add_component(component)?)
        }
        "remove" => {
            let file = positional(args, 2, "component remove <file>")?;
            let values = Options::parse(&args[3..], &["--refdes"], &[])?;
            print_json(&session(file)?.remove_component(values.require("--refdes")?)?)
        }
        _ => Err(SmtError::new("cli/missing-argument", "usage: osmt component add|remove <file> ...")),
    }
}

fn feeder_command(args: &[String]) -> Result<i32, SmtError> {
    let action = args.get(1).map(String::as_str).unwrap_or("");
    match action {
        "add" => {
            let file = positional(args, 2, "feeder add <file>")?;
            let values = Options::parse(&args[3..], &["--id", "--package", "--pitch", "--capacity", "--loaded"], &[])?;
            let capacity = values.integer("--capacity")?;
            let mut feeder =
                Feeder::new(values.require("--id")?, values.require("--package")?, values.number("--pitch")?, capacity);
            if let Some(loaded) = values.optional_integer("--loaded")? {
                feeder.loaded = loaded;
            }
            print_json(&session(file)?.add_feeder(feeder)?)
        }
        "remove" => {
            let file = positional(args, 2, "feeder remove <file>")?;
            let values = Options::parse(&args[3..], &["--id"], &[])?;
            print_json(&session(file)?.remove_feeder(values.require("--id")?)?)
        }
        "report" => {
            let file = positional(args, 2, "feeder report <file>")?;
            print_json(&session(file)?.feeder_report()?)
        }
        _ => Err(SmtError::new("cli/missing-argument", "usage: osmt feeder add|remove|report <file> ...")),
    }
}

fn profile_command(args: &[String]) -> Result<i32, SmtError> {
    if args.get(1).map(String::as_str) != Some("set") {
        return Err(SmtError::new("cli/missing-argument", "usage: osmt profile set <file> ..."));
    }
    let file = positional(args, 2, "profile set <file>")?;
    let values = Options::parse(&args[3..], &["--speed", "--liquidus", "--zones"], &[])?;
    let zones = parse_zones(values.require("--zones")?)?;
    let profile = ReflowProfile { conveyor_speed: values.number("--speed")?, liquidus: values.number("--liquidus")?, zones };
    print_json(&session(file)?.set_profile(profile)?)
}

fn schedule_command(args: &[String]) -> Result<i32, SmtError> {
    match args.get(1).map(String::as_str) {
        Some("dual") => {
            let file = positional(args, 2, "schedule dual <file>")?;
            let values = Options::parse(&args[3..], &["--strategy"], &["--improve"])?;
            let strategy = match values.value("--strategy") {
                Some(raw) => raw.parse::<DualLaneStrategy>()?,
                None => DualLaneStrategy::Greedy,
            };
            print_json(&session(file)?.dual_lane_with(strategy, values.flag("--improve"))?)
        }
        Some("lanes") => {
            let file = positional(args, 2, "schedule lanes <file> --count <n>")?;
            let values = Options::parse(&args[3..], &["--count", "--strategy"], &["--improve"])?;
            let strategy = match values.value("--strategy") {
                Some(raw) => raw.parse::<DualLaneStrategy>()?,
                None => DualLaneStrategy::Greedy,
            };
            let report = session(file)?.lanes(values.integer("--count")?, strategy, values.flag("--improve"))?;
            print_json(&report)
        }
        _ => Err(SmtError::new("cli/missing-argument", "usage: osmt schedule dual|lanes <file> ...")),
    }
}

fn parse_zones(raw: &str) -> Result<Vec<Zone>, SmtError> {
    let mut zones = Vec::new();
    for entry in raw.split(';').filter(|entry| !entry.trim().is_empty()) {
        let parts: Vec<&str> = entry.split(':').collect();
        if parts.len() != 3 {
            return Err(SmtError::new("cli/invalid-option", format!("zone `{entry}` must be `name:length:temperature`")));
        }
        zones.push(Zone::new(
            parts[0].trim(),
            parse_number("--zones", parts[1].trim())?,
            parse_number("--zones", parts[2].trim())?,
        ));
    }
    Ok(zones)
}

fn session(file: &str) -> Result<OpenSmt, SmtError> {
    let mut session = OpenSmt::new();
    session.open(Path::new(file))?;
    Ok(session)
}

fn positional<'a>(args: &'a [String], index: usize, usage: &str) -> Result<&'a str, SmtError> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| SmtError::new("cli/missing-argument", format!("missing argument; usage: osmt {usage}")))
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<i32, SmtError> {
    let text = serde_json::to_string_pretty(value).map_err(|error| SmtError::new("document/serialize", error.to_string()))?;
    println!("{text}");
    Ok(0)
}

fn print_balance(report: &BalanceReport) {
    let components: usize = report.machines.iter().map(|machine| machine.components.len()).sum();
    println!("Line balance: {} machines, {} components", report.machines.len(), components);
    println!(
        "total {} s, max {} s, min {} s, imbalance {:.1}%",
        number(report.total_seconds),
        number(report.max_seconds),
        number(report.min_seconds),
        report.imbalance * 100.0
    );
    for machine in &report.machines {
        println!(
            "machine {}: {} components, {} mm, {} s",
            machine.index,
            machine.components.len(),
            number(machine.travel),
            number(machine.seconds)
        );
        if !machine.components.is_empty() {
            println!("  {}", machine.components.join(", "));
        }
        if !machine.feeders.is_empty() {
            let feeders: Vec<String> = machine
                .feeders
                .iter()
                .map(|feeder| {
                    let state = if feeder.ok { "ok" } else { "short" };
                    format!("{} {}/{} {state}", feeder.id, feeder.assigned, feeder.loaded)
                })
                .collect();
            println!("  feeders: {}", feeders.join(", "));
        }
    }
    println!("coverage: {}", if report.ok { "ok" } else { "short" });
}

fn number(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded.abs() < 1e-9 { "0".to_string() } else { format!("{rounded}") }
}

struct Options {
    map: HashMap<String, String>,
}

impl Options {
    fn parse(args: &[String], values: &[&str], flags: &[&str]) -> Result<Self, SmtError> {
        let mut map = HashMap::new();
        let mut index = 0;
        while index < args.len() {
            let argument = args[index].as_str();
            if flags.contains(&argument) {
                map.insert(argument.to_string(), String::new());
                index += 1;
            }
            else if values.contains(&argument) {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| SmtError::new("cli/missing-value", format!("option `{argument}` needs a value")))?;
                map.insert(argument.to_string(), value.clone());
                index += 2;
            }
            else {
                return Err(SmtError::new("cli/unknown-option", format!("unknown option `{argument}`")));
            }
        }
        Ok(Self { map })
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(String::as_str)
    }

    fn flag(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    fn require(&self, name: &str) -> Result<&str, SmtError> {
        self.value(name).ok_or_else(|| SmtError::new("cli/missing-option", format!("option `{name}` is required")))
    }

    fn number(&self, name: &str) -> Result<f64, SmtError> {
        parse_number(name, self.require(name)?)
    }

    fn optional_number(&self, name: &str) -> Result<Option<f64>, SmtError> {
        self.value(name).map(|raw| parse_number(name, raw)).transpose()
    }

    fn number_or(&self, name: &str, fallback: f64) -> Result<f64, SmtError> {
        Ok(self.optional_number(name)?.unwrap_or(fallback))
    }

    fn integer(&self, name: &str) -> Result<u32, SmtError> {
        let raw = self.require(name)?;
        raw.parse::<u32>()
            .map_err(|_| SmtError::new("cli/invalid-number", format!("option `{name}` must be a non-negative integer")))
    }

    fn optional_integer(&self, name: &str) -> Result<Option<u32>, SmtError> {
        match self.value(name) {
            None => Ok(None),
            Some(raw) => raw
                .parse::<u32>()
                .map(Some)
                .map_err(|_| SmtError::new("cli/invalid-number", format!("option `{name}` must be a non-negative integer"))),
        }
    }
}

fn parse_number(name: &str, raw: &str) -> Result<f64, SmtError> {
    raw.parse::<f64>().map_err(|_| SmtError::new("cli/invalid-number", format!("option `{name}` is not a number: `{raw}`")))
}
