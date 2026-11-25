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
| component list report with package counts and feeder assignment | ✅ supported |
| partial component update (in-place edit) | ✅ supported |
| feeder setup with package, pitch, capacity, loaded quantity and leader/trailer/manual scrap | ✅ supported |
| reel splice at a placement step | ✅ supported |
| partial feeder update (in-place edit, including scrap) | ✅ supported |
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
| reflow process-window margins (peak, time above liquidus, ramp rates) | ✅ supported |
| placement program CSV | ✅ supported |
| neutral machine program CSV with readback verification | ✅ supported |
| parallel pick-cycle grouping (nozzle and head count) | ✅ supported (consecutive same-package picks, package as nozzle identity; no acceleration, vision or nozzle physics) |
| feeder setup CSV | ✅ supported |
| JSON / Markdown reports | ✅ supported |
| SVG placement drawing (panel, components, labels, legend, title block) | ✅ supported (1:1 mm at 8 px/mm; bodies from the package rules table; ASCII-only collision-free labels) |
| PDF placement drawing (same sheet, A4 landscape) | ✅ supported (fit to page, Helvetica; same body sizing and ASCII-only labels) |
| feeder setup drawing (SVG tape bank with slots, load bars and lane labels) | ✅ supported (same allocation as the feeders report; models no machine slot, tape direction or pitch constraint) |
| pick-and-place sequence drawing (SVG and PDF, one numbered route per side) | ✅ supported (default nearest-neighbour program; models no acceleration, tape advance, vision or nozzle physics) |
| solder paste volume report (JSON and CSV, per package and run) | ✅ supported (aperture area × foil thickness × boards + waste; models no squeegee transfer loss or reel waste) |
| paste aperture / stencil generation | ✅ supported (fixed package rules table keyed by package name; one area reduction per family; CSV report plus SVG/PDF sheet) |
| component height and clearance check | ✅ supported (rotated bounding boxes from the package rules table; warning findings; configurable minimum gap and maximum height) |
| vendor machine programs (Fuji, Yamaha, Juki, ASM) | ❌ not implemented |
| line balancing | ✅ supported |
| multi-lane scheduling (2–16 lanes, including the dual-lane report) | ✅ supported (greedy or LPT, optional swaps; not optimal) |
| cycle time and throughput estimate (takt) | ✅ supported (analytical machine model; not a measured rate) |
| feeder station assignment along the tape bank | ✅ supported (heuristic by mean placement X; not optimal) |
| feeder changeover sequence between two documents | ✅ supported (id/package/pitch comparison; fixed time per change) |
| feeder consumption plan for a build quantity | ✅ supported (net picks, gross consumption and scrap; reels from lane capacity and splices) |
| feeder replenishment schedule across a production run | ✅ supported (splice and scrap aware; one full reel when the next board is not covered; no partial-reel model) |
| panel coordinate map (single board to every panel instance) | ✅ supported |
| first-article inspection sheet (JSON and CSV) | ✅ supported |
| takt bottleneck heat map (step, package and side shares) | ✅ supported |
| line-balance bar chart (SVG, bottleneck and target line) | ✅ supported (three-machine line; export formats carry no options; no changeover or staffing) |
| nozzle and feeder setup table (SVG and paginated PDF) | ✅ supported (nozzle class and tape width derived from the package rules; no machine slot, tape direction or pitch limit) |
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
    `capacity`, `loaded`, `leaderWaste`, `trailerWaste` or `scrap` must be
    supplied; omitted fields keep their value. An
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

17. **Reflow process window.** `reflowWindow` (CLI
    `reflow window <file> [--peak-min <c>] [--peak-max <c>] [--tal-min <s>]
    [--tal-max <s>] [--max-ramp <c/s>] [--max-cooling <c/s>]`) reruns the
    `reflow` evaluation and derives the signed rate of every zone,
    `(exit − entry) / seconds`. `maxHeatingRate` is the largest positive rate and
    `maxCoolingRate` the largest magnitude of a negative rate; both are zero when
    no zone moves that way. Each supplied limit adds one check in the fixed order
    peak minimum, peak maximum, time-above-liquidus minimum, time-above-liquidus
    maximum, max heating rate and max cooling rate. A check reports `value`,
    `limit`, `margin` and `pass`; `margin` is `value − limit` for a minimum and
    `limit − value` for a maximum, and `pass` is `margin >= 0`. `ok` is true when
    every check passes, and when no limit is supplied. A limit must be finite and
    non-negative, and a minimum may not exceed its maximum, otherwise
    `reflow/invalid`. The document is never changed.

    ```shell
    open-smt reflow window panel.osmt --peak-max 250 --tal-max 60 --max-ramp 5
    ```

18. **Feeder consumption plan.** `consumption` (CLI
    `feeder consumption <file> --boards <n>`) runs the same allocation as
    `feeders`, so a lane's per-board picks equal its `assigned` count. With
    `run = boards`, a lane's `picks = assigned × run` is the **net** placed
    quantity. A reel provides
    `usable = capacity − leaderWaste − trailerWaste` parts; every splice adds one
    reel with `usable − spliceLeaderWaste − spliceTrailerWaste` parts. Reels are
    `1 + splices` plus the extra full reels needed for the remaining
    `picks + feederScrap + spliceLeaderWaste + spliceTrailerWaste + spliceScrap`
    demand (zero when the lane supplies nothing). `scrap` sums per-reel
    leader/trailer waste over every reel plus the splice and feeder manual scrap,
    and `gross = picks + scrap`; scrap is never folded into `picks`.
    `remainder = picks − (reels − 1) × usable` (zero when no reel is needed) and
    `short` is `gross > loaded + splices × usable`, so the loaded reel and the
    spliced reels cannot cover gross consumption. Packages are aggregated in name
    order with their per-board component count, total picks, total gross, scrap,
    lane count, summed reels and any short lane. `boards` must be at least 1,
    otherwise `consumption/invalid`. `ok` is true when no lane is short.
    Allocation errors (`feeder/missing`, `feeder/capacity`) propagate, and the
    plan never changes the document.

    ```shell
    open-smt feeder consumption panel.osmt --boards 250
    ```

