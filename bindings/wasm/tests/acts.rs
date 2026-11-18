//! What each command reads and the parameter `on` fills, derived from the code (task card
//! C4/C5/C6-board): this test follows every command from where the engine dispatches it
//! (`kition_board_model::run`'s table and the binding's own commands in `run_command`) through the
//! functions it calls, and fails when `kition_board_model::acts` says otherwise.
//!
//! - **reads**: a command reads the shown page ([`EditorState::Part`]) when its code reaches
//!   `ctx.active_page` (the page the binding hands every model command) or the binding's
//!   `self.active_page()`. A board has no selection, time, mode or active object to read.
//! - **targets**: the declared parameter must be one the command's code reads as element ids
//!   (`params::id`/`params::ids`, or the binding's `params.get`), and every element or page id the
//!   model code reads is either the declared one or listed in `acts::NOT_TARGETS` with why.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use kition_board_kit::BoardDocument;
use kition_board_model::acts;
use kition_kit::{EditorState, KitDocument};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A function's body, by `(module, name)`; the binding's are module `self`.
struct Code {
    bodies: HashMap<(String, String), String>,
}

impl Code {
    fn read() -> Self {
        let mut bodies = HashMap::new();
        let model = repo().join("crates/engine/model/src");
        for entry in std::fs::read_dir(&model).unwrap() {
            let path = entry.unwrap().path();
            let module = path.file_stem().unwrap().to_string_lossy().into_owned();
            Self::functions(&std::fs::read_to_string(&path).unwrap(), &module, &mut bodies);
        }
        for file in ["lib.rs", "scene.rs", "render.rs", "context.rs"] {
            Self::functions(&std::fs::read_to_string(repo().join("bindings/wasm/src").join(file)).unwrap(), "self", &mut bodies);
        }
        Self { bodies }
    }

    /// Every `fn` of `source` (rustfmt's layout: a body ends at the first `}` at the `fn`'s indent).
    fn functions(source: &str, module: &str, out: &mut HashMap<(String, String), String>) {
        let lines: Vec<&str> = source.lines().collect();
        for (at, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            let rest = trimmed.strip_prefix("pub ").or_else(|| trimmed.strip_prefix("pub(crate) ")).unwrap_or(trimmed);
            let Some(signature) = rest.strip_prefix("fn ") else { continue };
            let name: String = signature.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            let close = format!("{indent}}}");
            let end = lines.iter().skip(at).position(|line| *line == close).map_or(lines.len(), |n| at + n + 1);
            out.insert((module.to_owned(), name), lines[at..end].join("\n"));
        }
    }

    /// The functions `body` (in `module`) calls that this code defines.
    fn calls(&self, module: &str, body: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let bytes = body.as_bytes();
        let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
        for (open, _) in body.match_indices('(') {
            let mut start = open;
            while start > 0 && ident(bytes[start - 1]) {
                start -= 1;
            }
            let name = &body[start..open];
            if name.is_empty() {
                continue;
            }
            let before = &body[..start];
            let key = if let Some(prefix) = before.strip_suffix("::") {
                let mut from = prefix.len();
                while from > 0 && ident(prefix.as_bytes()[from - 1]) {
                    from -= 1;
                }
                (prefix[from..].to_owned(), name.to_owned())
            } else if before.ends_with("self.") {
                ("self".to_owned(), name.to_owned())
            } else if before.ends_with('.') {
                continue;
            } else {
                (module.to_owned(), name.to_owned())
            };
            if self.bodies.contains_key(&key) {
                out.push(key);
            }
        }
        out
    }

    /// Every body reachable from `start`, `start` included.
    fn reach(&self, start: (String, String)) -> Vec<(String, String)> {
        let mut seen = BTreeSet::new();
        let mut queue = vec![start];
        while let Some(key) = queue.pop() {
            if !seen.insert(key.clone()) {
                continue;
            }
            if let Some(body) = self.bodies.get(&key) {
                queue.extend(self.calls(&key.0, body));
            }
        }
        seen.into_iter().collect()
    }

    fn text(&self, keys: &[(String, String)]) -> String {
        keys.iter().filter_map(|key| self.bodies.get(key)).cloned().collect::<Vec<_>>().join("\n")
    }
}

/// A function by `(module, name)`.
type Key = (String, String);
/// Which function a dispatch arm's `module::name` or `self.name` calls, if it is one.
type Resolve<'a> = &'a dyn Fn(&str, &str) -> Option<Key>;

