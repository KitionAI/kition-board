# Target-app parity

> **Last updated:** 2026-11-21 · **Source:** `features.yaml` (updated 2026-11-21) and the engine's command list

Each feature in `features.yaml` records its status against what the feature promises and, where it
has one, the target application it is compared with (Miro, web, 2026-11, for the editing features).
The difference column is the feature's recorded gap.

| Feature | Priority | Status | Target | Difference |
| --- | --- | --- | --- | --- |
| `board.agent.act` An agent extends a flow chart (make, then connect) | P0 | has | — | — |
| `board.agent.context` No agent call depends on or leaves behind the page the editor shows; commands say what they read and the parameter on fills | P0 | has | — | — |
| `board.agent.guard` Independent file writers preserve per-call premises | P0 | partial | — | Measured on kition-sdk 2ce294a1 (the kit pins f5ea478): the SDK permits stale numeric/implicit page destinations and deleted page gaps; raw z-index writes and deletion coverage hold (their waiting_for_sdk tests pass). Scoped hierarchy facts and z-order windows give false refusals; removed/moved derived dependencies can report changed instead of deleted/relocated. waiting_for_sdk tests keep their necessity expectations. Undo and redo hold the objects and fields of the step they take back or put back. All 83 independent reading rows run on flat and nested boards. |
| `board.agent.ids` One id space for pages and elements that survives reopening; receipts and the guard between agents name them | P0 | has | — | — |
| `board.agent.observe` An agent reads the flow and answers on the board | P0 | has | — | — |
| `board.agent.recover` An agent fixes what the check reports | P0 | has | — | — |
| `board.connectors` Connectors bound to elements, with labels and waypoints | P0 | has | Miro (web, 2026-11) | — |
| `board.connectors.ends` Nine arrowheads, and a bound end that meets its target's outline | P0 | has | Miro (web, 2026-11) | Ends meet the outline of ellipses and polygon shapes; a star, cloud, heart, cylinder and the other curved shapes still meet their box |
| `board.drawing` Freehand strokes | P0 | has | — | — |
| `board.editing` Copy, cut, paste, Alt-drag duplicate, the canvas menu, pasted and dropped pictures, changing a shape, bold/italic/link in a label | P0 | partial | Miro (web, 2026-11) | Alt duplicates when pressed during a drag, not at the press, because Alt held from the press still moves without snapping (web/tests/flow.mjs). Changing the type of several shapes at once is one undo step per shape, and a label edit with marks is one undo step for the text and one per mark: the engine has no command that sets shapeType on several elements (element.style refuses it) or writes a text with its marks. The change-shape and marks cases have not yet been through a CI run. |
| `board.elements` Create, select, move, resize, rotate, style, group, stack and lock elements | P0 | has | Miro (web, 2026-11) | — |
| `board.frames` Frames that hold and fit elements | P0 | has | — | — |
| `board.images` Pictures on the board, with alternative text | P0 | has | Miro (web, 2026-11) | Inserting a picture from a lent file (image.insert with `file`) is covered by the command tests, not by the round trip in bindings/wasm/tests/round_trip.rs. |
| `board.layout` Align, distribute and stack elements | P0 | has | — | — |
| `board.pages` Pages: create, rename, duplicate, delete, reorder, switch | P0 | has | — | — |
| `board.shapes` A library of 44 shapes in four groups (basic, flowchart, arrows, marks), each drawn by the engine with its label inside the shape | P0 | has | Miro (web, 2026-11) | — |
| `board.text` Text on the board: marks on a text run | P0 | has | Miro (web, 2026-11) | — |
| `board.agent.conversation` An agent builds on the board over a conversation | P1 | has | — | — |
| `board.agent.draft` An agent's draft of a board is a replica of it, pictures included (draft: true; board_draft_ready, board_draft_reopen) | P1 | has | — | — |
| `board.agent.environment` Offline environment tasks use board words and valid model values | P1 | partial | — | Seed 7 gives 100 verified tasks but no creates. Element.kind is a creation-only register a raw batch create gives (limits12); whether Page replay comparison treats fresh UUIDs as the same objects on the pinned kition-sdk is not measured. Rich-text scalars and derived fields replay. |
| `board.agent.preview` An agent's preview of a board is drawn by the engine with the fonts the kit ships (board_inspect preview: true) | P1 | has | — | — |
| `board.cards` Cards that link to a file or a web page, and links retargeted when files move | P1 | partial | — | Link cards (card.create with `url`) survive save and reopen (bindings/wasm/tests/round_trip.rs); file cards need a lent file and are covered by the command tests only, not by a round trip. |
| `board.collab.live` People co-edit a board live (shapes, text, connectors, frames, cards, groups, z-order) and converge | P1 | partial | — | The engine side is done: every edit is a KDM change with stable ids, imports repair deterministically, and the convergence suite and the board's own scenarios pass on 2 to 5 replicas. Not yet: presence (the editor does not list `presence`, so cursors and selections are not shared or drawn), a gesture or text box over an element another person deletes is not cancelled before its commit fails, and there is no two-editor browser check (no workspace host in web/tests). |
| `board.export` Export the board | P1 | partial | — | The editor case board.export/editor (web/tests/proof.mjs: PNG, SVG and PDF from the export dialog, written through the host; a library shape drawn as itself in the SVG) has not yet been through a CI run. |
| `board.history` Undo and redo | P1 | partial | — | The editor case board.history/editor (web/tests/proof.mjs: create, move, style and delete undone and redone one by one with the keys and the frame's buttons, then saved and reopened) has not yet been through a CI run. |
| `board.mindmap` Mind maps: lay out, add children and siblings, collapse, set direction | P1 | has | — | — |
| `board.platform.brand` The embedded editor uses the canonical kit palette | P1 | has | — | — |
| `board.platform.color-scheme` The board follows the host's color scheme: in dark, the canvas, the default ink, the colour tokens, the selection handles, the cards (their bodies, preview stand-ins and badge glyphs) and the minimap are drawn for a dark surface, live, nothing moving; a file's own colours, exports, previews and thumbnails stay as they are | P1 | has | — | — |
| `board.platform.display-modes` The editor in every display mode: Menus by default, Single line and Classic open on Edit, Focus shows only the board and leaves on Escape | P1 | has | — | — |
| `board.platform.document-identity` A host reads the document id a .kboard carries and forks a copied board into a new document with no history (document.identity, document.fork; Kit Process Protocol 5.6) | P1 | has | — | — |
| `board.templates` Insert a template | P2 | partial | — | The editor case board.templates/editor (web/tests/proof.mjs: each of the six templates from the gallery, one undo step, back with redo, all six after save and reopen) has not yet been through a CI run. |

| Priority | has | partial | missing |
| --- | ---: | ---: | ---: |
| P0 | 15 | 2 | 0 |
| P1 | 8 | 5 | 0 |
| P2 | 0 | 1 | 0 |
