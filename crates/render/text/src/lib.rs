//! `kition-board-text`: how wide the Board engine's text is, so labels it lays out line up with
//! what a browser draws (spike S2, risk R-B1).
//!
//! **Fonts.** A web build has no system fonts: the host hands the engine font files (`addFont`,
//! standard E9) before a document opens, and the page's `@font-face` names the same files. The
//! editor gives a Latin face first (Inter) and then Droid Sans Fallback for Chinese and Japanese,
//! because Droid Sans Fallback has no Latin letters. A character takes the first registered face
//! that has a glyph for it, as the browser's font fallback does along the `font-family` list
//! ([`css_family`]).
//!
//! **Measurement.** A line is the sum of its characters' advances: no kerning, no ligatures, no
//! shaping. The engine's SVG sets `font-kerning: none`, `font-variant-ligatures: none` and
//! `font-optical-sizing: none` so the browser draws those same advances (spike S2 measured the
//! difference at 0.01 px or less). Variable faces are measured at the requested weight (`wght`);
//! other axes stay at their defaults, as `font-optical-sizing: none` leaves them in the browser.
//! A character no registered face has is estimated (0.6 em, 1 em when wide); so is every serif and
//! monospace text, whose faces the editor does not register at D0.
//!
//! **Wrapping.** Greedy, at spaces and between wide (CJK) characters; a word longer than the line
//! breaks between characters.
//!
//! **Styled text.** Text with bold, italic or link marks, and Markdown text ([`markdown`]), is laid
//! out in runs ([`layout_rich`]): each run measured in its own style (bold at weight 700, code in
//! the monospace family, headings larger), wrapped as one flow.

pub mod markdown;
mod rich;

pub use rich::{BOLD, Paragraph, Piece, RichLine, Run, layout_rich, rich_block};

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

/// Line height, in em.
pub const LINE_HEIGHT: f64 = 1.25;
/// Where the baseline sits below the top of its line, in em.
pub const BASELINE: f64 = 0.95;
/// The size of a `text` element without `fontSize`, in board units.
pub const TEXT_SIZE: f64 = 20.0;
/// The size of a label (shape, connector, frame, card) without `fontSize`.
pub const LABEL_SIZE: f64 = 16.0;
/// Regular and semibold.
pub const REGULAR: u16 = 400;
pub const SEMIBOLD: u16 = 600;

/// An element's `fontFamily`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Family {
    #[default]
    Sans,
    Serif,
    Mono,
}

impl Family {
    /// `sans` (also for anything unknown), `serif`, `mono`.
    pub fn parse(value: Option<&str>) -> Family {
        match value {
            Some("serif") => Family::Serif,
            Some("mono") => Family::Mono,
            _ => Family::Sans,
        }
    }
}

/// How a run of text is set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// In board units; non-finite or non-positive sizes read as [`LABEL_SIZE`].
    pub size: f64,
    pub weight: u16,
    pub family: Family,
}

impl Style {
    pub fn new(size: f64, weight: u16, family: Family) -> Self {
        let size = if size.is_finite() && size > 0.0 { size.min(1000.0) } else { LABEL_SIZE };
        Self { size, weight, family }
    }

    pub fn line_height(&self) -> f64 {
        self.size * LINE_HEIGHT
    }
}

/// One laid-out line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// The characters drawn (trailing spaces of a wrapped line removed).
    pub text: String,
    /// Its advance width, in board units.
    pub width: f64,
}

/// A registered face.
struct Font {
    family: String,
    data: Arc<Vec<u8>>,
    index: u32,
    weight_axis: Option<(f32, f32)>,
    /// Advances in em by (character, weight); `None` when the face has no glyph.
    advances: Mutex<HashMap<(char, u16), Option<f64>>>,
}

static FONTS: RwLock<Vec<Arc<Font>>> = RwLock::new(Vec::new());
static EPOCH: AtomicU64 = AtomicU64::new(0);

const WGHT: ttf_parser::Tag = ttf_parser::Tag::from_bytes(b"wght");

