//! Merging segments into one: compaction reads the stored contents back,
//! so no blob is fetched again.

use std::collections::HashSet;

use bytes::Bytes;

use super::reader::Segment;
use super::writer::SegmentBuilder;
use crate::error::Result;

/// One segment holding every distinct blob of `segments`.
pub fn merge_segments(segments: &[&Segment]) -> Result<Bytes> {
    let mut builder = SegmentBuilder::new();
    let mut seen = HashSet::new();
    for seg in segments {
        for (i, doc) in seg.docs().iter().enumerate() {
            if !seen.insert(doc.hash) {
                continue;
            }
            let content = seg.content(i as u32)?;
            let symbols = seg.symbols(i as u32)?;
            builder.add(doc.hash, &content, &symbols)?;
        }
    }
    builder.build()
}

#[cfg(test)]
mod tests {
    use rill_types::{ObjectHash, ObjectKind};

    use super::*;
    use crate::segment::SegmentBytes;
    use crate::symbols::Symbol;
    use crate::symbols::SymbolKind;
    use crate::trigram::{required_trigrams_for_regex, Candidates};

    fn hash(s: &str) -> ObjectHash {
        ObjectHash::digest(ObjectKind::Blob, s.as_bytes())
    }

    fn segment(docs: &[&str]) -> Segment {
        let mut b = SegmentBuilder::new();
        for d in docs {
            let sym = Symbol {
                name: format!("s{}", d.len()),
                kind: SymbolKind::Function,
                line: 1,
            };
            b.add(hash(d), d.as_bytes(), &[sym]).unwrap();
        }
        Segment::open(SegmentBytes::Owned(b.build().unwrap())).unwrap()
    }

    #[test]
    fn a_segment_answers_trigram_queries_and_keeps_content() {
        let seg = segment(&["fn main() {}", "struct Main;", "nothing here"]);
        let q = required_trigrams_for_regex("main", true).unwrap();
        let Candidates::Docs(docs) = seg.candidates(&q).unwrap() else {
            panic!("pruned")
        };
        assert_eq!(
            docs.iter().collect::<Vec<_>>(),
            [0, 1],
            "folded: Main matches"
        );
        assert_eq!(seg.content(1).unwrap(), b"struct Main;");
        assert_eq!(seg.symbols(2).unwrap()[0].name, "s12");
        assert_eq!(seg.doc_of(&hash("nothing here")), Some(2));
        assert!(Segment::open(SegmentBytes::Owned(Bytes::from_static(b"nope"))).is_err());
    }

    #[test]
    fn merging_keeps_every_distinct_blob_once() {
        let a = segment(&["alpha beta", "gamma"]);
        let b = segment(&["gamma", "delta epsilon"]);
        let merged =
            Segment::open(SegmentBytes::Owned(merge_segments(&[&a, &b]).unwrap())).unwrap();
        assert_eq!(merged.docs().len(), 3);
        let q = required_trigrams_for_regex("epsilon", false).unwrap();
        let Candidates::Docs(docs) = merged.candidates(&q).unwrap() else {
            panic!("pruned")
        };
        let doc = docs.iter().next().unwrap();
        assert_eq!(merged.content(doc).unwrap(), b"delta epsilon");
    }
}
