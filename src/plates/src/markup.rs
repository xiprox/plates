//! The markdown Claude writes in a line a tile shows, such as `code` and **bold**, so the tile can
//! style each span rather than show the marks around it, and the blocks of a whole message, for
//! the hover card. It reads the forms Claude's messages use, counted over 20,211 of them
//! (record/areas/rack/log.md), and leaves out the ones they don't, such as ~~strikethrough~~.

use std::ops::Range;

use plates_core::session::{Fence, is_rule, table_cells};

/// One block of a message as the hover card draws it, with its inline marks still in its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// A paragraph, or a quote without its >.
    Text(String),
    /// A heading without its #s.
    Heading(String),
    /// A list item: its marker, how many levels it nests under the items above it, and its text.
    /// The marker is "•" for a - or * item, "▢" or "✓" for a task item's open or checked box, or
    /// the number for a numbered one, such as "2.".
    Item {
        marker: String,
        depth: usize,
        text: String,
    },
    /// The lines of a fenced code block.
    Code(Vec<String>),
    /// A table: its header's cells, how each column's cells sit, and its rows, each with as many
    /// cells as the header.
    Table {
        header: Vec<String>,
        align: Vec<Align>,
        rows: Vec<Vec<String>>,
    },
}

/// Where a table column's cells sit, from its rule row: `:-:` centers them, and `-:` puts them on
/// the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// The deepest a list item nests, counted from 0 for an item under no other, so the card draws
/// items at three indents, and an item nested further sits at the last.
const MAX_DEPTH: usize = 2;

/// A message's blocks, up to about `budget` characters, cut at a space with "…" after it, or
/// after a table's last whole row. Blank lines and horizontal rules go.
pub fn blocks(message: &str, budget: usize) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut left = budget;
    let mut fence: Option<Fence> = None;
    // The indents of the list items above the line that it could nest under, outermost first.
    // An item nests one level under the nearest item indented less than it, so an item indented
    // 4 spaces under "1. " sits one level in, as one indented 2 spaces under "- " does.
    let mut indents: Vec<usize> = Vec::new();
    let mut lines = message.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        match fence {
            Some(f) if f.closed_by(trimmed) => {
                fence = None;
                continue;
            }
            Some(_) => {}
            None => {
                fence = Fence::open(trimmed);
                if fence.is_some() {
                    out.push(Block::Code(Vec::new()));
                    continue;
                }
            }
        }
        if fence.is_none() && (trimmed.is_empty() || is_rule(trimmed)) {
            continue;
        }
        if left == 0 {
            mark_cut(&mut out);
            break;
        }
        if fence.is_some() {
            let (text, cut) = cut_to(line.trim_end(), left);
            left = left.saturating_sub(text.chars().count());
            if let Some(Block::Code(lines)) = out.last_mut() {
                lines.push(text);
            }
            if cut {
                break;
            }
            continue;
        }
        // A row followed by a rule row with a cell for each of its own starts a table, which
        // runs to the first line that isn't a row.
        if let Some(header) = table_cells(trimmed)
            && let Some(align) = lines.peek().and_then(|next| rule_row(next, header.len()))
        {
            lines.next();
            left = left.saturating_sub(chars(&header));
            let mut rows = Vec::new();
            let mut cut = false;
            while let Some(row) = lines.next_if(|l| l.trim_start().starts_with('|')) {
                let mut cells = table_cells(row).unwrap_or_default();
                cells.resize(header.len(), String::new());
                if chars(&cells) > left {
                    cut = true;
                    break;
                }
                left -= chars(&cells);
                rows.push(cells);
            }
            out.push(Block::Table {
                header,
                align,
                rows,
            });
            if cut {
                mark_cut(&mut out);
                break;
            }
            continue;
        }
        let (mut block, indent) = classify(line);
        // A quote's blank line, a lone >, has nothing to draw.
        if matches!(&block, Block::Text(t) | Block::Heading(t) if t.is_empty()) {
            continue;
        }
        match &mut block {
            Block::Item { depth, .. } => {
                while indents.last().is_some_and(|&i| i >= indent) {
                    indents.pop();
                }
                *depth = indents.len().min(MAX_DEPTH);
                indents.push(indent);
            }
            // A paragraph indented under an item continues it, and one at the margin ends the
            // list.
            _ if indent == 0 => indents.clear(),
            _ => {}
        }
        let text = match &mut block {
            Block::Text(t) | Block::Heading(t) | Block::Item { text: t, .. } => t,
            Block::Code(_) | Block::Table { .. } => unreachable!("classify makes only text blocks"),
        };
        let (cut_text, cut) = cut_to(text, left);
        left = left.saturating_sub(cut_text.chars().count());
        *text = cut_text;
        out.push(block);
        if cut {
            break;
        }
    }
    out.retain(|b| !matches!(b, Block::Code(lines) if lines.is_empty()));
    out
}

