//! `kition-board-kit`: the Board kit's engine (`kit.board`) on the Kition SDK, engine contract 2.
//!
//! A board is a KDM replica (`kition-board-kdm`); a `.kboard` file is a ZIP holding the
//! document's content (the replica's content snapshot: its state, no history) and its blobs. The
//! history stays with the host (`snapshot`), never in the file. Commands are transactions of the replica (`kition_kit::session::KitSession`);
//! the same document runs in the page as WebAssembly under the kit's editor (`web/`) and headless as
//! `kition-board`.

use std::sync::{Arc, Mutex};

use kition_board_kdm::blobs::SharedBlobs;
use kition_board_kdm::file::{self, FileBase};
use kition_board_kdm::{BoardModel, DanglingEnd, PageView, referenced_blobs};
use kition_board_scene::{Raster, Revisions};
use kition_kit::kdm::{ActorId, ObjectId, Origin, UndoScope, Version};
use kition_kit::session::{BlobSource, Damage, KitSession, KitSessionDocument, history_scope};
use kition_kit::{KitCommand, KitDocument};
use serde_json::{Value, json};

mod collab;
mod context;
mod files;
mod render;
mod scene;

pub use collab::BlobLog;
pub use context::EDITOR_STATE;
pub use scene::add_font;

/// The file extension a board opens and saves.
pub const EXTENSION: &str = "kboard";

/// Commands whose file parameters the engine reads through lent files (`kition-board`'s
/// SANDBOXED list), until typed `file` parameters land in `kition-headless`.
pub const SANDBOXED: &[&str] = &["card.add", "card.setTarget", "image.insert"];

/// The formats a board exports to (`kit.json` `exports`, KHP A41), as `file.export` writes them.
pub const EXPORTS: &[&str] = &["png", "svg", "pdf"];

/// The person a command from the in-page editor runs for when the host names none.
fn local_person() -> Origin {
    Origin::person("person:local")
}

/// One open board.
pub struct BoardDocument {
    session: KitSession<BoardModel>,
    /// The session's blob store (a clone shares it), enumerable for saves and the blob exchange.
    blobs: SharedBlobs,
    blob_log: BlobLog,
    /// What the file holds: a save of its version writes the same bytes.
    base: Option<FileBase>,
    /// The connector ends the file had dangling, which opening detached (`board_check` reports
    /// them while they stay detached).
    detached: Vec<(ObjectId, DanglingEnd)>,
    /// What the file holds after the last `save`, taken as the base by `mark_saved` once the host
    /// has stored those bytes (edits made in between stay unsaved).
    saving: Mutex<Option<FileBase>>,
    /// The page the editor shows (view state, `inspect().page`). It keeps naming a page another
    /// writer, an undo or a delete removed (an agent's command there is refused, and a redo or an
    /// undo that brings the page back shows it again); meanwhile the editor shows `fallback`.
    active_page: Option<ObjectId>,
    /// The index among the pages of the page shown when the board last changed: where the
    /// neighbour of a page that goes is found.
    shown_at: usize,
    /// The page shown while `active_page` names a page that is gone: the page the person came
    /// from when they made it (`came_from`), else its neighbour (the page before it, or the one
    /// that took its place when it was first). Kept by id, so a later move of pages does not
    /// change what the person sees.
    fallback: Option<ObjectId>,
    /// The page the person showed when their `page.add { activate }` made the page they show
    /// now: an undo of that creation takes them back there. Cleared when they show another page.
    came_from: Option<ObjectId>,
    revision: u64,
    saved: u64,
    /// The appearance revisions `elementSvg` handed out.
    revisions: Mutex<Revisions>,
    /// The raster of the shown page, kept until the board, the page, the fonts or the blobs change.
    raster: Mutex<Option<(RasterKey, Arc<Raster>)>>,
    /// The editor's minimap in the host's dark scheme (`renderScheme`), kept like the raster; a
    /// view concern only: it is never saved, and every other picture is drawn light.
    dark_raster: Mutex<Option<(RasterKey, Arc<Raster>)>>,
    /// The document frame of the shown page, kept like the raster (`inspect`, `width` and `height`
    /// each ask for it after every command).
    area: Mutex<Option<(RasterKey, kition_board_scene::Area)>>,
}

