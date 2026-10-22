//! Errors on the wire: a `ConnectError` with its details for the API, a
//! tonic `Status` for REAPI, and back.

use connectrpc::{ConnectError, ErrorCode, ErrorDetail};
use rill_proto::google::rpc;
use tonic::{Code, Status};

use super::Error;

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        let code = err.code();
        if code == Code::Internal {
            tracing::error!(error = %err, "internal error in handler");
        }
        let retry_after = retry_after(&err);
        let mut status = Status::new(code, err.to_string());
        if let Some(secs) = retry_after {
            status.metadata_mut().insert(
                "retry-after",
                secs.to_string().parse().expect("digits are ASCII"),
            );
        }
        status
    }
}

impl From<Error> for ConnectError {
    fn from(err: Error) -> Self {
        let reason = err.reason();
        let retry_after = retry_after(&err);
        let field = match &err {
            Error::Convert(e) => Some(e.field()),
            _ => None,
        };
        let missing = match &err {
            Error::PolicyDenied { missing, .. } => missing.clone(),
            _ => Vec::new(),
        };
        let status = Status::from(err);
        let code = ErrorCode::from_grpc_code(status.code() as u32).unwrap_or(ErrorCode::Unknown);
        let message = status.message().to_string();
        let name = buffa::Enumeration::proto_name(&reason);
        let mut error = ConnectError::new(code, message.clone()).with_detail(detail(
            "google.rpc.ErrorInfo",
            &rpc::ErrorInfo {
                reason: name.strip_prefix("ERROR_REASON_").unwrap_or(name).into(),
                domain: rill_proto::ERROR_DOMAIN.into(),
                metadata: Default::default(),
            },
        ));
        if let Some(field) = field {
            error = error.with_detail(detail(
                "google.rpc.BadRequest",
                &rpc::BadRequest {
                    field_violations: vec![rpc::bad_request::FieldViolation {
                        field: field.into(),
                        description: message,
                        ..Default::default()
                    }],
                },
            ));
        }
        if !missing.is_empty() {
            error = error.with_detail(detail(
                "google.rpc.PreconditionFailure",
                &rpc::PreconditionFailure {
                    violations: missing
                        .into_iter()
                        .map(|description| rpc::precondition_failure::Violation {
                            r#type: "POLICY".into(),
                            subject: String::new(),
                            description,
                        })
                        .collect(),
                },
            ));
        }
        if let Some(secs) = retry_after {
            error = error.with_detail(detail(
                "google.rpc.RetryInfo",
                &rpc::RetryInfo {
                    retry_delay: buffa_types::google::protobuf::Duration {
                        seconds: secs as i64,
                        ..Default::default()
                    }
                    .into(),
                },
            ));
            let mut headers = http::HeaderMap::new();
            headers.insert("retry-after", http::HeaderValue::from(secs));
            error = error.with_headers(headers);
        }
        error
    }
}

/// A detail carrying `message`, with its JSON form as `debug` for people
/// reading the error with `curl`.
fn detail<M: buffa::Message + serde::Serialize>(type_name: &str, message: &M) -> ErrorDetail {
    let mut detail = ErrorDetail::from_message(type_name, message);
    detail.debug = serde_json::to_value(message).ok();
    detail
}

/// How long the caller should wait before retrying, when the error says.
fn retry_after(err: &Error) -> Option<u64> {
    match err {
        Error::Lander(rill_lander::Error::QueueFull {
            retry_after_secs, ..
        })
        | Error::RateLimited {
            retry_after_secs, ..
        } => Some(*retry_after_secs),
        _ => None,
    }
}

impl From<ConnectError> for Error {
    fn from(err: ConnectError) -> Self {
        let msg = err.message.unwrap_or_default();
        match err.code {
            ErrorCode::NotFound => Error::NotFound(msg),
            ErrorCode::PermissionDenied => Error::PermissionDenied(msg),
            ErrorCode::Unauthenticated => Error::Unauthenticated(msg),
            ErrorCode::InvalidArgument => Error::InvalidArgument(msg),
            ErrorCode::FailedPrecondition => Error::FailedPrecondition(msg),
            ErrorCode::AlreadyExists => Error::AlreadyExists(msg),
            ErrorCode::Unimplemented => Error::Unimplemented(msg),
            ErrorCode::Unavailable => Error::Unavailable(msg),
            _ => Error::Internal(msg),
        }
    }
}