19. **Panel coordinate map.** `panelMap` (CLI
    `panel map <file> [--out <panel.json>]`) maps every component of the single
    board into every board instance of the stored panel array without changing
    the document. A board that is not finite and positive reports
    `board/invalid`; zero rows or columns, or a negative or non-finite gap,
    reports `panel/invalid`. Instances run row-major and are numbered from 1:
    `board = (row − 1) × columns + column`,
    `offsetX = (column − 1) × (width + gapX)` and
    `offsetY = (row − 1) × (depth + gapY)`. `panelWidth` and `panelDepth` follow
    the stored-panel arithmetic. Each component keeps its document order and
    reports `offsetX + x` and `offsetY + y` for every board; a component with
    non-finite coordinates carries no placements. `outsideBoard` counts
    components whose stored coordinates lie outside the single board and `ok`
    is true when that count is zero.

    ```shell
    open-smt panel map panel.osmt --out panel-map.json
    ```

20. **First-article sheet.** `firstArticle` (CLI
    `quality first-article <file>`, CSV via `export --format fai`) runs the same
    allocation and sequence as the placement program, so `feeder/missing` and
    `feeder/capacity` propagate, and turns them into an inspection record: one
    `steps` row per program step in program order with `step` counted from 1,
    its coordinates, rotation, lane id and lane pitch; one `materials` block per
    package in package order with the per-board component count, the distinct
    sides (`top` before `bottom`) and the supplying lanes in id order (`pitch`,
    `loaded`, `capacity`, `assigned`); the document `revision`; the board and
    panel geometry; and the fixed `checklist` `placement`, `orientation`,
    `package`, `feeder`. The CSV sheet has the same rows in program order and
    leaves the four check columns empty for the inspector. The sheet records no
    inspection result and never changes the document.

    ```shell
    open-smt quality first-article panel.osmt
    open-smt export panel.osmt --format fai --out first-article.csv
    ```

21. **Machine program interchange.** `machineProgram` (CLI
    `machine export <file> --out <file.csv> [--nozzles "pkg=nozzle;..."]`) reruns
    the allocation and sequence of the placement program and renders one CSV row
    per step in program order. The header is fixed:
    `sequence,side,refdes,package,x_mm,y_mm,rotation_deg,feeder,station,pitch_mm,nozzle,speed_mm_s`.
    `station` is the one-based position of the lane in the document feeder setup
    (not the heuristic `feeder optimize` slot), `pitch` is that lane's tape
    pitch, `nozzle` comes from the caller map keyed by package and is empty when
    the package has no entry, and `speed` is the model head speed of `200` mm/s.
    Coordinates, rotation, pitch and speed are rounded to three decimals once,
    so rendering and parsing are exact inverses. The export writes the file,
    parses it back and compares every field; a difference reports
    `program/invalid` and the file is left on disk. `machine verify` (CLI
    `machine verify <file> --program <file.csv> [--nozzles ...]`) rebuilds the
    document program and lists every differing field with its step (`sequence`
    `0` names a step-count difference). An empty package or nozzle in the map, a
    missing or unknown column, a wrong field count, an unknown side, an empty
    reference/package/feeder, a non-finite number, a zero sequence or station, a
    non-positive speed and a malformed file report `program/invalid`. The format
    is a neutral interchange, not a vendor dialect; feeder coverage errors
    propagate from the allocation. Neither the document nor the source is
    modified by verification.

    ```shell
    open-smt machine export panel.osmt --out machine.csv --nozzles "0603=N1;QFN-32=N2"
    open-smt machine verify panel.osmt --program machine.csv --nozzles "0603=N1;QFN-32=N2"
    ```

22. **Takt bottleneck heat map.** `heatmap` (CLI `takt heatmap <file>`) runs the
    allocation and nearest-neighbour sequence of the default `takt` estimate and
    attributes each step's time to the step, its package and its side. A step's
    time is `0.6 s + inbound travel / 200 mm·s⁻¹`, measured from the previous
    step of the same side with a fresh origin at every side, exactly like the
    placement program. Step times therefore sum to the `takt` `boardSeconds` of
    the default machine model with no feeder or nozzle change overhead. `share`
    is a step's time divided by `totalSeconds` (zero for an empty document);
    `packages` are ordered by descending time, ties by package name, and each
    carries `placeSeconds`, `travelSeconds`, `seconds`, `share` and a running
    `cumulativeShare` for a Pareto-style heat map. `sides` lists only sides with
    at least one step, `top` before `bottom`, and `bottleneck` names the package
    with the largest time (`null` when empty). Feeder coverage errors propagate
    and the document is never modified.

    ```shell
    open-smt takt heatmap panel.osmt
    ```