/// What a cached raster was drawn from.
#[derive(Clone, Debug, PartialEq)]
struct RasterKey {
    revision: u64,
    page: Option<ObjectId>,
    fonts: u64,
    blobs: usize,
}

fn extension(name: &str) -> String {
    std::path::Path::new(name).extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// Where the page shown instead of a page at index `at` that went is found: the page before it,
/// or, for the first page, the page that took its place.
const fn neighbour(at: usize) -> usize {
    at.saturating_sub(1)
}

/// Bytes are opened as a board only under a `.kboard` name.
fn check_name(name: &str) -> Result<(), String> {
    if extension(name) == EXTENSION { Ok(()) } else { Err(format!("a board is a .kboard file, not {name:?}")) }
}

/// A new board's title: the file name without its folder and extension.
fn title_of(name: &str) -> String {
    let stem = std::path::Path::new(name).file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    if stem.trim().is_empty() { "Board".to_owned() } else { stem }
}

impl BoardDocument {
    /// Each command's parameter schema, as the catalogue offers it to agents; read once, since a
    /// command's parameters do not change with the board.
    fn parameter_schemas(&self) -> &'static std::collections::HashMap<String, Value> {
        static SCHEMAS: std::sync::OnceLock<std::collections::HashMap<String, Value>> = std::sync::OnceLock::new();
        SCHEMAS.get_or_init(|| self.commands().into_iter().filter_map(|command| Some((command.id, command.schema?))).collect())
    }

    fn new(session: KitSession<BoardModel>, blobs: SharedBlobs, base: Option<FileBase>) -> Self {
        Self {
            session,
            blobs,
            blob_log: BlobLog::default(),
            base,
            detached: Vec::new(),
            saving: Mutex::new(None),
            active_page: None,
            shown_at: 0,
            fallback: None,
            came_from: None,
            revision: 0,
            saved: 0,
            revisions: Mutex::new(Revisions::default()),
            raster: Mutex::new(None),
            dark_raster: Mutex::new(None),
            area: Mutex::new(None),
        }
    }

    /// A document over `replica` with `blobs` as its store.
    fn attach(replica: kition_kit::kdm::Replica, blobs: SharedBlobs, base: Option<FileBase>) -> Result<Self, String> {
        let session = KitSession::from_replica(replica, Box::new(blobs.clone()))?;
        let mut document = Self::new(session, blobs, base);
        // The blobs a document starts with arrived with the state it opened.
        document.blob_log.record(document.session.version(), document.blobs.hashes());
        Ok(document)
    }

    /// The view.
    pub fn model(&self) -> &BoardModel {
        self.session.model()
    }

    /// `page.activate { page }`: the page the editor shows (view state, not an undo step).
    fn activate(&mut self, params: &Value) -> Result<Value, String> {
        let page = params.get("page").and_then(Value::as_str).ok_or("invalid-params: `page` must be a page id")?;
        let id = ObjectId::from_string(page);
        if self.session.model().page(&id).is_none() {
            return Err(format!("not-found: no page {page}"));
        }
        self.active_page = Some(id);
        self.came_from = None;
        self.settle();
        Ok(json!({ "page": page }))
    }

    /// The page the editor shows: the active page while it exists, else the page shown in its
    /// place ([`Self::settle`]), else the first.
    pub fn active_page(&self) -> Option<&PageView> {
        let model = self.session.model();
        self.active_page
            .as_ref()
            .and_then(|id| model.page(id))
            .or_else(|| self.fallback.as_ref().and_then(|id| model.page(id)))
            .or_else(|| model.pages().get(neighbour(self.shown_at)))
            .or_else(|| model.pages().first())
    }

    /// After the board or the shown page changed: notes where the shown page is, and when the
    /// active page is gone (deleted by the person, an undo, another writer or an agent), chooses
    /// the page shown instead. View state only: no edit, no revision.
    pub(crate) fn settle(&mut self) {
        let pages = self.session.model().pages();
        let index = |id: &ObjectId| pages.iter().position(|page| page.id == *id);
        if let Some(at) = self.active_page.as_ref().and_then(index) {
            self.shown_at = at;
            self.fallback = None;
            return;
        }
        if self.active_page.is_none() {
            self.shown_at = 0;
            self.fallback = None;
            return;
        }
        if let Some(at) = self.fallback.as_ref().and_then(index) {
            self.shown_at = at;
            return;
        }
        let back = self.came_from.as_ref().and_then(index);
        let at = back.unwrap_or_else(|| neighbour(self.shown_at).min(pages.len().saturating_sub(1)));
        self.fallback = pages.get(at).map(|page| page.id.clone());
        self.shown_at = at;
    }

    /// What the file holds (the base of the next save).
    pub fn file_base(&self) -> Option<&FileBase> {
        self.base.as_ref()
    }

    /// Records that the document changed (`damage` other than none).
    fn changed(&mut self, damage: &Damage) {
        if *damage != Damage::None {
            self.revision += 1;
        }
        self.settle();
    }

    /// Logs the blobs the store gained since it held `before` hashes, at the replica's version now.
    fn log_new_blobs(&mut self, before: usize) {
        if self.blobs.len() != before {
            let known = self.blob_log.known();
            let added: Vec<String> = self.blobs.hashes().into_iter().filter(|hash| !known.contains(hash)).collect();
            self.blob_log.record(self.session.version(), added);
        }
    }

    /// Hashes the view references that the store lacks (pictures whose bytes have not arrived).
    pub fn missing_blobs(&self) -> Vec<String> {
        self.session.model().image_hashes().into_iter().filter(|hash| !self.blobs.contains(hash)).collect()
    }

    fn file_blobs(&self) -> std::collections::BTreeMap<String, Vec<u8>> {
        referenced_blobs(self.session.model(), &self.blobs)
    }

    /// The version the session is at, encoded (for tests and hosts).
    pub fn version(&self) -> Version {
        self.session.version()
    }
}

