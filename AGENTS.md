# AGENTS.md

Guidance for agents working in the OpenSMT repository.

## Purpose

OpenSMT is an SMT assembly-planning product. A document is one `.osmt` JSON file;
the product exposes it through a Rust session, a Node-API addon, a TypeScript SDK
and the `open-smt` CLI.

## Documentation map

| File | Audience |
|------|----------|
| [README.md](README.md) | Product landing page |
| [projects/packages/open-smt/README.md](projects/packages/open-smt/README.md) | npm registry page |
| [documentation/architecture.md](documentation/architecture.md) | Module boundaries and data flow |
| [documentation/capabilities.md](documentation/capabilities.md) | Allocation, sequencing, reflow, tests, limitations |
| [documentation/cli.md](documentation/cli.md) | Command and flag reference |
| [documentation/development.md](documentation/development.md) | Build, test, native staging, release |
| [documentation/README.md](documentation/README.md) | Maintainer doc index |

## Architecture invariants

1. **The Rust session is the only implementation.** Document I/O, editing,
   validation, feeder allocation, placement sequencing, reflow and reports live
   in `projects/crates/smt-model`. Do not reimplement any rule elsewhere.
2. **The Node-API layer maps values only.** Each export accepts a path or JSON
   request and returns a JSON string.
3. **The TypeScript SDK/CLI must not duplicate domain logic.** It parses
   arguments, calls native functions, formats output and maps error codes to exit
   codes (`capability/*` → `2`, other failures → `1`).
4. **The capability matrix is the only support claim.** If a feature is not
   marked supported in `documentation/capabilities.md` and `CAPABILITIES_JSON`,
   it is not supported.
5. **Unsupported work fails explicitly.** Return `capability/not-implemented`
   instead of approximating silently.
6. **Editing validates structure; analysis validates coverage.** A component may
   be saved before its feeder exists; `validate`, `placement` and `report` keep
   reporting `feeder/missing` or `feeder/capacity` until coverage is complete.
7. **Edits persist atomically.** Every edit writes through the temporary-file and
   rename path. There is no undo stack.
8. **Output stays deterministic.** No timestamps and no randomised tie-breaks.
9. **Units are millimetres; temperatures are degrees Celsius.** Nothing is
   converted.

## Verified commands

```shell
pnpm install
pnpm run build:ts
pnpm --filter @wae/open-smt run typecheck
pnpm --filter @wae/open-smt run test

cargo fmt -p smt-model -p open-smt-napi
cargo test -p smt-model -p open-smt-napi --offline
cargo clippy -p smt-model -p open-smt-napi --offline --all-targets -- -D warnings
```

`pnpm run build` performs the full native → TypeScript → staging build.
`pnpm run build:napi` builds only the addon and `pnpm run stage:native` copies it
into the platform packages. The addon is not committed.

## Change checklist

- Search the repository before assuming a command exists; verify flags against
  `projects/packages/open-smt/src/cli.ts`. The CLI exists only in TypeScript,
  assembled with `@wae/commander` on top of the SDK; there is no Rust binary.
- Add or update a test for every behavioural change.
- Keep the Rust session, Node-API surface, SDK types and CLI in sync when a
  field or analysis is added or removed.
- Update `documentation/capabilities.md`, `documentation/cli.md` and
  `CAPABILITIES_JSON` when the support claim changes.
- Keep tests in `tests/`; never place a test beside `src/`.
- Run the checks above plus `git diff --check` before committing.
- Stage explicit paths; never `git add -A`. Do not push unless asked.

## Commit style

Use a one-line subject with an emoji prefix:

- `✨ Add ...` — new capability or user-visible feature
- `🐛 Fix ...` — bug fix
- `📝 ...` — documentation, comments or prose

Keep the subject imperative and specific. If a change spans documentation and
behavior, split it into separate commits.
