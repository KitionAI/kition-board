//! Markdown text (`textFormat: markdown`) as styled paragraphs: what a text element or sticky
//! written in Markdown draws. Covers the syntax a board note uses: ATX headings, bullet and
//! numbered lists, block quotes, fenced code, thematic breaks, and inline bold, italic, code,
//! strikethrough, links and images (their text). Markers are not drawn; anything else is text.

use crate::rich::{Paragraph, Run};

/// Heading sizes over the body text, by level.
const HEADING_SCALE: [f64; 6] = [2.0, 1.5, 1.25, 1.1, 1.0, 1.0];

/// How a block sets its inline text.
#[derive(Clone, Copy, Default)]
struct Base {
    bold: bool,
    italic: bool,
    scale: f64,
}

/// `text` as paragraphs of runs, one per line of the source (blank lines stay blank).
pub fn parse(text: &str) -> Vec<Paragraph> {
    let mut out = Vec::new();
    let mut fence: Option<String> = None;
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = line.trim_start();
        if let Some(open) = &fence {
            if trimmed.starts_with(open.as_str()) {
                fence = None;
            } else {
                out.push(vec![Run { mono: true, ..Run::plain(line) }]);
            }
            continue;
        }
        if let Some(open) = ["```", "~~~"].into_iter().find(|open| trimmed.starts_with(open)) {
            fence = Some(open.to_owned());
            continue;
        }
        out.push(block(line));
    }
    out
}

/// `text` after its first `n` bytes (empty when there are fewer, or `n` is inside a character).
fn after(text: &str, n: usize) -> &str {
    text.get(n..).unwrap_or_default()
}

fn block(line: &str) -> Paragraph {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim_start();
    let base = Base { scale: 1.0, ..Base::default() };
    // A thematic break: a quiet rule.
    let marks: Vec<char> = trimmed.chars().filter(|ch| *ch != ' ').collect();
    if marks.len() >= 3 && marks.first().is_some_and(|first| matches!(first, '-' | '*' | '_') && marks.iter().all(|ch| ch == first)) {
        return vec![Run::plain("———")];
    }
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    if let Some(scale) = HEADING_SCALE.get(hashes.wrapping_sub(1))
        && after(trimmed, hashes).starts_with(' ')
    {
        let heading = Base { bold: true, italic: false, scale: *scale };
        return inline(after(trimmed, hashes).trim().trim_end_matches('#').trim_end(), heading);
    }
    if let Some(rest) = trimmed.strip_prefix('>') {
        let mut rest = rest.trim_start();
        while let Some(more) = rest.strip_prefix('>') {
            rest = more.trim_start();
        }
        return inline(rest, Base { italic: true, ..base });
    }
    let pad = " ".repeat(indent.min(12));
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(bullet) {
            let (mark, rest) = match rest.strip_prefix("[ ] ").map(|r| ("☐ ", r)).or_else(|| rest.strip_prefix("[x] ").map(|r| ("☑ ", r))) {
                Some(task) => task,
                None => ("• ", rest),
            };
            let mut runs = vec![Run::plain(format!("{pad}{mark}"))];
            runs.extend(inline(rest, base));
            return runs;
        }
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if (1..=9).contains(&digits) && (after(trimmed, digits).starts_with(". ") || after(trimmed, digits).starts_with(") ")) {
        let mut runs = vec![Run::plain(format!("{pad}{}. ", trimmed.get(..digits).unwrap_or_default()))];
        runs.extend(inline(after(trimmed, digits + 2), base));
        return runs;
    }
    inline(line, base)
}

/// Whether `marker` appears in `rest` (so an opener has its closer).
fn closes(rest: &[char], marker: &[char]) -> bool {
    rest.windows(marker.len()).any(|window| window == marker)
}