23. **Feeder replenishment schedule.** `schedule` (CLI
    `feeder schedule <file> --boards <n> [--out <file>]`) runs the same
    allocation as `feeder consumption`, so a lane's per-board picks equal its
    `assigned` count. Boards are simulated from 1 to `boards`. A lane starts at
    its stored `loaded` quantity minus its per-reel leader/trailer waste and its
    manual `scrap`. A splice joins one new reel when the 1-based placement step
    `sequence` is reached on the first board; its picks before the joint are
    counted from the program, so the new reel is available for the rest of that
    board and the cumulative used quantity never resets. A change's `board` is
    the 1-based board after which it happens, its `reels` is the cumulative reel
    count and `sequence` carries the placement step of a splice. Splice waste is
    `leaderWaste + trailerWaste + scrap` on top of the lane's per-reel waste.
    When a lane with positive usable capacity cannot cover the next board
    (`remaining < picksPerBoard`), one full reel is installed after the current
    board. A lane's `reels` is the initial reel plus every splice and
    replenishment (`0` when the lane supplies nothing). A lane is `short` when
    its stock cannot cover a board at any point, including a zero-capacity lane
    with demand. A short lane still consumes what it has (never below zero),
    replenishes when it can and is simulated to the end, so the report is always
    produced and deterministic. `picks = picksPerBoard × boards`,
    `scrap` sums every reel's leader/trailer waste and manual scrap,
    `gross = picks + scrap`, `remainingAfterRun` is the usable stock left after
    the final board, `changeCount`, `reelCount`, `totalPicks`, `totalGross`,
    `totalScrap` and `splices` sum the lanes, and `ok` is true when no lane is
    short. Lanes are reported in feeder id order. `boards` must be at least 1,
    otherwise `schedule/invalid`. Feeder coverage errors (`feeder/missing`,
    `feeder/capacity`) propagate unchanged, and the document is never modified.
    A splice is edited with `feeder splice <file> --id <id> --sequence <n>
    [--leader-waste <n>] [--trailer-waste <n>] [--scrap <n>]`; per-feeder waste
    is edited with `feeder scrap <file> --id <id> [--leader-waste <n>]
    [--trailer-waste <n>] [--scrap <n>]`. The splice sequence must lie in the
    placement program and a lane may hold one splice per step, otherwise
    `feeder/splice-invalid`; an unknown lane reports `feeder/unknown-id`.

    ```shell
    open-smt feeder schedule panel.osmt --boards 250
    ```

24. **Component list report.** `componentList` (CLI `component list <file>`)
    lists every component in document order with its `refdes`, `package`,
    `side`, `x`, `y`, `rotation`, the `height` when the document stores one, and
    the feeder lane the current allocation assigns (`null` when unallocated).
    Packages are grouped in first-seen order with their `count` and reference
    designators in document order. The feeder column reuses the same allocation
    as the `feeders` report, so `unassignedCount` is the number of components
    with no loaded lane of their package. No validation runs: the report is
    meant to be read before a feeder setup exists, and it never writes. An empty
    document reports zero counts and empty arrays.

    ```shell
    open-smt component list panel.osmt
    ```

25. **Placement drawings.** `export --format svg` and `export --format pdf` render
    the stored board, panel, component and feeder data as a placement sheet. The
    sheet contains the panel frame, one outlined board per panel instance with
    its `B1…Bn` label, one rotated rectangle per component on every board,
    reference-label and package/lane labels, a colour legend, a feeder lane
    table and a title block. The title block records the document id, revision,
    units, board/component/placement/feeder counts, the body-size source and the
    scale. Component bodies take their length and width from the same package
    rules table the stencil and clearance check use, drawn with the stored
    clockwise rotation; an unrecognised package falls back to
    `DEFAULT_COMPONENT_WIDTH_MM` by `DEFAULT_COMPONENT_HEIGHT_MM`
    (`2.0 × 1.2` mm) and is counted in `defaultSizeComponents`. Labels scale
    with the body and are placed greedily in document order — the reference
    designator above the body (or below when that space is taken) and the
    package/lane line below — and a label that would collide with a neighbouring
    body or an accepted label is omitted. The lane tag follows
    the current feeder allocation when it is valid and otherwise the first
    feeder in document order that supplies the package. A multi-board panel
    labels board 1 only to keep the sheet readable. Labels are ASCII-only: a
    non-ASCII character becomes `?` and is reported as
    `drawing/non-ascii-text` in the export `losses`; `drawing` reports the
    geometry and counts. The SVG uses a millimetre `viewBox` and an
    eight-pixel-per-millimetre `width`/`height`; the PDF is one A4 landscape
    page (`/MediaBox [0 0 841.89 595.28]`) fitted and centred from the same
    layout, using 2.834646 pt/mm before the fit factor, with Helvetica.
    Coordinates are rounded to three decimals and no timestamp is written, so
    two exports of one document are byte-identical. An invalid board or panel
    reports `board/invalid` or `panel/invalid`; an empty document still writes a
    framed sheet with its title block.

    ```shell
    open-smt export panel.osmt --format svg --out panel.svg
    open-smt export panel.osmt --format pdf --out panel.pdf
    ```

