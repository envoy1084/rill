//! The minimal pages the browser flow ends on until the web UI exists.

use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title></head>\
         <body style=\"font-family:system-ui;margin:4rem\"><h1>{title}</h1><p>{body}</p></body></html>",
        title = escape(title),
        body = escape(body),
    )
}

pub fn signed_in(name: &str, org: &str) -> Response {
    Html(page(
        "Signed in",
        &format!("You are signed in to Rill as {name} ({org}). You can close this tab."),
    ))
    .into_response()
}

pub fn failed(status: StatusCode, reason: &str) -> Response {
    (status, Html(page("Sign-in failed", reason))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_text_is_escaped() {
        assert!(!page("<x>", "\"<script>").contains("<script>"));
    }
}
