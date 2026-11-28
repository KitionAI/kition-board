# Gaps

> **Last updated:** 2026-11-28 · **Source:** `features.yaml` (updated 2026-11-28) and the engine's command list

Every known shortfall of Kition Board, from the feature catalogue: features that are not yet `has`,
then the differences recorded on features that work. Update the feature in `features.yaml` when you
land work, then this list. The comparison with the target application is in
[parity/target-app-parity.md](parity/target-app-parity.md); the order of work is in [roadmap.md](roadmap.md).

## Features not done

- `board.agent.guard` (P0, partial): Independent file writers preserve per-call premises. Measured on kition-sdk 2ce294a1 (the kit pins f5ea478): the SDK permits stale numeric/implicit page destinations and deleted page gaps; raw z-index writes and deletion coverage hold (their waiting_for_sdk tests pass). Scoped hierarchy facts and z-order windows give false refusals; removed/moved derived dependencies can report changed instead of deleted/relocated. waiting_for_sdk tests keep their necessity expectations. Undo and redo hold the objects and fields of the step they take back or put back. All 83 independent reading rows run on flat and nested boards. A bound end on a shape meets its outline, so a connector's route and an anchored connector.add end hold the shape's shapeType (regressions/issue-outline-end-premises); connector.bind without an anchor also reads the other end's target shapeType (the new end faces that end's outline point) and does not hold it: the two connector.bind rows of bindings/wasm/tests/declarations_independent.rs (every_write_rests_on_what_its_handler_reads, nested_write_rows_match_the_second_independent_reading) pin that target's premise as rotation and ~bounds, and changing them needs a maintainer.
- `board.agent.environment` (P1, partial): Offline environment tasks use board words and valid model values. Seed 7 gives 100 verified tasks but no creates. Element.kind is a creation-only register a raw batch create gives (limits12); whether Page replay comparison treats fresh UUIDs as the same objects on the pinned kition-sdk is not measured. Rich-text scalars and derived fields replay.
- `board.editor.type-to-edit` (P0, partial): Typing into the selection: a character typed with one sticky note, text, mind-map node or shape selected starts its text with that character; a double-click on the empty canvas makes a text there. With a plain shape selected, a lower-case tool letter (v h r n m t a p l e f) still picks its tool instead of starting the text, because web/tests/flow.mjs presses tool letters with a shape selected (plan section 5): typing "hello" into a selected shape starts with the hand tool, typing "Hello" or "2 days" writes. Sticky notes, texts and mind-map nodes take every letter. The first character typed through an input method (a composition) does not start the text.
- `board.cards` (P1, partial): Cards that link to a file or a web page, and links retargeted when files move. Link cards (card.add with `url`) survive save and reopen (bindings/wasm/tests/round_trip.rs); file cards need a lent file and are covered by the command tests only, not by a round trip.
- `board.collab.live` (P1, partial): People co-edit a board live. Kition Desktop is for one person since the plain document model (kition-sdk 91cf510): the KDM no longer merges concurrent edits and the convergence suite is gone; co-editing comes back with Kition One's server-ordered changes.

## Differences on working features

- `board.editing`: Alt duplicates when pressed during a drag, not at the press, because Alt held from the press still moves without snapping (web/tests/flow.mjs). Changing the type of several shapes at once is one undo step per shape, and a label edit with marks is one undo step for the text and one per mark: the engine has no command that sets shapeType on several elements (element.style refuses it) or writes a text with its marks. The box's marks follow the typing at its caret, but element.setText places a change of the stored text by comparing the old and new text from both ends (kition-sdk's diff_text), so an x typed inside a run of identical characters goes in at the run's end: a stored mark beside the run can take the typed character (marks made in the box are placed right).
- `board.connectors.ends`: Ends meet the outline of the ellipses, the polygon shapes of outline.rs (the diamond, triangles, hexagon, parallelogram and the others in its table) and the curved document, delay, display and stored-data (each curve sampled as a polygon within 0.3% of the box); a star, cloud, heart, cylinder, pill, multi-document, note and the four single arrows, and the actor and the other open marks (line, check, the braces), still meet their box.
- `board.images`: Inserting a picture from a lent file (image.insert with `file`) is covered by the command tests, not by the round trip in bindings/wasm/tests/round_trip.rs.

## Catalogue

The catalogue has 38 features: 33 `has`, 5 not yet (5 `partial`).
