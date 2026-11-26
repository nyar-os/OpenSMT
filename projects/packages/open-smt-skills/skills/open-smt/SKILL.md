---
name: open-smt
description: Prepare and review SMT builds with OpenSMT. Use when a task involves an `.osmt` document, placement programs, feeder setup and consumption, line balance and takt, stencil apertures and paste volume, reflow profiles, first-article sheets, or placement, feeder and process drawings.
---

# open-smt

## Status

The SMT planning core is executable. The CLI and SDK create, import, edit and
reopen one `.osmt` document, allocate feeder lanes, generate a deterministic
placement program, estimate cycle time and line balance, check clearance and
reflow, and export CSV, Markdown, SVG and PDF sheets. Units are millimetres and
degrees Celsius and are never converted.

Documents can be created empty, imported from a KiCad `.kicad_pcb` (only
`F.Cu`/`B.Cu` footprints and one axis-aligned rectangle outline) or from a
generic placement CSV. `kicad`, `altium` and `odb` are declared export formats
but unimplemented and exit with code `2`. Vendor machine dialects (Fuji,
Yamaha, Juki, ASM) are not implemented; `machine export` writes a neutral CSV
program only.

## Executable commands

```bash
open-smt capabilities
open-smt create <file.osmt>
open-smt import <file.kicad_pcb> --out <file.osmt>
open-smt import csv <file.csv> --out <file.osmt> [--side top|bottom]
open-smt validate <file>
open-smt report <file>

open-smt board <file> --width <w> --depth <d> --thickness <t>
open-smt panel <file> --rows <r> --columns <c> [--gap-x <g>] [--gap-y <g>]
open-smt panel fit <file> --max-width <mm> --max-depth <mm> [--gap-x <mm>] [--gap-y <mm>]
open-smt panel map <file> [--out <file>]

open-smt component add <file> --refdes <R1> --package <pkg> --x <x> --y <y> [--side top|bottom] [--rotation <deg>] [--height <h>]
open-smt component update <file> --refdes <R1> [--package <pkg>] [--x <x>] [--y <y>] [--rotation <deg>] [--height <h>] [--clear-height]
open-smt component remove <file> --refdes <R1>
open-smt component list <file>

open-smt feeder add <file> --id <id> --package <pkg> --pitch <mm> --capacity <n> [--loaded <n>] [--leader-waste <n>] [--trailer-waste <n>] [--scrap <n>]
open-smt feeder update <file> --id <id> [--package <pkg>] [--pitch <mm>] [--capacity <n>] [--loaded <n>]
open-smt feeder splice <file> --id <id> --sequence <n> [--leader-waste <n>] [--trailer-waste <n>] [--scrap <n>]
open-smt feeder scrap <file> --id <id> [--leader-waste <n>] [--trailer-waste <n>] [--scrap <n>]
open-smt feeder remove <file> --id <id>
open-smt feeder report <file>
open-smt feeder optimize <file> [--slot-pitch <mm>]
open-smt feeder consumption <file> --boards <n>
open-smt feeder schedule <file> --boards <n> [--out <file>]
open-smt feeders <file> [--out <file>]

open-smt bom check <file> --bom <bom.csv>
open-smt quality first-article <file>
open-smt profile set <file> --speed <mm/min> --liquidus <c> --zones <zones>

open-smt placement <file> [--out <file>] [--optimize]
open-smt stencil <file> [--out <file>]
open-smt stencil paste <file> --boards <n> [--foil <mm>] [--waste <pct>] [--out <file>] [--csv <file>]
open-smt clearance <file> [--min-gap <mm>] [--max-height <mm>]
open-smt balance <file> --machines <n> [--optimize]
open-smt schedule dual <file> [--strategy greedy|lpt] [--improve]
open-smt schedule lanes <file> --count <n> [--strategy greedy|lpt] [--improve]
open-smt reflow <file>
open-smt reflow window <file> [--peak-min <c>] [--peak-max <c>] [--tal-min <s>] [--tal-max <s>] [--max-ramp <c/s>] [--max-cooling <c/s>]
open-smt takt <file> [--place-seconds <s>] [--head-speed <mm/s>] [--machines <n>] [--optimize]
open-smt takt heatmap <file>
open-smt pick-cycle <file> [--heads <n>]
open-smt machine export <file> --out <file.csv> [--nozzles <map>]
open-smt machine verify <file> --program <file.csv> [--nozzles <map>]
open-smt changeover <current.osmt> <next.osmt> [--feeder-change-seconds <s>]

open-smt export <file> --format <format> --out <path>
```

Exit codes: `0` success, `1` invalid input, `2` the capability is not
implemented.

## Preflight

1. Run `open-smt capabilities` and confirm the report or export the task needs
   is supported. A command name or a file extension is not a capability.
2. Run `open-smt validate <file>` and `open-smt report <file>`; confirm the
   components, references and feeder lanes the task will address exist.
