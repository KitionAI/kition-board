//! Text in runs of their own style (bold, italic and link marks; Markdown): each paragraph wrapped
//! as one flow, each line a row of pieces in one style each. Measured as plain text is (a sum of
//! advances, [`crate::width`]); bold is weight 700, italic is drawn slanted with upright advances.

use crate::{Family, LINE_HEIGHT, Style, advance_em, is_wide, registry};

/// The weight bold runs are set at.
pub const BOLD: u16 = 700;

/// A run of text and how it differs from the element's text style.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    /// Code: the monospace family.
    pub mono: bool,
    /// The run's size over the element's (headings are larger).
    pub scale: f64,
}

impl Run {
    pub fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), bold: false, italic: false, mono: false, scale: 1.0 }
    }

    /// The run's style on an element whose text is set in `base`.
    pub fn style(&self, base: Style) -> Style {
        let scale = if self.scale.is_finite() && self.scale > 0.0 { self.scale } else { 1.0 };
        let weight = if self.bold { base.weight.max(BOLD) } else { base.weight };
        Style::new(base.size * scale, weight, if self.mono { Family::Mono } else { base.family })
    }
}

/// One paragraph: its runs, in order.
pub type Paragraph = Vec<Run>;

/// A piece of a laid-out line: text in one style, `x` from the line's start.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub text: String,
    pub x: f64,
    pub width: f64,
    pub style: Style,
    pub italic: bool,
}

/// One laid-out line of styled text.
#[derive(Clone, Debug, PartialEq)]
pub struct RichLine {
    pub pieces: Vec<Piece>,
    /// Its advance width.
    pub width: f64,
    /// Its height: the line height of its largest piece (the base style's when it is empty).
    pub height: f64,
    /// Where its baseline sits below its top.
    pub baseline: f64,
}

/// Text in one run's style, before it is placed on a line.
struct Atom<'a> {
    run: usize,
    text: &'a str,
    style: Style,
    italic: bool,
    /// A line may break after it.
    breaks: bool,
}

fn measure(text: &str, style: Style) -> f64 {
    let fonts = registry();
    text.chars().map(|ch| advance_em(&fonts, ch, style.weight, style.family)).sum::<f64>() * style.size
}

/// Splits a paragraph's runs at its break opportunities: after a run of spaces, and before and
/// after each wide character, as plain text wraps.
fn atoms<'a>(paragraph: &'a [Run], base: Style) -> Vec<Atom<'a>> {
    let mut out: Vec<Atom<'a>> = Vec::new();
    for (index, run) in paragraph.iter().enumerate() {
        let style = run.style(base);
        let mut start = 0;
        let mut chars = run.text.char_indices().peekable();
        while let Some((at, ch)) = chars.next() {
            let end = at + ch.len_utf8();
            let cut = match chars.peek() {
                None => true,
                Some((_, next)) => (ch.is_whitespace() && !next.is_whitespace()) || is_wide(ch) || is_wide(*next),
            };
            if cut && let Some(text) = run.text.get(start..end) {
                out.push(Atom { run: index, text, style, italic: run.italic, breaks: ch.is_whitespace() || is_wide(ch) });
                start = end;
            }
        }
    }
    // A run that ends mid-word does not break there unless the next one starts with a wide
    // character or a space.
    let starts: Vec<bool> = out.iter().map(|atom| atom.text.chars().next().is_some_and(|ch| is_wide(ch) || ch.is_whitespace())).collect();
    for (atom, next_starts) in out.iter_mut().zip(starts.into_iter().skip(1)) {
        if next_starts {
            atom.breaks = true;
        }
    }
    if let Some(last) = out.last_mut() {
        last.breaks = true;
    }
    out
}

