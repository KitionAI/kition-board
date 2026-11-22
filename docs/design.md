# Board kit design: schema `kit.board` v2, `.kboard`, cards, editor interface, commands

This is the design reference of the Board kit: the KDM schema, the `.kboard` file, cards, the
editor's interface to the engine, and the commands, agent tools and check catalogue. Code
comments cite its sections as "design note <section>". Requirement ids (M3, N7, PT10, ...) are
rows of the Kition kit requirements table; KHP-000x and the A, AX, LK and PV ids are Kit Harness
Protocol rows.

## 0. Overview

- The kit is a Rust engine implementing kition-sdk's `KitDocument` over `kition_kit::session`
  (engine contract 2), compiled to WebAssembly (`kition-board-kit`, `bindings/wasm`) for the
  editor and built as the headless `kition-board` executable (`bindings/headless`) for agents.
- **The replica is the document.** A `.kboard` file is a ZIP of the KDM snapshot, its change log
  and the blobs (section 2). A save re-emits what it read byte for byte and appends one change
  entry.
- **Bound connector ends are derived** from the target's box and the stored anchor (1.4).
- **Repairs are deterministic** (1.5).
- **The editor draws nothing of its own:** it draws each element with the engine's SVG fragment
  and sends every change as one engine command at the end of a gesture (section 4).

## 1. Schema `kit.board` v2

### 1.1 The schema

`crates/kdm/kdm-model/model/schema.json`. There is no reader for another schema version.

```json
{
  "kdm": 1,
  "kit": "kit.board",
  "schemaVersion": 2,
  "root": "Board",
  "types": {
    "Board": { "fields": {
      "title": { "kind": "string" },
      "size":  { "kind": "json", "optional": true },
      "pages": { "kind": "list", "of": "Page" } } },
    "Page": { "fields": {
      "name": { "kind": "string" },
      "elements": { "kind": "list", "of": "Element" } } },
    "Element": { "fields": {
      "kind":        { "kind": "enum", "values": ["shape", "text", "stroke", "connector", "image", "card", "frame", "group"] },
      "position":    { "kind": "json", "optional": true },
      "size":        { "kind": "json", "optional": true },
      "rotation":    { "kind": "number", "optional": true },
      "locked":      { "kind": "boolean", "optional": true },
      "parent":      { "kind": "ref", "of": "Element", "optional": true },
      "key":         { "kind": "string", "optional": true },
      "shapeType":   { "kind": "string", "optional": true },
      "shapeStyle":  { "kind": "string", "optional": true },
      "strokeColor": { "kind": "string", "optional": true },
      "fillColor":   { "kind": "string", "optional": true },
      "fillStyle":   { "kind": "string", "optional": true },
      "dashStyle":   { "kind": "string", "optional": true },
      "strokeSize":  { "kind": "string", "optional": true },
      "opacity":     { "kind": "number", "optional": true },
      "roundness":   { "kind": "number", "optional": true },
      "text":        { "kind": "text", "marks": {
                         "bold":   { "expand": "after" },
                         "italic": { "expand": "after" },
                         "link":   { "expand": "none" } } },
      "textFormat":  { "kind": "enum", "values": ["plain", "markdown"], "optional": true },
      "fontSize":    { "kind": "number", "optional": true },
      "fontFamily":  { "kind": "enum", "values": ["sans", "serif", "mono"], "optional": true },
      "textAlign":   { "kind": "enum", "values": ["left", "center", "right"], "optional": true },
      "points":      { "kind": "json", "optional": true },
      "start":       { "kind": "json", "optional": true },
      "end":         { "kind": "json", "optional": true },
      "waypoints":   { "kind": "json", "optional": true },
      "labelPosition": { "kind": "number", "optional": true },
      "connectorType": { "kind": "string", "optional": true },
      "startArrowhead": { "kind": "string", "optional": true },
      "endArrowhead":   { "kind": "string", "optional": true },
      "connectorRole":  { "kind": "string", "optional": true },
      "mindMapBranchAxis": { "kind": "string", "optional": true },
      "mindMapBranchSide": { "kind": "string", "optional": true },
      "mindMapDirection":  { "kind": "string", "optional": true },
      "mindMapCollapsed":  { "kind": "boolean", "optional": true },
      "image":       { "kind": "blob", "optional": true },
      "alt":         { "kind": "string", "optional": true },
      "card":        { "kind": "json", "optional": true },
      "sourceRefIds": { "kind": "json", "optional": true } } },
    "CommentThread": "kition_kdm::comment::COMMENT_THREAD_TYPE_JSON",
    "Comment":       "kition_kdm::comment::COMMENT_TYPE_JSON"
  }
}
```

The two comment types are the kition-sdk constants `COMMENT_THREAD_TYPE_JSON` and `COMMENT_TYPE_JSON` (`crates/kition-kdm/src/comment.rs`); threads are free objects found by type (`kition_kdm::comment::threads`), so `Board` has no thread list (M23: content never references a comment). The file passes `M6` (`check-schema`), and a test keeps it equal to the Rust constant (K12).

`Board.size` (`{ "w", "h" }` in board units, written by `create(name, width, height)`) is the document the board was made as, from the origin: renders, thumbnails and `inspect().area` show exactly it while the content stays inside, and grow with a 40-unit margin only on a side the content goes past. A board without one is framed by its content plus the margin, at least 1920 × 1080 (`regressions/issue-1`).

### 1.2 Fields