/// One line as a tile's summary line shows it, without the marks the hover card drops from the
/// block the line starts: a heading's #s and a quote's >, and a list item's marker as the card
/// draws it, such as "•" for "- " (user, 2026-11-25).
pub fn summary_line(line: &str) -> String {
    match classify(line).0 {
        Block::Heading(text) | Block::Text(text) => text,
        Block::Item { marker, text, .. } => format!("{marker} {text}"),
        Block::Code(_) | Block::Table { .. } => line.to_string(),
    }
}

// What kind of block one line outside a code block is, past the >s of any quote around it, and
// how far its text is indented inside the quote. An item's depth is 0 here, since it depends on
// the items above it, which `blocks` keeps.
fn classify(line: &str) -> (Block, usize) {
    let line = unquote(line);
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    if let Some(text) = heading(trimmed) {
        return (Block::Heading(text.to_string()), indent);
    }
    let item = |marker: &str, text: &str| Block::Item {
        marker: marker.to_string(),
        depth: 0,
        text: text.to_string(),
    };
    let bullet = ["- ", "* ", "+ "]
        .iter()
        .find_map(|p| trimmed.strip_prefix(p));
    if let Some(text) = bullet {
        let (marker, text) = task_box(text).unwrap_or(("•", text));
        return (item(marker, text), indent);
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0
        && let Some(text) = trimmed[digits..]
            .strip_prefix(". ")
            .or_else(|| trimmed[digits..].strip_prefix(") "))
    {
        return (item(&format!("{}.", &trimmed[..digits]), text), indent);
    }
    (Block::Text(trimmed.to_string()), indent)
}

// A line without the >s that quote it, such as "- item" for "> > - item".
fn unquote(line: &str) -> &str {
    let mut rest = line;
    while let Some(after) = rest.trim_start().strip_prefix('>') {
        rest = after.strip_prefix(' ').unwrap_or(after);
    }
    rest
}

// A heading's text, such as "Done" for "## Done ##". One to six #s and a space open a heading,
// so a line that starts with an issue number, such as "#503 merged", is no heading. A run of #s
// after a space closes one, and a # after a word, as in "C#", is the word's.
fn heading(line: &str) -> Option<&str> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    let rest = &line[hashes..];
    if !(1..=6).contains(&hashes) || !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
        return None;
    }
    let text = rest.trim();
    let open = text.trim_end_matches('#');
    Some(if open.is_empty() || open.ends_with(char::is_whitespace) {
        open.trim_end()
    } else {
        text
    })
}

// A task item's box and the text after it: "▢" for "[ ] text" and "✓" for "[x] text". Inter,
// the rack's font, carries both glyphs and no ballot box, ☐.
fn task_box(text: &str) -> Option<(&'static str, &str)> {
    if let Some(rest) = text.strip_prefix("[ ] ") {
        return Some(("▢", rest));
    }
    text.strip_prefix("[x] ")
        .or_else(|| text.strip_prefix("[X] "))
        .map(|rest| ("✓", rest))
}

// A table's rule row with `columns` cells, such as |---|:--:|, as where each column's cells sit.
fn rule_row(line: &str, columns: usize) -> Option<Vec<Align>> {
    let cells = table_cells(line)?;
    if cells.len() != columns {
        return None;
    }
    cells
        .iter()
        .map(|c| {
            let dashes = c.trim_start_matches(':').trim_end_matches(':');
            if dashes.is_empty() || !dashes.chars().all(|ch| ch == '-') {
                return None;
            }
            Some(match (c.starts_with(':'), c.ends_with(':')) {
                (true, true) => Align::Center,
                (false, true) => Align::Right,
                _ => Align::Left,
            })
        })
        .collect()
}

