# Developing Kition Board

How to find your way around the repository, build Board from source, and run its checks. To use
Board in your own app, start with the [README](../README.md).

## Repository layout

| Path | What |
|---|---|
| `kit.json` | The kit manifest (KHP 1.0 and Kition platform fields) |
| `crates/kdm/kdm-model` | `kition-board-kdm`: the schema, the materialised view, validation and repair, the file as a replica |
| `crates/engine/model` | `kition-board-model`: the commands, each one transaction with a parameter schema |
| `crates/engine/check` | `kition-board-check`: `board_check`, twelve codes (catalogue: `agent/checks.json`, fixtures: `conformance/`) |
| `crates/render/scene` | `kition-board-scene`: every element as an SVG fragment (the editor draws these) and the same scene rasterised for `render` |
| `crates/render/text` | `kition-board-text`: the fonts given to `addFont`, line measurement and wrapping that match the browser |
| `crates/formats/kboard` | `kition-board-kboard`: the `.kboard` ZIP container, as bytes |
| `bindings/wasm` | `kition-board-kit`: `BoardDocument` (`KitDocument`, contract 2 session members, extras), the editor's engine |
| `bindings/headless` | `kition-board`: the same document over the Kit Protocol, the headless executable |
| `web/` | The kit's editor on `@kition/sdk/editor`: the canvas draws the engine's fragments over a page replica and sends each gesture as one engine command (R2c) |
| `agent/` | The agent guide and the check catalogue |
| `conformance/` | One fixture per check code |
| `cases/` | The test cases of the feature catalogue (`features.yaml`, beside `kit.json`), and `waivers.yaml` |
| `assets/readme/` | The README's pictures ([Take the README pictures](#take-the-readme-pictures)) |
| `docs/` | This file, the design (`design.md`), the writing format, gaps, roadmap, the quality reference ([quality.md](quality.md)), attribution; `parity/` |
| `tools/` | `xtask`: `cargo xtask ci`, `wasm`, `manifest` |

## The file and the engine

- **One replica, one file.** A board is a KDM replica (`crates/kdm/kdm-model/model/schema.json`,
  schema `kit.board` v2). A `.kboard` file (`application/vnd.kition.board+zip`) is a ZIP holding
  the replica's content (its state, sealed, no history) and the pictures it shows as blobs; the
  history stays in the host's history store.
  New files are `Untitled board.kboard`. The details are in [design.md](design.md).
- **One engine everywhere.** The engine implements kition-sdk's `KitDocument` on a KDM session
  (engine contract 2). It runs in the page as WebAssembly in a Web Worker under the kit's own
  editor (`web/`, on `@kition/sdk/editor`), and headless as `kition-board` for agents, which write
  boards through commands ([writing-format.md](writing-format.md)).

## Build and check

```sh
cargo xtask ci              # fmt, clippy -D warnings, tests (incl. check_contract), wasm check
cargo xtask manifest        # kition-sdk tools/check-manifest.mjs on kit.json
cargo build --release -p kition-board
```

