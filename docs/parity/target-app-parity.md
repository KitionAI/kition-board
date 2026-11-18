# Target-app parity

> **Last updated:** 2026-11-17 · **Source:** `features.yaml` (updated 2026-11-17) and the engine's command list

Each feature in `features.yaml` records its status against what the feature promises and, where it
has one, the target application it is compared with (none yet).
The difference column is the feature's recorded gap.

| Feature | Priority | Status | Target | Difference |
| --- | --- | --- | --- | --- |
| `board.agent.act` An agent extends a flow chart (make, then connect) | P0 | has | — | — |
| `board.agent.context` No agent call depends on or leaves behind the page the editor shows; commands say what they read and the parameter on fills | P0 | has | — | — |
| `board.agent.guard` Independent file writers preserve per-call premises | P0 | partial | — | Measured on kition-sdk 2ce294a1 (the kit pins f5ea478): the SDK permits stale numeric/implicit page destinations and deleted page gaps; raw z-index writes and deletion coverage hold (their waiting_for_sdk tests pass). Scoped hierarchy facts and z-order windows give false refusals; removed/moved derived dependencies can report changed instead of deleted/relocated. waiting_for_sdk tests keep their necessity expectations. Undo and redo hold the objects and fields of the step they take back or put back. All 83 independent reading rows run on flat and nested boards. |
| `board.agent.ids` One id space for pages and elements that survives reopening; receipts and the guard between agents name them | P0 | has | — | — |
| `board.agent.observe` An agent reads the flow and answers on the board | P0 | has | — | — |
| `board.agent.recover` An agent fixes what the check reports | P0 | has | — | — |
| `board.connectors` Connectors bound to elements, with labels and waypoints | P0 | has | — | — |
| `board.drawing` Freehand strokes | P0 | has | — | — |
| `board.elements` Create, select, move, resize, rotate, style, group, stack and lock elements | P0 | has | — | — |
| `board.frames` Frames that hold and fit elements | P0 | has | — | — |
| `board.images` Pictures on the board, with alternative text | P0 | has | — | Inserting a picture from a lent file (image.insert with `file`) is covered by the command tests, not by the round trip in bindings/wasm/tests/round_trip.rs. |
| `board.layout` Align, distribute and stack elements | P0 | has | — | — |
| `board.pages` Pages: create, rename, duplicate, delete, reorder, switch | P0 | has | — | — |
| `board.text` Text on the board: marks on a text run | P0 | has | — | — |
| `board.agent.conversation` An agent builds on the board over a conversation | P1 | has | — | — |
| `board.agent.draft` An agent's draft of a board is a replica of it, pictures included (draft: true; board_draft_ready, board_draft_reopen) | P1 | has | — | — |
| `board.agent.environment` Offline environment tasks use board words and valid model values | P1 | partial | — | Seed 7 gives 100 verified tasks but no creates. Element.kind is a creation-only register a raw batch create gives (limits12); whether Page replay comparison treats fresh UUIDs as the same objects on the pinned kition-sdk is not measured. Rich-text scalars and derived fields replay. |
| `board.agent.preview` An agent's preview of a board is drawn by the engine with the fonts the kit ships (board_inspect preview: true) | P1 | has | — | — |
| `board.cards` Cards that link to a file or a web page, and links retargeted when files move | P1 | partial | — | Link cards (card.create with `url`) survive save and reopen (bindings/wasm/tests/round_trip.rs); file cards need a lent file and are covered by the command tests only, not by a round trip. |
| `board.collab.live` People co-edit a board live (shapes, text, connectors, frames, cards, groups, z-order) and converge | P1 | partial | — | The engine side is done: every edit is a KDM change with stable ids, imports repair deterministically, and the convergence suite and the board's own scenarios pass on 2 to 5 replicas. Not yet: presence (the editor does not list `presence`, so cursors and selections are not shared or drawn), a gesture or text box over an element another person deletes is not cancelled before its commit fails, and there is no two-editor browser check (no workspace host in web/tests). |
| `board.export` Export the board | P1 | partial | — | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `board.history` Undo and redo | P1 | partial | — | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `board.mindmap` Mind maps: lay out, add children and siblings, collapse, set direction | P1 | has | — | — |
| `board.platform.brand` The embedded editor uses the canonical kit palette | P1 | has | — | — |
| `board.platform.display-modes` The editor in every display mode: Menus by default, Single line and Classic open on Edit, Focus shows only the board and leaves on Escape | P1 | has | — | — |
| `board.templates` Insert a template | P2 | partial | — | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |

| Priority | has | partial | missing |
| --- | ---: | ---: | ---: |
| P0 | 13 | 1 | 0 |
| P1 | 6 | 5 | 0 |
| P2 | 0 | 1 | 0 |
