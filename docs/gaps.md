# Gaps

> **Last updated:** 2026-11-21 · **Source:** `features.yaml` (updated 2026-11-21) and the engine's command list

Every known shortfall of Kition Board, from the feature catalogue: features that are not yet `has`,
then the differences recorded on features that work. Update the feature in `features.yaml` when you
land work, then this list. The comparison with the target application is in
[parity/target-app-parity.md](parity/target-app-parity.md); the order of work is in [roadmap.md](roadmap.md).

## Features not done

- `board.agent.guard` (P0, partial): Independent file writers preserve per-call premises. Measured on kition-sdk 2ce294a1 (the kit pins f5ea478): the SDK permits stale numeric/implicit page destinations and deleted page gaps; raw z-index writes and deletion coverage hold (their waiting_for_sdk tests pass). Scoped hierarchy facts and z-order windows give false refusals; removed/moved derived dependencies can report changed instead of deleted/relocated. waiting_for_sdk tests keep their necessity expectations. Undo and redo hold the objects and fields of the step they take back or put back. All 83 independent reading rows run on flat and nested boards.
- `board.editing` (P0, partial): Copy, cut, paste, Alt-drag duplicate, the canvas menu, pasted and dropped pictures, changing a shape, bold/italic/link in a label. Alt duplicates when pressed during a drag, not at the press, because Alt held from the press still moves without snapping (web/tests/flow.mjs). Changing the type of several shapes at once is one undo step per shape, and a label edit with marks is one undo step for the text and one per mark: the engine has no command that sets shapeType on several elements (element.style refuses it) or writes a text with its marks. The change-shape and marks cases have not yet been through a CI run.
- `board.agent.environment` (P1, partial): Offline environment tasks use board words and valid model values. Seed 7 gives 100 verified tasks but no creates. Element.kind is a creation-only register a raw batch create gives (limits12); whether Page replay comparison treats fresh UUIDs as the same objects on the pinned kition-sdk is not measured. Rich-text scalars and derived fields replay.
- `board.cards` (P1, partial): Cards that link to a file or a web page, and links retargeted when files move. Link cards (card.create with `url`) survive save and reopen (bindings/wasm/tests/round_trip.rs); file cards need a lent file and are covered by the command tests only, not by a round trip.
- `board.collab.live` (P1, partial): People co-edit a board live (shapes, text, connectors, frames, cards, groups, z-order) and converge. The engine side is done: every edit is a KDM change with stable ids, imports repair deterministically, and the convergence suite and the board's own scenarios pass on 2 to 5 replicas. Not yet: presence (the editor does not list `presence`, so cursors and selections are not shared or drawn), a gesture or text box over an element another person deletes is not cancelled before its commit fails, and there is no two-editor browser check (no workspace host in web/tests).
- `board.export` (P1, partial): Export the board. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `board.history` (P1, partial): Undo and redo. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `board.templates` (P2, partial): Insert a template. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.

## Differences on working features

- `board.connectors.ends`: Ends meet the outline of ellipses and polygon shapes; a star, cloud, heart, cylinder and the other curved shapes still meet their box.
- `board.images`: Inserting a picture from a lent file (image.insert with `file`) is covered by the command tests, not by the round trip in bindings/wasm/tests/round_trip.rs.

## Catalogue

The catalogue has 31 features: 23 `has`, 8 not yet (8 `partial`).
