use serde_json::json;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::server_api::build_response;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn set_log_path(path: PathBuf) {
    let _ = LOG_PATH.set(path);
}

pub fn get_logs(query: &HashMap<&str, &str>) -> String {
    let path = LOG_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("server.log"));
    let requested = query
        .get("lines")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(200)
        .clamp(1, 1000);
    let level = query.get("level").copied().unwrap_or("all");
    let exists = path.exists();
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let mut lines: Vec<String> = Vec::new();

    if exists {
        if let Ok(text) = read_tail(&path, 256 * 1024) {
            let mut all: Vec<String> = text.lines().map(|s| s.to_string()).collect();
            // Tail may start mid-line.
            if size > 256 * 1024 && !all.is_empty() {
                all.remove(0);
            }
            for line in all {
                if level_matches(&line, level) {
                    lines.push(line);
                }
            }
            if lines.len() > requested {
                lines = lines.split_off(lines.len() - requested);
            }
        }
    }

    build_response(
        200,
        "OK",
        "application/json",
        &json!({
            "path": path.to_string_lossy(),
            "size": size,
            "exists": exists,
            "lines": lines,
        })
        .to_string(),
    )
}

fn read_tail(path: &Path, max: u64) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn level_matches(line: &str, level: &str) -> bool {
    match level {
        "error" => line.contains("ERROR"),
        "warn" => line.contains(" WARN ") || line.contains("ERROR"),
        _ => true,
    }
}