/// Where each command starts: `(module, function)`.
fn entries(code: &Code) -> BTreeMap<String, (String, String)> {
    let mut out = BTreeMap::new();
    let arm = |body: &str, module_of: Resolve<'_>, out: &mut BTreeMap<String, Key>| {
        for line in body.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix('"') else { continue };
            let Some((command, call)) = rest.split_once("\" => ") else { continue };
            let function: String = call.split('(').next().unwrap_or_default().to_owned();
            if let Some((module, name)) = function.rsplit_once("::").or_else(|| function.rsplit_once('.'))
                && let Some(key) = module_of(module, name)
            {
                out.insert(command.to_owned(), key);
            }
        }
    };
    let model = |module: &str, name: &str| Some((module.to_owned(), name.to_owned()));
    arm(&code.bodies[&("lib".to_owned(), "dispatch".to_owned())], &model, &mut out);
    arm(&code.bodies[&("lib".to_owned(), "read".to_owned())], &model, &mut out);
    let binding = |module: &str, name: &str| (module == "self").then(|| ("self".to_owned(), name.to_owned()));
    arm(&code.bodies[&("self".to_owned(), "run_command".to_owned())], &binding, &mut out);
    out
}

/// A command reads the shown page.
fn reads_part(text: &str) -> bool {
    text.contains("ctx.active_page") || text.contains("self.active_page()")
}

/// The parameters `text` reads as element or page ids.
fn id_params(text: &str, binding: bool) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut take = |prefix: &str| {
        for (at, _) in text.match_indices(prefix) {
            let rest = &text[at + prefix.len()..];
            if let Some(name) = rest.strip_prefix('"').and_then(|rest| rest.split('"').next()) {
                out.insert(name.to_owned());
            }
        }
    };
    take("params::id(params, ");
    take("params::ids(params, ");
    if binding {
        take("params.get(");
    }
    out
}

#[test]
fn what_each_command_reads_and_the_parameter_on_fills_follow_from_the_code() {
    let code = Code::read();
    let entries = entries(&code);
    let commands = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap().commands();
    let mut problems = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for command in &commands {
        let id = command.id.as_str();
        let declared = acts::reads(id);
        let (text, binding) = match entries.get(id) {
            Some(entry) => (code.text(&code.reach(entry.clone())), entry.0 == "self"),
            // Undo and Redo replay steps; they read no editor state.
            None if matches!(id, "edit.undo" | "edit.redo") => (String::new(), true),
            None => {
                problems.push(format!("{id}: no dispatch found for it"));
                continue;
            }
        };
        let derived: &[EditorState] = if reads_part(&text) { &[EditorState::Part] } else { &[] };
        if declared != derived {
            problems.push(format!("{id}: acts::reads says {declared:?}, its code reads {derived:?}"));
        }
        if command.reads != declared {
            problems.push(format!("{id}: the command lists reads {:?}, acts::reads says {declared:?}", command.reads));
        }
        *counts.entry(format!("{derived:?}")).or_default() += 1;
        let read = id_params(&text, binding);
        let not = |param: &str| acts::NOT_TARGETS.iter().any(|(command, named, _)| *command == id && *named == param);
        match acts::targets(id) {
            Some(target) => {
                if !read.contains(target) {
                    problems.push(format!("{id}: targets `{target}`, which its code does not read as ids (it reads {read:?})"));
                }
                if command.schema.is_some() && command.targets != Some(target) {
                    problems.push(format!("{id}: the command lists targets {:?}, acts::targets says `{target}`", command.targets));
                }
                *counts.entry(format!("targets {target}")).or_default() += 1;
            }
            None if !binding => {
                for param in read.iter().filter(|param| !not(param)) {
                    problems.push(format!("{id}: its code reads ids from `{param}`, which is neither its targets nor in acts::NOT_TARGETS"));
                }
            }
            None => {}
        }
        for (_, param, why) in acts::NOT_TARGETS.iter().filter(|(command, _, _)| *command == id) {
            if !read.contains(*param) || why.trim().is_empty() {
                problems.push(format!("{id}: acts::NOT_TARGETS names `{param}`, which its code does not read as ids (or gives no reason)"));
            }
        }
    }
    for (command, _, _) in acts::NOT_TARGETS {
        if !commands.iter().any(|listed| listed.id == *command) {
            problems.push(format!("acts::NOT_TARGETS names {command}, which is not a command"));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
    // What the code says today: nine commands read the shown page, and 29 take `on`.
    assert_eq!(counts.get("[Part]"), Some(&9), "{counts:?}");
    assert_eq!(counts.iter().filter(|(key, _)| key.starts_with("targets")).map(|(_, n)| n).sum::<usize>(), 29, "{counts:?}");
}
