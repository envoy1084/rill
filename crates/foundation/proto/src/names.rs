//! Resource names (`orgs/acme/repos/mono/changes/ch_…`): parsing what a
//! request names and formatting what a response returns.
//!
//! A pattern is a list of segments: a literal (`"repos"`), a variable
//! (`"{repo}"`), or a trailing `"{rest=**}"` that takes the remainder,
//! slashes included (ref names). Parsing answers the variables in order.

use thiserror::Error;

/// A name that does not match the pattern it should.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{name:?} is not a {kind} name ({pattern})")]
pub struct NameError {
    pub name: String,
    pub kind: &'static str,
    pub pattern: &'static str,
}

/// One kind of resource name.
#[derive(Debug, Clone, Copy)]
pub struct Pattern {
    /// What the resource is, for errors (`"repository"`).
    pub kind: &'static str,
    /// The segments, as written in the API docs.
    pub text: &'static str,
}

macro_rules! patterns {
    ($($const:ident = $kind:literal, $text:literal;)*) => {
        $(
            #[doc = concat!("`", $text, "`")]
            pub const $const: Pattern = Pattern { kind: $kind, text: $text };
        )*
    };
}

patterns! {
    ORG = "organization", "orgs/{org}";
    REPO = "repository", "orgs/{org}/repos/{repo}";
    REF = "ref", "orgs/{org}/repos/{repo}/refs/{ref=**}";
    CHANGE = "change", "orgs/{org}/repos/{repo}/changes/{change}";
    VERSION = "version", "orgs/{org}/repos/{repo}/changes/{change}/versions/{version}";
    REVIEW = "review", "orgs/{org}/repos/{repo}/changes/{change}/review";
    THREAD = "thread", "orgs/{org}/repos/{repo}/changes/{change}/review/threads/{thread}";
    WORKSPACE = "workspace", "orgs/{org}/repos/{repo}/workspaces/{workspace}";
    WORKSPACE_OPERATION = "workspace operation", "orgs/{org}/repos/{repo}/workspaces/{workspace}/operations/{operation}";
    LAND_REQUEST = "land request", "orgs/{org}/repos/{repo}/landRequests/{ticket}";
    TRUNK_POSITION = "trunk position", "orgs/{org}/repos/{repo}/trunk/positions/{position}";
    INCIDENT = "incident", "orgs/{org}/repos/{repo}/incidents/{incident}";
    RUN = "run", "orgs/{org}/repos/{repo}/runs/{run}";
    QUARANTINE = "quarantine", "orgs/{org}/repos/{repo}/quarantines/{target=**}";
    INTENT = "intent", "orgs/{org}/intents/{intent}";
    AGENT = "agent", "orgs/{org}/agents/{agent}";
    USER = "user", "orgs/{org}/users/{user}";
    IDENTITY_PROVIDER = "identity provider", "orgs/{org}/identityProviders/{provider}";
    WORKLOAD_ISSUER = "workload issuer", "orgs/{org}/workloadIssuers/{issuer}";
    SECRET = "secret", "orgs/{org}/secrets/{secret}";
    SCIM_CREDENTIAL = "SCIM credential", "orgs/{org}/scimCredentials/{credential}";
    REDACTION = "redaction", "orgs/{org}/redactions/{redaction}";
    OPERATION = "operation", "operations/{operation}";
}

impl Pattern {
    /// The variables of `name`, in order; an error naming the pattern if
    /// it does not match or a variable is empty.
    pub fn parse<'a>(&self, name: &'a str) -> Result<Vec<&'a str>, NameError> {
        let err = || NameError {
            name: name.to_string(),
            kind: self.kind,
            pattern: self.text,
        };
        let mut rest = name;
        let mut out = Vec::new();
        let segments: Vec<&str> = self.text.split('/').collect();
        for (i, segment) in segments.iter().enumerate() {
            let last = i + 1 == segments.len();
            if segment.ends_with("=**}") {
                if rest.is_empty() {
                    return Err(err());
                }
                out.push(rest);
                rest = "";
                break;
            }
            let (value, tail) = match rest.split_once('/') {
                Some((value, tail)) if !last => (value, tail),
                None if last => (rest, ""),
                _ => return Err(err()),
            };
            if segment.starts_with('{') {
                if value.is_empty() {
                    return Err(err());
                }
                out.push(value);
            } else if value != *segment {
                return Err(err());
            }
            rest = tail;
        }
        if !rest.is_empty() {
            return Err(err());
        }
        Ok(out)
    }

    /// The name with `values` filled in, in order.
    pub fn format(&self, values: &[&str]) -> String {
        let mut values = values.iter();
        self.text
            .split('/')
            .map(|segment| {
                if segment.starts_with('{') {
                    values.next().copied().unwrap_or_default().to_string()
                } else {
                    segment.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// `orgs/{org}/repos/{repo}` for a repository `org`/`repo`.
pub fn repo(org: &str, repo: &str) -> String {
    REPO.format(&[org, repo])
}

/// The `(org, repo)` a repository-scoped name starts with, whatever comes
/// after (`orgs/o/repos/r/changes/…` → `("o", "r")`).
pub fn repo_of(name: &str) -> Result<(&str, &str), NameError> {
    let err = || NameError {
        name: name.to_string(),
        kind: "repository",
        pattern: REPO.text,
    };
    let mut parts = name.splitn(5, '/');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("orgs"), Some(org), Some("repos"), Some(repo))
            if !org.is_empty() && !repo.is_empty() =>
        {
            Ok((org, repo))
        }
        _ => Err(err()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse_to_their_variables_and_format_back() {
        let name = "orgs/acme/repos/mono/changes/ch_1/versions/ab";
        let parts = VERSION.parse(name).unwrap();
        assert_eq!(parts, ["acme", "mono", "ch_1", "ab"]);
        assert_eq!(VERSION.format(&parts), name);
        assert_eq!(repo_of(name).unwrap(), ("acme", "mono"));
    }

    #[test]
    fn a_trailing_rest_variable_keeps_slashes() {
        let parts = REF.parse("orgs/o/repos/r/refs/releases/1.0").unwrap();
        assert_eq!(parts, ["o", "r", "releases/1.0"]);
        assert!(REF.parse("orgs/o/repos/r/refs/").is_err());
    }

    #[test]
    fn wrong_literals_empty_variables_and_extra_segments_are_refused() {
        for bad in [
            "orgs/acme/projects/mono",
            "orgs//repos/mono",
            "orgs/acme/repos/mono/extra",
            "orgs/acme",
            "",
        ] {
            let err = REPO.parse(bad).unwrap_err();
            assert_eq!(err.kind, "repository", "{bad}");
        }
        assert!(repo_of("orgs/acme/repos/").is_err());
    }
}
