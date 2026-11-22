# Development

## Layout

```
Cargo.toml                     Rust workspace (smt-model, open-smt-napi)
package.json                   pnpm workspace scripts
wae.config.ts                  native targets and staging
projects/crates/smt-model      engine, CLI binary `osmt`
projects/crates/smt-napi       Node-API addon
projects/packages/open-smt     TypeScript SDK and `open-smt` CLI
projects/packages/open-smt-*   per-platform addon placeholders
projects/sites/homepage        static product site
documentation/                 maintainer documentation
```

## Prerequisites

- Rust stable with `cargo`, `rustfmt` and `clippy`;
- Node.js 22.6 or newer;
- pnpm 10.28.

## Commands

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

## Native addon

`wae.config.ts` declares five targets (`win32-x64`, `darwin-x64`,
`darwin-arm64`, `linux-x64`, `linux-arm64`). Each target stages
`open-smt.<target>.node` into `projects/packages/open-smt-<target>`. The SDK
loads `@wae/open-smt-<platform>-<arch>`; set `OPEN_SMT_NATIVE` to an absolute
path to test a locally built addon.

## Tests

- Rust integration tests: `projects/crates/smt-model/tests/*.rs`.
- TypeScript tests: `projects/packages/open-smt/tests/*.test.ts`, compiled to
  `dist-test/` by `tsconfig.test.json`.
- `dist-test/` is ignored.

A behavioural change needs a test that fails before the change. A new capability
also updates `documentation/capabilities.md` and `CAPABILITIES_JSON`.

## Release

Tag `v*` and push. `.github/workflows/publish-npm.yml` builds every target,
stages the addon into the platform packages and publishes through npm Trusted
Publisher. `check-rust.yml` runs formatting, clippy and tests;
`check-typescript.yml` runs the frozen install, typecheck and build.

## Style

- Keep `smt-model` free of argument parsing and CLI text.
- Keep `smt-napi` free of domain rules.
- Keep the TypeScript layer free of allocation and reflow rules.
- Comments explain non-obvious intent, not syntax.
