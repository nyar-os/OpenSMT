# OpenSMT

SMT assembly planning in plain text: a placement program, a feeder setup and a
reflow check from one `.osmt` job.

## 💡 What is OpenSMT?

OpenSMT turns a board, a component list and a feeder table into the three things
a line needs before it runs:

1. **a placement program** — components sequenced top side first, nearest-first
   from the machine origin, with a feeder lane for each step;
2. **a feeder setup sheet** — each lane's package, loaded quantity, assigned
   reference designators and remaining parts;
3. **a reflow evaluation** — peak temperature, total time and time above
   liquidus from a zone profile and conveyor speed.

Everything is deterministic: the same document produces the same program, the
same allocation and the same reflow numbers. The Rust session owns every rule;
the CLI, Node-API addon and TypeScript SDK are thin surfaces over it.

Typical uses:

- preparing a repeatable feeder setup before a small production run;
- checking that the loaded reels cover the build quantity;
- auditing a reflow profile against the alloy liquidus;
- teaching placement sequencing and panel arithmetic with numbers you can check
  by hand.

**OpenSMT is not:**

- a paste-aperture or stencil generator;
- a vendor machine-program translator (no Fuji, Yamaha, Juki or ASM dialect);
- a dual-lane scheduler or line-changeover planner;
- a place where an unsupported feature silently degrades — an unimplemented
  route returns `capability/not-implemented` with exit code `2`.

## 🚀 Getting started

### 1. Point your agent at the repository

OpenSMT does not ship an agent skills package. Point your agent at the
documentation and the `open-smt` CLI:

```text
Use OpenSMT (`open-smt`) to plan a 100 x 80 mm board with two 0603 resistors and
one QFN-32. Load a 0603 feeder and a QFN feeder, generate the placement program,
and report the peak reflow temperature and time above liquidus. Read
documentation/cli.md and documentation/capabilities.md before claiming a result.
```

### 2. Install and run

```shell
pnpm add @wae/open-smt

pnpm exec open-smt create panel.osmt
pnpm exec open-smt board panel.osmt --width 100 --depth 80 --thickness 1.6
pnpm exec open-smt component add panel.osmt --refdes R1 --package 0603 --x 10 --y 10
pnpm exec open-smt component add panel.osmt --refdes R2 --package 0603 --x 40 --y 20
pnpm exec open-smt component add panel.osmt --refdes U1 --package QFN-32 --x 60 --y 40
pnpm exec open-smt feeder add panel.osmt --id F1 --package 0603 --pitch 4 --capacity 500
pnpm exec open-smt feeder add panel.osmt --id F2 --package QFN-32 --pitch 12 --capacity 50
pnpm exec open-smt validate panel.osmt
pnpm exec open-smt placement panel.osmt
pnpm exec open-smt placement panel.osmt --optimize
pnpm exec open-smt reflow panel.osmt
pnpm exec open-smt export panel.osmt --format program --out program.csv
```

`create` starts from a 100 × 80 × 1.6 mm board, a single-board panel and a
four-zone profile (600 mm/min, liquidus 217 °C), so the file is useful
immediately.

The SDK is the same engine with a typed API:

```ts
import { OpenSMT } from '@wae/open-smt';

const smt = new OpenSMT();
const document = await smt.create('panel.osmt');
await document.addComponent({ refdes: 'R1', package: '0603', x: 10, y: 10 });
await document.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 500 });
const program = await document.placement();
const reflow = await document.reflow();
console.log(program.entries.length, reflow.timeAboveLiquidus);
await smt.dispose();
```

Exit code `1` means invalid input; `2` means the requested capability is not
implemented.

## ✨ Highlights

- **Deterministic placement** — top side first, nearest remaining component,
  ties broken by reference designator.
- **Optional 2-opt** — `--optimize` shortens each side's route beyond nearest
  neighbour with a bounded, deterministic local search; `travelSaved` reports
  the gain.
- **Feeder allocation by id** — every component gets a lane; multiple lanes of
  one package split the load in id order.
- **Panel arithmetic** — board count, panel width and depth including gaps.
- **Reflow integration** — time above liquidus is integrated over linear zone
  ramps, not approximated by zone count.
- **Machine-readable setup** — placement CSV, feeder CSV, JSON and Markdown.
- **Honest validation** — every finding carries a stable code; the capability
  matrix is the only support claim.
- **One native core** — the Rust session owns every rule; bindings only map
  values.

## 🧪 Examples

- [Package README — CLI and SDK usage](projects/packages/open-smt/README.md)
- [Placement and allocation tests](projects/crates/smt-model/tests/place.rs)
- [Reflow integration tests](projects/crates/smt-model/tests/reflow.rs)
- [Validation codes](projects/crates/smt-model/tests/validation.rs)
- [Session, atomic save and export](projects/crates/smt-model/tests/session.rs)
- [Capability matrix](documentation/capabilities.md)

## 📚 Documentation

| Document | Contents |
|----------|----------|
| [documentation/architecture.md](documentation/architecture.md) | Layers, invariants, data flow |
| [documentation/capabilities.md](documentation/capabilities.md) | Sequencing, allocation, reflow and limitations |
| [documentation/cli.md](documentation/cli.md) | Every `open-smt` command, flag and example |
| [documentation/development.md](documentation/development.md) | Build, test, native staging and release |
| [projects/packages/open-smt/README.md](projects/packages/open-smt/README.md) | The npm package page |

## 📜 License

[Mozilla Public License 2.0](License.md)