26. **Stencil apertures.** `stencil` (CLI `open-smt stencil <file>
    [--out stencil.json]`, SDK `document.stencil()`) generates paste apertures
    from a fixed package rules table keyed by package name, because the model
    stores no footprint data. Names are matched case-insensitively: a leading
    letter (`R0603`) keeps the chip code and a trailing number (`SOIC-8`,
    `QFP-44`) names the lead count. Each family contributes one rule: `chip`
    (0201–2010) gets two rectangular terminal apertures at the body ends,
    `sot23` three, `soic` one row per side at 1.27 mm pitch, `qfp` and `qfn`
    four rows at 0.5 mm pitch, `lga` (also `csp`/`bga`) a square grid of round
    terminals, and an unrecognised package the `generic` rule (one aperture on
    the default drawing footprint, listed in `unmatchedPackages`). Every
    aperture side is the nominal pad side multiplied by the square root of the
    family's `areaReduction` (0.8–1.0), so the aperture area is reduced by the
    stated factor. Apertures inherit the component position and rotation;
    coordinates are rounded to three decimals. The report lists one rule per
    distinct package in first-seen order and one entry per aperture in
    component order for a single board. `export --format stencil` writes the
    same entries as CSV, and `export --format stencil-svg` / `stencil-pdf`
    render a stencil sheet that reuses the placement-drawing infrastructure:
    the board outline, apertures coloured by side, reference labels placed
    above each body and skipped when they would collide with an aperture or
    another label, the rule table, a legend and
    a title block; labels are ASCII-only and non-ASCII text is reported as
    `stencil/non-ascii-text`. An invalid board reports `board/invalid`. The
    generation is read-only, deterministic and byte-identical on repeat.

    ```shell
    open-smt stencil panel.osmt --out stencil.json
    open-smt export panel.osmt --format stencil --out stencil.csv
    open-smt export panel.osmt --format stencil-svg --out stencil.svg
    open-smt export panel.osmt --format stencil-pdf --out stencil.pdf
    ```

27. **Height and clearance check.** `clearance` (CLI `open-smt clearance <file>
    [--min-gap <mm>] [--max-height <mm>]`, SDK `document.clearance(limits)`)
    warns about components taller than the maximum height and about same-side
    bodies that overlap or sit closer than the minimum gap. The model stores no
    footprint data, so each body outline comes from the same package rules
    table as the stencil apertures, and a rotated body is compared as its
    axis-aligned bounding box, which handles any rotation conservatively.
    Heights are only checked for components that store one. The gap between two
    boxes is the diagonal separation when they are apart on both axes and the
    largest single-axis separation otherwise; an overlap reports gap `0`.
    Findings are warnings with codes `clearance/height`, `clearance/overlap`
    and `clearance/gap`, listed height-first in component order and then in
    pair order, and `pairsChecked` counts the same-side pairs compared.
    Components with non-finite coordinates are skipped. `minGap` defaults to
    0.5 mm and `maxHeight` to 12 mm; invalid limits report `clearance/invalid`.
    The check is read-only and deterministic; it never changes the document and
    never affects the validation report.

    ```shell
    open-smt clearance panel.osmt --min-gap 0.4 --max-height 10
    ```

28. **Feeder setup drawing.** `export --format feeder-svg` renders the feeder
    tape bank as a millimetre SVG sheet at 8 px/mm: one lane row per feeder in
    id order with a slot box, a load bar for the loaded share of the capacity,
    the lane id, package, pitch, capacity and loaded quantity, and the assigned
    reference designators. It reads the same allocation as the `feeders`
    report, so a package without a feeder fails with `feeder/missing`; it
    models no machine slot, tape direction or pitch constraint. An empty setup
    still writes a framed sheet. Text is ASCII-only and dropped characters are
    reported as `feeder/non-ascii-text` in the export `losses`. The render is
    deterministic and byte-identical on repeat, and it never changes the
    document.

    ```shell
    open-smt export panel.osmt --format feeder-svg --out feeders.svg
    ```

29. **Pick-and-place sequence drawing.** `export --format sequence-svg` and
    `export --format sequence-pdf` render the default nearest-neighbour
    placement program as a route map: one polyline per board side starting at
    the machine origin `(0, 0)`, one numbered step marker per placement in
    program order and a legend with the step counts, the travel length and the
    estimated program time. The routes come from the same allocation and
    sequence as the `program` export, so a missing or undersized feeder fails
    with `feeder/missing` or `feeder/capacity`. The SVG is a millimetre sheet
    at 8 px/mm and the PDF is one A4 landscape page fitted from the same
    layout with Helvetica; an empty program still writes a framed sheet. Text
    is ASCII-only and dropped characters are reported as
    `sequence/non-ascii-text` in the export `losses`; `summary` reports the
    board size, step counts per side and the travel length. The render is
    deterministic and byte-identical on repeat and never changes the document.

    ```shell
    open-smt export panel.osmt --format sequence-svg --out sequence.svg
    open-smt export panel.osmt --format sequence-pdf --out sequence.pdf
    ```

30. **Solder paste volume report.** `stencil paste --boards <n> [--foil <mm>]
    [--waste <pct>]` estimates paste consumption from the generated stencil
    apertures: each rect aperture contributes `width × height × foil` and each
    circle `π × (diameter / 2)² × foil`, the per-board volume is scaled by the
    board count and the waste percentage is added on top. The foil thickness
    defaults to `0.12 mm`, the board count must be at least one and the waste
    percentage must lie between 0 and 100; anything else fails with
    `paste/invalid`. Volumes are rounded to three decimals at every reported
    level, the report is deterministic and byte-identical on repeat, `--out`
    writes the JSON, `--csv` a per-package sheet with a `TOTAL` row, and the
    document is never modified.

    ```shell
    open-smt stencil paste panel.osmt --boards 24 --waste 5 --out paste.json --csv paste.csv
    ```

