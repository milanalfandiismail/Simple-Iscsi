use std::fs;
use std::path::Path;
use crate::server_api::build_response;

pub fn handle_static_files(path: &str) -> String {
    // Release script menaruh UI di ui/; fallback ke assets/ui saat dev.
    let base = if Path::new("ui/index.html").exists() {
        "ui"
    } else {
        "assets/ui"
    };
    let file_path = if path == "/" || path == "/index.html" {
        format!("{}/index.html", base)
    } else {
        format!("{}{}", base, path)
    };

    let content_type = if file_path.ends_with(".css") {
        "text/css"
    } else if file_path.ends_with(".js") {
        "application/javascript"
    } else if file_path.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "text/html"
    };

    match fs::read_to_string(&file_path) {
        Ok(content) => build_response(200, "OK", content_type, &content),
        Err(_) => build_response(404, "Not Found", "text/plain", "File not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_index_from_assets_when_ui_absent() {
        let response = handle_static_files("/");
        assert!(response.contains("200 OK"));
        assert!(response.contains("<html"));
    }
}
