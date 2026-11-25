# Board writing format

How a model writes Kition Board documents: the steps it sends, how it names what it changes,
what a new document starts with, the named values commands accept, and worked examples.
The package ships the short version for models as [agent/guide.md](../agent/guide.md); this file is
the complete reference it is drawn from. Everything below comes from the engine itself
(its 47 commands and their parameter schemas, and what `inspect` reports) and from the guide.

## What a model writes

A model never writes the file bytes. It calls the kit's agent tools (prefix `board`), and the
engine applies each change, keeps it in undo history and saves the file:

| Tool | Use it to |
| --- | --- |
| `board_inspect {path}` | See the board: its pages with their ids and element counts, and a preview |
| `board_query {path, follow, fields?}` | List elements with their ids: `follow: "pages.0.elements"` reads the first page's elements (positions count from 0), with `fields` such as `kind`, `text`, `position`, `size`, `parent`, `shapeStyle`, `start`, `end` |
| `board_commands {path, query}` | Find commands by keywords (`element`, `connector`, `frame`, `mindmap`, `layout`, `page`); at most 12 answers |
| `board_run {path, steps}` | Run 1 to 50 steps `{command, params}` in order |
| `board_batch` | Set fields through the object model (ids from `board_query`) |
| `board_check {path}` | List problems: disconnected flow nodes, elements outside their frame, overlaps, connectors crossing nodes, labels that overflow, empty frames, mind nodes without a label, broken cards and pictures |
| `board_create {path}` | Make a new, empty board |

## Syntax

A step is one engine command with its parameters, as JSON:

```json
{"command": "<command id>", "params": {"<name>": <value>}}
```

`board_run` takes a list of steps and runs them in order, stopping at the first failure. Parameter
names, types and required fields come from each command's schema; `board_commands` returns them.
Commands by namespace (the part of the id before the first dot):

| Namespace | Commands | Change the document | Open to the agent | Examples |
| --- | ---: | ---: | ---: | --- |
| `arrange` | 2 | 2 | 2 | `arrange.group`, `arrange.ungroup` |
| `card` | 2 | 2 | 2 | `card.add`, `card.setTarget` |
| `connector` | 4 | 4 | 4 | `connector.add`, `connector.bind`, `connector.setLabel` |
| `edit` | 4 | 3 | 4 | `edit.undo`, `edit.redo`, `edit.copy` |
| `element` | 12 | 11 | 12 | `element.duplicate`, `element.delete`, `element.add` |
| `file` | 1 | 0 | 1 | `file.export` |
| `frame` | 2 | 2 | 2 | `frame.add`, `frame.fit` |
| `image` | 2 | 2 | 2 | `image.insert`, `image.setAlt` |
| `layout` | 4 | 4 | 4 | `layout.align`, `layout.distribute`, `layout.stack` |
| `link` | 1 | 1 | 1 | `link.retarget` |
| `mindmap` | 4 | 4 | 4 | `mindmap.addChild`, `mindmap.addSibling`, `mindmap.collapse` |
| `page` | 6 | 5 | 6 | `page.add`, `page.rename`, `page.duplicate` |
| `stroke` | 1 | 1 | 1 | `stroke.add` |
| `template` | 1 | 1 | 1 | `template.insert` |
| `text` | 1 | 1 | 1 | `text.mark` |

### How things are addressed

- **Elements are addressed by id**, a UUID string. Get them from `board_query` or from a command's
  answer: `element.add` answers `{"created": [id, …]}` in the order you gave the elements.
- **Ids are only known after a call answers.** A later step in the same `board_run` cannot refer to
  an element an earlier step made: there are no references such as `$0.created.0`, and `key` is an
  authoring label, not an id. Make the elements, read the ids from the answer, then connect them in
  the next call.
- **Pages**: `page` parameters take a page id from `board_inspect`; without one, a command acts on
  the shown page. In `board_inspect`'s `target` and in diagnostics, `page` counts from 1.
