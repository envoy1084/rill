use proptest::prelude::*;

/// File content made of lines from a small vocabulary, so independent
/// edits collide often. Covers CRLF endings, a missing final newline and
/// non-UTF-8 bytes.
pub fn text() -> impl Strategy<Value = Vec<u8>> {
    (
        prop::collection::vec(line(), 0..12),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(lines, crlf, trailing)| join(&lines, crlf, trailing))
}

/// `base` with a few random line insertions, deletions and replacements.
pub fn edited(base: Vec<u8>) -> impl Strategy<Value = Vec<u8>> {
    let lines: Vec<Vec<u8>> = base
        .split_inclusive(|&b| b == b'\n')
        .map(<[u8]>::to_vec)
        .collect();
    let n = lines.len();
    prop::collection::vec((0..=n, 0u8..3, line()), 0..4).prop_map(move |edits| {
        let mut out = lines.clone();
        for (at, op, new) in edits {
            let at = at.min(out.len());
            let mut new = new;
            new.push(b'\n');
            match op {
                0 => out.insert(at, new),
                1 if at < out.len() => {
                    out.remove(at);
                }
                _ if at < out.len() => out[at] = new,
                _ => out.push(new),
            }
        }
        out.concat()
    })
}

fn line() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        4 => prop::sample::select(vec!["a", "b", "c", "fn x() {}", "", "}", "  let y = 1;"])
            .prop_map(|s| s.as_bytes().to_vec()),
        1 => prop::collection::vec(any::<u8>().prop_filter("no newline", |b| *b != b'\n'), 1..6),
    ]
}

fn join(lines: &[Vec<u8>], crlf: bool, trailing: bool) -> Vec<u8> {
    let eol: &[u8] = if crlf { b"\r\n" } else { b"\n" };
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        out.extend_from_slice(line);
        if i + 1 < lines.len() || trailing {
            out.extend_from_slice(eol);
        }
    }
    out
}