31. **Reflow profile drawing.** `export --format reflow-svg` renders the same
    linear-ramp evaluation as the `reflow` report as a millimetre time-temperature
    sheet: the profile polyline enters each zone at the previous set-point and
    ramps to its own, ambient/liquidus/peak reference lines carry their
    temperatures, dashed boundaries split the time axis into zones with their
    names and seconds, and a marker sits on the peak. It validates like the
    `reflow` report, so an inconsistent profile fails with the same code. Text
    is ASCII-only and dropped characters are reported as `reflow/non-ascii-text`
    in the export `losses`. The render is deterministic and byte-identical on
    repeat and never changes the document.

    ```shell
    open-smt export panel.osmt --format reflow-svg --out reflow.svg
    ```

32. **Panel map drawing.** `export --format panel-svg` (also accepted as
    `panel-map-svg`) draws the stored panel array from the `panel map` report:
    the panel outline, one rectangle per board instance numbered row-major from
    the panel origin, every component as a side-coloured dot at its panel
    coordinates on every instance and an origin cross at `(0, 0)`. It fails with
    the same `board/invalid` and `panel/invalid` codes as the report. Text is
    ASCII-only and dropped characters are reported as `panel-map/non-ascii-text`
    in the export `losses`. The render is deterministic and byte-identical on
    repeat and never changes the document.

    ```shell
    open-smt export panel.osmt --format panel-svg --out panel-map.svg
    ```

33. **Takt heat map drawing.** `export --format heatmap-svg` plots the
    `takt heatmap` report on the board outline: one dot per program step whose
    colour and radius grow with its share of the board time, and a Pareto column
    ranking up to eight packages by share. It runs the same allocation and
    sequence as the heat map report, so it fails with `feeder/missing` or
    `feeder/capacity` the same way. Text is ASCII-only and dropped characters
    are reported as `heatmap/non-ascii-text` in the export `losses`. The render
    is deterministic and byte-identical on repeat and never changes the
    document.

    ```shell
    open-smt export panel.osmt --format heatmap-svg --out heatmap.svg
    ```

34. **Line-balance bar chart.** `export --format balance-svg` (also accepted as
    `line-balance-svg`) plots the per-station cycle time of the `balance` report:
    one bar per machine, the slowest station in the accent colour, its value
    above the bar, a dashed average target line and a y axis in seconds with
    tick labels. The chart always balances across three machines because export
    formats carry no options; the title block states that assumption. It fails
    with the same `board/invalid` code as the report. Text is ASCII-only and
    dropped characters are reported as `balance/non-ascii-text` in the export
    `losses`. The render is deterministic and byte-identical on repeat and never
    changes the document.

    ```shell
    open-smt export line.osmt --format balance-svg --out balance.svg
    ```

35. **Nozzle and feeder setup table.** `export --format setup-svg` (also
    accepted as `setup-sheet-svg`) and `setup-pdf` (`setup-sheet-pdf`) render the
    allocation as a table: slot, lane, part number, nozzle, tape width in mm,
    loaded/capacity quantity, assigned count and reference designators. The
    document has no nozzle or tape-width field, so the nozzle class is derived
    from the package family and body length and the tape width from the family;
    the page header says so. Columns share fixed left edges and rows alternate a
    light fill. Long tables continue on further pages with a repeated header and
    `page N of M`; the SVG stacks framed pages, the PDF emits one A4 landscape
    page per chunk. It fails with `feeder/missing` or `feeder/capacity` like the
    feeders report. Text is ASCII-only and dropped characters are reported as
    `setup/non-ascii-text` in the export `losses`. The render is deterministic
    and never changes the document.

    ```shell
    open-smt export panel.osmt --format setup-svg --out setup.svg
    open-smt export panel.osmt --format setup-pdf --out setup.pdf
    ```

36. **Parallel pick-cycle grouping.** `pickCycle` (CLI
    `pick-cycle <file> [--heads <n>]`) runs the same allocation and
    nearest-neighbour sequence as `placement` and packs consecutive placements
    that share a nozzle into one cycle. The document stores no nozzle field, so
    the package name is the nozzle identity; a package change or a board-side
    change starts a new cycle and a cycle holds at most `heads` picks. The
    program is never reordered. `heads` defaults to `4` and must be between 1 and
    64, otherwise `pick-cycle/invalid`. The report contains `components`,
    `cycles`, `picks`, `headSlots` (`cycles × heads`), `wastedSlots`
    (`headSlots − picks`), `utilization` (`picks / headSlots`, zero when empty),
    `singleHeadCycles` and `cyclesSaved` (`components − cycles`, the saving
    against a one-head program), plus one `entries` row per cycle in program
    order with its `cycle`, `package`, `side`, `picks`, `refdes` and
    `unusedSlots`. Feeder coverage errors propagate unchanged; acceleration,
    vision, tape advance and nozzle physics are not modelled, and the document
    is never modified.

    ```shell
    open-smt pick-cycle panel.osmt --heads 4
    ```

37. **Pick-cycle matrix drawing.** `export --format pick-cycle-svg` renders the
    `pick-cycle` report as a cycle × head-slot matrix: one row per cycle, one
    column per head slot, used slots filled with the side colour and carrying the
    package plus reference designator, unused slots greyed out, and a cycle
    detail column repeating package, side, pick count and references. A summary
    band reports utilisation, head slots, picks, wasted slots and the cycles
    saved against a single-head program, with a utilisation bar. Export formats
    carry no options, so the sheet always plans four heads and says so in the
    subtitle; acceleration, vision, tape advance and nozzle physics are not
    modelled. It fails with the same feeder coverage codes as the report. Text is
    ASCII-only, the render is deterministic and byte-identical on repeat, and the
    document is never modified.

    ```shell
    open-smt export panel.osmt --format pick-cycle-svg --out pick-cycle.svg
    ```