fn chars(cells: &[String]) -> usize {
    cells.iter().map(|c| c.chars().count()).sum()
}

fn cut_to(text: &str, max: usize) -> (String, bool) {
    if text.chars().count() <= max {
        return (text.to_string(), false);
    }
    let head: String = text.chars().take(max).collect();
    let head = match head.rfind(' ') {
        Some(i) if i > 0 => &head[..i],
        _ => head.as_str(),
    };
    (format!("{}…", head.trim_end()), true)
}

fn mark_cut(out: &mut [Block]) {
    match out.last_mut() {
        Some(Block::Text(text) | Block::Heading(text) | Block::Item { text, .. }) => text.push('…'),
        Some(Block::Code(lines)) => lines.push("…".to_string()),
        Some(Block::Table { header, rows, .. }) => {
            let mut row = vec![String::new(); header.len()];
            row[0] = "…".to_string();
            rows.push(row);
        }
        None => {}
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Code,
    Bold,
    /// Emphasis, `*text*` or `_text_`.
    Italic,
    /// A markdown link, `[text](target)`, or an image, `![text](target)`, of which only the text
    /// shows.
    Link,
}

/// The characters a backslash before them keeps from being read as a mark, such as the * in
/// `\*`. A backslash before anything else stays, so a path such as C:\Users\ keeps its own.
const ESCAPED: [char; 11] = ['`', '*', '_', '[', ']', '<', '>', '|', '#', '~', '!'];

/// The line without its marks, and where each marked span sits in it, in bytes.
/// Bold, italics, and a link's text can hold marks of their own, as in **`Dialog`:**, so spans
/// can nest, and a span inside another comes before it.
/// A mark with no partner, or with nothing between the pair, stays in the text as it is.
pub fn parse(line: &str) -> (String, Vec<(Range<usize>, Mark)>) {
    let mut text = String::with_capacity(line.len());
    let mut spans = Vec::new();
    let mut i = 0;
    while let Some(c) = line[i..].chars().next() {
        let rest = &line[i..];
        if c == '\\'
            && let Some(next) = rest[1..].chars().next().filter(|n| ESCAPED.contains(n))
        {
            text.push(next);
            i += 1 + next.len_utf8();
            continue;
        }
        if c == '`' {
            let n = run(rest, '`');
            match code_end(&rest[n..], n) {
                Some(end) => {
                    let start = text.len();
                    text.push_str(trim_code(&rest[n..n + end]));
                    spans.push((start..text.len(), Mark::Code));
                    i += n + end + n;
                }
                None => {
                    text.push_str(&rest[..n]);
                    i += n;
                }
            }
            continue;
        }
        if let Some((label, len)) = link(rest) {
            let r = push_parsed(&mut text, &mut spans, label);
            spans.push((r, Mark::Link));
            i += len;
            continue;
        }
        if let Some(len) = line_break(rest) {
            text.push(' ');
            i += len;
            continue;
        }
        if c == '*' || c == '_' {
            let before = line[..i].chars().next_back();
            if let Some((marks, inner, len)) = emphasis(rest, before) {
                let r = push_parsed(&mut text, &mut spans, inner);
                spans.extend(marks.iter().map(|&m| (r.clone(), m)));
                i += len;
            } else {
                let n = run(rest, c);
                text.push_str(&rest[..n]);
                i += n;
            }
            continue;
        }
        // Text runs to the next character that could start a mark, and a character that
        // started none, such as a lone [, is text.
        let first = c.len_utf8();
        let next = rest[first..]
            .find(['\\', '`', '[', '!', '<', '*', '_'])
            .map_or(rest.len(), |j| j + first);
        text.push_str(&rest[..next]);
        i += next;
    }
    (text, spans)
}

// Parses `inner` onto the end of `text`, with its spans moved to where it lands, and returns
// where it landed.
fn push_parsed(
    text: &mut String,
    spans: &mut Vec<(Range<usize>, Mark)>,
    inner: &str,
) -> Range<usize> {
    let start = text.len();
    let (inner_text, inner_spans) = parse(inner);
    text.push_str(&inner_text);
    spans.extend(
        inner_spans
            .into_iter()
            .map(|(r, mark)| (r.start + start..r.end + start, mark)),
    );
    start..text.len()
}

// How many `c`s start `s`, in bytes, since each mark is one byte.
fn run(s: &str, c: char) -> usize {
    s.len() - s.trim_start_matches(c).len()
}

// Where the code span opened by `n` backticks ends in `after`, the text past them: at the next
// run of exactly `n`, so ``a ` b`` holds a backtick (documented,
// https://spec.commonmark.org/0.31.2/#code-spans).
fn code_end(after: &str, n: usize) -> Option<usize> {
    let mut j = 0;
    while let Some(k) = after[j..].find('`') {
        let at = j + k;
        let len = run(&after[at..], '`');
        if len == n {
            return Some(at);
        }
        j = at + len;
    }
    None
}

// A code span's text without the one space each side that lets it start or end with a
// backtick, as in `` `x` ``. A span of only spaces keeps them.
fn trim_code(code: &str) -> &str {
    match code.strip_prefix(' ').and_then(|c| c.strip_suffix(' ')) {
        Some(inner) if !code.trim().is_empty() => inner,
        _ => code,
    }
}

// Emphasis at the start of `rest`, whose character before is `before`: its marks, its text, and
// how long it runs with its marks, in bytes. One * or _ is italic, two *s bold, and three both.
// It opens only before a character that isn't a space, so "2 * 3" stays as it is, and a _ inside
// a word opens or closes nothing, so tile_pulls keeps its underscore (documented,
// https://spec.commonmark.org/0.31.2/#emphasis-and-strong-emphasis). Claude never writes __bold__
// in our messages, and Python's __init__ would read as it.
fn emphasis(rest: &str, before: Option<char>) -> Option<(&'static [Mark], &str, usize)> {
    let c = rest.chars().next()?;
    let n = run(rest, c);
    let marks: &'static [Mark] = match (c, n) {
        ('*' | '_', 1) => &[Mark::Italic],
        ('*', 2) => &[Mark::Bold],
        ('*', 3) => &[Mark::Bold, Mark::Italic],
        _ => return None,
    };
    let after = &rest[n..];
    if after.chars().next().is_none_or(char::is_whitespace)
        || (c == '_' && before.is_some_and(char::is_alphanumeric))
    {
        return None;
    }
    let end = closer(after, c, n)?;
    Some((marks, &after[..end], n + end + n))
}