`cargo xtask ci` is the check every change passes ([quality.md](quality.md)), in the kit's CI run
on GitHub (`.github/workflows/ci.yml` calls Kition's reusable kit CI, which also runs clippy,
`cargo test --workspace` and every `test:*` script of `web/`). On your machine, run only the fast
checks: `cargo fmt --all -- --check`, `cargo check -p <touched crates> --all-targets`,
`cargo xtask wasm` and `pnpm typecheck` in `web/`. A change is done when the CI run of HEAD passes
and HEAD is on `origin/main` (Kition's `scripts/kit-ci.sh` `push`, `wait`, `land`). kition-sdk is
a git dependency pinned by commit in `Cargo.toml`; the toolchain is Rust 1.95 or newer.

The editor (`web/`) and its development host:

```sh
cd web && pnpm install
sh scripts/build-engine.sh   # the engine as WebAssembly into public/engine, the fonts into public/fonts
pnpm dev                     # dev.html opens public/sample/Sample.kboard
pnpm test:flow               # tests/flow.mjs in Chromium and WebKit (proxy variables unset)
```

The flow's commit budget is spike S2-1's 16 ms (one frame at 60 Hz), a product target measured on
a maintainer's machine. On a slower machine, `KITION_BUDGET_SCALE=<n>` (a number ≥ 1) multiplies
it; it is 1 by default and 2 on GitHub's hosted runners (`GITHUB_ACTIONS=true`), whose 4 shared
vCPUs measured a 22 ms median. The pan budgets are paced by the display and are not scaled.

The engine build (`web/scripts/build-engine.sh`) copies its fonts from a kition-fonts checkout
beside this repository (or `KITION_FONTS_DIR`). On machines where Apple's toolchain cannot archive
WebAssembly, set `CC_wasm32_unknown_unknown` and `AR_wasm32_unknown_unknown` to an LLVM `clang`
and `llvm-ar` for the engine build.

By default `pnpm install` in `web/` installs the published `@kition/sdk` and `@kition/tessera`
packages from npm, at the versions the committed `web/pnpm-lock.yaml` pins, so the editor needs no
other checkout. Until their first release under `@kition` they install from the rehearsal copies by
npm alias (`npm:@kitionstage/sdk@…`, `npm:@kitionstage/tessera@…` in `web/package.json`), so the code
imports the final names. To work against local checkouts instead, run `KITION_LINK=1 pnpm install`
in `web/`: `web/.pnpmfile.cjs` then links `@kition/sdk` from the kition-sdk checkout
(`packages/sdk`) and `@kition/tessera` from the kition-tessera checkout (`packages/tessera`) next to
this repository (or the ones `KITION_SDK` and `KITION_TESSERA` name), each one whose package is
there. Both packages' `exports` point at their built `dist/`, so run `pnpm build` in those two
folders first. The linked install rewrites `web/pnpm-lock.yaml`; do not commit that lockfile.

## Take the README pictures

The README's editor pictures (`assets/readme/hero.png`, `see-1.png` … `see-4.png`) are taken by
[`web/scripts/readme-shots.mjs`](../web/scripts/readme-shots.mjs) on kition-sdk's shared runner
(`tools/readme-shots/shots.mjs`). It starts the development host and opens the showcase samples
`board/cafe-launch-mind-map`, `board/web-shop-architecture`, `board/start-stop-continue-retro` and
`board/bike-coop-swot-analysis` from a kition-showcase checkout next to this repository (or the
one `KITION_SHOWCASE` names). Each picture is taken at 1440 × 900, device scale 2, in the Classic
display mode, light theme and English. It needs the built engine in `web/public/` and a
kition-sdk checkout next to this repository (or `KITION_SDK`); it is a tool, not a test.

```sh
cd web
node scripts/readme-shots.mjs              # every picture, or name some: hero see-2
pngquant --quality 70-90 --force --ext .png ../assets/readme/hero.png ../assets/readme/see-*.png
```

Look at every picture before committing it, and keep each one under 400 KB. `made-1.png` …
`made-4.png` are the `thumb.png` of four showcase samples (`checkout-user-journey`,
`design-studio-org-chart`, `notes-app-roadmap-2027`, `sprint-kanban-board`), copied as they are.

## Publish

Kition ships the kit's package, `kition-board-<version>.tgz` (the `board-ui/` folder), a release
asset of this repository tagged `package-v<version>`. It is built on GitHub, never on a developer
machine: the Package workflow (`.github/workflows/package.yml`, which calls Kition's reusable
`kit-package.yml`) builds the commit it is started on, with `gh workflow run package.yml -R
KitionAI/kition-board`, and publishes the Release with the package's lock entry,
`kition-board-<version>.json`. Kition pins it with `node scripts/pin-kits.mjs board` (see
`docs/kits.md` there). A version is published once: bump `version` in `kit.json` to publish a
change.
