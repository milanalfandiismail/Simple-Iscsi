use std::fs;
use crate::server_api::build_response;

pub fn handle_static_files(path: &str) -> String {
    let file_path = if path == "/" || path == "/index.html" {
        "ui/index.html".to_string()
    } else {
        format!("ui{}", path)
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
