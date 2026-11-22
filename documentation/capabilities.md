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
| line balancing | ❌ not implemented |
| dual-lane scheduling | ❌ not implemented |
| KiCad / Altium / ODB import | ❌ not implemented |

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
3. **Travel and time.** Travel is the sum of straight-line distances inside each
   side, with a fresh origin per side. Estimated placement time is
   `components × 0.6 s + travel / 200 mm·s⁻¹`.
4. **Panel arithmetic.** `boardCount = rows × columns`,
   `panelWidth = columns × width + (columns − 1) × gapX`, and
   `panelDepth = rows × depth + (rows − 1) × gapY`.
5. **Reflow.** The board enters the first zone at 25 °C. Each zone ramps linearly
   from the previous set-point to its own set-point over
   `length / conveyor_speed × 60` seconds. Time above liquidus is integrated over
   each ramp, including partial crossings in either direction. Peak temperature
   is the highest set-point.

## Validation

| Code | Meaning |
|------|---------|
| `document/format`, `document/version`, `document/units` | not an OpenSMT millimetre document |
| `board/invalid` | a board size is not finite and positive |
| `panel/invalid` | zero rows/columns or a negative gap |
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
| `tests/place.rs` | allocation order, nearest-first sequence, tie-breaks, panel size, multi-feeder split, determinism |
| `tests/reflow.rs` | default profile numbers, exact single-zone integral, below-liquidus profile, zone order |
| `tests/validation.rs` | every code above and warning-only documents |
| `tests/session.rs` | create, edit, reopen, export, in-use feeder, atomic save-as |
| `tests/smoke.test.ts` (TypeScript) | session lifecycle and argument errors before the addon |

## Limitations

- Placement is a deterministic nearest-neighbour heuristic, not an optimiser; it
  does not model nozzles, head count, feeder positions or vision time.
- The feeder model has no lane index, tape direction or splice handling.
- Reflow uses set-points, not measured thermocouple data, and models no
  convection or board thermal mass.
- Only the top and bottom sides are supported; there is no dual-lane or
  multi-machine scheduling.
- Vendor dialects and paste apertures are out of scope and reported as such.
