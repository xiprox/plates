//! The markdown Claude writes in a line a tile shows, such as `code` and **bold**, so the tile can
//! style each span rather than show the marks around it.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Code,
    Bold,
}

/// The line without its marks, and where each marked span sits in it, in bytes.
/// A mark with no partner, or with nothing between the pair, stays in the text as it is.
pub fn parse(line: &str) -> (String, Vec<(Range<usize>, Mark)>) {
    let mut text = String::with_capacity(line.len());
    let mut spans = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        let (open, mark) = if rest.starts_with("**") {
            ("**", Mark::Bold)
        } else if rest.starts_with('`') {
            ("`", Mark::Code)
        } else {
            // A lone `*` is text, so the copy runs past it to the next mark.
            let next = match rest.find(['`', '*']) {
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
    fn a_line_with_other_scripts_keeps_its_characters() {
        let (text, spans) = parse("Çalışıyor: `ğ` ✓");
        assert_eq!(text, "Çalışıyor: ğ ✓");
        assert_eq!(&text[spans[0].0.clone()], "ğ");
    }
}
