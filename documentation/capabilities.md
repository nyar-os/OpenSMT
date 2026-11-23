# Capabilities

This document is the support claim for OpenSMT. Anything not marked supported
here is not a support claim, even if an API name suggests it.

The machine-readable version is printed by `open-smt capabilities` and by
`OpenSMT.capabilities()`.

## Capability matrix

| Capability | Status |
|------------|--------|
| rectangular board in millimetres | ✅ supported |
| panel array with row/column gaps | ✅ supported |
| component list with side, rotation and height | ✅ supported |
| partial component update (in-place edit) | ✅ supported |
| feeder setup with package, pitch, capacity and loaded quantity | ✅ supported |
| partial feeder update (in-place edit) | ✅ supported |
| deterministic placement sequence | ✅ supported |
| optional 2-opt placement optimisation | ✅ supported |
| feeder allocation by id and loaded quantity | ✅ supported |
| feeder loading report (package counts and suggested slots) | ✅ supported |
| BOM cross-check (missing, extra and quantity findings) | ✅ supported |
| multi-feeder split of one package | ✅ supported |
| panel board count and panel dimensions | ✅ supported |
| panel fit report (boards fitting a maximum panel size) | ✅ supported |
| reflow zone report and peak temperature | ✅ supported |
| time above liquidus over linear ramps | ✅ supported |
| placement program CSV | ✅ supported |
| feeder setup CSV | ✅ supported |
| JSON / Markdown reports | ✅ supported |
| paste aperture / stencil generation | ❌ not implemented |
| vendor machine programs (Fuji, Yamaha, Juki, ASM) | ❌ not implemented |
| line balancing | ✅ supported |
| multi-lane scheduling (2–16 lanes, including the dual-lane report) | ✅ supported (greedy or LPT, optional swaps; not optimal) |
| cycle time and throughput estimate (takt) | ✅ supported (analytical machine model; not a measured rate) |
| feeder station assignment along the tape bank | ✅ supported (heuristic by mean placement X; not optimal) |
| feeder changeover sequence between two documents | ✅ supported (id/package/pitch comparison; fixed time per change) |
| KiCad PCB (`.kicad_pcb`) import | ✅ supported |
| CSV placement (`.csv`) import | ✅ supported |
| Altium / ODB import | ❌ not implemented |

## Algorithms

1. **Feeder allocation.** Components are sorted by reference designator. For each
   component, the feeder of the same package with the smallest id that still has
   remaining loaded parts is chosen; `remaining` starts at `loaded`. A package
   with no feeder reports `feeder/missing`; a package whose total loaded quantity
   is smaller than its component count reports `feeder/capacity`.
2. **Placement sequence.** The top side is processed before the bottom side.
   Each side starts at the machine origin `(0, 0)`. At every step the head visits
   the nearest remaining component; ties are broken by reference designator. The
   nearest-neighbour search is exact but not a production optimiser.
3. **Optional 2-opt.** With `optimize` enabled, every side's nearest-neighbour
   route is improved by 2-opt: the segment whose reversal saves the most
   Euclidean distance is reversed, repeating until no reversal improves by more
   than `1e-9` mm or 64 reversals have been applied. Ties keep the earliest
   segment, so the result is deterministic. 2-opt is a local search; it does not
   guarantee the shortest possible route. The route is per side and the origin
   remains first. `travelLength`, `placementSeconds` and the report's
   `travelSaved` (nearest neighbour minus 2-opt, zero when nothing improves)
   describe the optimized order. `optimized` appears only when the pass was
   requested.
4. **Travel and time.** Travel is the sum of straight-line distances inside each
   side, with a fresh origin per side. Estimated placement time is
   `components × 0.6 s + travel / 200 mm·s⁻¹`.
5. **Line balance.** `balance(machineCount)` accepts 1–64 machines and rejects
   anything else with `balance/invalid-machines`. Components are sorted by
   reference designator and assigned one at a time to the machine with the
   smallest estimated completion time; a tie goes to the lowest machine index.
   Every machine keeps a full feeder copy, starts each side at its own origin
   and is re-sequenced with the same nearest-neighbour rule and the same
   `0.6 s` per component plus `travel / 200 mm·s⁻¹` estimate. `imbalance` is
   `(max − min) / max`, or zero when the slowest machine takes zero seconds.
   Each feeder copy reports `assigned` and `loaded` and sets `ok: false` when
   the loaded quantity does not cover the assignments; the report is still
   produced and the top-level `ok` becomes false. With `optimize` enabled, each
   machine's own program is 2-opt improved independently; assignments, component
   counts, `totalSeconds` conservation and the `imbalance` formula are unchanged.
