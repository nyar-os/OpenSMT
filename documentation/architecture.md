# Architecture

OpenSMT is one native core with three thin surfaces. This document records the
boundaries a change must respect.

## Layers

```
projects/crates/smt-model      document model, validation, allocation, sequencing, balancing, reflow, reports
projects/crates/smt-napi       Node-API exports; values in, JSON strings out
projects/packages/open-smt     TypeScript SDK (session + document) and CLI
projects/sites/homepage        static product site (VMZ)
```

## Data flow

1. `OpenSmt.create` writes a fresh `.osmt` document; `OpenSmt.open` parses one.
   `OpenSmt.import` and `OpenSmt.importCsv` write a new document from a KiCad
   PCB or a CSV placement table.
2. An edit mutates a candidate document in memory, validates it structurally,
   writes it atomically, then returns a `DocumentSummary`.
3. `feeders` validates and allocates every component to a lane.
4. `placement` sequences the allocated components and returns the program.
5. `balance` assigns components to 1–64 machines and reports per-machine
   programs and feeder coverage. `schedule dual` assigns placement tasks to two
   lanes by cumulative task time and reports per-lane sequences and makespan.
6. `reflow` evaluates the profile.
7. `export` writes the program CSV, feeder CSV, JSON, Markdown or an SVG/PDF
   placement drawing through the same atomic writer.

Because the Node-API addon, the SDK and the CLI all call the same
session, they cannot disagree about a program or an error code.

## Invariants

1. **The Rust session is the only implementation.** No allocation, sequencing,
   balancing, dual-lane, panel or reflow rule may be reimplemented in TypeScript.
2. **The Node-API layer maps values only.** Each export accepts a path or JSON
   request and returns a JSON string.
3. **The TypeScript layer parses arguments and formats output.** It owns help
   text and exit codes (`capability/*` → `2`, everything else → `1`).
4. **The capability matrix is the only support claim.**
5. **Editing validates structure; analysis validates coverage.** A document may
   be saved while the feeder setup is still incomplete, because components are
   normally entered before feeders. `validate`, `placement` and `report` report
   `feeder/missing` or `feeder/capacity` until coverage is complete.
6. **Writes are atomic.** The writer uses a temporary file and a rename.
7. **Output is deterministic.** No timestamps and no randomised tie-breaks.
8. **Units are millimetres.** Temperatures are degrees Celsius; nothing is
   converted.

## Document format

```json
{
  "format": "open-smt",
  "version": 1,
  "revision": 3,
  "units": "mm",
  "board": { "width": 100, "depth": 80, "thickness": 1.6 },
  "panel": { "rows": 1, "columns": 1, "gapX": 0, "gapY": 0 },
  "components": [
    { "refdes": "R1", "package": "0603", "side": "top", "x": 10, "y": 10,
      "rotation": 0, "height": null }
  ],
  "feeders": [
    { "id": "F1", "package": "0603", "pitch": 4, "capacity": 500, "loaded": 500,
      "leaderWaste": 0, "trailerWaste": 0, "scrap": 0 }
  ],
  "splices": [
    { "feeder": "F1", "sequence": 12, "leaderWaste": 0, "trailerWaste": 0, "scrap": 0 }
  ],
  "profile": {
    "conveyorSpeed": 600,
    "liquidus": 217,
    "zones": [
      { "name": "preheat", "length": 300, "temperature": 150 },
      { "name": "soak", "length": 400, "temperature": 180 },
      { "name": "reflow", "length": 300, "temperature": 245 },
      { "name": "cool", "length": 400, "temperature": 30 }
    ]
  }
}
```

`revision` increases on every successful edit and never decreases.

## Error semantics

Errors carry a stable `code` and a human `message`. The CLI prints
`code: message` to stderr. `capability/not-implemented` is reserved for declared
but unimplemented routes and maps to exit code `2`.

## Node-API surface

`version`, `capabilities`, `create`, `open`, `import`, `import_csv`, `save`, `board_set`, `panel_set`,
`component_add`, `component_update`, `component_remove`, `feeder_add`, `feeder_update`, `feeder_splice`, `feeder_remove`,
`profile_set`, `validate`, `placement`, `feeders`, `balance`, `dual_lane`,
`reflow`, `export`, `report`.