/// Inline Markdown as runs over `base`.
fn inline(text: &str, base: Base) -> Paragraph {
    let chars: Vec<char> = text.chars().collect();
    let at = |i: usize| chars.get(i).copied();
    let from = |i: usize| chars.get(i..).unwrap_or_default();
    let mut runs: Paragraph = Vec::new();
    let mut current = String::new();
    let (mut bold, mut italic, mut struck) = (false, false, false);
    let push = |runs: &mut Paragraph, current: &mut String, bold: bool, italic: bool, mono: bool| {
        if !current.is_empty() {
            runs.push(Run { text: std::mem::take(current), bold: base.bold || bold, italic: base.italic || italic, mono, scale: base.scale });
        }
    };
    let mut i = 0;
    while let Some(ch) = at(i) {
        let next = at(i + 1);
        match ch {
            '\\' if next.is_some_and(|n| n.is_ascii_punctuation()) => {
                current.extend(next);
                i += 2;
            }
            '`' if closes(from(i + 1), &['`']) => {
                push(&mut runs, &mut current, bold, italic, false);
                let end = from(i + 1).iter().position(|c| *c == '`').unwrap_or(0);
                current.extend(from(i + 1).iter().take(end));
                push(&mut runs, &mut current, bold, italic, true);
                i += end + 2;
            }
            '*' | '_' if next == Some(ch) && (bold || closes(from(i + 2), &[ch, ch])) => {
                push(&mut runs, &mut current, bold, italic, false);
                bold = !bold;
                i += 2;
            }
            // A doubled marker without its closer is text.
            '*' | '_' if next == Some(ch) => {
                current.extend([ch, ch]);
                i += 2;
            }
            '*' | '_' => {
                let before = i.checked_sub(1).and_then(at);
                let opens = !italic
                    && next.is_some_and(|n| !n.is_whitespace())
                    && closes(from(i + 1), &[ch])
                    && (ch == '*' || before.is_none_or(|b| !b.is_alphanumeric()));
                let shuts = italic && before.is_some_and(|b| !b.is_whitespace()) && (ch == '*' || next.is_none_or(|n| !n.is_alphanumeric()));
                if opens || shuts {
                    push(&mut runs, &mut current, bold, italic, false);
                    italic = !italic;
                } else {
                    current.push(ch);
                }
                i += 1;
            }
            // Strikethrough: its markers are dropped (the text is drawn as it is).
            '~' if next == Some('~') && (struck || closes(from(i + 2), &['~', '~'])) => {
                struck = !struck;
                i += 2;
            }
            '!' if next == Some('[') && link(&chars, i + 1).is_some() => {
                i += 1;
            }
            '[' if link(&chars, i).is_some() => {
                if let Some((label, end)) = link(&chars, i) {
                    current.push_str(&label);
                    i = end;
                }
            }
            _ => {
                current.push(ch);
                i += 1;
            }
        }
    }
    push(&mut runs, &mut current, bold, italic, false);
    if runs.is_empty() {
        runs.push(Run { scale: base.scale, bold: base.bold, italic: base.italic, ..Run::plain("") });
    }
    runs
}

/// A link `[label](target)` starting at `chars[at]`: its label and the index after it.
fn link(chars: &[char], at: usize) -> Option<(String, usize)> {
    let close = at + 1 + chars.get(at + 1..)?.iter().position(|c| *c == ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = close + 2 + chars.get(close + 2..)?.iter().position(|c| *c == ')')?;
    Some((chars.get(at + 1..close)?.iter().collect(), end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(paragraph: &Paragraph) -> String {
        paragraph.iter().map(|run| run.text.as_str()).collect()
    }

    #[test]
    fn markers_are_not_drawn() {
        let parsed =
            parse("# Plan\nShip **on time** and *on budget*.\n- Tests\n2. Docs `ready`\n> Keep [it](https://x.y) short\n\n```\nlet a = 1;\n```");
        let text: Vec<String> = parsed.iter().map(shown).collect();
        assert_eq!(text, ["Plan", "Ship on time and on budget.", "• Tests", "2. Docs ready", "Keep it short", "", "let a = 1;"]);
        assert!(parsed[0][0].bold && parsed[0][0].scale == 2.0);
        assert!(parsed[1].iter().any(|run| run.text == "on time" && run.bold && !run.italic));
        assert!(parsed[1].iter().any(|run| run.text == "on budget" && run.italic && !run.bold));
        assert!(parsed[3].iter().any(|run| run.text == "ready" && run.mono));
        assert!(parsed[4].iter().all(|run| run.italic));
        assert!(parsed[6][0].mono);
    }

    #[test]
    fn lone_markers_stay_text() {
        let text: Vec<String> = parse("2 * 3 = 6\nsnake_case_name\n**open\n#hashtag").iter().map(shown).collect();
        assert_eq!(text, ["2 * 3 = 6", "snake_case_name", "**open", "#hashtag"]);
    }
}
