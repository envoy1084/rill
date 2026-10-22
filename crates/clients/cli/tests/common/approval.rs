//! Votes that let a change through the default submit policy.

use std::path::Path;

use super::dev_server::Server;

/// Votes that satisfy the default submit policy whatever the risk class:
/// a human Review +2 (normal and above) and Verified +1 (trivial/low).
pub async fn approve(server: &Server, cwd: &Path, change: &str) {
    server
        .json(cwd, &["review", "vote", change, "Review", "+2"])
        .await;
    server
        .json(cwd, &["review", "vote", change, "Verified", "+1"])
        .await;
}