- **Sticky notes, flow-chart nodes and mind-map nodes are shapes** with a `shapeStyle`: `sticky`,
  `flow-node`, `mind-node`. There is no element kind `sticky`.
- A shape belongs to a frame through `parent` (the frame's id). Moving the frame moves it.

## Text writing format

kition-sdk `docs/platform/data-model.md` (section 9.3) plans a Mermaid-like format that the kit compiles
into document operations. The engine has none yet, so a model writes through the commands above.

## Defaults

What a new, empty document starts with:

- File name `Untitled board.kboard`, size 1920 × 1080 (`kit.json` `creates`).

What `board_inspect` reports for a new document made at that size (generated from the engine; lists shortened to 20 entries):

```json
{
  "area": {
    "height": 1080,
    "unit": 1,
    "width": 1920,
    "x": 0,
    "y": 0
  },
  "canRedo": false,
  "canUndo": false,
  "elements": 0,
  "missingBlobs": [],
  "page": "01a1a5c9-71b4-77ea-9045-89f42e550af6",
  "pages": [
    {
      "elements": 0,
      "id": "01a1a5c9-71b4-77ea-9045-89f42e550af6",
      "name": "Page 1"
    }
  ],
  "title": "Untitled board"
}
```

## Themes, layouts and other named values

Parameters that take one of a fixed list of names (themes, layouts, presets, styles, kinds, modes,
units and similar), from the command schemas. A parameter not listed here takes free values; its
schema says which.

| Parameter | Values | Used by |
| --- | --- | --- |
| `connectorType` | `straight`, `elbow`, `curved` | `element.add`, `element.style` |
| `dashStyle` | `solid`, `dashed`, `dotted` | `element.add`, `element.style`, `stroke.add` |
| `direction` | `both`, `right`, `left`, `down` | `mindmap.setDirection` |
| `fillStyle` | `none`, `solid`, `semi`, `pattern` | `element.add`, `element.style` |
| `fontFamily` | `sans`, `serif`, `mono` | `element.add`, `element.style` |
| `format` | `png`, `svg`, `pdf` | `file.export` |
| `kind` | `note`, `table`, `board`, `file`, `link` | `card.add`, `card.setTarget` |
| `kind` | `shape`, `text`, `frame`, `stroke` | `element.add` |
| `kind` | `shape`, `text`, `stroke`, `connector`, `image`, `card`, `frame` | `element.getStyle` |
| `shapeStyle` | `default`, `sticky`, `mind-node`, `flow-node`, `image-placeholder` | `element.add`, `element.getStyle` |
| `shapeType` | `rectangle`, `ellipse`, `triangle`, `diamond`, `hexagon`, `pill`, `parallelogram`, `star`, `cloud`, `heart`, `x-box`, `check-box`, `check`, `arrow-left`, `arrow-right`, `arrow-up`, `arrow-down`, `line`, `pentagon`, `octagon`, `trapezoid`, `right-triangle`, `cross`, `chevron`, `arrow-left-right`, `arrow-up-down`, `callout`, `brace-left`, `brace-right`, `document`, `multi-document`, `predefined-process`, `cylinder`, `manual-input`, `manual-operation`, `delay`, `display`, `off-page`, `stored-data`, `internal-storage`, `summing-junction`, `or-junction`, `note`, `actor` | `element.add` |
| `strokeSize` | `s`, `m`, `l`, `xl` | `element.add`, `element.style`, `stroke.add` |
| `template` | `mindmap`, `flowchart`, `roadmap`, `kanban`, `retrospective`, `storyboard` | `template.insert` |
| `textAlign` | `left`, `center`, `right` | `element.add`, `element.style` |
| `textFormat` | `plain`, `markdown` | `element.add` |
| `type` | `straight`, `elbow`, `curved` | `connector.add` |
| `units` | `scalar`, `utf16` | `text.mark` |

## Examples

### Example 1: add a step to a flow chart

`board_query {"path": "plan.kboard", "follow": "pages.0.elements", "fields": ["kind", "text", "position", "parent"]}`
lists the frame `Sprint` (`01a18a90-…e169c1`, at 0,0, 640 × 320) holding flow nodes `Plan` and
`Build` (`01a18a90-…11c5e1`, at 420,120), joined by a connector. Add `Ship` to the frame:

```json
{"tool": "board_run", "arguments": {"path": "plan.kboard", "steps": [
  {"command": "element.add", "params": {"elements": [{"kind": "shape", "shapeStyle": "flow-node",
    "parent": "01a18a90-f63d-74f3-bebb-52aee9e169c1", "position": [780, 120], "size": [160, 80], "text": "Ship"}]}}
]}}
{"tool": "board_check", "arguments": {"path": "plan.kboard"}}
```

It answers `{"created": ["01a18a95-a876-…"]}`, and the check warns that `Ship` has no connector and
reaches outside `Sprint`, with fixes. Run them with the new id:

```json
{"tool": "board_run", "arguments": {"path": "plan.kboard", "steps": [
  {"command": "connector.add", "params": {"from": {"element": "01a18a90-f63e-7428-86a6-6e459211c5e1", "anchor": [1, 0.5]},
    "to": {"element": "01a18a95-a876-7716-b136-47cc59a663c6", "anchor": [0, 0.5]}}},
  {"command": "frame.fit", "params": {"id": "01a18a90-f63d-74f3-bebb-52aee9e169c1"}}
]}}
{"tool": "board_check", "arguments": {"path": "plan.kboard"}}
```

The check now finds nothing wrong.

### Example 2: a mind map on a new board

```json
{"tool": "board_create", "arguments": {"path": "ideas.kboard"}}
{"tool": "board_run", "arguments": {"path": "ideas.kboard", "steps": [
  {"command": "element.add", "params": {"elements": [{"kind": "shape", "shapeStyle": "mind-node", "position": [0, 0], "text": "Launch"}]}}
]}}
```

It answers the root's id. Branch from it and lay the map out:

```json
{"tool": "board_run", "arguments": {"path": "ideas.kboard", "steps": [
  {"command": "mindmap.addChild", "params": {"node": "01a18a95-f2c9-72e1-ba55-52cd7449e7eb", "text": "Audience"}},
  {"command": "mindmap.addChild", "params": {"node": "01a18a95-f2c9-72e1-ba55-52cd7449e7eb", "text": "Channels"}},
  {"command": "layout.mindmap", "params": {"root": "01a18a95-f2c9-72e1-ba55-52cd7449e7eb"}}
]}}
{"tool": "board_check", "arguments": {"path": "ideas.kboard"}}
```

Each `mindmap.addChild` answers the new node (`node`) and its connector (`created`); the check finds
nothing wrong.

## Workflow

1. `board_inspect`, then `board_query {follow: "pages.0.elements"}` to get ids and positions.
2. `board_commands` with keywords; use only command ids it returned.
3. `board_run` a few steps; take new ids from the answer.
4. `board_check`. Most diagnostics carry a `fix` (`connector.add`, `frame.fit`, …): run it with
   the ids it names.

## Pitfalls

- A step that fails stops the call and the kit undoes the steps before it, so the board is as it was
  before the call and nothing is saved. Read `error.index` for the failing step, fix it, and send the
  whole run again.
- An element placed partly outside its frame is reported (`board.outside-frame`); run
  `frame.fit {id}` on the frame, or move the element.
- A flow node with no connector is reported (`board.disconnected-node`).
- Elements that overlap, and connectors that cross a node, are reported: leave space between them
  (shapes are 160 × 100 by default) and use `anchor` (`[x, y]` on the box, each 0 to 1) to choose the
  side a connector leaves from.
