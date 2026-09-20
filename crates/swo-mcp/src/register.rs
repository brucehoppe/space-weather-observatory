//! `swo-mcp register` / `unregister`: add this server to an MCP client's
//! configuration so the user does not have to edit JSON by hand.
//!
//! The config files belong to other applications and usually list other
//! servers, so the rules are strict: merge, never overwrite; touch only our
//! own entry; refuse a file that is not valid JSON rather than "repairing" it;
//! and copy the file to a timestamped backup before every change.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// Name of our entry under `mcpServers`.
pub const SERVER_NAME: &str = "space-weather";

#[derive(Debug, PartialEq)]
pub enum Change {
    Added,
    Updated,
    AlreadyCurrent,
    Removed,
    NotPresent,
}

fn parse(config: Option<&str>) -> Result<Map<String, Value>, String> {
    match config.map(str::trim).filter(|c| !c.is_empty()) {
        None => Ok(Map::new()),
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(map)) => Ok(map),
            Ok(_) => Err("the config file is not a JSON object; left untouched".into()),
            Err(e) => Err(format!(
                "the config file is not valid JSON ({e}); left untouched"
            )),
        },
    }
}

fn render(root: Map<String, Value>) -> String {
    let mut out = serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default();
    out.push('\n');
    out
}

/// Config text with our server pointing at `command`. Every other key and
/// server is kept. `None`/empty input means the file does not exist yet.
pub fn upsert(config: Option<&str>, command: &str) -> Result<(String, Change), String> {
    let mut root = parse(config)?;
    let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or("`mcpServers` in the config file is not an object; left untouched")?;
    let entry = json!({ "command": command });
    let change = match servers.get(SERVER_NAME) {
        Some(existing) if *existing == entry => Change::AlreadyCurrent,
        Some(_) => Change::Updated,
        None => Change::Added,
    };
    servers.insert(SERVER_NAME.into(), entry);
    Ok((render(root), change))
}

/// Config text without our server. Everything else is kept.
pub fn remove(config: Option<&str>) -> Result<(String, Change), String> {
    let mut root = parse(config)?;
    let removed = root
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .and_then(|servers| servers.remove(SERVER_NAME))
        .is_some();
    Ok((
        render(root),
        if removed {
            Change::Removed
        } else {
            Change::NotPresent
        },
    ))
}

/// Claude Desktop's config file for this user.
///   macOS   ~/Library/Application Support/Claude/claude_desktop_config.json
///   Windows %APPDATA%\Claude\claude_desktop_config.json
pub fn claude_desktop_config() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("Claude").join("claude_desktop_config.json"))
}

/// Apply `edit` to the file at `path`, backing the original up first.
/// Returns the change and the backup path, if one was made.
pub fn edit_file(
    path: &Path,
    stamp: &str,
    edit: impl FnOnce(Option<&str>) -> Result<(String, Change), String>,
) -> Result<(Change, Option<PathBuf>), String> {
    let original = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let (updated, change) = edit(original.as_deref())?;
    if matches!(change, Change::AlreadyCurrent | Change::NotPresent) {
        return Ok((change, None));
    }
    let mut backup = None;
    if original.is_some() {
        let name = format!(
            "{}.backup-{stamp}",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("config")
        );
        let to = path.with_file_name(name);
        std::fs::copy(path, &to).map_err(|e| {
            format!(
                "cannot back up {}: {e}; nothing was changed",
                path.display()
            )
        })?;
        backup = Some(to);
    } else if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    std::fs::write(path, updated).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok((change, backup))
}