impl KitDocument for BoardDocument {
    /// An object-model batch (an agent's `board_batch`) leaves a repaired board, as every command
    /// does: the repair (rules V1 to V9, design note 1.5) runs at the end of each of the batch's
    /// transactions, so it is part of the batch's own undo step.
    fn batch_settler(&self) -> Option<kition_kit::BatchSettler> {
        Some(Box::new(|tx| kition_board_kdm::validate::repair(tx).map(|_| ()).map_err(|error| error.to_string())))
    }

    fn batch_applied(&mut self) {
        self.changed(&Damage::Full);
    }

    /// An `edit.undo` or `edit.redo` holds what the step it takes back (or puts back) wrote: the
    /// objects and fields of the next step of its scope (call-context.md, "Isolated Planning And
    /// Raw Batches"). The guard asks only for an agent's write, so a call that names no scope is
    /// in the agent's.
    fn under(&self, command: &str, params: &Value) -> Result<Option<kition_kit::Under>, String> {
        if matches!(command, "edit.undo" | "edit.redo") {
            let scope = history_scope(params, &Origin::agent("agent")).map_err(|error| error.to_string())?;
            let replica = self.session.replica();
            let touches = if command == "edit.undo" { replica.undo_touches_in(scope) } else { replica.redo_touches_in(scope) };
            let premises = touches
                .into_iter()
                .map(|touch| kition_kit::Premise {
                    id: Value::String(touch.id),
                    fields: touch.fields.map(|fields| fields.into_iter().collect()),
                    ..Default::default()
                })
                .collect();
            return Ok(Some(kition_kit::Under { premises, ..Default::default() }));
        }
        Ok(kition_board_model::premises::under(self.model(), self.active_page().map(|page| &page.id), command, params))
    }

    fn derived(&self, ids: &[Value], names: &[String]) -> Result<Vec<kition_kit::DerivedValues>, String> {
        Ok(kition_board_model::premises::derived(self.model(), ids, names))
    }

    fn covered(&self, command: &str, params: &Value, objects: &[kition_kit::CoveredAsk]) -> Result<Option<kition_kit::CoveredAt>, String> {
        Ok(kition_board_model::premises::covered(self.model(), command, params, objects))
    }

    /// Opens a `.kboard` file. Refuses other names and files that are not boards
    /// (`not-a-board: …`), damaged ones (`damaged: …`) and newer ones (`newer-version: …`).
    fn open(name: &str, bytes: &[u8]) -> Result<Self, String> {
        check_name(name)?;
        let opened = file::open_file(bytes, ActorId::random())?;
        let blobs = SharedBlobs::default();
        for (hash, bytes) in &opened.blobs {
            blobs.insert(hash, bytes)?;
        }
        let mut document = Self::attach(opened.replica, blobs, Some(opened.base))?;
        document.detached = opened.detached;
        Ok(document)
    }

