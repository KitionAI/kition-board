# Changelog

Changes to Kition Board, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

## 0.1.0

First public release of the source, under the Apache License 2.0. It contains:

- The Rust engine: boards of pages holding eighteen shapes, text, sticky notes, connectors bound
  to the shapes they join (with labels and waypoints), frames, freehand strokes, pictures, cards
  and mind maps; groups, stacking, locks, align and distribute; templates; and export of a page
  or a frame to PNG, SVG and PDF from the engine's own scene.
- The kit engine (`kition-board-kit`), native and WebAssembly: a `.kboard` file is the board's KDM
  replica (snapshot, change log and blobs), replicas merge concurrent edits with deterministic
  repairs, and undo is per person and per agent. A check catalogue of 12 codes reports what a
  board gets wrong, with the commands that fix it.
- The headless kit (`kition-board`) for agents over the Kit Protocol: it inspects and queries a
  board, edits it with the editor's own commands, previews a page with the fonts it ships, and
  works in drafts. A write names the revision it read and is refused when someone else changed
  what it rests on.
- The kit's editor (`web/`): the board canvas, the tool rail, the Style panel, mind-map controls,
  cards, the template gallery and the export dialog, in kit-editor's frame.
- NOTICE and docs/attribution.md list the libraries the engine links, the fonts the package and
  the executable ship, and Loro (MIT), bundled by the editor and linked by the engine.
