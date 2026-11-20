# Changelog

Changes to Kition Board, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

## 0.1.3

- In the dark scheme a card's badge glyph is drawn in the paper's colour, so cards have no light part left.
- `kit.json` declares the engine's editor state (`engine.editorState`), so a host that does not run the engine's Rust code reads `inspect()` without it.

## 0.1.2

- A person reaches every page of a board: page tabs under the canvas show the pages and the one shown; a click or Enter switches (the file stays saved, and Undo does not take back a switch), and Add page, rename (double-click or F2), duplicate, move left or right and delete (not the last page) are in the tabs and their menu, each one undo step. Each page keeps its own view and the selection stays on its page.
- View > New page shows the page it makes.
- When the page shown goes (deleted by the person, by an Undo of a page they made, by an agent or by a co-editor), the editor shows the page the person came from, else the page before it, instead of jumping to the first page; Redo, or an Undo that brings the page back, shows it again. An agent's step still never moves the person's page.
- The minimap draws the page shown at once when the person switches pages.

## 0.1.1

- The board follows the host's dark scheme: in a dark host the canvas, the default ink and the minimap are drawn dark (lines and text at WCAG AA), and they repaint when the host switches scheme, without a reload.
- An agent can edit an open board: the editor reads the schema of the board's model description, whose text marks are named by key (kition-sdk 1637bf8; the editor now builds on `@kition/kit-editor`, `@kition/kdm`, `@kition/kit-host`, `@kition/kit-ui-protocol` and `@kition/kit-dev-host` 0.5.1). Before, every agent step on an open board failed with "the schema does not parse".

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