    /// A new board titled after `name` (its stem), with one empty page, `width` × `height` board
    /// units from the origin: the document renders and thumbnails show (content past an edge
    /// grows it on that side).
    fn create(name: &str, width: u32, height: u32) -> Result<Self, String> {
        let size = kition_board_kdm::Size::new(f64::from(width), f64::from(height));
        let replica = kition_board_kdm::create_replica(&title_of(name), Some(size), ActorId::random(), &Origin::system("create"))?;
        Self::attach(replica, SharedBlobs::default(), None)
    }

    fn width(&self) -> u32 {
        self.area().width
    }

    fn height(&self) -> u32 {
        self.area().height
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn dirty(&self) -> bool {
        self.revision != self.saved
    }

    fn render(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        self.raster()?.render(x, y, scale, width, height)
    }

    fn render_area(&self, x0: i32, y0: i32, x1: i32, y1: i32, width: u32, height: u32) -> Result<Vec<u8>, String> {
        self.raster()?.render_area(x0, y0, x1, y1, width, height)
    }

    fn execute(&mut self, command: &str, params: Value) -> Result<Value, String> {
        self.execute_as(command, params, &local_person())
    }

    fn inspect(&self) -> Result<Value, String> {
        let model = self.session.model();
        Ok(json!({
            "canUndo": self.session.can_undo(),
            "canRedo": self.session.can_redo(),
            "title": model.title(),
            "page": self.active_page().map(|page| page.id.as_str().to_owned()),
            "pages": model
                .pages()
                .iter()
                .map(|page| json!({ "id": page.id.as_str(), "name": page.name, "elements": page.elements.len(), "area": kition_board_scene::area(model, Some(page)).to_json() }))
                .collect::<Vec<_>>(),
            "elements": model.element_count(),
            "missingBlobs": self.missing_blobs(),
            "area": self.area().to_json(),
        }))
    }

    fn commands(&self) -> Vec<KitCommand> {
        let mut commands: Vec<kition_kit::KitCommand> = { kition_board_model::spec::list(self.can_step(true), self.can_step(false)) };
        kition_kit::files::declare(&mut commands, files::FILES);
        commands
    }

    /// Asked after every change: answered without building the command table's schemas (only
    /// Undo and Redo are ever disabled).
    fn disabled_commands(&self) -> Vec<String> {
        [("edit.undo", self.can_step(true)), ("edit.redo", self.can_step(false))]
            .into_iter()
            .filter(|(_, enabled)| !enabled)
            .map(|(id, _)| id.to_owned())
            .collect()
    }

    /// The board as a `.kboard` file: the document's content (`content.json`, the state with no
    /// history, rule M16b) and the blobs its state references; the same bytes for the same state.
    /// `name`, when given, must be a `.kboard` name.
    fn save(&self, name: Option<&str>) -> Result<Vec<u8>, String> {
        if let Some(name) = name {
            check_name(name)?;
        }
        let saved = file::save_file(self.session.replica(), self.file_blobs(), self.base.as_ref())?;
        if let Ok(mut saving) = self.saving.lock() {
            *saving = Some(saved.base);
        }
        Ok(saved.bytes)
    }

    /// `board_check` (design note 5.3): broken references, missing pictures, overflowing labels and
    /// flow-chart problems, errors first.
    fn check(&self) -> Vec<kition_kit::diagnostics::Diagnostic> {
        kition_board_check::check_opened(self.session.model(), &self.blobs, &self.detached)
    }

    /// The shown page as `format`, one of [`EXPORTS`], as `file.export` draws it with `options`
    /// (its parameters but `format`); the board does not change.
    fn export(&self, format: &str, options: &Value) -> Result<Vec<u8>, String> {
        let mut params = match options {
            Value::Object(options) => options.clone(),
            _ => serde_json::Map::new(),
        };
        params.insert("format".into(), Value::String(format.to_owned()));
        self.export_bytes(&Value::Object(params)).map(|(_, exported)| exported.bytes)
    }

    fn exports() -> Vec<&'static str> {
        EXPORTS.to_vec()
    }

