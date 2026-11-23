# Command line

`open-smt` is the packaged CLI. The Rust binary `osmt` accepts the same commands
for development. Both print the same JSON, except `balance`, which prints the
same human-readable summary.

```text
open-smt <command> [options]
```

Exit codes: `0` success, `1` invalid input, `2` the capability is not
implemented.

## create

```shell
open-smt create <file.osmt>
```

Writes a document with `open-smt` format version 1, units `mm`, revision 0, a
100 × 80 × 1.6 mm board, a 1 × 1 panel, no components, no feeders and a
four-zone reflow profile (600 mm/min, liquidus 217 °C). An existing file is
rejected with `file/exists`.

## import

```shell
open-smt import <file.kicad_pcb> --out <file.osmt>
```

Reads a KiCad PCB and writes a new millimetre document. `--out` must not exist
yet; an existing target reports `file/exists`. The JSON import report contains
`format` (`kicad_pcb`), `losses` (one `code`/`count` entry per skipped construct
category), `componentCount` and `board`.

Only footprints on `F.Cu`/`B.Cu` and straight `Edge.Cuts` lines that close one
axis-aligned rectangle are imported. The reference designator comes from
`(property "Reference" ...)` or `(fp_text reference ...)`; footprints without
one, or with a repeated one, are skipped and counted in `losses`. Component
coordinates are translated so the outline corner is the origin, rotation is
normalised to `[0, 360)` and height stays `null`. A missing or non-rectangular
outline reports `import/outline-unsupported`, and a file that is not a
`kicad_pcb` S-expression reports `import/parse`. The loss codes are listed in
[capabilities.md](capabilities.md).

### import csv

```shell
open-smt import csv <file.csv> --out <file.osmt> [--side top|bottom]
```

Reads a generic placement table into a new millimetre document. The first
non-empty line is the header and must declare `refdes`, `package`, `x` and `y`;
`rotation` and `side` are optional. Column names are case-insensitive and may
appear in any order, fields may be double-quoted (including commas and `""`),
and empty lines are skipped. Rows become components in source order.

`x` and `y` are millimetres and are not converted or translated.
`--side` (default `top`) applies to rows without a `side` value; a row's own
`side` wins and accepts `top`/`bottom` in any case. Rotation defaults to `0` and
is normalised to `[0, 360)`. The board is sized to the largest coordinate, never
smaller than the default 100 × 80 mm, with 1.6 mm thickness. The import is one
edit, so the document is written with revision 1. An existing `--out` file
reports `file/exists`.

A missing or unknown column, a row with the wrong field count, a non-numeric
coordinate, an unknown side, an empty `refdes`/`package`, a repeated reference
designator or a negative coordinate reports an `import/...` code that names the
physical line. The JSON import report contains `format` (`csv`),
`componentCount`, `board` and an empty `losses` list.

## board

```shell
open-smt board <file> --width <w> --depth <d> --thickness <t>
```

All three values are required and must be positive.

## panel

```shell
open-smt panel <file> --rows <r> --columns <c> [--gap-x <g>] [--gap-y <g>]
```

`rows` and `columns` default to 1 and must be positive integers; gaps default to
0.

### panel fit

```shell
open-smt panel fit <file> --max-width <mm> --max-depth <mm> [--gap-x <mm>] [--gap-y <mm>]
```

Prints a read-only report of how many boards fit into the maximum panel size.
The stored panel array and the document are not changed. `--max-width` and
`--max-depth` are required and must be finite and positive. `--gap-x` and
`--gap-y` default to the stored panel gaps and must be finite and non-negative.
Invalid input reports `panel/invalid` and exits with code `1`.

The report contains `format` (`open-smt-panel-fit`), `version`, `units`, the
inputs `maxWidth`, `maxDepth`, `gapX`, `gapY`, the board `width` and `depth`,
and the result `rows`, `columns`, `boardCount`, `panelWidth`, `panelDepth`,
`fits`, `wastedWidth`, `wastedDepth` and `utilization`. Counts are
`floor((max + gap) / (size + gap))`, floored at zero. When no board fits,
`fits` is false and `wastedWidth`, `wastedDepth` and `utilization` are zero.

## component add

```shell
open-smt component add <file> --refdes <R1> --package <pkg> --x <x> --y <y>
                     [--side top|bottom] [--rotation <deg>] [--height <h>]
```

`--side` defaults to `top` and `--rotation` to `0`. A duplicate refdes reports
`component/duplicate-refdes`; a position outside the board reports
`component/outside-board`.

## component update

```shell
open-smt component update <file> --refdes <R1> [--package <pkg>]
                     [--side top|bottom] [--x <x>] [--y <y>]
                     [--rotation <deg>] [--height <h>] [--clear-height]
```