// Where the run of `n` `c`s that closes emphasis sits in `s`: the first run of exactly `n` after a
// character other than a space, outside code spans, so the ** of bold inside *italics* stays
// bold. Without one, the last `n` of a longer run close it, and the run's first ones close the
// emphasis inside it, as the *** in **everything vs *exceptions*** closes both.
fn closer(s: &str, c: char, n: usize) -> Option<usize> {
    closing_run(s, c, n, |k| k == n).or_else(|| closing_run(s, c, n, |k| k > n))
}

// Where the first run of `c`s whose length `fits` closes emphasis of `n` in `s`, at its last `n`.
fn closing_run(s: &str, c: char, n: usize, fits: impl Fn(usize) -> bool) -> Option<usize> {
    let mut j = 0;
    while let Some(ch) = s[j..].chars().next() {
        let rest = &s[j..];
        if ch == '`' {
            let k = run(rest, '`');
            j += code_end(&rest[k..], k).map_or(k, |end| k + end + k);
            continue;
        }
        if ch == '\\' {
            j += 1 + rest[1..].chars().next().map_or(0, char::len_utf8);
            continue;
        }
        if ch == c {
            let k = run(rest, c);
            let before = s[..j].chars().next_back();
            let after = rest[k..].chars().next();
            if fits(k)
                && before.is_some_and(|b| !b.is_whitespace())
                && (c == '*' || !after.is_some_and(char::is_alphanumeric))
            {
                return Some(j + k - n);
            }
            j += k;
            continue;
        }
        j += ch.len_utf8();
    }
    None
}