6. **Multi-lane schedule.** `planLanes` (CLI `schedule lanes --count <n>`)
   accepts 2–16 lanes and rejects anything else with `schedule/invalid-lanes`;
   `planDualLane` (CLI `schedule dual`) is the two-lane special case and keeps
   its report shape. Both derive one task per placement step in the same
   nearest-neighbour program order. A task's fixed time is `0.6 s + distance
   from the previous step of the same side / 200 mm·s⁻¹`; the times therefore
   sum to the single-lane placement estimate. The `greedy` strategy (default)
   visits tasks in program order and the `lpt` strategy visits them by
   decreasing task time, ties keeping program order. Both give each task to the
   lane with the smaller accumulated time; a tie goes to the lowest lane index.
   Each lane keeps its tasks in program order. `--improve` (or `improve: true`)
   then exchanges one task of each lane pair while the slowest lane gets faster,
   up to 64 rounds, and still reports each lane in program order. `makespan` is
   the slowest lane total and `balanceRatio` is `makespan / singleLaneSeconds`,
   or zero for an empty document. The report names the assignment in `strategy`
   and the lane count in `laneCount`. Both strategies and the swap pass are
   heuristics; none is an optimal multi-lane partition.
7. **Stored panel arithmetic.** The document's panel array reports
   `boardCount = rows × columns`,
   `panelWidth = columns × width + (columns − 1) × gapX`, and
   `panelDepth = rows × depth + (rows − 1) × gapY`.
8. **Reflow.** The board enters the first zone at 25 °C. Each zone ramps linearly
   from the previous set-point to its own set-point over
   `length / conveyor_speed × 60` seconds. Time above liquidus is integrated over
   each ramp, including partial crossings in either direction. Peak temperature
   is the highest set-point.
9. **Feeder loading report.** `feederReport` (CLI `feeder report <file>`)
   aggregates the placement list by package without reading or writing the
   feeder setup. Packages are visited in ascending name order and receive one
   suggested slot each, numbered from 1. A slot records the package, its
   placement count, its reference designators in document order and its distinct
   sides in sorted order. `totalPlacements` is the number of components and
   `uniquePackages` the number of slots. An empty document yields zero counts and
   no slots. Slot numbers are suggestions only; no physical machine slot, tape
   direction or pitch constraint is modelled.
10. **Panel fit report.** `panelFit` (CLI `panel fit <file> --max-width <mm>
    --max-depth <mm> [--gap-x <mm>] [--gap-y <mm>]`) reports how many boards fit
    into a maximum panel size without changing the document or its stored panel.
    `maxWidth` and `maxDepth` must be finite and positive. Gaps default to the
    stored panel gaps and must be finite and non-negative when supplied;
    invalid input reports `panel/invalid`. The counts are
    `columns = floor((maxWidth + gapX) / (width + gapX))` and
    `rows = floor((maxDepth + gapY) / (depth + gapY))`, each floored at zero,
    with `boardCount = rows × columns` and
    `panelWidth = columns × width + (columns − 1) × gapX`,
    `panelDepth = rows × depth + (rows − 1) × gapY` (both zero when the matching
    count is zero). `fits` is `boardCount > 0`. When a board fits,
    `wastedWidth = maxWidth − panelWidth`, `wastedDepth = maxDepth − panelDepth`
    and `utilization = panelWidth × panelDepth / (maxWidth × maxDepth)`;
    otherwise all three are zero.
11. **Component update.** `updateComponent` (CLI
    `component update <file> --refdes <R1> ...`) applies a partial edit to one
    placed component and saves one revision. The reference designator identifies
    the component and never changes. At least one of `package`, `side`, `x`, `y`,
    `rotation`, `height` or `clearHeight` must be supplied; omitted fields keep
    their value. A supplied `rotation` is normalised to `[0, 360)` (for example
    `450` becomes `90`), an omitted rotation is left unchanged, and `clearHeight`
    sets the height back to `null`. An empty request, or `height` together with
    `clearHeight`, reports `component/invalid`; an unknown reference designator
    reports `component/unknown-refdes`. The edited document is validated with the
    same board-bounds, finite-value, side, rotation, package and height rules as
    `component add`, so a failed update writes nothing and keeps the revision.
12. **Feeder update.** `updateFeeder` (CLI `feeder update <file> --id <id> ...`)
    applies a partial edit to one feeder and saves one revision. The feeder id
    identifies the lane and never changes. At least one of `package`, `pitch`,
    `capacity` or `loaded` must be supplied; omitted fields keep their value. An
    empty request, an empty package, a non-positive or non-finite pitch, a zero
    capacity or a loaded quantity above the capacity reports `feeder/invalid`,
    so lowering `capacity` below the current `loaded` is rejected the same way
    as `feeder add` rejects `loaded > capacity`. An unknown id reports
    `feeder/unknown-id`. The edited document is validated with the same feeder
    rules as `feeder add`, so a failed update writes nothing and keeps the
    revision. Allocation and the placement program are recomputed from the
    edited document, so a changed package is reflected by the next
    `feeders`/`placement` call.
13. **BOM cross-check.** `bomCheck` (CLI `bom check <file> --bom <bom.csv>`)
    compares the placement list with a BOM CSV without changing either side.
    The header must declare `package` and `quantity` and may declare `refdes`
    (a `;`-separated list); names are case-insensitive, fields may be
    double-quoted and empty lines are skipped, exactly as in the CSV placement
    import. Unknown or duplicate columns, a wrong field count, an empty
    package, a non-integer or zero quantity and a repeated reference
    designator report a `bom/...` code that names the physical line. BOM rows
    are grouped by package name and their quantities summed; placements are
    grouped by package and counted. For every package in ascending name order
    the report lists the BOM reference designators with no placement
    (`missing`), the placed reference designators the BOM does not declare
    (`extra`) and the packages whose summed BOM quantity differs from the
    placed count (`quantityMismatches`, with `difference = placed − bom`).
    A reference-designator-level check only runs for packages whose BOM rows
    enumerate refdes; a BOM row without refdes is compared by quantity only,
    and a package absent from the BOM contributes its whole placement list to
    `extra`. Packages are matched by exact name; part-number aliases are not
    resolved. `ok` is true only when all three lists are empty. The findings
    are ordered by package name, then by reference designator, and the report
    is deterministic.

14. **Cycle time estimate.** `takt` (CLI `open-smt takt <file>`) runs the same
    feeder allocation and placement sequence as the placement program and
    applies a fixed machine model: `0.6` s per component, `200` mm/s head speed,
    `30` s per feeder change and `5` s per nozzle change, all overridable. The
    report contains `components`, `picks`, `travelMm` (identical to the
    program's `travelLength`), the `placeSeconds`, `travelSeconds` and
    `overheadSeconds` segments, `boardSeconds`, `machineSeconds`
    (`boardSeconds / machineCount`, evenly over 1–64 machines), `boardsPerHour`
    and `componentsPerHour`. Invalid machine parameters report `takt/invalid`.
    The estimate is read-only and never changes the document.

    ```shell
    open-smt takt panel.osmt --machines 2
    ```

15. **Feeder station assignment.** `feederOptimize` (CLI
    `feeder optimize <file> [--slot-pitch <mm>]`) runs the same feeder
    allocation and nearest-neighbour sequence as the placement program and
    orders the feeder lanes along the tape bank: every lane is placed at the
    slot closest to the mean placement X of the picks it supplies, ties broken
    by feeder id, and lanes the program never uses are appended in id order.
    Slots are numbered from 1 at `(slot − 1) × slotPitch`; `slotPitch` defaults
    to `16` mm and must be finite and positive, otherwise `station/invalid`.
    The travel model is the tape-bank axis only: the sum of the absolute
    slot-to-slot moves between consecutive picks, restarting from slot position
    zero at the start of every side. The report measures the stored feeder
    order (`baselineTravelMm`) and the optimized order (`travelMm`) and
    reports `travelSaved` (floored at zero). The assignment is a heuristic, not
    an optimal slot permutation, and it never writes the document.

    ```shell
    open-smt feeder optimize panel.osmt --slot-pitch 16
    ```

16. **Changeover sequence.** `changeover` (CLI
    `open-smt changeover <current.osmt> <next.osmt> [--feeder-change-seconds <s>]`)
    compares the feeder setups of two documents by feeder id. Lanes only the
    current document declares are removed, lanes only the next declares are
    added, lanes in both with a different package or a different pitch are
    swapped, and lanes matching by package and exact pitch stay unchanged. The
    sequence runs removals first, then swaps, then additions, each group in
    ascending id order, so stale reels leave before new ones enter. A swap
    counts as two physical reel changes; `estimatedSeconds` is
    `changes × feederChangeSeconds` with a default of `30` s per change, which
    must be finite and positive, otherwise `changeover/invalid`. Both
    documents must pass the structural validation (feeder coverage errors are
    ignored because the plan compares setups, not builds). Neither document is
    modified.

    ```shell
    open-smt changeover job-a.osmt job-b.osmt
    ```

## KiCad PCB import

`open-smt import <file.kicad_pcb> --out <file.osmt>` reads the S-expression
dialect KiCad writes. Only one slice of it can become an OpenSMT document:

- **Outline.** The `Edge.Cuts` layer must consist of straight lines that close
  exactly one axis-aligned rectangle. Multiple collinear lines per edge are
  accepted. A missing outline, a gap, a diagonal, or any other `Edge.Cuts`
  construct is refused with `import/outline-unsupported`; the reader never
  substitutes a bounding box.
- **Components.** Each `(footprint "Package" (layer "F.Cu"|"B.Cu") (at x y rot))`
  becomes one component: `package` is the footprint name, `x`/`y` are millimetres
  translated so the outline corner is `(0, 0)`, `rotation` is normalised to
  `[0, 360)`, and `side` is `top` for `F.Cu` and `bottom` for `B.Cu`. The
  reference designator comes from `(property "Reference" ...)` or, for older
  files, from `(fp_text reference ...)`.
- **Height.** This subset carries no component height, so `height` stays
  `null`. Board thickness also comes from no field in the subset; the imported
  board uses the 1.6 mm default.

Everything else is skipped rather than approximated, and the import report
counts each skipped construct by category. The ledger is sorted by code and
deterministic.

| Loss code | Skipped construct |
|-----------|-------------------|
| `kicad/footprint-graphics` | `fp_line`, `fp_arc`, `fp_curve`, `fp_poly`, `fp_rect`, `fp_circle` |
| `kicad/footprint-text` | `fp_text` other than the reference |
| `kicad/footprint-property` | `property` other than `Reference` |
| `kicad/footprint-model` | `model` |
| `kicad/pad-not-imported` | `pad` |
| `kicad/zone-not-imported` | `zone` |
| `kicad/construct-not-imported` | any other skipped line, segment, via, graphic or construct |
| `kicad/footprint-invalid` | footprint without a usable name or position |
| `kicad/footprint-layer-missing` | footprint without a `layer` |
| `kicad/footprint-layer-unsupported` | footprint on a layer other than `F.Cu`/`B.Cu` |
| `kicad/footprint-without-reference` | footprint without a reference designator, skipped |
| `kicad/footprint-duplicate-reference` | repeated reference designator, skipped |

A footprint that is skipped by a reference or layer loss no longer contributes
its internal constructs to the ledger. Components are validated with the same
rules as hand-entered components; the imported document may still report
`feeder/missing` and `feeder/capacity` until a feeder setup exists. An existing
target file is never overwritten (`file/exists`).

## CSV placement import

`open-smt import csv <file.csv> --out <file.osmt> [--side top|bottom]` reads a
generic placement table into a new document. The first non-empty line is the
header and must declare `refdes`, `package`, `x` and `y`; `rotation` and `side`
are optional. Names are case-insensitive, columns may appear in any order, a
double-quoted field may contain commas or a doubled `""` quote, and empty lines
are skipped. Rows keep the order of the source.

- **Rows.** One component per row. `x` and `y` are millimetres and are neither
  converted nor translated. `rotation` defaults to `0` and is normalised to
  `[0, 360)`; `side` defaults to the command's `--side` (or `top`) and accepts
  `top`/`bottom` in any case. `height` stays `null`.
- **Board.** The source carries no outline, so the board is sized to the largest
  coordinate and is never smaller than the default 100 × 80 mm; thickness is the
  1.6 mm default. Negative coordinates are refused because the board origin is a
  corner.
- **Revision.** The import is one edit: the new document is written atomically
  with revision 1. An existing target is never overwritten (`file/exists`).
- **Checks.** Unknown or duplicate columns and rows whose field count differs
  from the header are refused. A missing column, a non-numeric coordinate, an
  unknown side, an empty `refdes`/`package` or a repeated reference designator
  reports an `import/...` code that names the physical line.

## Validation

| Code | Meaning |
|------|---------|
| `document/format`, `document/version`, `document/units` | not an OpenSMT millimetre document |
| `import/parse` | the file is not a KiCad PCB S-expression, or the CSV cannot be tokenised |
| `import/outline-unsupported` | `Edge.Cuts` is missing or is not one axis-aligned rectangle |
| `import/missing-column` | the CSV header lacks `refdes`, `package`, `x` or `y` |
| `import/unknown-column` | the CSV header declares a column other than the six known names |
| `import/duplicate-column` | the CSV header declares the same column twice |
| `import/invalid-header` | a CSV header column has an empty name |
| `import/invalid-row` | a CSV row has a different field count than the header |
| `import/invalid-value` | a CSV row has an empty `refdes` or `package` |
| `import/invalid-number` | a CSV coordinate or rotation is not a finite number |
| `import/invalid-side` | a CSV row states a side other than top or bottom |
| `import/duplicate-refdes` | a CSV row repeats a reference designator |
| `import/outside-board` | a CSV row has a negative coordinate |
| `bom/parse` | the BOM CSV cannot be tokenised or is empty |
| `bom/missing-column` | the BOM header lacks `package` or `quantity` |
| `bom/unknown-column` | the BOM header declares a column other than `package`, `quantity` or `refdes` |
| `bom/duplicate-column` | the BOM header declares the same column twice |
| `bom/invalid-header` | a BOM header column has an empty name |
| `bom/invalid-row` | a BOM row has a different field count than the header |
| `bom/invalid-value` | a BOM row has an empty package or a zero quantity |
| `bom/invalid-number` | a BOM `quantity` field is not an integer |
| `bom/duplicate-refdes` | the BOM declares the same reference designator twice |
| `board/invalid` | a board size is not finite and positive |
| `panel/invalid` | zero rows/columns, a negative gap, or an invalid panel-fit maximum or gap |
| `balance/invalid-machines` | a balance requested fewer than 1 or more than 64 machines |
| `schedule/invalid-strategy` | a schedule requested a strategy other than `greedy` or `lpt` |
| `schedule/invalid-lanes` | a multi-lane schedule requested fewer than 2 or more than 16 lanes |
| `component/invalid` | empty refdes/package, non-finite values or bad rotation |
| `component/duplicate-refdes` | a reference designator is declared twice |
| `component/unknown-refdes` | an update or removal names a component that does not exist |
| `component/outside-board` | a component lies outside the board |
| `feeder/invalid` | empty id/package, non-positive pitch/capacity, loaded > capacity |
| `feeder/duplicate-id` | a feeder id is declared twice |
| `feeder/missing` | a used package has no feeder |
| `feeder/capacity` | loaded parts do not cover the component count |
| `profile/invalid` | empty zones, non-positive length/temperature/speed/liquidus |
| `station/invalid` | a feeder station assignment requested a non-positive or non-finite slot pitch |
| `changeover/invalid` | a changeover requested a non-positive or non-finite time per reel change |

Warnings do not make `ok` false: `component/none`, `feeder/none`,
`feeder/unused`, `component/rotation` (not a multiple of 90) and
`profile/below-liquidus`.

## Tests

| File | Asserts |
|------|---------|
| `tests/place.rs` | allocation order, nearest-first sequence, tie-breaks, panel size, multi-feeder split, determinism, 2-opt never longer than nearest neighbour, an improving crossing route, unchanged single/empty documents, additive report fields, byte-identical default output |
| `tests/balance.rs` | single-machine parity, component conservation, side order, determinism, empty machines, invalid counts, per-machine feeder shortages, independent per-machine 2-opt, imbalance identity |
| `tests/dual_lane.rs` | greedy lane assignment, LPT beating greedy, swap-pass improvement and order preservation, explicit greedy parity, one empty lane, empty input, work/ratio arithmetic, per-lane order preservation, placement-program parity, invalid strategy, determinism, three-lane LPT, task conservation, exact two-lane parity, invalid lane counts |
| `tests/reflow.rs` | default profile numbers, exact single-zone integral, below-liquidus profile, zone order |
| `tests/validation.rs` | every code above and warning-only documents |
| `tests/feeder.rs` | package counts, slot numbering in package order, reference designator document order, deduplicated sorted sides, empty document, determinism; partial feeder update (pitch/capacity/loaded, package move reflected by allocation, invalid capacity/loaded/pitch/package, unknown id, empty request, one revision, untouched fields, round trip, determinism) |
| `tests/panel_fit.rs` | exact-fit 2 × 3 arithmetic, leftover gaps, too-small dimensions, zero waste, gap effects, utilization, stored-gap default versus override, invalid maxima and gaps, determinism, unchanged document |
| `tests/bom.rs` | matching BOM, missing/extra/quantity findings, a refdes placed under another package, quantity-only rows, empty document and empty BOM, case-insensitive quoted headers, deterministic order-independent JSON, stable `bom/...` error codes, unchanged document bytes and revision, missing BOM file |
| `tests/takt.rs` | default machine model matching the placement program, changeover overhead segments, custom machine parameters, even multi-machine split and throughput, empty document zero time, 2-opt saving fields, feeder error propagation, invalid machine parameters, serde defaults, determinism, unchanged document |
| `tests/session.rs` | create, edit, reopen, export, in-use feeder, atomic save-as; partial component update (move, rotation normalisation, side and package changes, height set and cleared, untouched fields, invalid requests, one revision, determinism, round trip) |
| `tests/import.rs` | KiCad footprint mapping, rotation, both sides, reference sources, loss ledger, outline errors, duplicate/missing references, empty board, determinism, existing target; CSV row order, reordered/quoted/UPPERCASE columns, empty lines, side and rotation defaults, duplicate refdes, missing column, invalid number, invalid side, line numbers, negative coordinate, determinism, one revision, existing target |
| `tests/station.rs` | mean-placement-X order shortening the tape-bank travel, a matching stored order saving nothing, tie-breaks by feeder id, unused lanes last with `meanX: null`, empty document, feeder coverage propagation, invalid slot pitches, determinism, unchanged document |
| `tests/changeover.rs` | removal/swap/addition ordering with steps, swap counting twice, pitch-only swap, identical setups, empty current, ignored feeder coverage, structurally invalid documents, invalid change seconds, determinism, unchanged documents |
| `tests/smoke.test.ts` (TypeScript) | session lifecycle, argument errors before the addon, native-gated balance, multi-lane, panel-fit, BOM-check, takt, component-update, feeder-optimize and changeover smokes |

## Limitations

- Placement defaults to a deterministic nearest-neighbour heuristic. The
  optional 2-opt pass is a bounded local search over each side's route; it
  improves travel but does not guarantee an optimal tour. Neither mode models
  nozzles, head count, feeder positions or vision time.
- The feeder model has no lane index, tape direction or splice handling. The
  feeder loading report numbers packages from 1 in name order, but those slot
  numbers are suggestions only: they do not describe a physical machine slot or
  a tape layout, and the report never writes a feeder setup.
- The panel fit report assumes an axis-aligned grid of whole boards. It does not
  rotate boards, add edge rails, reserve fiducial or tooling areas, or derive a
  panel outline, and it never changes the stored panel array.
- The BOM cross-check matches packages by exact name; reference-designator
  findings only run for packages whose BOM rows enumerate refdes, and
  part-number aliases are not resolved. It never writes the document or the BOM.
- The cycle time estimate is analytical: it assumes a constant head speed and a
  fixed overhead per component, and models no acceleration, tape advance, vision
  or nozzle physics. The feeder and nozzle change counts are caller estimates,
  not plan data, and multi-machine throughput is an even split, not a line
  simulation.
- The feeder station assignment is a heuristic: it orders lanes by the mean
  placement X of the nearest-neighbour program, models tape-bank travel along X
  only and ignores slot capacity, tape direction, nozzle constraints and reel
  lengths. It never writes a feeder setup; applying the assignment is a manual
  step.
- The changeover plan matches lanes by feeder id and compares them by package
  name and exact pitch; it does not detect a lane that moved to another id, and
  the downtime is a fixed time per reel change, not a measured rate. It reads
  both documents and writes neither.
- Reflow uses set-points, not measured thermocouple data, and models no
  convection or board thermal mass.
- Only the top and bottom sides are supported.
- Multi-lane scheduling assigns whole placement tasks to 2–16 lanes with a
  greedy or longest-processing-time-first rule and an optional pairwise-swap
  pass. The two-lane report is the special case of the same engine. It does not
  model conveyor lanes, lane transfer, nozzle counts or per-lane re-sequencing,
  and its result is not an optimal partition.
- Line balancing distributes whole components across 1–64 machines but does not
  emit vendor machine programs, model conveyor lanes or account for line
  changeover time.
- Vendor dialects and paste apertures are out of scope and reported as such.
- KiCad import covers the component list and a rectangular outline only. Nets,
  tracks, vias, pads, zones, graphics and 3D models are counted in the loss
  ledger and are not part of the document. Altium and ODB import remain
  unimplemented.
- CSV import covers the placement table only. The board has no outline and is
  sized to the coordinates, components carry no height, and unknown columns are
  refused instead of ignored. Nets, feeders, panel arrays and reflow profiles
  are not read from CSV.