Applies a partial edit to one placed component and writes one revision. The
reference designator identifies the component and never changes. Every supplied
option replaces the stored value; omitted options keep theirs, so the command
must name at least one field. `--rotation` is normalised to `[0, 360)` (for
example `450` becomes `90`) and an omitted rotation is left unchanged.
`--height` sets the height and `--clear-height` sets it back to `null`; naming
both reports `component/invalid`, as does an empty edit. An unknown `--refdes`
reports `component/unknown-refdes`. The resulting document is checked against
the same board-bounds, finite-value, package and height rules as `component add`,
so an outside-board position reports `component/outside-board` and nothing is
written.

## component remove

```shell
open-smt component remove <file> --refdes <R1>
```

## feeder add

```shell
open-smt feeder add <file> --id <id> --package <pkg> --pitch <mm> --capacity <n>
                     [--loaded <n>]
```

`--loaded` defaults to `--capacity`. A loaded quantity larger than the capacity
is rejected with `feeder/invalid`.

## feeder update

```shell
open-smt feeder update <file> --id <id> [--package <pkg>] [--pitch <mm>]
                     [--capacity <n>] [--loaded <n>]
```

Applies a partial edit to one feeder and writes one revision. `--id` selects the
feeder and never changes. Every supplied option replaces the stored value and
omitted options keep theirs, so the command must name at least one field. An
empty edit, an empty package, a non-positive or non-finite pitch, a capacity
below 1 or a loaded quantity above the capacity reports `feeder/invalid`; an
unknown `--id` reports `feeder/unknown-id`. Lowering `--capacity` below the
current loaded quantity is rejected the same way as `feeder add` rejects
`--loaded` above `--capacity`. The edited document is checked with the same
feeder rules as `feeder add`, so a failed update writes nothing and keeps the
revision. Allocation and the placement program are recomputed from the document,
so a changed package is visible to the next `feeders` and `placement` call.

## feeder remove

```shell
open-smt feeder remove <file> --id <id>
```

The only feeder of a package that has components reports `feeder/in-use`.

## feeder report

```shell
open-smt feeder report <file>
```

Prints a read-only loading report aggregated by package. It does not read the
feeder setup (a document without feeders is accepted) and writes nothing. The
report contains `format` (`open-smt-feeder-report`), `version`, `totalPlacements`
and `uniquePackages`, plus one `slots` entry per package in ascending package
name order. Each slot is `slot` (1 to n), `package`, `count`, `refdes` in
document order and `sides` deduplicated and sorted. An empty document reports
zero placements, zero packages and no slots. Slot numbers are suggestions only:
they do not describe a physical machine slot or a tape layout.

## bom check

```shell
open-smt bom check <file> --bom <bom.csv>
```

Prints the read-only BOM cross-check as JSON. The BOM CSV declares `package`
and `quantity` per row and may declare a `;`-separated `refdes` list; column
names are case-insensitive, fields may be double-quoted and empty lines are
skipped. The report contains `format` (`open-smt-bom-check`), `version`,
`units`, `ok`, `bomLines`, `components`, `missing` (BOM reference designators
with no placement), `extra` (placed reference designators the BOM does not
declare) and `quantityMismatches` (per package: `bomQuantity`,
`placedQuantity` and `difference = placedQuantity - bomQuantity`). Findings
are ordered by package name, then by reference designator. A malformed BOM
reports a `bom/...` code naming the physical line. Neither the document nor
the BOM file is modified.

## profile set

```shell
open-smt profile set <file> --speed <mm/min> --liquidus <c> \
                     --zones "name:length:temp;..."
```

Each zone is `name:length:temperature`; zones are applied in the order given.

## validate

```shell
open-smt validate <file>
```

Prints `{ "ok": true|false, "issues": [...] }`. Each issue has `code`,
`severity` and `message`. `feeder/missing` and `feeder/capacity` stay errors
until the loaded lanes cover every package.

## placement

```shell
open-smt placement <file> [--out program.json] [--optimize]
```

Prints the placement program and optionally writes the same JSON. The report
contains `boardCount`, `panelWidth`, `panelDepth`, `travelLength`,
`placementSeconds` and one entry per component in execution order.

`--optimize` runs a deterministic 2-opt pass on each side's nearest-neighbour
route. The report then also contains `optimized: true` and `travelSaved` (the
millimetres saved, `0` when no reversal improves the route). Without the flag
the output is byte-for-byte the nearest-neighbour program and carries neither
field.

## feeders

```shell
open-smt feeders <file> [--out setup.csv]
```

Prints the feeder allocation and optionally writes a CSV setup sheet with id,
package, loaded, capacity, assigned, remaining and assigned refdes.

## balance

```shell
open-smt balance <file> --machines <n> [--optimize]
```