impl From<Status> for Error {
    fn from(status: Status) -> Self {
        let msg = status.message().to_string();
        match status.code() {
            Code::NotFound => Error::NotFound(msg),
            Code::PermissionDenied => Error::PermissionDenied(msg),
            Code::Unauthenticated => Error::Unauthenticated(msg),
            Code::InvalidArgument => Error::InvalidArgument(msg),
            Code::FailedPrecondition => Error::FailedPrecondition(msg),
            Code::AlreadyExists => Error::AlreadyExists(msg),
            Code::Unimplemented => Error::Unimplemented(msg),
            Code::Unavailable => Error::Unavailable(msg),
            _ => Error::Internal(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use buffa::Message;

    use super::*;

    fn info(err: &ConnectError) -> rpc::ErrorInfo {
        let detail = err
            .details
            .iter()
            .find(|d| d.type_url.ends_with("google.rpc.ErrorInfo"))
            .expect("every error carries an ErrorInfo");
        decode(detail.value.as_deref().unwrap())
    }

    fn decode<M: Message>(value: &str) -> M {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(value.trim_end_matches('='))
            .unwrap();
        M::decode_from_slice(&bytes).unwrap()
    }

    #[test]
    fn every_error_names_its_reason_in_the_rill_domain() {
        let err = ConnectError::from(Error::EtagMismatch("stale".into()));
        assert_eq!(err.code, ErrorCode::Aborted);
        let info = info(&err);
        assert_eq!(
            (info.reason.as_str(), info.domain.as_str()),
            ("ETAG_MISMATCH", "rill.dev")
        );
        let err = ConnectError::from(Error::not_found("gone"));
        assert_eq!(info_of(err), "NOT_FOUND");
    }

    fn info_of(err: ConnectError) -> String {
        info(&err).reason
    }

    #[test]
    fn a_policy_denial_lists_what_is_missing() {
        let err = ConnectError::from(Error::PolicyDenied {
            message: "not yet".into(),
            missing: vec!["Review +2".into(), "Verified +1".into()],
        });
        assert_eq!(err.code, ErrorCode::FailedPrecondition);
        let detail = err
            .details
            .iter()
            .find(|d| d.type_url.ends_with("PreconditionFailure"))
            .unwrap();
        let failure: rpc::PreconditionFailure = decode(detail.value.as_deref().unwrap());
        let missing: Vec<_> = failure
            .violations
            .iter()
            .map(|v| v.description.as_str())
            .collect();
        assert_eq!(missing, ["Review +2", "Verified +1"]);
        assert_eq!(info_of(err), "POLICY_DENIED");
    }

    #[test]
    fn a_malformed_field_is_named_and_a_rate_limit_says_when_to_retry() {
        let err = ConnectError::from(Error::Convert(rill_proto::ConvertError::missing(
            "repo.name",
        )));
        let detail = err
            .details
            .iter()
            .find(|d| d.type_url.ends_with("BadRequest"))
            .unwrap();
        let bad: rpc::BadRequest = decode(detail.value.as_deref().unwrap());
        assert_eq!(bad.field_violations[0].field, "repo.name");

        let err = ConnectError::from(Error::RateLimited {
            principal: "ann".into(),
            retry_after_secs: 3,
        });
        let detail = err
            .details
            .iter()
            .find(|d| d.type_url.ends_with("RetryInfo"))
            .unwrap();
        let retry: rpc::RetryInfo = decode(detail.value.as_deref().unwrap());
        assert_eq!(retry.retry_delay.seconds, 3);
        assert_eq!(err.response_headers().get("retry-after").unwrap(), "3");
        assert_eq!(info_of(err), "RATE_LIMITED");
    }
}