// A link, `[text](target)`, or an image, `![text](target)`, at the start of `rest`: its text,
// and how long it runs, in bytes. A list's "[ ]" checkbox, or brackets with a space before the
// parenthesis, start none.
fn link(rest: &str) -> Option<(&str, usize)> {
    let inside = rest.strip_prefix('!').unwrap_or(rest).strip_prefix('[')?;
    let close = inside.find(']')?;
    let label = &inside[..close];
    let target = inside[close + 1..].strip_prefix('(')?;
    let end = target.find(')')?;
    let after = target.len() - end - 1;
    (!label.trim().is_empty()).then_some((label, rest.len() - after))
}

// An HTML line break, <br>, <br/>, or <br />, which Claude writes to break a line inside a
// table's cell, and how long it runs. A tile's line and a table's cell hold one line, so it
// becomes a space.
fn line_break(rest: &str) -> Option<usize> {
    ["<br>", "<br/>", "<br />"]
        .iter()
        .find(|b| {
            rest.get(..b.len())
                .is_some_and(|h| h.eq_ignore_ascii_case(b))
        })
        .map(|b| b.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_and_bold_lose_their_marks() {
        let (text, spans) = parse("`Traffic` now writes **only** what changed");
        assert_eq!(text, "Traffic now writes only what changed");
        assert_eq!(spans, vec![(0..7, Mark::Code), (19..23, Mark::Bold)]);
    }

    #[test]
    fn an_unpaired_or_empty_mark_stays() {
        assert_eq!(parse("a ` b").0, "a ` b");
        assert_eq!(parse("2 * 3 ** 4").0, "2 * 3 ** 4");
        assert_eq!(parse("``").0, "``");
        assert_eq!(parse("**").0, "**");
        assert_eq!(parse("*** admits it ***").0, "*** admits it ***");
        assert!(parse("a ` b").1.is_empty());
    }

    #[test]
    fn bold_keeps_the_marks_inside_it() {
        let (text, spans) = parse("**`Dialog`:** the layout");
        assert_eq!(text, "Dialog: the layout");
        assert_eq!(spans, vec![(0..6, Mark::Code), (0..7, Mark::Bold)]);
        let (text, spans) = parse("**a `**` b**");
        assert_eq!(text, "a ** b", "a ** in code doesn't close the bold");
        assert_eq!(spans, vec![(2..4, Mark::Code), (0..6, Mark::Bold)]);
    }

    #[test]
    fn stars_and_underscores_make_italics() {
        let (text, spans) = parse("*On the tile:* the chip, and ***password***");
        assert_eq!(text, "On the tile: the chip, and password");
        assert_eq!(
            spans,
            vec![
                (0..12, Mark::Italic),
                (27..35, Mark::Bold),
                (27..35, Mark::Italic)
            ]
        );
        let (text, spans) = parse("*italic **bold** more*");
        assert_eq!(text, "italic bold more");
        assert_eq!(spans, vec![(7..11, Mark::Bold), (0..16, Mark::Italic)]);
        let (text, spans) = parse("it's **everything vs *exceptions***.");
        assert_eq!(text, "it's everything vs exceptions.");
        assert_eq!(spans, vec![(19..29, Mark::Italic), (5..29, Mark::Bold)]);
        assert_eq!(
            parse("*a **b** c*").1,
            vec![(2..3, Mark::Bold), (0..5, Mark::Italic)]
        );
        assert_eq!(parse("_Permanent password_").1, vec![(0..18, Mark::Italic)]);
        assert_eq!(parse("tile_pulls and on_tile").0, "tile_pulls and on_tile");
        assert_eq!(parse("src/*.rs and *.md").0, "src/*.rs and *.md");
        assert_eq!(
            parse("a *b * c").0,
            "a *b * c",
            "a * after a space closes nothing"
        );
    }

    #[test]
    fn a_code_span_runs_to_a_run_as_long_as_its_own() {
        let (text, spans) = parse("`` ` `` and ``a`b``");
        assert_eq!(text, "` and a`b");
        assert_eq!(spans, vec![(0..1, Mark::Code), (6..9, Mark::Code)]);
        assert_eq!(parse("` `").0, " ", "a span of spaces keeps them");
    }

    #[test]
    fn a_backslash_keeps_a_mark_as_text() {
        assert_eq!(parse(r"\*not italic\* and \`x\`").0, "*not italic* and `x`");
        assert_eq!(parse(r"C:\Users\x\.plates").0, r"C:\Users\x\.plates");
        assert!(parse(r"\*a\*").1.is_empty());
    }

    #[test]
    fn a_link_or_image_shows_only_its_text_and_keeps_the_marks_inside_it() {
        let (text, spans) =
            parse("See [`card.rs`](src/plates/src/card.rs) and [the docs](https://x.y/a_b).");
        assert_eq!(text, "See card.rs and the docs.");
        assert_eq!(
            spans,
            vec![
                (4..11, Mark::Code),
                (4..11, Mark::Link),
                (16..24, Mark::Link)
            ]
        );
        assert_eq!(parse("- [ ] task").0, "- [ ] task");
        assert_eq!(parse("[a] (b)").0, "[a] (b)");
        assert_eq!(parse("[a](b").0, "[a](b");
        assert_eq!(
            parse("![Logo cloud](cloud.png) wraps").0,
            "Logo cloud wraps"
        );
        assert_eq!(parse("Wow! Done").0, "Wow! Done");
    }

    #[test]
    fn a_line_break_becomes_a_space() {
        assert_eq!(parse("one<br>two<BR />three").0, "one two three");
        assert_eq!(parse("a <b> c").0, "a <b> c");
    }

    #[test]
    fn a_message_becomes_blocks_without_its_markdown_scaffolding() {
        let message = "## Done\n\nThe card opens after 400ms.\n\n- grows in\n  * closes on leave\n2. Then **this**\n\n| a | b |\n|---|:-:|\n\n```rust\nlet x = 1;\n```\n> quoted";
        let item = |marker: &str, depth: usize, text: &str| Block::Item {
            marker: marker.into(),
            depth,
            text: text.into(),
        };
        assert_eq!(
            blocks(message, 500),
            vec![
                Block::Heading("Done".into()),
                Block::Text("The card opens after 400ms.".into()),
                item("•", 0, "grows in"),
                item("•", 1, "closes on leave"),
                item("2.", 0, "Then **this**"),
                Block::Table {
                    header: vec!["a".into(), "b".into()],
                    align: vec![Align::Left, Align::Center],
                    rows: vec![],
                },
                Block::Code(vec!["let x = 1;".into()]),
                Block::Text("quoted".into()),
            ]
        );
        assert_eq!(
            blocks("2026 was a year.", 500),
            vec![Block::Text("2026 was a year.".into())],
            "digits alone don't make a numbered item"
        );
    }

    #[test]
    fn an_item_nests_under_the_item_above_it_however_far_it_is_indented() {
        let message = "1. First\n    - four spaces in\n        - eight in\n2. Second\n   - three in\nAfter.\n  - two in, under no item";
        let item = |marker: &str, depth: usize, text: &str| Block::Item {
            marker: marker.into(),
            depth,
            text: text.into(),
        };
        assert_eq!(
            blocks(message, 500),
            vec![
                item("1.", 0, "First"),
                item("•", 1, "four spaces in"),
                item("•", 2, "eight in"),
                item("2.", 0, "Second"),
                item("•", 1, "three in"),
                Block::Text("After.".into()),
                item("•", 0, "two in, under no item"),
            ]
        );
    }

    #[test]
    fn a_quote_keeps_the_blocks_inside_it_and_a_task_its_box() {
        let item = |marker: &str, depth: usize, text: &str| Block::Item {
            marker: marker.into(),
            depth,
            text: text.into(),
        };
        assert_eq!(
            blocks("> 1. Per-year impact\n>- Draw them\n> > ## Deep", 500),
            vec![
                item("1.", 0, "Per-year impact"),
                item("•", 0, "Draw them"),
                Block::Heading("Deep".into()),
            ]
        );
        assert_eq!(
            blocks(
                "- [ ] Load with funds\n- [x] Create the first\n- [link](x)",
                500
            ),
            vec![
                item("▢", 0, "Load with funds"),
                item("✓", 0, "Create the first"),
                item("•", 0, "[link](x)"),
            ]
        );
    }

    #[test]
    fn a_fence_holds_a_shorter_fence() {
        let message = "````markdown\n```rust\nlet x = 1;\n```\n````\nAfter.";
        assert_eq!(
            blocks(message, 500),
            vec![
                Block::Code(vec!["```rust".into(), "let x = 1;".into(), "```".into()]),
                Block::Text("After.".into()),
            ]
        );
        assert_eq!(
            blocks("~~~\na\n~~~", 500),
            vec![Block::Code(vec!["a".into()])]
        );
    }

    #[test]
    fn a_summary_line_drops_its_blocks_marks_as_the_card_does() {
        assert_eq!(
            summary_line("## The sync engine in five parts"),
            "The sync engine in five parts"
        );
        assert_eq!(summary_line("> Quoted"), "Quoted");
        assert_eq!(summary_line("- First **bold**"), "• First **bold**");
        assert_eq!(summary_line("2. Second"), "2. Second");
        assert_eq!(summary_line("- [x] Shipped"), "✓ Shipped");
        assert_eq!(summary_line("Plain text."), "Plain text.");
        assert_eq!(
            summary_line("#503 is on `main`"),
            "#503 is on `main`",
            "an issue number isn't a heading"
        );
        assert_eq!(summary_line("### Done ###"), "Done");
        assert_eq!(summary_line("## In C#"), "In C#");
    }

    #[test]
    fn a_table_keeps_its_rows_and_where_its_columns_sit() {
        let message = "| Width | Count | Note |\n|---|--:|:-:|\n| 1024px | 7 | `a \\| b` |\n| 768px |\nAfter.";
        let cells = |c: &[&str]| c.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            blocks(message, 500),
            vec![
                Block::Table {
                    header: cells(&["Width", "Count", "Note"]),
                    align: vec![Align::Left, Align::Right, Align::Center],
                    rows: vec![
                        cells(&["1024px", "7", "`a | b`"]),
                        cells(&["768px", "", ""])
                    ],
                },
                Block::Text("After.".into()),
            ],
            "a short row takes empty cells"
        );
        assert_eq!(
            blocks("| a | b |\nnot a rule row", 500),
            vec![
                Block::Text("| a | b |".into()),
                Block::Text("not a rule row".into())
            ],
            "a row with no rule row under it isn't a table"
        );
        assert_eq!(
            blocks("| a | b |\n|---|", 500),
            vec![Block::Text("| a | b |".into())],
            "a rule row with too few cells isn't the table's"
        );
    }

    #[test]
    fn a_table_is_cut_after_its_last_whole_row() {
        let message = "| a | b |\n|---|---|\n| one | two |\n| three | four |";
        assert_eq!(
            blocks(message, 12),
            vec![Block::Table {
                header: vec!["a".into(), "b".into()],
                align: vec![Align::Left, Align::Left],
                rows: vec![
                    vec!["one".into(), "two".into()],
                    vec!["…".into(), String::new()]
                ],
            }]
        );
    }

    #[test]
    fn a_long_message_is_cut_at_a_space() {
        assert_eq!(
            blocks("one two three four", 9),
            vec![Block::Text("one two…".into())]
        );
        assert_eq!(
            blocks("First.\n\nSecond.", 6),
            vec![Block::Text("First.…".into())],
            "a cut between blocks marks the last one"
        );
        assert_eq!(blocks("Short.", 100), vec![Block::Text("Short.".into())]);
    }

    #[test]
    fn a_line_with_other_scripts_keeps_its_characters() {
        let (text, spans) = parse("Çalışıyor: `ğ` ✓ *é*");
        assert_eq!(text, "Çalışıyor: ğ ✓ é");
        assert_eq!(&text[spans[0].0.clone()], "ğ");
        assert_eq!(&text[spans[1].0.clone()], "é");
    }
}
