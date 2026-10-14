use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result};

use crate::models::CodexAuth;

pub fn read_access_token() -> Result<String> {
    let codex_home = match env::var_os("CODEX_HOME") {
        Some(path) => PathBuf::from(path),
        None => {
            let home = env::var_os("HOME").context("Не задан HOME")?;

            PathBuf::from(home).join(".codex")
        }
    };

    let auth_path = codex_home.join("auth.json");

    let content = fs::read_to_string(&auth_path)
        .with_context(|| format!("Не удалось прочитать {}", auth_path.display()))?;

    let auth: CodexAuth =
        serde_json::from_str(&content).context("Некорректный формат auth.json")?;

    Ok(auth.tokens.access_token)
}