    /// The page the editor shows (opaque to hosts).
    fn editor_state(&self) -> Value {
        self.editor_snapshot()
    }

    /// Puts back what [`KitDocument::editor_state`] answered; no edit.
    fn restore_editor_state(&mut self, saved: &Value) -> Result<(), String> {
        self.put_back(saved)
    }

    /// Takes an outside save of the file this clean session read (`fresh`, the file read again)
    /// into the live replica instead of replacing it: nothing when both hold the same state,
    /// otherwise the file's state written by object id as one `system` transaction labelled
    /// `import`, outside both undo histories and repaired like any import. Object ids, the undo
    /// histories, the page the editor shows and the blobs stay; the file read becomes the base of
    /// the next save. A file of another document is not reconciled (`false`, nothing changed).
    /// Refuses, changing nothing, when the file deleted the page the editor shows
    /// (`deleted active page`).
    fn reconcile_reread(&mut self, fresh: &Self) -> Result<bool, String> {
        if fresh.session.replica().doc_id() != self.session.replica().doc_id() {
            return Ok(false);
        }
        if self.active_page.as_ref().is_some_and(|page| fresh.model().page(page).is_none()) {
            return Err("deleted active page".into());
        }
        let before = kition_kit::session::state(self.session.replica())?;
        let after = kition_kit::session::state(fresh.session.replica())?;
        let blobs_before = self.blobs.len();
        for hash in fresh.blobs.hashes() {
            if let Some(bytes) = BlobSource::get(&fresh.blobs, &hash) {
                self.blobs.insert(&hash, &bytes)?;
            }
        }
        if before != after {
            let schema = kition_board_kdm::schema();
            let ((), damage) =
                self.session.execute(&Origin::system("import"), |_, tx| kition_kit::session::write_state(tx, &schema, &before, &after))?;
            self.changed(&damage);
        }
        self.log_new_blobs(blobs_before);
        self.base.clone_from(&fresh.base);
        self.detached.clone_from(&fresh.detached);
        // A save taken before the reread is no longer what the file holds.
        if let Ok(mut saving) = self.saving.lock() {
            *saving = None;
        }
        Ok(true)
    }

    /// Shows the page a call acts in (`part`, by id) and checks `on`'s element ids; a board keeps
    /// no selection and no time ([`context`]). No edit.
    fn set_context(&mut self, context: &kition_kit::CallContext) -> Result<(), String> {
        self.set_call_context(context)
    }

    /// Records the last save as what the file holds (no edit: the state, the revision and the undo
    /// histories stay as they are).
    fn mark_saved(&mut self) {
        if let Some(base) = self.saving.get_mut().ok().and_then(Option::take) {
            self.base = Some(base);
        }
        self.saved = self.revision;
    }
}

impl BoardDocument {
    /// Undo (or Redo) is enabled while either scope has a step: the person's or the agent's (an
    /// agent's `edit.undo` takes back only its own, KitModel M12a).
    fn can_step(&self, undo: bool) -> bool {
        let replica = self.session.replica();
        [UndoScope::Person, UndoScope::Agent].into_iter().any(|scope| if undo { replica.can_undo_in(scope) } else { replica.can_redo_in(scope) })
    }
}

impl KitSessionDocument for BoardDocument {
    type Model = BoardModel;

    fn session(&self) -> &KitSession<BoardModel> {
        &self.session
    }

    fn session_mut(&mut self) -> &mut KitSession<BoardModel> {
        &mut self.session
    }

    /// Runs `command`. An agent's command leaves the page the editor shows as it was, also when it
    /// fails; only the commands whose work is moving the editor (`page.activate`, `page.add`
    /// with `activate`) keep their effect. A person's commands run as they always did.
    fn execute_as(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        let agent = kition_kit::dispatch::role() == Some(kition_kit::dispatch::Role::Agent);
        if !agent || context::moves_the_editor(command, &params) {
            return self.run_command(command, params, origin);
        }
        let kept = self.active_page.clone();
        let result = self.run_command(command, params, origin);
        self.active_page = kept;
        self.settle();
        result
    }

    fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage, String> {
        let damage = self.session.import_changes(bytes)?;
        self.changed(&damage);
        Ok(damage)
    }