/// Lays out `paragraphs` set in `base` (each run in its own style), wrapped to `max_width` when
/// given.
pub fn layout_rich(paragraphs: &[Paragraph], base: Style, max_width: Option<f64>) -> Vec<RichLine> {
    const SLACK: f64 = 0.01;
    let max = max_width.filter(|max| max.is_finite() && *max > 0.0);
    let mut lines = Vec::new();
    for paragraph in paragraphs {
        let atoms = atoms(paragraph, base);
        // Words: atoms with no break between them.
        let mut words: Vec<Vec<&Atom<'_>>> = Vec::new();
        let mut word = Vec::new();
        for atom in &atoms {
            word.push(atom);
            if atom.breaks {
                words.push(std::mem::take(&mut word));
            }
        }
        let width_of = |atoms: &[&Atom<'_>], trim: bool| -> f64 {
            atoms
                .iter()
                .enumerate()
                .map(
                    |(i, atom)| if trim && i + 1 == atoms.len() { measure(atom.text.trim_end(), atom.style) } else { measure(atom.text, atom.style) },
                )
                .sum()
        };
        // A word wider than a line on its own breaks between characters.
        let pieces: Vec<Atom<'_>> = atoms
            .iter()
            .flat_map(|atom| {
                atom.text.char_indices().map(move |(at, ch)| Atom {
                    run: atom.run,
                    text: atom.text.get(at..at + ch.len_utf8()).unwrap_or_default(),
                    style: atom.style,
                    italic: atom.italic,
                    breaks: false,
                })
            })
            .collect();
        let mut line: Vec<&Atom<'_>> = Vec::new();
        let mut produced = false;
        let mut start = 0;
        for word in words {
            let chars: usize = word.iter().map(|atom| atom.text.chars().count()).sum();
            let span = start..start + chars;
            start += chars;
            let blank = word.iter().all(|atom| atom.text.chars().all(char::is_whitespace));
            let Some(max) = max.filter(|_| !blank) else {
                line.extend(word);
                continue;
            };
            let mut candidate = line.clone();
            candidate.extend(word.iter().copied());
            if !line.is_empty() && width_of(&candidate, true) > max + SLACK {
                lines.push(finish(&line, base));
                produced = true;
                line.clear();
            }
            if line.is_empty() && width_of(&word, true) > max + SLACK {
                for piece in pieces.get(span).unwrap_or_default() {
                    let mut candidate = line.clone();
                    candidate.push(piece);
                    if !line.is_empty() && width_of(&candidate, true) > max + SLACK {
                        lines.push(finish(&line, base));
                        produced = true;
                        line.clear();
                    }
                    line.push(piece);
                }
                continue;
            }
            line.extend(word);
        }
        if !line.is_empty() || !produced {
            lines.push(finish(&line, base));
        }
    }
    lines
}

/// A line from its atoms: trailing spaces dropped, neighbouring atoms of one run joined.
fn finish(atoms: &[&Atom<'_>], base: Style) -> RichLine {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut runs: Vec<usize> = Vec::new();
    let mut x = 0.0;
    let last = atoms.len().saturating_sub(1);
    for (i, atom) in atoms.iter().enumerate() {
        let text = if i == last { atom.text.trim_end() } else { atom.text };
        if text.is_empty() {
            continue;
        }
        let width = measure(text, atom.style);
        match (pieces.last_mut(), runs.last()) {
            (Some(piece), Some(run)) if *run == atom.run => {
                piece.text.push_str(text);
                piece.width += width;
            }
            _ => {
                pieces.push(Piece { text: text.to_owned(), x, width, style: atom.style, italic: atom.italic });
                runs.push(atom.run);
            }
        }
        x += width;
    }
    let size = pieces.iter().map(|piece| piece.style.size).fold(0.0, f64::max);
    let size = if size > 0.0 { size } else { base.size };
    RichLine { pieces, width: x, height: size * LINE_HEIGHT, baseline: size * crate::BASELINE }
}

/// The size of the lines' block: the widest line and the lines' heights.
pub fn rich_block(paragraphs: &[Paragraph], base: Style, max_width: Option<f64>) -> (f64, f64) {
    let lines = layout_rich(paragraphs, base, max_width);
    let widest = lines.iter().map(|line| line.width).fold(0.0, f64::max);
    let height = lines.iter().map(|line| line.height).sum::<f64>();
    (widest, if lines.is_empty() { base.line_height() } else { height })
}