38. **Consumption drawing.** `export --format consumption-svg` renders the
    `consumption` report for a fixed 100-board run: a horizontal bar per feeder
    lane stacks net picks and start-up scrap against the first-reel threshold
    (drawn as a dashed tick; a bar past it needs a splice), a value column lists
    picks, reel count and parts left on the last reel, and a reel-change timeline
    marks the board number at which each lane exhausts a loaded reel. The summary
    band reports picks, gross, scrap, splice count and short lanes. A legend
    explains the bars, the threshold and the changeover marker. The report is the
    same one as `feeder consumption`, so missing or undersized lanes fail the
    same way; partial reels, moisture exposure and changeover time are not
    modelled. Text is ASCII-only, the render is deterministic and byte-identical
    on repeat, and the document is never modified.

    ```shell
    open-smt export panel.osmt --format consumption-svg --out consumption.svg
    ```

## Sheet conventions

Every generated sheet has a drawing frame, a title block with the drawing name,
drawing number, work order (the document id), units, scale and face, and an
8 mm drawing margin that is also the frame inset. The content is measured and
fitted into that area, so the panel grid or chart fills at least 60 % of the
sheet instead of leaving a wide blank band. Bottom-side geometry is mirrored
about the X axis and the face is stated in the title block.

Labels are placed in the first free slot of eight candidates around their body
(above/below left, centre and right, plus both sides); a side label gets a leader
line and a label that finds no free slot is omitted rather than printed over a
neighbour. The side columns carry a package legend, so a dense board can print
reference designators only and still name every package. Every table measures its
own columns from the cell contents, drops empty columns, right-aligns numeric
cells, sets row height from the font and stretches to the available width; no
cell is clipped and no two columns overlap. Legends only list styles the sheet
actually draws.

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
| `feeder/unknown-id` | an update, removal or splice names a feeder that does not exist |
| `feeder/in-use` | a feeder is the only supplier of a used package or still has splices |
| `feeder/missing` | a used package has no feeder |
| `feeder/capacity` | loaded parts do not cover the component count |
| `feeder/splice-invalid` | a splice step is outside the placement program, or a lane repeats a step |
| `profile/invalid` | empty zones, non-positive length/temperature/speed/liquidus |
| `station/invalid` | a feeder station assignment requested a non-positive or non-finite slot pitch |
| `changeover/invalid` | a changeover requested a non-positive or non-finite time per reel change |
| `clearance/invalid` | a clearance check requested a non-finite or negative minimum gap, or a non-finite or non-positive maximum height |
| `reflow/invalid` | a process-window limit is non-finite or negative, or a minimum exceeds its maximum |
| `consumption/invalid` | a consumption plan requested fewer than one board |
| `schedule/invalid` | a feeder replenishment schedule requested fewer than one board |
| `paste/invalid` | a paste volume report requested a non-positive foil thickness, fewer than one board or a waste percentage outside 0–100 |
| `program/invalid` | a machine program file is malformed, a nozzle map is invalid, or an export did not read back identically |
| `pick-cycle/invalid` | a pick-cycle grouping requested fewer than 1 or more than 64 heads |

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
| `tests/component_list.rs` | document-order rows with side, rotation and height, first-seen package grouping and reference designators, height omitted when unknown, split allocation across loaded lanes matching the `feeders` report, unassigned components, empty document, determinism and unchanged document |
| `tests/panel_fit.rs` | exact-fit 2 × 3 arithmetic, leftover gaps, too-small dimensions, zero waste, gap effects, utilization, stored-gap default versus override, invalid maxima and gaps, determinism, unchanged document |
| `tests/bom.rs` | matching BOM, missing/extra/quantity findings, a refdes placed under another package, quantity-only rows, empty document and empty BOM, case-insensitive quoted headers, deterministic order-independent JSON, stable `bom/...` error codes, unchanged document bytes and revision, missing BOM file |
| `tests/takt.rs` | default machine model matching the placement program, changeover overhead segments, custom machine parameters, even multi-machine split and throughput, empty document zero time, 2-opt saving fields, feeder error propagation, invalid machine parameters, serde defaults, determinism, unchanged document |
| `tests/session.rs` | create, edit, reopen, export, in-use feeder, atomic save-as; partial component update (move, rotation normalisation, side and package changes, height set and cleared, untouched fields, invalid requests, one revision, determinism, round trip) |
| `tests/import.rs` | KiCad footprint mapping, rotation, both sides, reference sources, loss ledger, outline errors, duplicate/missing references, empty board, determinism, existing target; CSV row order, reordered/quoted/UPPERCASE columns, empty lines, side and rotation defaults, duplicate refdes, missing column, invalid number, invalid side, line numbers, negative coordinate, determinism, one revision, existing target |
| `tests/station.rs` | mean-placement-X order shortening the tape-bank travel, a matching stored order saving nothing, tie-breaks by feeder id, unused lanes last with `meanX: null`, empty document, feeder coverage propagation, invalid slot pitches, determinism, unchanged document |
| `tests/changeover.rs` | removal/swap/addition ordering with steps, swap counting twice, pitch-only swap, identical setups, empty current, ignored feeder coverage, structurally invalid documents, invalid change seconds, determinism, unchanged documents |
| `tests/reflow_window.rs` | default profile metrics and zone ramp rates, margin arithmetic, pass/fail verdict, heating-only profile, invalid limits, determinism, unchanged document |
| `tests/consumption.rs` | reels and last-reel remainder arithmetic, a short run, multi-lane package totals, empty document, zero boards, feeder coverage propagation, determinism, unchanged document |
| `tests/feeder_schedule.rs` | no reel for a covered run, replenishment after the board that exhausts the stock, multiple reels, a short lane and a zero-capacity lane through the report builder, zero capacity rejected by validation, empty document, zero boards, feeder coverage propagation, id order, determinism, unchanged document |
| `tests/splice_scrap.rs` | cross-splice consumption without resetting used quantity, splice delaying replenishment, leader/trailer and manual scrap separated from net picks, scrap edited through `updateFeeder`, invalid step/feeder/duplicate splice rejection with one revision, persisted splice history, deterministic reports, zero-capacity splice lane |
| `tests/panel_map.rs` | row-major instance offsets and panel arithmetic, document-order component mapping into every board, single-board identity, outside and non-finite component handling, empty document, invalid board and panel codes, determinism, unchanged document |
| `tests/first_article.rs` | program-order rows with lane pitch, package material grouping and side totals, recorded revision, fixed checklist, empty document, feeder coverage propagation, CSV header and empty check columns, determinism, unchanged document |
| `tests/machine_program.rs` | fixed header and program order, station/pitch/nozzle/speed fields, three-decimal canonical values, render-parse round trip, tampered field and reordered row mismatches, step-count mismatch, malformed files, empty document, invalid nozzle map, feeder coverage propagation, deterministic unchanged document, export-readback and verify-file paths |
| `tests/heatmap.rs` | step placement and inbound travel time, package ordering with cumulative shares, per-side origin restart, equality with the default takt board time, empty document, feeder coverage propagation, determinism, unchanged document |
| `tests/pick_cycle.rs` | consecutive same-package grouping, package and side cuts, one-head and default head counts, partial-cycle head-slot waste and utilization, empty document, invalid head counts, feeder coverage propagation, determinism and unchanged document |
| `tests/drawing.rs` | SVG panel/board/component/label/legend/title structure, XML escaping, rotated rectangle coordinates, byte-identical repeat renders, one-page A4 PDF with Helvetica and exact MediaBox, empty document, non-ASCII loss reporting, invalid board/panel codes, `svg`/`pdf` format parsing |
| `tests/stencil.rs` | chip rule aperture size after the area reduction and terminal offsets, rotated offsets, one aperture per terminal for SOIC/QFP/QFN/SOT-23/LGA, generic fallback and `unmatchedPackages`, determinism and unchanged document, CSV/SVG/PDF export contents, bottom sheet mirrored along X with reversed rotation, `stencil`/`stencil-svg`/`stencil-pdf` format parsing, invalid board code, non-ASCII loss reporting |
| `tests/clearance.rs` | height, overlap and gap findings with exact messages and rounded gaps, rotated bounding boxes, default and custom limits, opposite sides never paired, `clearance/invalid`, non-finite coordinates skipped, determinism and unchanged document, session forwarding |
| `tests/feeder_drawing.rs` | lane rows in feeder id order with slots, load bars and reference designators, valid hex fill and stroke attributes, `feeder/missing` propagation, empty setup framed sheet, byte-identical repeat renders, non-ASCII loss reporting, dropped scale note and panel multiplier annotation, `feeder-svg` format parsing, session export path |
| `tests/sequence_drawing.rs` | one numbered route per side with step markers, collision-avoiding step labels, travel length matching the drawn segments, valid hex fill and stroke attributes, one-page A4 PDF with Helvetica and the routes, empty program framed sheet, `feeder/missing` propagation, byte-identical repeat renders, non-ASCII loss reporting, `sequence-svg`/`sequence-pdf` format parsing, session export path |
| `tests/reflow_drawing.rs` | four-zone profile sheet with plot frame, profile polyline, peak marker, ambient/liquidus/peak references and zone labels, valid hex fill and stroke attributes, byte-identical repeat renders, non-ASCII loss reporting, `reflow-svg` format parsing, session export path |
| `tests/panel_map_drawing.rs` | row-major board rectangles with labels, component dots on every instance, origin cross, valid hex fill and stroke attributes, byte-identical repeat renders, non-ASCII loss reporting, `panel-svg`/`panel-map-svg` format parsing, session export path |
| `tests/heatmap_drawing.rs` | one heat dot per program step with Pareto package column, valid hex fill and stroke attributes, `feeder/missing` propagation, byte-identical repeat renders, non-ASCII loss reporting, `heatmap-svg` format parsing, session export path |
| `tests/paste.rs` | empty document zero report, exact rect and circle aperture arithmetic against the rule table, board and waste scaling, custom foil thickness, `paste/invalid` for every bad input, per-package CSV sheet, byte-identical repeat reports, unchanged document, session `paste-csv` file write |
| `tests/smoke.test.ts` (TypeScript) | session lifecycle, argument errors before the addon, native-gated balance, multi-lane, panel-fit, panel-map, first-article, BOM-check, takt, takt-heatmap, pick-cycle (default, one head, invalid SDK/CLI counts), machine-export/machine-verify, component-update, component-list, feeder-optimize, reflow-window, feeder-consumption, splice/scrap SDK edits, splice/scrap CLI commands, SVG/PDF placement drawing exports, feeder/sequence drawing exports and the stencil paste volume report through the SDK and CLI, and changeover smokes |

