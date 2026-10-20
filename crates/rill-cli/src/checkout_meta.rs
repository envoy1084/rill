//! The one edit to `.rill/checkout.toml` the SDK does not expose: clearing
//! or replacing `working_change`, which `rill new` needs to start a second
//! change on top of the first.
//!
//! The file is edited as a generic TOML table so this stays correct as
//! long as the SDK keeps the `working_change` key, whatever else it adds.

use std::path::Path;

use rill_sdk::local::META_DIR;
use rill_types::ChangeId;

use crate::error::CliError;
use crate::Result;

const CHECKOUT_FILE: &str = "checkout.toml";

/// Set or clear the checkout's working change.
pub async fn set_working_change(dir: &Path, change: Option<ChangeId>) -> Result<()> {
    let path = dir.join(META_DIR).join(CHECKOUT_FILE);
    tokio::task::spawn_blocking(move || rewrite(&path, change))
        .await
        .map_err(|e| CliError::Other(anyhow::anyhow!("meta task failed: {e}")))?
}

fn rewrite(path: &Path, change: Option<ChangeId>) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let mut table: toml::Table = text
        .parse()
        .map_err(|e| CliError::Other(anyhow::anyhow!("{}: {e}", path.display())))?;
    match change {
        Some(id) => {
            table.insert("working_change".into(), toml::Value::String(id.to_string()));
        }
        None => {
            table.remove("working_change");
        }
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(
        &tmp,
        toml::to_string_pretty(&table).map_err(|e| anyhow::anyhow!(e))?,
    )?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clears_and_sets_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let meta = dir.path().join(META_DIR);
        std::fs::create_dir_all(&meta).unwrap();
        let file = meta.join(CHECKOUT_FILE);
        std::fs::write(
            &file,
            "endpoint = \"http://x\"\nrepo = \"r\"\nworking_change = \"ch_00\"\n[sparse]\ninclude = []\nexclude = []\n",
        )
        .unwrap();
        rewrite(&file, None).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("working_change"));
        assert!(text.contains("endpoint"));
        let id = ChangeId::new();
        rewrite(&file, Some(id)).unwrap();
        let table: toml::Table = std::fs::read_to_string(&file).unwrap().parse().unwrap();
        assert_eq!(
            table["working_change"].as_str(),
            Some(id.to_string().as_str())
        );
        assert!(table["sparse"].is_table());
    }
}
