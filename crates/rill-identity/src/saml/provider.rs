//! One SAML identity provider as Rill's service provider sees it.

use rill_types::identity::SamlSettings;
use samael::crypto::AllowedSignatureAlgorithm;
use samael::metadata::{EntityDescriptor, HTTP_POST_BINDING, HTTP_REDIRECT_BINDING};
use samael::service_provider::ServiceProvider;

use super::claims::assertion_claims;
use crate::error::{Error, Result};
use crate::oidc::Claims;

/// Signatures Rill accepts; SHA-1 and DSA are refused.
const ALGORITHMS: [AllowedSignatureAlgorithm; 6] = [
    AllowedSignatureAlgorithm::RsaSha256,
    AllowedSignatureAlgorithm::RsaSha384,
    AllowedSignatureAlgorithm::RsaSha512,
    AllowedSignatureAlgorithm::EcdsaSha256,
    AllowedSignatureAlgorithm::EcdsaSha384,
    AllowedSignatureAlgorithm::EcdsaSha512,
];

/// A started sign-in: the id the response must answer and where to send
/// the browser.
#[derive(Debug, Clone)]
pub struct SamlRequest {
    pub id: String,
    pub redirect_url: String,
}

/// An IdP from its metadata, with Rill's entity id and assertion consumer
/// URL.
pub struct SamlProvider {
    sp: ServiceProvider,
    sso_url: String,
}

fn metadata(xml: &str) -> Result<EntityDescriptor> {
    xml.parse()
        .map_err(|e| Error::Saml(format!("unreadable IdP metadata: {e}")))
}

/// The entity id in IdP metadata, which assertions name as their issuer.
pub fn idp_entity_id(xml: &str) -> Result<String> {
    metadata(xml)?
        .entity_id
        .ok_or_else(|| Error::Saml("the IdP metadata has no entityID".into()))
}

impl SamlProvider {
    /// Refuses metadata without a signing certificate or a redirect-binding
    /// SSO endpoint: a response could not be verified, or a request sent.
    pub fn new(settings: &SamlSettings, acs_url: &str) -> Result<Self> {
        let sp = ServiceProvider {
            entity_id: Some(settings.sp_entity_id.clone()),
            acs_url: Some(acs_url.to_string()),
            metadata_url: None,
            slo_url: None,
            idp_metadata: metadata(&settings.idp_metadata)?,
            allow_idp_initiated: false,
            allowed_signature_algorithms: Some(ALGORITHMS.to_vec()),
            ..ServiceProvider::default()
        };
        if sp.idp_metadata.entity_id.is_none() {
            return Err(Error::Saml("the IdP metadata has no entityID".into()));
        }
        // samael skips verification when the metadata has no certificate.
        match sp.idp_signing_certs() {
            Ok(Some(certs)) if !certs.is_empty() => {}
            Ok(_) => {
                return Err(Error::Saml(
                    "the IdP metadata has no signing certificate".into(),
                ))
            }
            Err(e) => return Err(Error::Saml(format!("IdP certificates: {e}"))),
        }
        let sso_url = sp
            .sso_binding_location(HTTP_REDIRECT_BINDING)
            .ok_or_else(|| Error::Saml("the IdP has no HTTP-Redirect SSO endpoint".into()))?;
        Ok(Self { sp, sso_url })
    }

    /// The IdP's entity id.
    pub fn entity_id(&self) -> &str {
        self.sp
            .idp_metadata
            .entity_id
            .as_deref()
            .unwrap_or_default()
    }

    /// An AuthnRequest with an unguessable id, sent by redirect.
    pub fn request(&self, relay_state: &str) -> Result<SamlRequest> {
        let mut request = self
            .sp
            .make_authentication_request(&self.sso_url)
            .map_err(|e| Error::Saml(format!("AuthnRequest: {e}")))?;
        // An xs:ID, which must not start with a digit.
        request.id = format!("rill-{}", hex::encode(rand::random::<[u8; 20]>()));
        let url = request
            .redirect(relay_state)
            .map_err(|e| Error::Saml(format!("AuthnRequest: {e}")))?
            .ok_or_else(|| Error::Saml("the AuthnRequest has no destination".into()))?;
        Ok(SamlRequest {
            id: request.id,
            redirect_url: url.to_string(),
        })
    }

    /// The claims of a base64 `SAMLResponse` answering `request_id`, once
    /// its signature, issuer, audience, recipient and validity window
    /// check out.
    pub fn verify(&self, response: &str, request_id: &str) -> Result<Claims> {
        let assertion = self
            .sp
            .parse_base64_response(response.trim(), Some(&[request_id]))
            .map_err(|e| Error::Saml(format!("response refused: {e}")))?;
        let audience_ok = assertion
            .conditions
            .as_ref()
            .and_then(|c| c.audience_restrictions.as_ref())
            .is_some_and(|rs| {
                rs.iter().any(|r| {
                    r.audience
                        .iter()
                        .any(|a| Some(a) == self.sp.entity_id.as_ref())
                })
            });
        if !audience_ok {
            return Err(Error::Saml(
                "the assertion is not restricted to this service".into(),
            ));
        }
        assertion_claims(&assertion)
    }