| Field | Kind | Values and units | Why |
| --- | --- | --- | --- |
| `kind` | enum | `shape`, `text`, `stroke`, `connector`, `image`, `card`, `frame`, `group`; read-only after creation | frames and groups are kinds of their own, as in both common interchange formats |
| `position` | json | `{ "x": number, "y": number }`, board units (1 unit = 1 CSS pixel at 100%), top-left of the unrotated box, world coordinates (not relative to `parent`) | M3/H4: one register, so concurrent moves never mix x of one writer with y of another. Absent on `stroke` and `connector` (their geometry is `points` / `start`+`end`) |
| `size` | json | `{ "w": number, "h": number }`, both ≥ 0 | M3/H4. Absent on `text` = auto size from text metrics; absent on `stroke`, `connector` |
| `start`, `end` | json | `{ "point": [x, y], "binding"?: { "target": "<element id>", "anchor": [ax, ay] } }`; `anchor` normalised to the target's unrotated box, each in [0, 1] | M3/H4: an end and its binding are one value (a concurrent rebind and drag cannot leave a binding pointing one way and a point another) |
| `waypoints` | json | `[[x, y], ...]`, world coordinates, in path order; empty or absent = automatic route | multi-point connectors |
| `labelPosition` | number | 0..1 along the route, default 0.5 | the label itself is the element's `text` |
| `card` | json | section 3.2 | cards |
| `textFormat` | enum | `plain` (default) or `markdown` | Markdown text; when `markdown`, marks are not used (the syntax is in the text) |
| `text` marks | text | `bold`, `italic` (`expand: after`), `link` (`expand: none`, value `{ "href": string }`) | M4c; same shape as khp `spec/sets/model.md` M4c's example |
| `fontFamily`, `textAlign` | enum | as listed; defaults `sans`, `center` in shapes and `left` in `text` | hand-drawn fonts are not modelled |
| `roundness` | number | corner radius in board units, ≥ 0; absent = the shape's default | |
| `strokeColor`, `fillColor` | string | a token (`ink`, `gray`, `purple`, `green`, `orange`, `red`, `yellow`, `blue`, `white`) or `#rrggbb` (lower case); `opacity` stays separate | tokens stay tokens so themes can restyle them |
| `points` | json | `[[x, y], ...]` world coordinates | arrays instead of objects: a 50-point stroke is about half the bytes |
| `image` | blob | `{ hash, mime, size }` (sha256 hex, as `kition_kdm::schema`) | pictures travel with the board |
| `alt` | string | free text | its own register, editable without touching the blob |
| `key` | string | an authoring id (`[A-Za-z_][A-Za-z0-9_]*`), unique per page among live elements | V8 |
| `shapeStyle` | string | `default`, `sticky`, `mind-node`, `flow-node`, `image-placeholder` | |
| `shapeType` | string | one of the 44 shapes, in four groups of the shape library: basic (16), flowchart (14), arrows (7), marks (7) (`SHAPE_TYPES`, `SHAPE_GROUPS`) | `string`, not `enum`, so a new shape is not a schema change (M5) |
| `CommentThread`, `Comment` | types | anchor `{ "object": <element or page id>, "point"?: [x, y] }` (point relative to the object's box; `point` is the kit's own key, M23 allows it) | H10: declared so comments need no schema change; there are no comment commands (section 6) |
| Others | | `title`, `pages`, `name`, `elements`, `parent`, `rotation` (degrees, about the box centre), `locked`, `fillStyle`, `dashStyle`, `strokeSize`, `opacity` (0..1), `fontSize`, `connectorType`, `startArrowhead` and `endArrowhead` (one of nine: `none`, `arrow`, `dot`, `triangle`, `triangle-open`, `diamond`, `diamond-open`, `circle-open`, `bar`; `ARROWHEADS`, strings like `shapeType`), `connectorRole`, the four mind-map fields, `sourceRefIds` | |

Which fields each kind uses (others stay absent; a command refuses them with `invalid-params`):

| kind | geometry | content | style |
| --- | --- | --- | --- |
| `shape` | `position`, `size`, `rotation` | `text` (label), `textFormat` | `shapeType`, `shapeStyle`, colours, fill, dash, stroke, opacity, roundness, font fields; mind-map fields for `mind-node` |
| `text` | `position`, `size?`, `rotation` | `text`, `textFormat` | font fields, `strokeColor` (text colour), opacity |
| `stroke` | `points` | — | `strokeColor`, `strokeSize`, `opacity` (the highlighter is the pen with its own default style; no field marks it) |
| `connector` | `start`, `end`, `waypoints` | `text` (label), `labelPosition` | `connectorType`, arrowheads, `connectorRole`, `dashStyle`, `strokeSize`, `strokeColor` |
| `image` | `position`, `size`, `rotation` | `image`, `alt` | `opacity` |
| `card` | `position`, `size` | `card`, `text` (optional caption) | `fillColor`, `strokeColor` |
| `frame` | `position`, `size` | `text` (title) | `fillColor`, `strokeColor` |
| `group` | none (bounds derived from children) | — | — |

### 1.3 Coupled geometry in one `json` register (H4, M3)

Kit-standard has no `atomic` groups (`spec/sets/model.md`: `json` is "for coupled values (a transform)"); requirement M3 0.2 is "coupled values in one json register, reviewed one by one". The review record M3 asks for:

