//! Parsing principals given on the command line.

use crate::error::{Error, Result};

/// Parse `kind:name` or `name`.
pub fn parse_principal(spec: &str) -> Result<rill_types::Principal> {
    use rill_types::Principal;
    match spec.split_once(':') {
        None => Ok(Principal::human(spec)),
        Some(("human", name)) => Ok(Principal::human(name)),
        Some(("agent", name)) => Ok(Principal::agent(name)),
        Some(("service", name)) => Ok(Principal::service(name)),
        Some((kind, _)) => Err(Error::Usage(format!(
            "principal kind must be human, agent or service, not {kind:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principals_parse_by_kind() {
        assert_eq!(
            parse_principal("alice").unwrap(),
            rill_types::Principal::human("alice")
        );
        assert_eq!(
            parse_principal("service:load").unwrap(),
            rill_types::Principal::service("load")
        );
        assert!(parse_principal("robot:x").is_err());
    }
}