    /// A draft taken in (one undo step of the person, KitModel M12a): the views (revision,
    /// cached raster and frame, the page shown) follow it as they follow an import.
    fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<Damage, String> {
        let damage = self.session.merge_changes(bytes, origin)?;
        self.changed(&damage);
        Ok(damage)
    }

    /// An earlier state written over the board (one undo step of `origin`): the views follow it.
    fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<Damage, String> {
        let damage = self.session.restore_state(snapshot, origin)?;
        self.changed(&damage);
        Ok(damage)
    }

    fn undo(&mut self) -> Result<(bool, Damage), String> {
        self.undo_in(UndoScope::Person)
    }

    fn redo(&mut self) -> Result<(bool, Damage), String> {
        self.redo_in(UndoScope::Person)
    }

    // The agent's scope keeps the board's views current too, as the person's does.
    fn undo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage), String> {
        let (done, damage) = self.session.undo_in(scope)?;
        self.changed(&damage);
        Ok((done, damage))
    }

    fn redo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage), String> {
        let (done, damage) = self.session.redo_in(scope)?;
        self.changed(&damage);
        Ok((done, damage))
    }
}

impl BoardDocument {
    fn run_command(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        if !kition_board_model::is_known(command) {
            return Err(format!("unknown command {command}"));
        }
        let params = if params.is_null() { json!({}) } else { params };
        if !params.is_object() {
            return Err("invalid-params: params must be an object".into());
        }
        // A key the command's schema does not name is refused, not ignored (platform rule E7a).
        if let Some(schema) = self.parameter_schemas().get(command) {
            kition_kit::schema::refuse_unknown(command, schema, &params).map_err(|error| format!("invalid-params: {error}"))?;
        }
        let blobs_before = self.blobs.len();
        let result = match command {
            // In the scope its `scope` parameter names, else the origin's (E14a, M12a).
            "edit.undo" => history_scope(&params, origin)
                .map_err(|error| error.to_string())
                .and_then(|scope| KitSessionDocument::undo_in(self, scope))
                .map(|(done, damage)| json!({ "done": done, "damage": damage })),
            "edit.redo" => history_scope(&params, origin)
                .map_err(|error| error.to_string())
                .and_then(|scope| KitSessionDocument::redo_in(self, scope))
                .map(|(done, damage)| json!({ "done": done, "damage": damage })),
            "page.activate" => self.activate(&params),
            "element.getStyle" => self.style_of(&params),
            "shape.previews" => self.shape_previews(&params),
            "file.export" => self.export_file(&params),
            "file.render" => self.render_view(&params),
            _ if !kition_board_model::mutates(command) => Ok(kition_board_model::read(self.session.model(), command, &params)?),
            _ => {
                let mut blobs = self.blobs.clone();
                let doc_id = self.doc_id();
                // An agent's command works in the page it was told about: one another writer
                // deleted is refused (`params::page`), not replaced by the page the editor falls
                // back to showing, which is where a person's command works.
                let stale = self.active_page.clone().filter(|id| self.session.model().page(id).is_none());
                let active = match stale {
                    Some(stale) if origin.kind == kition_kit::kdm::OriginKind::Agent => Some(stale),
                    _ => self.active_page().map(|page| page.id.clone()),
                };
                let (value, damage) = self.session.execute(origin, |model, tx| {
                    let mut ctx = kition_board_model::Ctx { blobs: &mut blobs, doc_id: &doc_id, active_page: active.as_ref() };
                    kition_board_model::run(model, tx, &mut ctx, command, &params)
                })?;
                self.changed(&damage);
                if command == "page.add" && params.get("activate").and_then(Value::as_bool) == Some(true) {
                    let shown = self.active_page().map(|page| page.id.clone());
                    self.active_page = value.get("page").and_then(Value::as_str).map(ObjectId::from_string);
                    // An agent's run keeps its own editor state; only the person goes back.
                    if origin.kind != kition_kit::kdm::OriginKind::Agent {
                        self.came_from = shown;
                    }
                    self.settle();
                }
                Ok(value)
            }
        };
        self.log_new_blobs(blobs_before);
        result
    }
}

kition_kit::export_kit_document!(BoardDocument as "BoardDocument");
kition_kit::export_kit_session!(BoardDocument as "BoardDocument");