Prints a human-readable line balance: the machine count, total, maximum and
minimum seconds, the imbalance, and for each machine its components in
execution order, travel, estimated seconds and feeder copies as
`id assigned/loaded ok|short`. `n` must be an integer from 1 to 64; any other
value reports `balance/invalid-machines` and exits with code `1`. A feeder copy
that cannot cover its assignments is printed as `short` and sets the overall
`ok` to false instead of failing the command.

`--optimize` applies the same 2-opt pass to every machine independently.
Assignments, component counts, total and imbalance arithmetic do not change.

## schedule dual

```shell
open-smt schedule dual <file> [--strategy greedy|lpt] [--improve]
```

Prints the dual-lane schedule as JSON. Each placement step becomes one task with
a fixed time of `0.6 s + travel / 200 mm·s⁻¹` from the previous step of the same
side. `--strategy greedy` (the default) visits tasks in program order; `--strategy
lpt` visits them by decreasing task time, ties keeping program order. Both give
each task to the lane with the smaller accumulated time; a tie goes to the first
lane. `--improve` exchanges one task of each lane while the slower lane gets
faster, up to 64 rounds; a lane always keeps its tasks in program order. Any
other strategy reports `schedule/invalid-strategy` and exits with code `1`. The
report contains `strategy`, `lanes` (`index`, `tasks` in program order and
`seconds`), `singleLaneSeconds`, `makespan` and `balanceRatio`
(`makespan / singleLaneSeconds`, zero for an empty document). The assignment is a
heuristic and is not an optimal partition. `schedule dual` is the two-lane
special case and keeps this exact report shape.

## schedule lanes

```shell
open-smt schedule lanes <file> --count <n> [--strategy greedy|lpt] [--improve]
```

Prints a multi-lane schedule as JSON for 2 to 16 lanes. Task times and both
strategies follow `schedule dual`: `--strategy greedy` (the default) visits tasks
in program order, `--strategy lpt` visits them by decreasing task time with ties
in program order, and every task joins the lane with the smaller accumulated
time (a tie goes to the lowest lane index). `--improve` exchanges one task of
each lane pair while the slowest lane gets faster, up to 64 rounds, and each lane
always keeps its tasks in program order. A count outside 2–16 reports
`schedule/invalid-lanes`; any other strategy reports `schedule/invalid-strategy`.
Both exit with code `1`. The report contains `strategy`, `laneCount`, `lanes`
(`index`, `tasks` in program order and `seconds`), `singleLaneSeconds`,
`makespan` and `balanceRatio` (`makespan / singleLaneSeconds`, zero for an empty
document). The assignment is a heuristic and is not an optimal partition.

## reflow

```shell
open-smt reflow <file>
```

Prints peak temperature, total time, time above liquidus and one entry per zone
with entry/exit temperature and seconds above liquidus.

## export

```shell
open-smt export <file> --format program|feeders|json|md --out <path>
```

| Format | Content |
|--------|---------|
| `program` | placement program CSV, one row per step |
| `feeders` | feeder setup CSV, one row per lane |
| `json` | the full report |
| `md` | Markdown board, feeder, reflow and program tables |

These are export formats. `kicad`, `altium` and `odb` are still declared but
unimplemented and exit with code `2`. The separate `import` command reads a
`.kicad_pcb` source into a new document.

## report

```shell
open-smt report <file>
```

Prints the document summary, validation, placement, feeder allocation, reflow
and the program line count.

## capabilities

```shell
open-smt capabilities
```

Prints the machine-readable capability matrix.

## Example

```shell
open-smt create panel.osmt
open-smt import board.kicad_pcb --out imported.osmt
open-smt import csv placements.csv --out placements.osmt --side top
open-smt board panel.osmt --width 120 --depth 90 --thickness 1.6
open-smt panel panel.osmt --rows 2 --columns 2 --gap-x 5 --gap-y 5
open-smt panel fit panel.osmt --max-width 320 --max-depth 180 --gap-x 5 --gap-y 5
open-smt component add panel.osmt --refdes R1 --package 0603 --x 10 --y 10
open-smt component add panel.osmt --refdes U1 --package QFN-32 --x 50 --y 40 --side bottom
open-smt component update panel.osmt --refdes R1 --x 20 --y 15 --rotation 90
open-smt component update panel.osmt --refdes U1 --clear-height
open-smt feeder add panel.osmt --id F1 --package 0603 --pitch 4 --capacity 500
open-smt feeder add panel.osmt --id F2 --package QFN-32 --pitch 12 --capacity 50
open-smt feeder update panel.osmt --id F1 --loaded 300
open-smt feeder report panel.osmt
open-smt bom check panel.osmt --bom bom.csv
open-smt validate panel.osmt
open-smt placement panel.osmt
open-smt schedule dual panel.osmt
open-smt schedule lanes panel.osmt --count 3 --strategy lpt --improve
open-smt reflow panel.osmt
open-smt export panel.osmt --format program --out program.csv
```
