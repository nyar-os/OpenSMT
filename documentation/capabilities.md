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
| feeder setup with package, pitch, capacity and loaded quantity | ✅ supported |
| deterministic placement sequence | ✅ supported |
| optional 2-opt placement optimisation | ✅ supported |
| feeder allocation by id and loaded quantity | ✅ supported |
| multi-feeder split of one package | ✅ supported |
| panel board count and panel dimensions | ✅ supported |
| reflow zone report and peak temperature | ✅ supported |
| time above liquidus over linear ramps | ✅ supported |
| placement program CSV | ✅ supported |
| feeder setup CSV | ✅ supported |
| JSON / Markdown reports | ✅ supported |
| paste aperture / stencil generation | ❌ not implemented |
| vendor machine programs (Fuji, Yamaha, Juki, ASM) | ❌ not implemented |
| line balancing | ✅ supported |
| dual-lane scheduling | ❌ not implemented |
| KiCad PCB (`.kicad_pcb`) import | ✅ supported |
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
6. **Panel arithmetic.** `boardCount = rows × columns`,
   `panelWidth = columns × width + (columns − 1) × gapX`, and
   `panelDepth = rows × depth + (rows − 1) × gapY`.
7. **Reflow.** The board enters the first zone at 25 °C. Each zone ramps linearly
   from the previous set-point to its own set-point over
   `length / conveyor_speed × 60` seconds. Time above liquidus is integrated over
   each ramp, including partial crossings in either direction. Peak temperature
   is the highest set-point.

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

## Validation

| Code | Meaning |
|------|---------|
| `document/format`, `document/version`, `document/units` | not an OpenSMT millimetre document |
| `import/parse` | the file is not a KiCad PCB S-expression |
| `import/outline-unsupported` | `Edge.Cuts` is missing or is not one axis-aligned rectangle |
| `board/invalid` | a board size is not finite and positive |
| `panel/invalid` | zero rows/columns or a negative gap |
| `balance/invalid-machines` | a balance requested fewer than 1 or more than 64 machines |
| `component/invalid` | empty refdes/package, non-finite values or bad rotation |
| `component/duplicate-refdes` | a reference designator is declared twice |
| `component/outside-board` | a component lies outside the board |
| `feeder/invalid` | empty id/package, non-positive pitch/capacity, loaded > capacity |
| `feeder/duplicate-id` | a feeder id is declared twice |
| `feeder/missing` | a used package has no feeder |
| `feeder/capacity` | loaded parts do not cover the component count |
| `profile/invalid` | empty zones, non-positive length/temperature/speed/liquidus |

Warnings do not make `ok` false: `component/none`, `feeder/none`,
`feeder/unused`, `component/rotation` (not a multiple of 90) and
`profile/below-liquidus`.

## Tests

| File | Asserts |
|------|---------|
| `tests/place.rs` | allocation order, nearest-first sequence, tie-breaks, panel size, multi-feeder split, determinism, 2-opt never longer than nearest neighbour, an improving crossing route, unchanged single/empty documents, additive report fields, byte-identical default output |
| `tests/balance.rs` | single-machine parity, component conservation, side order, determinism, empty machines, invalid counts, per-machine feeder shortages, independent per-machine 2-opt, imbalance identity |
| `tests/reflow.rs` | default profile numbers, exact single-zone integral, below-liquidus profile, zone order |
| `tests/validation.rs` | every code above and warning-only documents |
| `tests/session.rs` | create, edit, reopen, export, in-use feeder, atomic save-as |
| `tests/import.rs` | KiCad footprint mapping, rotation, both sides, reference sources, loss ledger, outline errors, duplicate/missing references, empty board, determinism, existing target |
| `tests/smoke.test.ts` (TypeScript) | session lifecycle, argument errors before the addon and a native-gated balance smoke |

## Limitations

- Placement defaults to a deterministic nearest-neighbour heuristic. The
  optional 2-opt pass is a bounded local search over each side's route; it
  improves travel but does not guarantee an optimal tour. Neither mode models
  nozzles, head count, feeder positions or vision time.
- The feeder model has no lane index, tape direction or splice handling.
- Reflow uses set-points, not measured thermocouple data, and models no
  convection or board thermal mass.
- Only the top and bottom sides are supported; there is no dual-lane
  scheduling.
- Line balancing distributes whole components across 1–64 machines but does not
  emit vendor machine programs, model conveyor lanes or account for line
  changeover time.
- Vendor dialects and paste apertures are out of scope and reported as such.
- KiCad import covers the component list and a rectangular outline only. Nets,
  tracks, vias, pads, zones, graphics and 3D models are counted in the loss
  ledger and are not part of the document. Altium and ODB import remain
  unimplemented.