    /// Rill's service provider metadata, for registering it at the IdP:
    /// unsigned requests, signed assertions posted to the ACS, no logout.
    pub fn sp_metadata(&self) -> String {
        format!(
            r#"<md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata" entityID="{entity}"><md:SPSSODescriptor AuthnRequestsSigned="false" WantAssertionsSigned="true" protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol"><md:AssertionConsumerService Binding="{binding}" Location="{acs}" index="0" isDefault="true"/></md:SPSSODescriptor></md:EntityDescriptor>"#,
            entity = xml_attr(self.sp.entity_id.as_deref().unwrap_or_default()),
            binding = HTTP_POST_BINDING,
            acs = xml_attr(self.sp.acs_url.as_deref().unwrap_or_default()),
        )
    }
}

fn xml_attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keycloak's self-signed realm certificate.
    const CERT: &str = "MIIClzCCAX8CBgGhXpyycDANBgkqhkiG9w0BAQsFADAPMQ0wCwYDVQQDDARyaWxsMB4XDTI2MTAyMTEzMTYzMFoXDTM2MTAyMTEzMTgxMFowDzENMAsGA1UEAwwEcmlsbDCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAL3HRxKIaGLaSqd7NZoSnRTY1AyYJT9QD2JLyxYLXKIEYG4nWZUDt/qdaLWwhiNKCuCRhKwst4UXhLOR+w9sCnQ9S3PaY3N5yyWQhMDu9WFJafibPWj3VEyOIdbt+WgmGk309GHXeSLxPtu/1imsSDP6ATniE5HBgOwG9MpGSLJWwN79ra7JG559HxcMLkQiXBA4d+bm/K7GW44lhyNS9Rbdk4x6oq7UPwjxWTytPuWvS5oqprrLDsXmSTtVGkXt+hZXQ68lxiifqTP84p5iwqD+VjXLfUi5T49Rx7y5QYqtLnIktiVq/aQMOjlu7Iw/3HO5FHLh0YPMuN2Xr3FuEFkCAwEAATANBgkqhkiG9w0BAQsFAAOCAQEACagHe0mHkw2YZHBpmUiNN9wnV0mWyDeMH97qnvlb/dsX5oPX9+1pt8Xhw66MKo6zZXnzl8hgQFgG/yxb52FBoKskqqz733y+BBpqUbnzyKmPObhBUF7288z84Q+zXwW994cJwPdVbxbNeRtMTYDqlnVi+H49GgNcFhsv7mSIzavcs24MTNJHBRg00wmnb62o913LORfnkvF9n/9mjD9vE/hFy3pCHJfWZCbL3r/cLHn7NfXVoPP+qn4jg8Sq8ArH/a64uyiR+ZSN5xQEfgz57SG0eAVD93a4kvc+BAQmNBDaCtBmRsSg7pBRWQyvpHB4CClqKCE/ChRlmVPuUKZi4g==";

    const UNSIGNED: &str = r#"<md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata" entityID="https://idp.example">
  <md:IDPSSODescriptor protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol">
    <md:SingleSignOnService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect" Location="https://idp.example/sso"/>
  </md:IDPSSODescriptor>
</md:EntityDescriptor>"#;

    #[test]
    fn metadata_without_a_signing_certificate_is_refused() {
        assert_eq!(idp_entity_id(UNSIGNED).unwrap(), "https://idp.example");
        let settings = SamlSettings {
            idp_metadata: UNSIGNED.into(),
            sp_entity_id: "https://rill.example/saml".into(),
        };
        let err = SamlProvider::new(&settings, "https://rill.example/auth/saml/acs")
            .err()
            .expect("unverifiable metadata must be refused");
        assert!(err.to_string().contains("signing certificate"), "{err}");
    }

    #[test]
    fn sp_metadata_escapes_and_reads_back() {
        let signed = UNSIGNED.replace(
            "<md:SingleSignOnService",
            &format!(
                r#"<md:KeyDescriptor use="signing"><ds:KeyInfo xmlns:ds="http://www.w3.org/2000/09/xmldsig#"><ds:X509Data><ds:X509Certificate>{CERT}</ds:X509Certificate></ds:X509Data></ds:KeyInfo></md:KeyDescriptor><md:SingleSignOnService"#
            ),
        );
        let settings = SamlSettings {
            idp_metadata: signed,
            sp_entity_id: "urn:rill:a&b\"<x>".into(),
        };
        let sp = SamlProvider::new(&settings, "https://rill.example/auth/saml/acs").unwrap();
        assert_eq!(sp.entity_id(), "https://idp.example");
        let metadata: EntityDescriptor = sp.sp_metadata().parse().unwrap();
        assert_eq!(metadata.entity_id.as_deref(), Some("urn:rill:a&b\"<x>"));
        let request = sp.request("relay").unwrap();
        assert!(request.id.starts_with("rill-") && request.id.len() == 45);
        assert!(request
            .redirect_url
            .starts_with("https://idp.example/sso?SAMLRequest="));
    }
}
