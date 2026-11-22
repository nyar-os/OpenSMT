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

## component add

```shell
open-smt component add <file> --refdes <R1> --package <pkg> --x <x> --y <y>
                     [--side top|bottom] [--rotation <deg>] [--height <h>]
```

`--side` defaults to `top` and `--rotation` to `0`. A duplicate refdes reports
`component/duplicate-refdes`; a position outside the board reports
`component/outside-board`.

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

## feeder remove

```shell
open-smt feeder remove <file> --id <id>
```

The only feeder of a package that has components reports `feeder/in-use`.

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

`kicad`, `altium` and `odb` are declared but unimplemented and exit with code
`2`.

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
open-smt board panel.osmt --width 120 --depth 90 --thickness 1.6
open-smt panel panel.osmt --rows 2 --columns 2 --gap-x 5 --gap-y 5
open-smt component add panel.osmt --refdes R1 --package 0603 --x 10 --y 10
open-smt component add panel.osmt --refdes U1 --package QFN-32 --x 50 --y 40 --side bottom
open-smt feeder add panel.osmt --id F1 --package 0603 --pitch 4 --capacity 500
open-smt feeder add panel.osmt --id F2 --package QFN-32 --pitch 12 --capacity 50
open-smt validate panel.osmt
open-smt placement panel.osmt
open-smt reflow panel.osmt
open-smt export panel.osmt --format program --out program.csv
```
