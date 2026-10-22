//! A verified assertion as claims: `iss` is the IdP's entity id, `sub` the
//! NameID, and each attribute appears under its name and friendly name.

use samael::schema::Assertion;
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::oidc::Claims;

/// The claims a verified `assertion` carries.
pub fn assertion_claims(assertion: &Assertion) -> Result<Claims> {
    let issuer = assertion
        .issuer
        .value
        .clone()
        .ok_or_else(|| Error::Saml("the assertion has no issuer".into()))?;
    let subject = assertion
        .subject
        .as_ref()
        .and_then(|s| s.name_id.as_ref())
        .map(|n| n.value.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| Error::Saml("the assertion has no NameID".into()))?;
    let mut map = Map::new();
    for statement in assertion.attribute_statements.iter().flatten() {
        for attribute in &statement.attributes {
            let values: Vec<Value> = attribute
                .values
                .iter()
                .filter_map(|v| v.value.clone())
                .map(Value::String)
                .collect();
            let value = match <[Value; 1]>::try_from(values) {
                Ok([one]) => one,
                Err(many) => Value::Array(many),
            };
            for name in [&attribute.name, &attribute.friendly_name]
                .into_iter()
                .flatten()
            {
                map.insert(name.clone(), value.clone());
            }
        }
    }
    map.insert("iss".into(), Value::String(issuer));
    map.insert("sub".into(), Value::String(subject));
    Ok(Claims(map))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASSERTION: &str = r#"<saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" ID="a1" IssueInstant="2026-01-01T00:00:00Z" Version="2.0">
  <saml:Issuer>https://idp.example/realms/rill</saml:Issuer>
  <saml:Subject><saml:NameID>ann</saml:NameID></saml:Subject>
  <saml:AttributeStatement>
    <saml:Attribute Name="urn:oid:0.9.2342.19200300.100.1.3" FriendlyName="email"><saml:AttributeValue>ann@example.com</saml:AttributeValue></saml:Attribute>
    <saml:Attribute Name="groups"><saml:AttributeValue>core</saml:AttributeValue><saml:AttributeValue>infra</saml:AttributeValue></saml:Attribute>
  </saml:AttributeStatement>
</saml:Assertion>"#;

    #[test]
    fn attributes_read_like_id_token_claims() {
        let assertion: Assertion = ASSERTION.parse().unwrap();
        let claims = assertion_claims(&assertion).unwrap();
        assert_eq!(claims.issuer().unwrap(), "https://idp.example/realms/rill");
        assert_eq!(claims.subject().unwrap(), "ann");
        assert_eq!(claims.str("email"), Some("ann@example.com"));
        assert_eq!(
            claims.str("urn:oid:0.9.2342.19200300.100.1.3"),
            Some("ann@example.com")
        );
        assert_eq!(claims.strings("groups"), ["core", "infra"]);
    }

    #[test]
    fn an_attribute_cannot_pose_as_the_subject() {
        let forged = ASSERTION.replace(
            r#"<saml:Attribute Name="groups">"#,
            r#"<saml:Attribute Name="sub"><saml:AttributeValue>root</saml:AttributeValue></saml:Attribute><saml:Attribute Name="groups">"#,
        );
        let assertion: Assertion = forged.parse().unwrap();
        assert_eq!(
            assertion_claims(&assertion).unwrap().subject().unwrap(),
            "ann"
        );
    }
}
