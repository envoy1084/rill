//! Synthetic source files: `d{top}/s{sub}/f{i}.{ext}`, about 250 bytes of
//! Rust, Python, TypeScript or Go each, whose identifiers follow a Zipf
//! distribution over a fixed vocabulary, as identifiers in real code do.
//! The vocabulary spans enough syllables that trigrams discriminate: a
//! first version built words from 16 syllables, so every file held nearly
//! every trigram and the index could not narrow anything.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

/// Distinct identifiers.
pub const VOCABULARY: usize = 50_000;
/// Top-level directories; each holds `SUBDIRS` directories.
pub const TOP_DIRS: usize = 100;
pub const SUBDIRS: usize = 100;

const CONSONANTS: &[u8] = b"bcdfghjklmnprstvwz";
const VOWELS: &[u8] = b"aeiou";
/// Consonant-vowel syllables, and three-syllable words drawn from them.
const SYLLABLES: usize = 90;
const WORDS: usize = SYLLABLES * SYLLABLES * SYLLABLES;

fn syllable(i: usize, out: &mut String) {
    out.push(CONSONANTS[i / VOWELS.len()] as char);
    out.push(VOWELS[i % VOWELS.len()] as char);
}

/// The identifier of rank `rank` (0 is the most frequent). Ranks spread
/// over the word space by a bijection, so neighbouring ranks share no
/// syllables and the corpus has as many distinct trigrams as code does.
pub fn word(rank: usize) -> String {
    let i = (rank * 7_919) % WORDS;
    let mut out = String::with_capacity(8);
    syllable(i / (SYLLABLES * SYLLABLES), &mut out);
    syllable(i / SYLLABLES % SYLLABLES, &mut out);
    out.push('_');
    syllable(i % SYLLABLES, &mut out);
    out
}

fn camel(rank: usize) -> String {
    word(rank)
        .split('_')
        .map(|p| {
            let mut c = p.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

/// Draws identifier ranks with probability proportional to 1 / (rank + 1).
pub struct Zipf {
    cumulative: Vec<f64>,
}

impl Default for Zipf {
    fn default() -> Self {
        let mut total = 0.0;
        let cumulative = (0..VOCABULARY)
            .map(|r| {
                total += 1.0 / (r as f64 + 1.0);
                total
            })
            .collect();
        Zipf { cumulative }
    }
}

impl Zipf {
    pub fn sample(&self, rng: &mut StdRng) -> usize {
        let total = *self.cumulative.last().expect("the vocabulary is not empty");
        let x = rng.random::<f64>() * total;
        self.cumulative
            .partition_point(|&c| c < x)
            .min(VOCABULARY - 1)
    }
}

/// Where file `i` of `files` lives.
pub fn path(i: usize, files: usize) -> String {
    let per_top = files.div_ceil(TOP_DIRS).max(1);
    let per_sub = per_top.div_ceil(SUBDIRS).max(1);
    let (top, sub) = (i / per_top, (i % per_top) / per_sub);
    let ext = ["rs", "py", "ts", "go"][i % 4];
    format!("d{top:02}/s{sub:02}/f{i}.{ext}")
}

/// The content of file `i` (unique: it names its own number), version
/// `rev` of it.
pub fn content(i: usize, rev: u32, zipf: &Zipf) -> String {
    let mut rng = StdRng::seed_from_u64((i as u64) << 16 | rev as u64);
    let mut w = || word(zipf.sample(&mut rng));
    let (a, b, c, d, e, f, g, h) = (w(), w(), w(), w(), w(), w(), w(), w());
    let ty = camel(zipf.sample(&mut StdRng::seed_from_u64(i as u64 ^ 0x5eed)));
    match i % 4 {
        0 => format!(
            "// file {i} rev {rev}\nuse crate::{a}::{b};\n\npub fn {c}({d}: u32) -> {ty} {{\n    let {e} = {f}({d});\n    {g}.{h}({e})\n}}\n"
        ),
        1 => format!(
            "# file {i} rev {rev}\nfrom {a} import {b}\n\n\ndef {c}({d}):\n    {e} = {f}({d})\n    return {g}.{h}({e})\n\n\nclass {ty}:\n    pass\n"
        ),
        2 => format!(
            "// file {i} rev {rev}\nimport {{ {b} }} from './{a}';\n\nexport function {c}({d}: number): {ty} {{\n  const {e} = {f}({d});\n  return {g}.{h}({e});\n}}\n"
        ),
        _ => format!(
            "// file {i} rev {rev}\npackage {a}\n\nimport \"{b}\"\n\nfunc {c}({d} int) {ty} {{\n\t{e} := {f}({d})\n\treturn {g}.{h}({e})\n}}\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_distinct_and_paths_fill_every_directory() {
        let words: std::collections::HashSet<_> = (0..VOCABULARY).map(word).collect();
        assert_eq!(words.len(), VOCABULARY);
        assert_eq!(path(0, 1_000_000), "d00/s00/f0.rs");
        assert!(path(999_999, 1_000_000).starts_with("d99/s99/"));
    }

    #[test]
    fn frequent_words_dominate() {
        let zipf = Zipf::default();
        let mut rng = StdRng::seed_from_u64(1);
        let top = (0..10_000).filter(|_| zipf.sample(&mut rng) < 100).count();
        assert!(top > 3_000, "{top}");
    }
}
