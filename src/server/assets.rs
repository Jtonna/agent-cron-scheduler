//! Embedded static assets for the web UI.

use axum::body::Body;
use axum::http::{header, Request, Response, StatusCode};
use rust_embed::RustEmbed;

/// Embedded web assets from the `web/` directory.
#[derive(RustEmbed)]
#[folder = "web/"]
pub struct WebAssets;

/// Serve embedded static files.
///
/// This handler serves files from the embedded `WebAssets` struct.
/// For empty paths or "/", it serves "index.html".
pub async fn serve_embedded(req: Request<Body>) -> Response<Body> {
    let path = req.uri().path();

    // Strip leading slash and default to index.html for empty/root path
    let path = path.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match WebAssets::get(path) {
        Some(content) => {
            // Determine MIME type from file extension
            let mime_type = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .body(Body::from(content.data.to_vec()))
                .unwrap()
        }
        None => {
            // File not found
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from("Not Found"))
                .unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;

    async fn body_string(body: Body) -> String {
        let bytes = body.collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_serve_index_html_for_root() {
        let req = Request::builder()
            .uri("/")
            .body(Body::empty())
            .unwrap();

        let response = serve_embedded(req).await;

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.contains("text/html"));
    }

    #[tokio::test]
    async fn test_serve_404_html() {
        let req = Request::builder()
            .uri("/404.html")
            .body(Body::empty())
            .unwrap();

        let response = serve_embedded(req).await;

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.contains("text/html"));
    }

    #[tokio::test]
    async fn test_serve_nonexistent_returns_404() {
        let req = Request::builder()
            .uri("/nonexistent.file")
            .body(Body::empty())
            .unwrap();

        let response = serve_embedded(req).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = body_string(response.into_body()).await;
        assert_eq!(body, "Not Found");
    }
}