impl Font {
    fn advance(&self, ch: char, weight: u16) -> Option<f64> {
        let key = (ch, if self.weight_axis.is_some() { weight } else { REGULAR });
        if let Ok(cache) = self.advances.lock()
            && let Some(known) = cache.get(&key)
        {
            return *known;
        }
        let measured = self.measure(ch, key.1);
        if let Ok(mut cache) = self.advances.lock() {
            cache.insert(key, measured);
        }
        measured
    }

    fn measure(&self, ch: char, weight: u16) -> Option<f64> {
        let mut face = ttf_parser::Face::parse(&self.data, self.index).ok()?;
        let glyph = face.glyph_index(ch)?;
        if let Some((min, max)) = self.weight_axis {
            face.set_variation(WGHT, f32::from(weight).clamp(min, max));
        }
        let units = f64::from(face.units_per_em());
        if units <= 0.0 {
            return None;
        }
        face.glyph_hor_advance(glyph).map(|advance| f64::from(advance) / units)
    }
}

fn family_name(face: &ttf_parser::Face<'_>) -> Option<String> {
    let names = face.names();
    let pick = |id: u16| names.into_iter().filter(|name| name.name_id == id && name.is_unicode()).find_map(|name| name.to_string());
    pick(ttf_parser::name_id::TYPOGRAPHIC_FAMILY).or_else(|| pick(ttf_parser::name_id::FAMILY)).filter(|name| !name.trim().is_empty())
}

/// Registers the faces of a font file (a collection gives each of its faces), after those already
/// registered. A family registered before is not added again. Answers how many faces were added;
/// bytes that are not a font add none.
pub fn add_font(bytes: Vec<u8>) -> usize {
    let data = Arc::new(bytes);
    let count = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
    let Ok(mut fonts) = FONTS.write() else { return 0 };
    let mut added = 0;
    for index in 0..count.min(64) {
        let Ok(face) = ttf_parser::Face::parse(&data, index) else { continue };
        let Some(family) = family_name(&face) else { continue };
        if fonts.iter().any(|font| font.family == family) {
            continue;
        }
        let weight_axis = face.variation_axes().into_iter().find(|axis| axis.tag == WGHT).map(|axis| (axis.min_value, axis.max_value));
        fonts.push(Arc::new(Font { family, data: data.clone(), index, weight_axis, advances: Mutex::new(HashMap::new()) }));
        added += 1;
    }
    if added > 0 {
        EPOCH.fetch_add(1, Ordering::SeqCst);
    }
    added
}

/// Moves each time faces are added: text measured before may measure differently now.
pub fn font_epoch() -> u64 {
    EPOCH.load(Ordering::SeqCst)
}

/// The registered families, in fallback order.
pub fn families() -> Vec<String> {
    FONTS.read().map(|fonts| fonts.iter().map(|font| font.family.clone()).collect()).unwrap_or_default()
}

/// The registered font files, each once, in registration order (for a raster that draws the
/// same faces).
pub fn font_files() -> Vec<Arc<Vec<u8>>> {
    let mut out: Vec<Arc<Vec<u8>>> = Vec::new();
    for font in registry() {
        if !out.iter().any(|data| Arc::ptr_eq(data, &font.data)) {
            out.push(font.data.clone());
        }
    }
    out
}

fn quoted(family: &str) -> String {
    let clean: String = family.chars().filter(|ch| !matches!(ch, '\'' | '"' | '\\' | '<' | '>' | '&')).collect();
    format!("'{clean}'")
}

/// The CSS `font-family` list for `family`: the registered faces in order, then the generic family
/// (sans), or the generic family with the CJK face behind it (serif, mono).
pub fn css_family(family: Family) -> String {
    let registered = families();
    match family {
        Family::Sans => registered.iter().map(|name| quoted(name)).chain(std::iter::once("sans-serif".to_owned())).collect::<Vec<_>>().join(", "),
        Family::Serif => "serif".to_owned(),
        Family::Mono => "monospace".to_owned(),
    }
}