## Limitations

- Placement defaults to a deterministic nearest-neighbour heuristic. The
  optional 2-opt pass is a bounded local search over each side's route; it
  improves travel but does not guarantee an optimal tour. Neither mode models
  nozzles, head count, feeder positions or vision time.
- The feeder model has no lane index or tape direction. Splice records carry a
  placement step and are applied on the first board; they do not model joint
  position inside a tape, splice time, operator identity or a partial reel's
  remaining length. The feeder loading report numbers packages from 1 in name
  order, but those slot numbers are suggestions only: they do not describe a
  physical machine slot or a tape layout, and the report never writes a feeder
  setup.
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
- The parallel pick-cycle grouping is a measurement, not an optimiser: it groups
  consecutive same-package picks of the existing nearest-neighbour program and
  never reorders it. The package stands in for the nozzle because the document
  has no nozzle field, a side change starts a new cycle, and no acceleration,
  vision, tape advance, nozzle physics or head interchange is modelled.
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
- The reflow process window compares zone set-point metrics with caller limits.
  Ramp rates are derived from the set-points, not from a measured profile, and
  the report does not model a soak dwell band or a cooling slope after the last
  zone.
- The feeder consumption plan derives reels from lane capacity, splices and the
  per-board allocation. It models leader/trailer and manual scrap, but not
  partial reels or moisture exposure.
