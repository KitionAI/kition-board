# Attribution

The Board kit is Kition's own code (Apache-2.0, [LICENSE](../LICENSE)).
The engine links these libraries; every one is shipped inside the kit's WebAssembly module and the
headless `kition-board` executable.

| Library | License | Through |
|---|---|---|
| Kition SDK (`kition-sdk`, `kition-kdm`, `kition-headless`, `kition-kit-server`) | Apache-2.0 | direct, pinned by commit in `Cargo.toml` |
| Loro (`loro`) | MIT | `kition-kdm` (the KDM's replication engine) |
| serde, serde_json | MIT OR Apache-2.0 | direct |
| zip | MIT | direct (the `.kboard` container) |
| sha2 | MIT OR Apache-2.0 | direct (blob hashes) |
| thiserror | MIT OR Apache-2.0 | direct |
| ttf-parser | MIT OR Apache-2.0 | direct (text metrics) |
| resvg, usvg | Apache-2.0 OR MIT | direct (the raster and the PNG and SVG exports) |
| tiny-skia | BSD-3-Clause | `resvg` |
| krilla, krilla-svg | MIT OR Apache-2.0 | direct (the PDF export) |
| base64 | MIT OR Apache-2.0 | direct (exported files in command answers) |

Fonts come from [KitionAI/kition-fonts](https://github.com/KitionAI/kition-fonts) (per-file author,
source and SHA-256 in its ATTRIBUTION.md), not from this repository. `web/scripts/build-engine.sh`
copies them, each with its license file, into the package's `fonts/`, and
`bindings/headless/build.rs` compiles them into the `kition-board` executable. The WebAssembly
module embeds none. [NOTICE](../NOTICE) carries their copyright lines and the OFL text.

| Font | License | Use |
|---|---|---|
| Inter (`Inter-Variable.ttf`) | OFL-1.1 | Latin text, measured and drawn |
| Droid Sans Fallback (`DroidSansFallbackFull.ttf`) | Apache-2.0 | Chinese and Japanese text |

The full dependency list with versions is `Cargo.lock`; `cargo tree -e normal` shows which crate
brings in which. When a dependency is added, add its row here in the same change.

The editor in `web/` is built on `@kition/sdk` (kition-sdk, Apache-2.0) and draws Kition Tessera's
icons from `@kition/tessera` (Apache-2.0); its npm dependencies are listed in `web/package.json`
under their own licenses.