/// Wide characters: CJK ideographs, kana, Hangul, full-width forms.
pub fn is_wide(ch: char) -> bool {
    matches!(u32::from(ch),
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
}

fn estimate(ch: char) -> f64 {
    if ch.is_control() || matches!(ch, '\u{200B}'..='\u{200D}' | '\u{FEFF}') {
        0.0
    } else if is_wide(ch) {
        1.0
    } else {
        0.6
    }
}

/// A character's advance in em at `weight` in `family`.
fn advance_em(fonts: &[Arc<Font>], ch: char, weight: u16, family: Family) -> f64 {
    if family == Family::Sans {
        for font in fonts {
            if let Some(advance) = font.advance(ch, weight) {
                return advance;
            }
        }
    }
    estimate(ch)
}

fn registry() -> Vec<Arc<Font>> {
    FONTS.read().map(|fonts| fonts.clone()).unwrap_or_default()
}

/// The advance width of `text` on one line, in board units.
pub fn width(text: &str, style: Style) -> f64 {
    let fonts = registry();
    text.chars().map(|ch| advance_em(&fonts, ch, style.weight, style.family)).sum::<f64>() * style.size
}

/// The text's lines: one per paragraph (`\n`), each wrapped to `max_width` when given.
pub fn layout(text: &str, style: Style, max_width: Option<f64>) -> Vec<Line> {
    let fonts = registry();
    let measure = |run: &str| run.chars().map(|ch| advance_em(&fonts, ch, style.weight, style.family)).sum::<f64>() * style.size;
    let max_width = max_width.filter(|max| max.is_finite() && *max > 0.0);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let paragraph = paragraph.strip_suffix('\r').unwrap_or(paragraph);
        let Some(max) = max_width else {
            lines.push(Line { text: paragraph.to_owned(), width: measure(paragraph) });
            continue;
        };
        wrap(paragraph, max, &measure, &mut lines);
    }
    lines
}

/// A break opportunity follows each run of spaces and each wide character.
fn tokens(paragraph: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = paragraph.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        let next = chars.peek().map(|(_, next)| *next);
        let end = at + ch.len_utf8();
        let breaks = match next {
            None => true,
            Some(next) => (ch.is_whitespace() && !next.is_whitespace()) || is_wide(ch) || is_wide(next),
        };
        if breaks {
            if let Some(token) = paragraph.get(start..end) {
                out.push(token);
            }
            start = end;
        }
    }
    out
}

fn wrap(paragraph: &str, max: f64, measure: &dyn Fn(&str) -> f64, lines: &mut Vec<Line>) {
    const SLACK: f64 = 0.01;
    let mut current = String::new();
    let mut first = true;
    let flush = |current: &mut String, lines: &mut Vec<Line>| {
        let text = current.trim_end().to_owned();
        let width = measure(&text);
        lines.push(Line { text, width });
        current.clear();
    };
    for token in tokens(paragraph) {
        let blank = token.chars().all(char::is_whitespace);
        if current.is_empty() && blank && !first {
            continue;
        }
        let candidate = format!("{current}{token}");
        if current.is_empty() || blank || measure(candidate.trim_end()) <= max + SLACK {
            current = candidate;
        } else {
            flush(&mut current, lines);
            current.push_str(token);
        }
        first = false;
        // A token wider than the line on its own breaks between characters.
        if measure(current.trim_end()) > max + SLACK && current.chars().count() > 1 {
            let mut piece = String::new();
            for ch in std::mem::take(&mut current).chars() {
                let next = format!("{piece}{ch}");
                if !piece.is_empty() && measure(next.trim_end()) > max + SLACK {
                    flush(&mut piece, lines);
                    piece.push(ch);
                } else {
                    piece = next;
                }
            }
            current = piece;
        }
    }
    if !current.is_empty() || first {
        flush(&mut current, lines);
    }
}

/// The size of the text's block: the widest line and the lines' height.
pub fn block(text: &str, style: Style, max_width: Option<f64>) -> (f64, f64) {
    let lines = layout(text, style, max_width);
    let widest = lines.iter().map(|line| line.width).fold(0.0, f64::max);
    (widest, lines.len().max(1) as f64 * style.line_height())
}

#[cfg(test)]
mod tests;
