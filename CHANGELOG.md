# Changelog

Changes to Kition Board, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

## Unreleased

- A `.kboard` file holds the board's content, not its history (kition-sdk rule M16b, data model 7.4): `content.json` (deflated), the board's state in the KDM content encoding under the same document id and element ids, with no version and no origins, and the pictures the board shows. No change log, no deleted text or elements, no overwritten values, no names of the people or agents who edited; `meta.json` holds only the format, kit, KDM and schema versions and the document id. An unchanged board saves to the same bytes. Files of earlier layouts (`content.bin`, or `snapshot.bin` and `changes/`) are not read.
- A board opens on the host's stored replica with its file taken in: as it is when both hold the same state, and otherwise with the file's state written by element id as one import step.
- The engine runs on kition-sdk's plain document model (no CRDT): Kition Desktop is for one person, so boards are no longer merged from concurrent edits; the page's replica follows the engine's changes in order. Loro is no longer bundled or linked.

## 0.1.5

- An undo group whose changes come to nothing (a `model.batch` that moves a shape and moves it back, or types in a sticky note and deletes it again) is no Undo step of its own: one Undo takes back the edit before it, as before kition-sdk 6b4d471 (kition-sdk 6672663, KitModel M12).

## 0.1.4

- A host tells a board file's document id (`document.identity`) and makes a copied board a new document (`document.fork`): the fork keeps the pages, elements and their ids, and carries a new id and none of the source's history (Kit Process Protocol section 5.6, kition-sdk 6b4d471). Two copies of the same `.kboard` were one document until now.
- A forked board can start a sync room: the fork records the document's birth, so its replica carries its whole history (sync rule S20), also for a fork of a fork and after edits and saves. Forks made by earlier 0.1.4 development builds record none and are refused as a room's first state; fork them again.
- In the dark scheme a shape's text reads on its own fill: on a fill in the file's own colour (a pastel sticky note, say) the text is drawn dark or light for contrast with that fill, not light because the scheme is dark.
- The status bar counts in the singular and plural ("1 element · 1 page", "10 elements · 2 pages"), in every locale.
- The headless program speaks Kit Process Protocol version 1 (`headless.protocol` is `kition-kit/1`), as hosts on kition-sdk from bb1b07b require.

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