3. If a feeder lane is missing or short, report `feeder/missing` or
   `feeder/capacity` instead of assuming the build is ready.

## Routes

| Route | Intent | Minimum evidence |
|-------|--------|------------------|
| prepare | Create or import a document and place components | `open-smt validate <file>` plus `open-smt component list <file>` |
| setup | Declare feeders and confirm coverage | `open-smt feeder report <file>` and the allocation report |
| plan | Produce a placement program and cycle-time estimate | the program JSON and its line count |
| review | Check clearance, reflow, paste and first article | the report with its findings and `ok` flag |
| deliver | Write drawings, tables or a machine program | the exported file and the same revision report |

## Board, panel and components

- `board` replaces the stock; all three dimensions must be positive.
- `panel` stores the array; `panel fit` and `panel map` are read-only.
  `panel fit` counts `floor((max + gap) / (size + gap))` per axis.
- `component add` rejects a duplicate reference designator and any position
  outside the board. `component update` never changes the reference designator,
  normalises rotation to `[0, 360)` and requires at least one field.
- The board is never resized automatically: resize it before placing parts
  outside the current outline.

## Feeders and consumption

- A feeder lane declares `package`, tape `pitch` and `capacity`; `--loaded`
  defaults to the capacity, and waste defaults to `0`.
- Allocation is derived, never stored: `feeder report`, `feeders`, `placement`,
  `quality first-article`, `takt` and the drawings all recompute it from the
  document.
- `feeder splice --sequence <n>` joins a new reel at a 1-based placement step.
  Consumption keeps `picks` **net** and `gross = picks + scrap`; scrap is never
  folded into picks.
- `feeder consumption <file> --boards <n>` and
  `open-smt feeder schedule <file> --boards <n> --out <file>` report short lanes
  with `short` and `ok: false` instead of failing.
- Slot numbers in `feeder report` and `feeder optimize` are suggestions; they are
  not a physical machine slot layout, and the assignment is a heuristic.

## Quality and process checks

- `bom check` compares the placed references with a BOM CSV and reports
  `missing`, `extra` and per-package quantity differences; it edits nothing.
- `clearance` compares rotated bodies as axis-aligned bounding boxes with
  defaults `--min-gap 0.5` and `--max-height 12`; its findings are warnings and
  never fail the document.
- `stencil` derives apertures from a fixed package rules table and lists an
  unrecognised package under `unmatchedPackages`. `stencil paste` multiplies
  aperture area by foil thickness and adds the waste percentage.
- `reflow` evaluates the stored linear-ramp profile; `reflow window` adds peak,
  time-above-liquidus and ramp checks in a fixed order.
- `balance` prints a human-readable line balance (1–64 machines). `takt`,
  `takt heatmap` and `pick-cycle` are read-only JSON estimates; `--optimize`
  runs a deterministic 2-opt pass and never changes the document.
- `changeover` compares two documents by feeder id and reports `remove`, `swap`
  and `add` steps; it edits neither document.

## Placement and machine export

- `placement <file> --out <file>` runs the nearest-neighbour program in
  execution order. With `--optimize` the report adds `optimized` and
  `travelSaved`; without it the output is byte-identical on repeat.
- `machine export` writes a neutral CSV and reads it back; `machine verify`
  checks a file field by field. Use the same `--nozzles` map for both.
- Vendor dialects remain unimplemented; never present the neutral CSV as a
  Fuji, Yamaha, Juki or ASM program.

## Exports

- `export --format program|feeders|fai|json|md|svg|pdf` writes the program,
  feeder, first-article, report, summary and placement sheets.
- `stencil`, `stencil-svg`, `stencil-pdf`, `feeder-svg`, `sequence-svg`,
  `sequence-pdf`, `reflow-svg`, `panel-svg`, `heatmap-svg`, `balance-svg`,
  `setup-svg`, `setup-pdf`, `pick-cycle-svg` and `consumption-svg` write the
  drawings and tables.
- SVG sheets are deterministic and byte-identical on repeat; labels are
  ASCII-only and non-ASCII text is reported as a loss. PDF maps one A4 landscape
  page with Helvetica.
- The `fai` CSV leaves `placement_ok`, `orientation_ok`, `package_ok` and
  `feeder_ok` empty for the inspector; the sheet holds expected values only and
  no inspection result.

## Hard rules

- Never claim a build is ready when `validate` reports `feeder/missing` or
  `feeder/capacity`.
- Never present a heuristic (balance, station order, takt, pick cycle) as an
  optimum or a measured machine result.
- Never claim vendor machine support or a KiCad/Altium/ODB export.
- Keep one model: every report and drawing must come from the document revision
  the command read.

## Safety

- Confirm the target path with the user before any command that creates or
  rewrites a document; every editing command rewrites it in place.
- Never overwrite an input document unless the task asks for an in-place edit.
- Fabrication inputs are manufacturing data: state exactly which program, sheet
  or drawing was exported and which checks passed, failed or were skipped.