- The feeder replenishment schedule installs one full reel whenever the next
  board is not covered and applies splices at board granularity on the first
  board. It does not model partial reels, moisture exposure, reel load time,
  splice downtime or machine downtime. Allocation never assigns a lane more
  parts than it has loaded and validation rejects zero capacity, so a normal
  document never reports a short lane.
- The panel coordinate map assumes the stored axis-aligned panel array and
  row-major instance order. It does not rotate boards, add edge rails, reserve
  fiducial or tooling areas, or derive a panel outline, and it never changes the
  stored panel array.
- The component list is a read-only view of the stored document and the current
  feeder allocation. It reports no derived placement sequence, does not validate
  the document and does not model physical machine slots.
- The first-article sheet repeats the placement program with the allocated lane
  pitch and a fixed four-item checklist. It records no inspection result, models
  no component polarity or marking direction, and does not read a BOM.
- The takt heat map splits the analytical step time of the default machine model
  by step, package and side. It inherits every assumption of the cycle time
  estimate: constant head speed, fixed placement overhead, no acceleration,
  tape advance, vision or nozzle physics.
- The machine program is a neutral CSV interchange of the placement program. It
  is not a Fuji, Yamaha, Juki or ASM dialect, carries no vendor-specific fields
  and takes nozzle ids only from a caller-supplied package map; `station` is the
  document feeder order, not a physical machine slot. Reading a program back
  only compares it with the document, it does not drive a machine.
- Only the top and bottom sides are supported.
- Multi-lane scheduling assigns whole placement tasks to 2–16 lanes with a
  greedy or longest-processing-time-first rule and an optional pairwise-swap
  pass. The two-lane report is the special case of the same engine. It does not
  model conveyor lanes, lane transfer, nozzle counts or per-lane re-sequencing,
  and its result is not an optimal partition.
- Line balancing distributes whole components across 1–64 machines but does not
  emit vendor machine programs, model conveyor lanes or account for line
  changeover time.
- Vendor dialects are out of scope and reported as such; the neutral machine
  program is an interchange format, not a vendor dialect.
- Stencil apertures come from a fixed package rules table keyed by package
  name, not from footprint or pad-stack data. Each family carries one nominal
  terminal size and one area reduction factor; QFN centre thermal pads and
  step/stencil foil thickness are not modelled, the sheet draws a single board
  rather than a panel, and unrecognised packages fall back to the generic rule
  and are listed in `unmatchedPackages`.
- The clearance check reuses the same fixed package rules table, so body
  outlines are rule defaults rather than footprint data, rotated bodies are
  compared conservatively as axis-aligned bounding boxes, findings are warnings
  only, and components without a stored height are never height-checked.
- KiCad import covers the component list and a rectangular outline only. Nets,
  tracks, vias, pads, zones, graphics and 3D models are counted in the loss
  ledger and are not part of the document. Altium and ODB import remain
  unimplemented.
- CSV import covers the placement table only. The board has no outline and is
  sized to the coordinates, components carry no height, and unknown columns are
  refused instead of ignored. Nets, feeders, panel arrays and reflow profiles
  are not read from CSV.
- Placement drawings size component bodies from the fixed package rules table
  because the model stores no footprint data, draw labels only on board 1 of a
  multi-board panel and omit labels that would collide with a body or another
  label.
  They are a placement reference, not a fabrication or assembly drawing, and
  they replace non-ASCII label text with `?` (`drawing/non-ascii-text`).
- The feeder setup drawing is a tape-bank suggestion built from the same
  allocation as the feeders report; it models no machine slot, tape direction
  or pitch constraint and replaces non-ASCII text with `?`
  (`feeder/non-ascii-text`).
- The pick-and-place sequence drawing shows the default nearest-neighbour
  routes only: it carries no 2-opt or optimised program, models no
  acceleration, tape advance, vision or nozzle physics, and replaces
  non-ASCII text with `?` (`sequence/non-ascii-text`).
