//! The markdown Claude writes in a line a tile shows, such as `code` and **bold**, so the tile can
//! style each span rather than show the marks around it, and the blocks of a whole message, for
//! the hover card.

use std::ops::Range;

use plates_core::session::table_cells;

/// One block of a message as the hover card draws it, with its inline marks still in its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// A paragraph, or a quote without its >.
    Text(String),
    /// A heading without its #s.
    Heading(String),
    /// A list item: its marker, "•" for a - or * item or the number for a numbered one, such as
    /// "2.", how many levels its indent nests it, and its text.
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

/// The deepest a list item nests, counted from 0 for an item with no indent, so the card draws
/// items at three indents, and an item indented further sits at the last.
const MAX_DEPTH: usize = 2;

/// A message's blocks, up to about `budget` characters, cut at a space with "…" after it, or
/// after a table's last whole row. Blank lines and horizontal rules go.
pub fn blocks(message: &str, budget: usize) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut left = budget;
    let mut fenced = false;
    let mut lines = message.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            if fenced {
                out.push(Block::Code(Vec::new()));
            }
            continue;
        }
        if !fenced && (trimmed.is_empty() || is_rule(trimmed)) {
            continue;
        }
        if left == 0 {
            mark_cut(&mut out);
            break;
        }
        if fenced {
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
        let mut block = classify(line);
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
    match classify(line) {
        Block::Heading(text) | Block::Text(text) => text,
        Block::Item { marker, text, .. } => format!("{marker} {text}"),
        Block::Code(_) | Block::Table { .. } => line.to_string(),
    }
}

// What kind of block one line outside a code block is. A list item nests a level for every two
// spaces of indent, as Claude indents a nested list.
fn classify(line: &str) -> Block {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return Block::Heading(trimmed.trim_start_matches('#').trim_start().to_string());
    }
    let depth = (indent / 2).min(MAX_DEPTH);
    let bullet = ["- ", "* ", "+ "]
        .iter()
        .find_map(|p| trimmed.strip_prefix(p));
    if let Some(text) = bullet {
        return Block::Item {
            marker: "•".to_string(),
            depth,
            text: text.to_string(),
        };
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0
        && let Some(text) = trimmed[digits..]
            .strip_prefix(". ")
            .or_else(|| trimmed[digits..].strip_prefix(") "))
    {
        return Block::Item {
            marker: format!("{}.", &trimmed[..digits]),
            depth,
            text: text.to_string(),
        };
    }
    Block::Text(trimmed.strip_prefix("> ").unwrap_or(trimmed).to_string())
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

// A horizontal rule, such as ---, or a table's rule row with no row above it.
fn is_rule(line: &str) -> bool {
    line.contains(['-', '*'])
        && line
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | '*' | ' '))
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
    /// A markdown link, `[text](target)`, of which only the text shows.
    Link,
}

/// The line without its marks, and where each marked span sits in it, in bytes.
/// A link's text can hold marks of its own, as in [`card.rs`](src/card.rs), so spans can nest.
/// A mark with no partner, or with nothing between the pair, stays in the text as it is.
pub fn parse(line: &str) -> (String, Vec<(Range<usize>, Mark)>) {
    let mut text = String::with_capacity(line.len());
    let mut spans = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        if let Some((label, after)) = link(rest) {
            let start = text.len();
            let (inner, inner_spans) = parse(label);
            text.push_str(&inner);
            spans.extend(
                inner_spans
                    .into_iter()
                    .map(|(r, mark)| (r.start + start..r.end + start, mark)),
            );
            spans.push((start..text.len(), Mark::Link));
            rest = after;
            continue;
        }
        let (open, mark) = if rest.starts_with("**") {
            ("**", Mark::Bold)
        } else if rest.starts_with('`') {
            ("`", Mark::Code)
        } else {
            // A lone `*`, or a `[` that starts no link, is text, so the copy runs past it to the
            // next mark.
            let next = match rest.find(['`', '*', '[']) {
                Some(0) => 1,
                Some(i) => i,
                None => rest.len(),
            };
            text.push_str(&rest[..next]);
            rest = &rest[next..];
            continue;
        };
        let inner = &rest[open.len()..];
        match inner.find(open) {
            Some(end) if end > 0 => {
                let start = text.len();
                text.push_str(&inner[..end]);
                spans.push((start..text.len(), mark));
                rest = &inner[end + open.len()..];
            }
            _ => {
                text.push_str(open);
                rest = inner;
            }
        }
    }
    (text, spans)
}

// A link, `[text](target)`, at the start of `rest`: its text, and what follows the link. A list's
// "[ ]" checkbox, or brackets with a space before the parenthesis, start none.
fn link(rest: &str) -> Option<(&str, &str)> {
    let inside = rest.strip_prefix('[')?;
    let close = inside.find(']')?;
    let label = &inside[..close];
    let target = inside[close + 1..].strip_prefix('(')?;
    let end = target.find(')')?;
    (!label.trim().is_empty()).then(|| (label, &target[end + 1..]))
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
        assert!(parse("a ` b").1.is_empty());
    }

    #[test]
    fn a_link_shows_only_its_text_and_keeps_the_marks_inside_it() {
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
    fn a_summary_line_drops_its_blocks_marks_as_the_card_does() {
        assert_eq!(
            summary_line("## The sync engine in five parts"),
            "The sync engine in five parts"
        );
        assert_eq!(summary_line("> Quoted"), "Quoted");
        assert_eq!(summary_line("- First **bold**"), "• First **bold**");
        assert_eq!(summary_line("2. Second"), "2. Second");
        assert_eq!(summary_line("Plain text."), "Plain text.");
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
        let (text, spans) = parse("Çalışıyor: `ğ` ✓");
        assert_eq!(text, "Çalışıyor: ğ ✓");
        assert_eq!(&text[spans[0].0.clone()], "ğ");
    }
}
