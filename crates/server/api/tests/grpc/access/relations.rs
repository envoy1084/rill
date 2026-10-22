//! The relation engine as clients see it: closed repositories, org
//! administrators, explanations, and versions every server honours.

use connectrpc::client::CallOptions;
use rill_types::identity::OrgRole;
use rill_types::Principal;

use super::super::*;
use super::fixture::*;

fn tuple(path: &str, relation: &str, subject: &str) -> Option<pb::Relation> {
    Some(pb::Relation {
        repo: "acl".into(),
        path: path.into(),
        relation: relation.into(),
        subject: subject.into(),
    })
}

async fn list_root(c: &mut Clients, repo: &str) -> Result<(), ConnectError> {
    c.repo
        .list_dir(pb::ListDirRequest {
            repo: repo.into(),
            root: None.into(),
            path: String::new(),
        })
        .await
        .map(|_| ())
}

#[tokio::test]
async fn a_repository_without_tuples_is_closed_but_to_org_admins() {
    let server = TestServer::start(ServerConfig::default()).await;
    server.state.stores.create_repo("orphan", "").await.unwrap();
    let olga = add_user(&server, "olga", vec![OrgRole::Admin], &[]).await;
    let mut stranger = clients(&server, Some(&server.token_for(&Principal::human("sam")))).await;
    assert_eq!(
        list_root(&mut stranger, "orphan").await.unwrap_err().code,
        Code::PermissionDenied
    );
    let mut admin = clients(&server, Some(&server.token_for(&olga))).await;
    list_root(&mut admin, "orphan").await.unwrap();
    let check = admin
        .access
        .check(pb::CheckRequest {
            repo: "orphan".into(),
            path: String::new(),
            relation: "owner".into(),
            subject: None,
        })
        .await
        .unwrap()
        .into_owned();
    assert!(check.allowed && check.org_admin && check.because.is_empty());
    server.stop().await;
}

#[tokio::test]
async fn check_expand_and_lookup_explain_access() {
    let acl = Acl::start().await;
    add_user(&acl.server, "carol", vec![], &["core"]).await;
    acl.owner
        .access
        .add_relation(pb::AddRelationRequest {
            operation_id: Bytes::new(),
            relation: (tuple("pub", "writer", "team:core")).into(),
        })
        .await
        .unwrap();
    let check = acl
        .owner
        .access
        .check(pb::CheckRequest {
            repo: "acl".into(),
            path: "pub/deep".into(),
            relation: "reader".into(),
            subject: Some("user:carol".into()),
        })
        .await
        .unwrap()
        .into_owned();
    assert!(check.allowed);
    assert_eq!(
        check.because,
        vec![tuple("pub", "writer", "team:core").unwrap()],
        "a writer is a reader, through the parent directory"
    );

    let expanded = acl
        .owner
        .access
        .expand(pb::ExpandRequest {
            repo: "acl".into(),
            path: "pub".into(),
            relation: "writer".into(),
        })
        .await
        .unwrap()
        .into_owned()
        .tuples;
    let team = expanded
        .iter()
        .find(|t| t.tuple.as_option().unwrap().subject == "team:core")
        .unwrap();
    assert_eq!(team.members.len(), 1);
    assert_eq!(team.members[0].name, "carol");
    assert!(
        expanded
            .iter()
            .any(|t| t.tuple.as_option().unwrap().relation == "owner"),
        "the repository owner writes everywhere"
    );

    let paths = acl
        .owner
        .access
        .lookup_paths(pb::LookupPathsRequest {
            repo: "acl".into(),
            relation: "reader".into(),
            subject: Some("user:carol".into()),
        })
        .await
        .unwrap()
        .into_owned()
        .paths;
    assert_eq!(paths, ["pub"]);

    // A non-owner may ask about itself, not about others.
    let carol = acl.as_principal(&Principal::human("carol")).await;
    let mine = carol
        .access
        .check(pb::CheckRequest {
            repo: "acl".into(),
            path: "secret".into(),
            relation: "reader".into(),
            subject: None,
        })
        .await
        .unwrap()
        .into_owned();
    assert!(!mine.allowed);
    let err = carol
        .access
        .expand(pb::ExpandRequest {
            repo: "acl".into(),
            path: String::new(),
            relation: "reader".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, Code::PermissionDenied);
    acl.server.stop().await;
}

#[tokio::test]
async fn unknown_subjects_and_relations_are_invalid() {
    let acl = Acl::start().await;
    for (relation, subject) in [
        ("reader", "user:nobody"),
        ("contributor", "*"),
        ("reader", "team:a/b"),
    ] {
        let err = acl
            .owner
            .access
            .add_relation(pb::AddRelationRequest {
                operation_id: Bytes::new(),
                relation: (tuple("", relation, subject)).into(),
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, Code::InvalidArgument, "{relation} {subject}");
    }
    acl.server.stop().await;
}

#[tokio::test]
async fn a_relation_version_from_the_future_is_refused() {
    let acl = Acl::start().await;
    let granted = acl
        .owner
        .access
        .add_relation(pb::AddRelationRequest {
            operation_id: Bytes::new(),
            relation: (tuple("pub", "reader", &format!("principal:{}", bob().id))).into(),
        })
        .await
        .unwrap()
        .into_owned();
    let token = acl.server.token_for(&bob());
    let channel = connect(&acl.server.grpc).await;
    let root = acl.at_version();
    let read = |seen: i64| {
        let root = root.clone();
        let repo = RepoServiceClient::authed(channel.clone(), Bearer(Some(token.clone())));
        let header = format!("{}={seen}", granted.repo_id);
        async move {
            let request = pb::ListDirRequest {
                repo: "acl".into(),
                root: root.into(),
                path: "pub".into(),
            };
            let options = CallOptions::default().with_header("x-rill-relations", header);
            repo.list_dir_with_options(request, options)
                .await
                .map(|_| ())
        }
    };
    read(granted.relation_version).await.unwrap();
    assert_eq!(
        read(granted.relation_version + 1000)
            .await
            .unwrap_err()
            .code,
        Code::InvalidArgument
    );
    acl.server.stop().await;
}
