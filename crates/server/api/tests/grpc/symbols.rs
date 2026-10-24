//! Code intelligence: definitions and references from extracted symbols
//! and text, made precise by an uploaded SCIP index; structural search.

use protobuf::Message as _;
use scip::types::{Document, Index, Occurrence, SymbolRole};

use super::*;
use crate::common::names;

const LIB: &str = "pub fn greet() {}\n";
const MAIN: &str = "fn main() { greet(); }\n// greet is mentioned here too\n";
const GREET: &str = "rust-analyzer cargo demo 0.1.0 lib/greet().";

fn scip_index() -> Vec<u8> {
    let doc = |path: &str, occs: &[([i32; 3], bool)]| {
        let mut d = Document::new();
        d.relative_path = path.into();
        for (range, def) in occs {
            let mut o = Occurrence::new();
            o.symbol = GREET.into();
            o.range = range.to_vec();
            o.symbol_roles = if *def {
                SymbolRole::Definition as i32
            } else {
                0
            };
            d.occurrences.push(o);
        }
        d
    };
    let mut index = Index::new();
    index
        .documents
        .push(doc("src/lib.rs", &[([0, 7, 12], true)]));
    index
        .documents
        .push(doc("src/main.rs", &[([0, 12, 17], false)]));
    index.write_to_bytes().unwrap()
}

async fn references(c: &Clients, req: pb::FindReferencesRequest) -> pb::FindReferencesResponse {
    c.content
        .find_references(pb::FindReferencesRequest {
            repo: names::repo("r"),
            ..req
        })
        .await
        .unwrap()
        .into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn definitions_and_references_turn_precise_with_scip() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: "r".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let version = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: names::repo("r"),
            edits: vec![
                set_edit("src/lib.rs", LIB.as_bytes()),
                set_edit("src/main.rs", MAIN.as_bytes()),
            ],
            message: "greet".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap();
    let seq = land(&mut c, "r", version.change_id.clone()).await;
    c.content
        .grep(pb::GrepRequest {
            repo: names::repo("r"),
            pattern: "greet".into(),
            min_seq: Some(seq),
            ..Default::default()
        })
        .await
        .unwrap();

    let defs = c
        .content
        .find_definitions(pb::FindDefinitionsRequest {
            repo: names::repo("r"),
            name: "greet".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .definitions;
    assert_eq!(defs.len(), 1);
    assert_eq!(
        (defs[0].path.as_str(), defs[0].line, defs[0].kind.as_str()),
        ("src/lib.rs", 1, "function")
    );
    assert!(!defs[0].precise);

    let text = references(
        &c,
        pb::FindReferencesRequest {
            name: "greet".into(),
            ..Default::default()
        },
    )
    .await;
    assert!(!text.precise);
    assert_eq!(text.references.len(), 3, "the comment counts as text");
    assert!(text
        .references
        .iter()
        .any(|r| r.definition && r.path == "src/lib.rs"));

    let structural = c
        .content
        .grep(pb::GrepRequest {
            repo: names::repo("r"),
            pattern: "greet()".into(),
            structural: true,
            languages: vec!["rust".into()],
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(structural.hits.len(), 1);
    assert_eq!(
        (structural.hits[0].path.as_str(), structural.hits[0].line_no),
        ("src/main.rs", 1)
    );

    let index = scip_index();
    let put = c
        .content
        .write_blob(tokio_stream::iter(vec![
            pb::WriteBlobRequest {
                msg: Some(pb::write_blob_request::Msg::Header(pb::WriteBlobHeader {
                    repo: names::repo("r"),
                    size: Some(index.len() as u64),
                    ..Default::default()
                })),
            },
            pb::WriteBlobRequest {
                msg: Some(pb::write_blob_request::Msg::Data(Bytes::from(index))),
            },
        ]))
        .await
        .unwrap()
        .into_owned();
    let stored = c
        .content
        .upload_scip(pb::UploadScipRequest {
            repo: names::repo("r"),
            index: put.content,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!((stored.files, stored.occurrences), (2, 2));

    let precise = references(
        &c,
        pb::FindReferencesRequest {
            path: "src/main.rs".into(),
            line: 1,
            column: 14,
            ..Default::default()
        },
    )
    .await;
    assert!(precise.precise);
    assert_eq!(
        precise.references.len(),
        2,
        "the comment is not a reference"
    );
    assert_eq!(precise.references[0].symbol, GREET);
    let defs = c
        .content
        .find_definitions(pb::FindDefinitionsRequest {
            repo: names::repo("r"),
            name: "greet".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .definitions;
    assert_eq!(defs.len(), 1);
    assert!(defs[0].precise);
    assert_eq!(defs[0].start_column, 7);

    let listed = c
        .content
        .list_symbols(pb::ListSymbolsRequest {
            repo: names::repo("r"),
            query: "GRE".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .symbols;
    assert_eq!(listed.len(), 1);
    server.stop().await;
}
