//! Newline-delimited JSON framing over async readers and writers.

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

/// Read the next non-empty line and parse it as JSON. `Ok(None)` at end
/// of input; `Ok(Some(Err(text)))` when a line is not valid JSON, so the
/// caller can answer with a parse error instead of stopping.
pub async fn read_message<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
) -> std::io::Result<Option<Result<Value, String>>> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return Ok(Some(
            serde_json::from_str(trimmed).map_err(|e| e.to_string()),
        ));
    }
}

/// Write one JSON value as a single line and flush.
pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    value: &Value,
) -> std::io::Result<()> {
    // Compact encoding never contains a raw newline, which keeps the
    // framing intact whatever the payload holds.
    let mut text = serde_json::to_vec(value)?;
    text.push(b'\n');
    writer.write_all(&text).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn skips_blank_lines_and_reports_bad_json() {
        let input = b"\n{\"a\":1}\nnot json\n";
        let mut reader = BufReader::new(&input[..]);
        assert_eq!(
            read_message(&mut reader).await.unwrap(),
            Some(Ok(json!({"a": 1})))
        );
        assert!(matches!(
            read_message(&mut reader).await.unwrap(),
            Some(Err(_))
        ));
        assert_eq!(read_message(&mut reader).await.unwrap(), None);
    }

    #[tokio::test]
    async fn writes_one_line() {
        let mut out = Vec::new();
        write_message(&mut out, &json!({"text": "a\nb"}))
            .await
            .unwrap();
        assert_eq!(out, b"{\"text\":\"a\\nb\"}\n");
    }
}
