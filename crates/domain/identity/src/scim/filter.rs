//! The filters provisioning systems send: `attribute eq "value"`, joined
//! by `and` (RFC 7644 §3.4.2.2). Other operators are refused.

use super::error::ScimError;

/// Conjoined equality tests; attribute names compare case-insensitively.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Filter {
    pub terms: Vec<(String, String)>,
}

impl Filter {
    pub fn parse(text: &str) -> Result<Self, ScimError> {
        let invalid =
            || ScimError::bad_request("invalidFilter", format!("unsupported filter {text:?}"));
        let mut terms = Vec::new();
        let mut rest = text.trim();
        loop {
            let (attr, after) = rest.split_once(char::is_whitespace).ok_or_else(invalid)?;
            let after = after.trim_start();
            let after = after
                .strip_prefix("eq ")
                .or_else(|| after.strip_prefix("EQ "))
                .ok_or_else(invalid)?
                .trim_start();
            let after = after.strip_prefix('"').ok_or_else(invalid)?;
            let mut value = String::new();
            let mut chars = after.char_indices();
            let end = loop {
                match chars.next() {
                    Some((_, '\\')) => match chars.next() {
                        Some((_, c)) => value.push(c),
                        None => return Err(invalid()),
                    },
                    Some((i, '"')) => break i,
                    Some((_, c)) => value.push(c),
                    None => return Err(invalid()),
                }
            };
            terms.push((attr.to_ascii_lowercase(), value));
            rest = after[end + 1..].trim();
            if rest.is_empty() {
                return Ok(Filter { terms });
            }
            rest = rest
                .strip_prefix("and ")
                .or_else(|| rest.strip_prefix("AND "))
                .ok_or_else(invalid)?
                .trim_start();
        }
    }

    /// The value tested for `attribute`, if the filter tests it.
    pub fn get(&self, attribute: &str) -> Option<&str> {
        self.terms
            .iter()
            .find(|(a, _)| a.eq_ignore_ascii_case(attribute))
            .map(|(_, v)| v.as_str())
    }

    /// Whether every term holds, given each attribute's value.
    pub fn matches(&self, value_of: impl Fn(&str) -> Option<String>) -> bool {
        self.terms.iter().all(|(attr, expected)| {
            value_of(attr).is_some_and(|v| v.eq_ignore_ascii_case(expected))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provisioning_system_filters_parse() {
        let f = Filter::parse(r#"userName eq "alice@example.com""#).unwrap();
        assert_eq!(f.get("username"), Some("alice@example.com"));
        let f =
            Filter::parse(r#"externalId eq "00u1" and userName eq "a \"quoted\" name""#).unwrap();
        assert_eq!(f.get("externalid"), Some("00u1"));
        assert_eq!(f.get("userName"), Some(r#"a "quoted" name"#));
        assert!(Filter::parse(r#"userName co "ali""#).is_err());
        assert!(Filter::parse(r#"userName eq "open"#).is_err());
        assert!(Filter::parse(r#"userName eq "a" or userName eq "b""#).is_err());
    }
}
