# Documentation

Maintainer documentation for OpenSMT. Product narrative lives in the
[repository README](../README.md) and the registry page lives in
[projects/packages/open-smt/README.md](../projects/packages/open-smt/README.md).

| Document | Contents |
|----------|----------|
| [architecture.md](architecture.md) | Layers, module boundaries, data flow, error semantics |
| [capabilities.md](capabilities.md) | Sequencing, allocation, reflow algorithms and limitations |
| [cli.md](cli.md) | Every `open-smt` command, flag and example |
| [development.md](development.md) | Build, test, native staging and release |

## Source of truth

1. `projects/crates/smt-model` owns the document model, validation, feeder
   allocation, placement sequencing, reflow and every report.
2. `projects/crates/smt-napi` maps values only.
3. `projects/packages/open-smt` is the TypeScript SDK and CLI.
4. Capability claims live only in [capabilities.md](capabilities.md) and in
   `CAPABILITIES_JSON`.

## Tests

Rust integration tests live in `projects/crates/smt-model/tests/`; TypeScript
tests live in `projects/packages/open-smt/tests/`. Nothing is mixed into `src/`.