| Coupled values | Register | What a split would allow | Why the rest stays separate |
| --- | --- | --- | --- |
| x and y | `position` | A writer's x with another's y: a position nobody chose | — |
| width and height | `size` | An aspect nobody chose (a resized square becomes a strip) | `size` and `position` stay separate: a concurrent move and resize both apply and give a valid box (resize from the top-left handle writes both in one transaction, so one writer still wins both) |
| an end's point and its binding | `start`, `end` | A binding to shape B with a point at shape A | `start` and `end` separate: a concurrent drag of each end is two people's intents, both valid |
| route bends | `waypoints` | — (one list, replaced whole) | separate from the ends: moving a bound target changes the ends, not the bends |
| a card's type and target | `card` | A note card naming a `.ktable` | — |
| stroke points | `points` | — (one list) | — |

`rotation` stays its own register: any rotation with any box is valid.

### 1.4 Bound ends are derived

Rule: **when an end has a `binding` whose target exists on the same page, every reader (the engine's view, render, export, inspect, check) computes the end point from the target's `position`, `size`, `rotation`, `shapeType` and the `anchor`: it is the point where the ray from the target box's centre through the anchor's point leaves the target's outline; the stored `point` is ignored.** Commands still write the freshly computed `point` (so a reader of the raw KDM JSON, such as `state_json` or a third-party tool, sees a near value), but nothing depends on it being current.

- The outline (`kition_board_kdm::outline::outline_point`, the one derivation every reader and every cache write goes through): the ellipse inscribed in the box for `ellipse`, `summing-junction` and `or-junction`; the polygon of `outline::POLYGONS` for a shape in that table, and the hexagon and the parallelogram with the insets they are drawn with; the box for every other shape and every other kind. Rectangles, sticky notes, frames, cards, pictures and text therefore resolve exactly to the anchor's point on the box, as they always did. An anchor at the centre, a box with a zero side, or anything not finite resolves to the anchor's point on the box.
- The anchor stays a point on the box (`nearest_edge_anchor` picks it when a connector is dropped on an element's body, facing the other end), so a board written before ends met outlines keeps its anchors and draws its ends on the outline without any write; a save of it without edits appends nothing. A new `shapeType` (`element.update`) writes the bound ends' cached points on the new outline in the same command.
- The editor derives the same point with the outlines the engine answers in `shape.previews` (`outline` per shape; `web/src/board/outline.ts`); until that answer arrives it uses the box.

- No repair transaction runs after a merge: two people who move the two shapes of a connector at once converge to positions, and the connector is drawn between them, with no system change to sync.
- An unbound end, or a binding whose target is gone or on another page, uses `point` (and the repair in 1.5 clears the binding).
- Connector routing (straight, elbow, curved) is computed from the ends and `waypoints` at read time, in `crates/engine/model`; nothing derived is stored.

### 1.5 Validation and repair (M4, E14)

Deterministic, run after open, import of changes, undo and redo, written as one `system` transaction only when something is wrong (a healthy board writes nothing). Every rule depends only on the state, never on arrival order or actor, so two replicas repair alike (M4 test: the convergence suite's `delete-edit`, `reparent`, `fuzz`, run by `bindings/wasm/tests/convergence.rs` on kition-sdk's `kition-convergence`, plus one synthetic case per rule). A command's own transaction ends healthy: `kition_board_model::run` runs the same repair at the end of every command, so a command never leaves a state for the next import or reopen to change (a replica would otherwise disagree with its own copy recovered after a crash; `regressions/issue-coedit-local-edit-left-a-repair`).

Co-edited, the registers above give these outcomes (`board.collab.live/converges` checks each on 2 to 5 replicas): concurrent moves of one element end on one person's move (`position` is one register), a concurrent resize is kept (`size` is another); a connector end bound to an element someone deletes lets go where it last was (V6), its other end stays bound; an element dropped into two frames at once ends in one, inside it and in front of it (`position`, `parent` and the list move are one person's); a frame deleted while someone drops into it takes what was in it when deleted and keeps what was dropped, out of any frame (V4); frames put inside each other make no cycle (V5); a group whose members are deleted or moved out goes (V9); a card keeps one person's whole target (`card` is one register); a label typed by several people keeps every word (text merge).

| # | State | Repair |
| --- | --- | --- |
| V1 | A list entry (`pages`, `elements`) naming no live object | drop the entry |
| V2 | A live element on no page's list, or on a deleted page | delete the element and its descendants |
| V3 | The same element in two lists | keep it in the list whose page id sorts first; drop the others |
| V4 | `parent` names a deleted element, an element on another page, or a kind that cannot contain (only `frame` and `group` can) | clear `parent` (the element keeps its world position) |
| V5 | A `parent` cycle (a merge can make one even though commands refuse them) | clear `parent` on the element of the cycle whose id sorts first |
| V6 | A `binding` naming a deleted element, an element on another page, or a non-bindable kind (`stroke`, `connector`, `group`) | remove `binding`, keep `point` (set to the last derived value in the same transaction) |
| V7 | An `anchor` outside [0, 1] | clamp |
| V8 | Duplicate `key` on a page (two writers created the same key) | clear `key` on the element whose id sorts later |
| V9 | An empty `group` (no children) | delete it |

Not repaired: a missing blob (it may arrive late; drawn as a placeholder, reported by `board.image-missing-blob`), an invalid `card` value (drawn as an "invalid card" placeholder, reported by `check`), anything in comment threads (`kition_kdm::comment` owns them).

### 1.6 Performance rules

- **P1:** one object per element; no element data in a `map`.
- **P3:** an interactive transaction touches at most 10,000 objects; a command or paste over that is refused with `board.too-many-objects` (`invalid-params`), never split silently.
- **P4:** the KDM runs inside the WebAssembly module.
- **Crash recovery:** the `.kboard` container keeps the saved snapshot and the change log (section 2), from which a board recovers its changes.

## 2. The `.kboard` file (H12)

### 2.1 Layout

A ZIP archive (PKZIP, no ZIP64). Entries, in this order:

| Entry | Compression | Content | Required |
| --- | --- | --- | --- |
| `mimetype` | STORE, first entry, no extra field | the ASCII bytes `application/vnd.kition.board+zip`, no newline | yes |
| `meta.json` | DEFLATE | section 2.2 | yes |
| `snapshot.bin` | STORE | a KDM snapshot envelope (M16, M17) of the document at the last compaction | yes |
| `changes/<seq>.bin` | STORE | KDM change envelopes (M14, M17): what the replica committed after the previous entry; `<seq>` is a zero-padded decimal, `000001`, `000002`, … | no |
| `blobs/<sha256>` | STORE | the bytes of one blob, named by the lower-case hex sha256 the KDM `blob` value carries; no extension (the MIME is in the KDM value) | as referenced |

`mimetype` first, uncompressed, is the ODF/EPUB convention: a host or the requirement N7 check reads the first local header (the name `mimetype` at byte offset 30, the 32-byte MIME from offset 38) and knows a `.kboard` without unzipping. KDM envelopes are already compact binary (Loro), and pictures are already compressed, so only `meta.json` is deflated.

### 2.2 `meta.json`

```json
{
  "format": "kition.board",
  "formatVersion": 1,
  "kdm": 1,
  "schemaVersion": 2,
  "docId": "8c1e…",
  "title": "Q4 plan",
  "snapshot": { "entry": "snapshot.bin", "version": "<base64 of the KDM version vector>" },
  "changes": ["changes/000001.bin", "changes/000002.bin"],
  "app": { "kit": "kit.board", "version": "0.1.0" }
}
```

| Field | Use |
| --- | --- |
| `format`, `formatVersion` | the container; a newer `formatVersion` is refused as too new |
| `kdm`, `schemaVersion` | copies for hosts that do not load KDM; the engine trusts the snapshot's own meta (M5, M20) and refuses a mismatch as `not-a-board` |
| `docId` | copy of the KDM `docId` (M7) so a host can index the file without the engine |
| `title` | copy of `Board.title` for file lists and placeholders |
| `snapshot.version`, `changes` | the replay order; an entry on disk but not listed is ignored (and dropped at the next compaction); a listed entry that is missing makes the file damaged |
| `app` | who wrote it, for support |

There is no thumbnail in the file: it would be rewritten on every save and duplicate what the engine's `render` gives.

### 2.3 Save, open, compaction

- **Open:** check `mimetype`, read `meta.json`, load `snapshot.bin`, import each listed change entry in order, register the blobs found under `blobs/`, then validate (1.5). Opening never writes the file.
- **Save** (`save()` returns the whole file, the host writes it, `markSaved` after; contract E4): the engine keeps the bytes of `snapshot.bin` and of the change entries it read or wrote, so a save re-emits them unchanged, appends one entry with the changes since the last entry, and writes the blobs the replica references. Nothing is re-encoded, so a save stays small however many pictures the board holds.
- **Compaction:** when the change entries together exceed a quarter of `snapshot.bin`, or there are more than 200 of them, a save writes a fresh snapshot and no change entries. Blobs the replica's retained state does not reference are dropped only at compaction.
- **Crash safety:** the host writes atomically (temp file + rename); the engine never edits the file in place.
- **External changes:** a file changed on disk while open is merged: the new file is opened as a replica and its changes are imported.
- **Identity and forks** (Kit Process Protocol 5.6, KitModel M7a; `kition_board_kdm::file::embedded_replica` and `replace_replica`, served as `document.identity` and `document.fork`): the file embeds its replica, so a byte copy is the same document. Its identity is the docId of `snapshot.bin` with the listed changes imported (`meta.json`'s copy must agree, or the file is damaged); an empty file has none. A fork writes the SDK's copy under a new docId (no history) as `snapshot.bin`, no change entries, `meta.json`'s `docId` and `snapshot.version` set to the copy's, and the blobs its state references; the title, `app` and unknown entries are kept. Cards' `target.docId` name other documents and stay as they are.

### 2.4 Limits and damaged files (Q7, E2)

The reader refuses, as `not-a-board` or `damaged` (opens nothing, never panics): a missing or wrong `mimetype`; a missing `meta.json` or `snapshot.bin`; a listed change entry that is missing; an entry name containing `..`, `\`, a leading `/`, or a drive letter; duplicate names; encrypted entries; ZIP64; more than 100,000 entries; a declared uncompressed size over 2 GiB for the archive or 512 MiB for one entry; a deflate ratio over 100:1 on `meta.json`; a blob whose bytes do not hash to its name. The limits are guards, not measured sizes.

**Unknown entries** (another safe name) are kept byte for byte and written back on save, and the file opens normally (F4).

### 2.5 MIME and manifest format

`formats: [{ "id": "kboard", "extensions": ["kboard"], "mime": "application/vnd.kition.board+zip", "role": "edit" }]` (N6, N7).

### 2.6 Tests that define the file

Zip round trip (open → save → open gives the same `state_json`, M15); save after one edit appends exactly one entry and leaves `snapshot.bin` bytes identical; compaction at the threshold; blob round trip and late blobs; the refusals in 2.4 as synthetic damaged files; an unknown entry survives a save (`crates/kdm/kdm-model/tests/file.rs`); open and save at 5,000 elements within a time budget (`crates/kdm/kdm-model/tests/perf.rs`).

## 3. Cards (H5, KHP-0005)

### 3.1 Types

| `card.kind` | Target | Drawn |
| --- | --- | --- |
| `note` | a `.md`/`.markdown` file | by the engine: the kind's badge, the target's name and path |
| `table` | a `.ktable` file | as `note`, with a slot rectangle for a preview of the target (3.5) |
| `board` | a `.kboard` file other than this one | as `table` |
| `file` | any other workspace file | as `note` |
| `link` | a URL | by the engine: the person's title or the URL's host name, and the URL |

- **No nesting:** `card.create` refuses this board as its own target (`invalid-params`).
- **Read-only:** a card shows its target; nothing is edited inside it.
- **Caption:** the element's `text` is an optional caption drawn under the card.

### 3.2 The `card` value

```json
{
  "kind": "note",
  "target": { "path": "../plans/Roadmap.md", "docId": "8c1e…" },
  "view": { "heading": ["Plan", "Risks"] },
  "title": null,
  "url": null
}
```

| Key | Type | Rule |
| --- | --- | --- |
| `kind` | `note` \| `table` \| `board` \| `file` \| `link` | set at creation from the target's extension (`.md`/`.markdown` → `note`, `.ktable` → `table`, `.kboard` → `board`, else `file`) unless given; `card.setTarget` may change it |
| `target.path` | string | relative to the board's folder, or root-anchored with a leading `/`, by the lent-path rules (`kition_kit::host::resolve_lent_path`, KHP-0005 HC-P2); never absolute, never a host URI. Absent for `link` |
| `target.docId` | string, optional | the target's M7 `docId` when known; lets a host follow a target moved while the board was closed (LK7) |
| `view` | object, optional | where in the target: `heading` (path) and `blockId` for notes; the KHP-0004 A24 `target` shape (`page`, `object`, `range`) for tables and boards |
| `title` | string, optional | `link` cards: the person's title (kits have no network, so there is no unfurl) |
| `url` | string, optional | `link` cards only; `http`, `https`, `mailto` schemes; anything else is refused at `card.create` |

Handles are never stored (KHP-0002 FA5). Links on a board are its cards only.

### 3.3 Target state and how a missing target is reported

The engine knows a target's state from a lent `host::size` on the handle (headless, or a host that answers `kitionHost.size`):

| State | `board_check` |
| --- | --- |
| resolved, readable | — |
| a board card's stored `docId` differs from the one its target board's file names now | `board.card-target-changed` (warning; fix `card.setTarget`) |
| the host says the target does not exist | `board.card-target-missing` (error; fix `card.setTarget` or `element.delete`) |
| the host lends no files, or the reader has no right to read | **no diagnostic**: the reader cannot tell missing from unreadable, and an error for every co-editor without the file would be noise |

Other targets' `docId`s come only from the host (`links/resolve`), which gives them to no engine, so `board.card-target-changed` checks board cards only.

### 3.4 Insertion and rename

- **Insert:** the editor's card tool places a file or link card and runs `card.create`.
- **Agent:** `board_run card.create { file: <file, access read>, kind?, view?, position?, size? }` (A28 typed parameter) or `{ url, title? }`.
- **Rename and move:** the host's `links.retarget { from, to }` on the board (KHP-0005 LK2) rewrites `card.target.path` of matching cards in one transaction, keeps `docId`, `view` and caption; idempotent.

### 3.5 Previews in the page

A table or board card's fragment carries a `slot` rectangle for a preview of the target (4.2). Headless renders and exports draw the card without a preview.

## 4. Editor interface: R2c

### 4.1 Shape

The page holds kit-editor's shell, the board editor (`web/src`: the canvas, its gestures, snapping, the spatial index, the viewport, connection handles; the tool rail, whose shape slot holds 18 shapes; the shape library panel above the style panel, with all 44 shapes in their four groups and a search, its groups and pictures from `shape.previews`, which the editor asks for once when the board opens and shares with the page replica's outlines and the style panel's Shape section), and a **page replica** of the KDM relayed to the worker's replica. The worker runs `BoardDocument` (WebAssembly). The editor:

- reads geometry, hierarchy, selection targets and culling bounds from the page replica (bound ends derived per 1.4, on the outlines `shape.previews` answers, read once when the board opens; a connector end dropped on a shape with an outline of its own is pinned on the ray from its centre through the pointer);
- draws each element with **the engine's SVG fragment**, never its own shape code;
- sends **every change to the document as one engine command** at the end of a gesture; it never writes the page replica itself (a page-replica write would be a second writer of document logic).

### 4.2 `elementSvg` (engine extra)

```text
elementSvg(request: string) -> string          // JSON in, JSON out (extras, K6)

request:  { "ids": ["<element id>", ...],      // at most 500 per call
            "lod": "full" | "low",             // "low" at low zoom: no text shaping, simplified paths
            "known": { "<id>": <rev>, ... } }  // fragments the page already has
answer:   { "fragments": [ { "id", "rev", "svg", "box": [x0, y0, x1, y1], "frame": "local" | "world", "size"?: [w, h], "slot"?: [x0, y0, x1, y1] } ],
            "unchanged": ["<id>", ...],
            "missing": ["<id>", ...],
            "fontEpoch": n }
```

| Rule | Detail |
| --- | --- |
| Coordinates | `svg` of a boxed kind is in the element's **local** frame: origin at `position`, unrotated. The editor places it with a transform from the page replica, so a move or rotation needs no new fragment. Connectors and strokes have world coordinates (their geometry is their appearance) |
| `rev` (appearance revision) | moves whenever a fragment's text or box changes; an element's own position and rotation are not in a local fragment, so moving it keeps its revision; a connector's route is, so moving its target moves it. Revisions come from one counter per document and never repeat |
| Cache | the editor caches fragments by `(id, rev)` and asks only for the visible elements whose `rev` it lacks, so panning and zooming within one LOD fetch nothing |
| Colours | tokens are emitted as CSS custom properties with a literal fallback (`fill="var(--kb-stroke-purple, #5645d4)"`); a theme change restyles without invalidating the cache. Hex colours are literal. In the host's dark scheme `web/src/app.css` gives the properties the engine's dark palette (`style::DARK_PALETTE`: lines and text at WCAG AA on the dark paper `#1e1e1e`), and the minimap is drawn with the extra `renderScheme(scheme, x, y, scale, width, height)`; `render`, `renderArea`, `file.render` and `file.export` are always light, and nothing about the scheme is stored |
| Text | shaped and wrapped by the engine with real font metrics, one `tspan` per line with explicit `x` and `y` |
| Slots | a table or board card's fragment has a `slot` rectangle (3.5) |
| Images | the fragment carries the blob hash, not the bytes; the editor gets the bytes with the `blob(hash)` extra |
| Hit testing | the fragments are real DOM: clicks hit painted geometry through the browser's own SVG hit testing; marquee and culling use `box` |

The other extras are `textMetrics(request)` (the engine's line layout, to measure a label while typing) and `blob(hash)`. Contract members `render`/`renderArea` draw the same scene as raster (minimap, agent previews, thumbnails).

### 4.3 Gestures and commits

- **During a gesture** (drag, resize, rotate, draw, connect), the editor changes only its own view: it transforms cached fragments (scale for resize, approximate until commit), draws the live connector of a drag as a plain polyline, and draws the pen stroke itself. Nothing is sent to the worker until the gesture ends.
- **At the end**, one command (`element.move { ids, delta }`, `element.resize`, `connector.create`, `stroke.create`, …) with the explicit targets (PT10); one gesture = one command = one undo step labelled with the command title (PT4). Bound connectors of moved shapes are recomputed by the engine in the same transaction (1.4).
- **No flicker:** the editor keeps the gesture's transform as a pending override until the relay delivers the command's change, then drops the override and swaps in the new fragments. A refused command drops the override (the element returns) and shows the error.
- **Text:** a focused text box over the element (UI-K6). An Enter during an IME composition does not commit: the composition is tracked as well as `isComposing`, because WebKit reports that Enter with `isComposing: false` (`web/src/board/ime.ts`). Typing stays local; commit (blur, Escape, or Enter without composition) runs `element.setText { id, text }`, which the engine applies as a minimal diff on the KDM `text` (so concurrent typing into one label keeps both, KDM sequence merge; marks on characters the edit does not touch stay).
- **Marks in a label** (`web/src/canvas/TextEditor.tsx`): over a shape's label or a text whose `textFormat` is not `markdown` (a Markdown label writes its emphasis as Markdown), a bar above the text box offers Bold (Cmd/Ctrl+B), Italic (Cmd/Ctrl+I) and Link (Cmd/Ctrl+K: a field for the address, `https://` added to a bare host; an empty address removes the link), on the text box's selection; Cmd is the key on a Mac and Ctrl elsewhere, as kit-editor's shortcuts, and the keys are taken only while the bar is shown and no composition is under way. The text box stays a plain text box: the label's stored marks (read from the page replica's runs) and the marks made while typing wait beside the text as `{ start, end, mark, value }` in UTF-16 offsets of the current text and move with each edit before them (bold and italic grow with typing at their end, as the schema's `expand: after`; a link does not). A bar button is pressed when every character of the selection (or the one before the caret) has that mark, stored or waiting. On commit the editor runs `element.setText` when the text changed, then one `text.mark { id, range, mark, value?, units: "utf16" }` per waiting mark in the order they were made; the engine has no command that writes a text and its marks together, so a label edit with marks is one undo step for the text and one per mark. The engine's fragment shows the marks once committed.
- **Shape type** (`web/src/panels/StylePanel.tsx`): when every selected element is an unlocked shape whose `shapeStyle` is the default or `flow-node`, the style panel's Shape section lists the library's 44 shapes in its four groups (`shape.previews`); choosing one runs `element.update { id, set: { shapeType } }` on each selected shape whose type differs. Box, label, style and bound connectors stay, and the connectors' ends move to the new outline in the same command (1.4). `element.style` does not take `shapeType` and `element.update` takes one id, so changing several shapes is one undo step per shape.
- **Clipboard** (`web/src/board/clipboard.ts`): Cmd/Ctrl+C copies the selection with `clipboard.copy` (a reading command, so also in a read-only board); the editor keeps the engine's payload and also writes it to the system clipboard as text prefixed `kition-board:` when the browser lets it (an embedded frame or headless WebKit may refuse). Cmd/Ctrl+X is that copy, then `element.delete`. Cmd/Ctrl+V is not taken by the editor's key handler, so the browser's `paste` event follows on the focused canvas and carries the system clipboard without a permission prompt: a picture in it wins (below); else a `kition-board:` text, when the last copy here reached the system clipboard or there is none here; else the copy kept here. Where a browser sends no `paste` event, the key pastes the copy kept here after 50 ms, and a late event of the same key is dropped. One `element.paste { page, payload, at }` puts the copies' top-left at the pointer (the middle of the view when the pointer is outside the canvas), each further paste at the same place 16 units on (a new copy starts again), and selects the copies' outermost elements. A read-only board neither cuts nor pastes.
- **Alt-drag duplicate:** Alt pressed during a move turns it into a copy: the originals stay drawn where they are, a dashed box shows where the copies land, and the drop runs one `element.duplicate { ids, offset }` on the unlocked elements the move would move (copies of a frame or a group take what is inside; a connector goes along only when it is selected, and its ends bound to copied elements are bound to their copies; copies are never locked). Releasing Alt before the drop makes it a move again. Alt held from the press moves without snapping, as before (`web/tests/flow.mjs` relies on it).
- **Canvas menu** (`web/src/canvas/CanvasMenu.tsx`, kit-editor's `ContextMenu`): a right-click on an element outside the selection selects it first; on elements the menu offers Copy, Cut, Paste, Duplicate, Bring to front, Bring forward, Send backward, Send to back (`element.reorder`), Group, Ungroup, Lock or Unlock (`element.lock`) and Delete, each one command on the elements the menu opened on, and a row that cannot apply is off (Cut, Delete and Group when something in the selection is locked); on the empty canvas Paste (at the right-click) and Select all; in a read-only board Copy only. The menu's Paste reads the system clipboard's text with `navigator.clipboard.readText` only when it could be newer than the copy here, and waits for it at most 500 ms.
- **Pasted and dropped pictures:** a paste whose clipboard holds a picture file (PNG, JPEG, GIF, WebP, SVG) and a drop of picture files on the canvas each make one `image.insert { page, data, mime, position, size }` per picture, the picture tool's path: centred on the pointer (or the drop point), at its natural size limited to 800 units on its longer side, each further picture 24 units on; a dropped file's name is its alternative text. A picture the engine refuses (larger than its blob limit, or not the type it claims) is reported as the picture tool reports one, and nothing is inserted.
- **Undo/redo:** `edit.undo`/`edit.redo` on the engine. The person's Undo takes back only the person's steps and an agent's only the agent's.
- The board editor's tools are **editor tools** (`KitTool` in TypeScript, KHP-0003 PT-P1), and KHP-0003 PT1–PT10 apply to them (roles `select`, `hand`, `zoom`, `place` for cards and images, `transform`, `draw` for pen and shapes, `text-edit` for labels).

## 5. Commands, agent tools, check

### 5.1 Commands

Conventions: every command has an id, title, menu path or none, shortcut or none, `mutates`, `enabled` (E6) and a `ParameterSchemaSpec` (E7, AX5); a parameter that names a workspace file is typed `file` with `access` and `accept` (KHP-0002 A28, requirement FA1); targets are explicit ids, never "the selection" (PT10); empty or malformed params answer a typed `invalid-params`, never a panic (E3); a key the schema does not name is refused (E7a). Geometry in board units; ids are KDM ids.

| Group | Command | Params (summary) | Notes |
| --- | --- | --- | --- |
| Elements | `element.create` | `{ page, elements: [{ kind, position?, size?, text?, style fields…, key? }] }` | ids are assigned by the engine and answered |
| | `element.update` | `{ id, set: { field: value } }` | only fields valid for the kind (1.2 table); a new `shapeType` moves the ends bound to the shape onto its new outline in the same transaction; the style panel's Shape section runs it once per selected shape (4.3) |
| | `element.delete` | `{ ids }` | deletes descendants of frames and groups; connectors bound to them keep their last point (V6) |
| | `element.move` | `{ ids, delta: [dx, dy] }` or `{ ids, to: [x, y] }` | recomputes bound connectors (1.4) |
| | `element.resize` | `{ id, box: [x0, y0, x1, y1] }` or `{ ids, box }` | |
| | `element.rotate` | `{ ids, degrees, about?: "each" \| "selection" }` | |
| | `element.style` | `{ ids, style: { strokeColor?, fillColor?, … } }` | colours: token or `#rrggbb` |
| | `element.getStyle` | `{ ids }` | `mutates: false` |
| | `shape.previews` | `{ size? }` (8 to 128, 28 by default) → `{ groups: [{ id, shapes: [{ shape, svg, outline? }] }] }` | `mutates: false`; the four groups of `SHAPE_GROUPS` and one SVG per shape, `size` × `size * 0.75`, drawn by the renderer's `shapes::outline` in `currentColor`, unfilled: the editor asks once when the board opens, and its shape library panel shows these and its style panel's Shape section lists their groups, so it has no shape drawing or shape table of its own. `outline` is what a bound end meets (1.4): `"ellipse"`, the polygon's corners as `[fx, fy]` fractions of the box, or `{ "slant": [fx, fy] }` (the parallelogram, inset `min(w * fx, h * fy)`); absent for the box |
| | `element.setText` | `{ id, text }` | minimal diff on the KDM text |
| | `text.mark` | `{ id, range: [start, end], mark: "bold" \| "italic" \| "link", value?, units? }` | Unicode scalar offsets for agents, UTF-16 (`units: "utf16"`) for the editor (M18); `value` `null` removes the mark, a link's is `{ href }` (http, https or mailto); the editor's marks bar sends one per mark after `element.setText` (4.3) |
| | `element.duplicate` | `{ ids, offset? }` | |
| | `element.reorder` | `{ ids, placement: front \| forward \| backward \| back }` | |
| | `element.lock` | `{ ids, locked }` | |
| | `element.group` / `element.ungroup` | `{ ids }` → new `group` id / `{ groupIds }` | |
| Frames | `frame.create` | `{ page, box, title?, children? }` | |
| | `frame.fit` | `{ id }` | fix hint of `board.outside-frame` |
| Strokes | `stroke.create` | `{ page, points, style? }` | |
| Connectors | `connector.create` | `{ page, from: end, to: end, label?, type?, arrowheads?, waypoints? }`, `end` = `{ element, anchor? }` or `{ point }` | |
| | `connector.bind` | `{ id, terminal: start \| end, to: end }` | fix hint of `board.dangling-binding` |
| | `connector.setLabel` | `{ id, text, position? }` | |
| | `connector.setWaypoints` | `{ id, waypoints }` | |
| Cards | `card.create` | `{ page, file?: file(read), url?, title?, kind?, view?, position?, size?, caption? }` | 3.4; refuses this board as target |
| | `card.setTarget` | `{ id, file?: file(read), url?, title?, kind?, view? }` | fix hint of both card checks |
| Images | `image.insert` | `{ page, file: file(read), position?, size?, alt? }` | reads the file once into a blob |
| | `image.setAlt` | `{ id, alt }` | |
| Layout | `layout.align` | `{ ids, edge: left \| center \| right \| top \| middle \| bottom }` | |
| | `layout.distribute` | `{ ids, axis: horizontal \| vertical }` | |
| | `layout.stack` | `{ ids, axis, gap? }` | |
| | `layout.mindmap` | `{ root }` | |
| Mind maps | `mindmap.addChild`, `mindmap.addSibling`, `mindmap.collapse`, `mindmap.setDirection` | `{ node, … }` | |
| Pages | `page.create`, `page.rename`, `page.duplicate`, `page.delete`, `page.move` | | |
| | `page.activate` | `{ page }` | `mutates: false`, editor; the active page is view state (M2). When the shown page goes (a delete, an undo, an agent's step, a co-editor), the editor shows the page the person came from to a page they made with `page.create { activate }`, else the page before it (or the one that took its place); a redo or an undo that brings it back shows it again. The editor's page tabs under the canvas run these commands |
| Clipboard | `clipboard.copy` | `{ ids }` → payload (`application/x-kition-board+json`) | `mutates: false` |
| | `element.paste` | `{ page, payload, at? }` | |
| Templates | `template.insert` | `{ page, template: mindmap \| flowchart \| roadmap \| kanban \| retrospective \| storyboard, at? }` | |
| Files | `file.export` | `{ format: png \| svg \| pdf, page?, frame?, scale?, background? }` | a page or one frame, drawn from the engine's scene, answered as base64 |
| | `file.render` | a view (a page, a frame, an element or an area) | a PNG for looking at the board, without changing it |
| Links | `links.retarget` | `{ from, to }` | KHP-0005 LK2; host-run with origin `system` |
| Edit | `edit.undo`, `edit.redo` | `{ scope? }` | per person and per agent |

The parameter schemas in `crates/engine/model/src/spec.rs` are the reference; this table summarises them.

### 5.2 Agent tools

From the manifest's `agent: { prefix: "board", tools: "document" }`: `board_inspect`, `board_commands`, `board_run`, `board_query`, `board_batch`, `board_check`, plus `board_create`, `board_export` (KHP-0004 A41) and the draft tools (`drafts: true`, A35). `board_inspect` reports the board's pages, frames and element counts; `preview: true` (A39, AX7) adds the engine's `render` of the page or a target. `board_query` follows the object model (`objectModel: true`) and `board_batch` runs the KDM batch (AX4). The agent's guide is `agent/guide.md`.

### 5.3 `board_check`: 12 codes

Each code has a synthetic trigger file in `conformance/` and an entry in the package's `agent/checks.json` (KHP-0004 A38, AX6); errors first (A21–A23).

| Code | Severity | Target | Fix hint |
| --- | --- | --- | --- |
| `board.card-target-missing` | error | card | `card.setTarget` |
| `board.image-missing-blob` | error | image | `image.insert` |
| `board.dangling-binding` | error | connector | `connector.bind` |
| `board.card-target-changed` | warning | card | `card.setTarget` |
| `board.label-overflow` | warning | element | `element.resize` |
| `board.overlapping-label` | warning | element | `element.move` |
| `board.overlap` | warning | elements | `layout.distribute` |
| `board.disconnected-node` | warning | element | `connector.create` |
| `board.missing-label` | warning | element | `element.setText` |
| `board.outside-frame` | warning | element | `frame.fit` |
| `board.connector-crosses-node` | info | connector | `connector.setWaypoints` |
| `board.empty-frame` | info | frame | — |

`board.card-target-missing` follows 3.3 (only when the reader knows). `board.dangling-binding` reports the current state and the ends a file had dangling that opening detached.

## 6. Not in the kit

- No comment commands: the schema declares the comment types, but an agent could then write threads nobody can see, so `collaboration.comments.commands` stays absent until the editor shows comments.
- No text authoring format (such as a Mermaid subset) and no automatic graph layout command.
- No import or export of Excalidraw or JSON Canvas files.
- Cards do not open their target on click, and the editor does not fill a card's preview slot.
