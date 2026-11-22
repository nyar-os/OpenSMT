# @wae/open-smt

Deterministic SMT placement programs, feeder setup sheets and reflow evaluation,
exposed as a CLI and a typed SDK over one native core.

## Install

```shell
pnpm add @wae/open-smt
```

The package resolves a per-platform native addon. If the addon is not installed,
calls report `capability/not-implemented` instead of returning a fabricated
result.

## CLI

```text
open-smt create <file.osmt>
open-smt import <file.kicad_pcb> --out <file.osmt>
open-smt board <file> --width <w> --depth <d> --thickness <t>
open-smt panel <file> --rows <r> --columns <c> [--gap-x <g>] [--gap-y <g>]
open-smt component add <file> --refdes <R1> --package <pkg> --x <x> --y <y>
                     [--side top|bottom] [--rotation <deg>] [--height <h>]
open-smt component remove <file> --refdes <R1>
open-smt feeder add <file> --id <id> --package <pkg> --pitch <mm> --capacity <n>
                     [--loaded <n>]
open-smt feeder remove <file> --id <id>
open-smt profile set <file> --speed <mm/min> --liquidus <c>
                     --zones "name:length:temp;..."
open-smt validate <file>
open-smt placement <file> [--out program.json] [--optimize]
open-smt feeders <file> [--out setup.csv]
open-smt balance <file> --machines <n> [--optimize]
open-smt reflow <file>
open-smt export <file> --format program|feeders|json|md --out <path>
open-smt report <file>
```

A complete job:

```shell
open-smt create panel.osmt
open-smt component add panel.osmt --refdes R1 --package 0603 --x 10 --y 10
open-smt component add panel.osmt --refdes C1 --package 0603 --x 30 --y 20
open-smt feeder add panel.osmt --id F1 --package 0603 --pitch 4 --capacity 500
open-smt validate panel.osmt
open-smt placement panel.osmt
open-smt reflow panel.osmt
open-smt export panel.osmt --format program --out program.csv
```

`import` reads a KiCad `.kicad_pcb` file and writes a new `.osmt` document. It
imports footprints on `F.Cu`/`B.Cu` and a rectangular `Edge.Cuts` outline only,
translates coordinates so the outline corner is the origin, and returns a report
with `componentCount`, `board` and a `losses` ledger of skipped construct
categories. An existing `--out` file reports `file/exists`; a missing or
non-rectangular outline reports `import/outline-unsupported`.

`placement` prints the program and can write it as JSON; `feeders` prints the
allocation and can write a CSV setup sheet; `balance` prints a human-readable
multi-machine balance for 1–64 machines. `--optimize` runs a deterministic
2-opt pass per side (placement) or per machine (balance); the placement report
then adds `optimized` and `travelSaved`. Exit code `0` is success, `1` is
invalid input, and `2` is an unimplemented capability.

## SDK

```ts
import { OpenSMT } from '@wae/open-smt';

const smt = new OpenSMT();
const document = await smt.create('panel.osmt');

await document.setBoard({ width: 120, depth: 90, thickness: 1.6 });
await document.setPanel({ rows: 2, columns: 2, gapX: 5, gapY: 5 });
await document.addComponent({ refdes: 'R1', package: '0603', x: 10, y: 10 });
await document.addComponent({ refdes: 'U1', package: 'QFN-32', side: 'bottom', x: 50, y: 40 });
await document.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 500 });
await document.addFeeder({ id: 'F2', package: 'QFN-32', pitch: 12, capacity: 50, loaded: 20 });

const report = await document.report();
console.log(report.placement.entries.length, report.reflow.timeAboveLiquidus);

const optimized = await document.placement(true);
console.log(optimized.travelSaved);

const balance = await document.balance(2);
console.log(balance.maxSeconds, balance.imbalance);

await document.exportDocument('md', 'report.md');
await smt.dispose();
```

Open an existing file with `await smt.open('panel.osmt')`. Editing methods return
the summary as written to disk.

## Document model

| Field | Meaning |
|-------|---------|
| `board` | width, depth and thickness in millimetres |
| `panel` | rows, columns and gaps; `1 × 1` is a single board |
| `components` | refdes, package, side, x, y, rotation and optional height |
| `feeders` | id, package, pitch, capacity and loaded quantity |
| `profile` | conveyor speed, liquidus and zones |

Component coverage is checked when the program is generated: `validate`,
`placement` and `report` report `feeder/missing` or `feeder/capacity` until the
loaded lanes cover every package. Editing can save an incomplete feeder setup,
because a component list is normally entered before the feeder table.

## Constraints

- Units are millimetres; temperatures are degrees Celsius; no conversion.
- Placement defaults to a deterministic nearest-neighbour sequence; `--optimize`
  adds a bounded 2-opt local search. Neither mode is a production optimiser or
  guarantees an optimal tour.
- Line balancing assigns whole components to 1–64 machines; it does not emit
  vendor machine programs, model dual-lane conveyors or line changeover time.
- Vendor machine programs and paste apertures are not implemented.
- `validate()` is the support claim for machineability.

## License

MPL-2.0.
