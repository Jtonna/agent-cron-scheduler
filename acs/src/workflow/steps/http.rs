use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

pub use crate::models::workflow::HttpStep;
use crate::workflow::step::{wait_for_kill, Step, StepContext, StepError, StepOutput};
use crate::workflow::template;

#[async_trait]
impl Step for HttpStep {
    fn kind(&self) -> &'static str {
        "http"
    }

    async fn execute(&self, ctx: &mut StepContext) -> Result<StepOutput, StepError> {
        // 1. Emit step start marker
        let _ = ctx
            .log_sink
            .write_step_start(&self.common.id, Utc::now())
            .await
            .map_err(StepError::Io)?;

        // 2. Substitute templates
        let url_sub = template::substitute(&self.url, &ctx.input, &ctx.steps);
        for warn in &url_sub.warnings {
            tracing::warn!(step_id = %self.common.id, "http url template warning: {}", warn);
        }
        let url = url_sub.output;

        let body: Option<String> = if let Some(ref b) = self.body {
            let sub = template::substitute(b, &ctx.input, &ctx.steps);
            for warn in &sub.warnings {
                tracing::warn!(step_id = %self.common.id, "http body template warning: {}", warn);
            }
            Some(sub.output)
        } else {
            None
        };

        let mut substituted_headers: Vec<(String, String)> = Vec::new();
        for (k, v) in &self.headers {
            let sub = template::substitute(v, &ctx.input, &ctx.steps);
            for warn in &sub.warnings {
                tracing::warn!(step_id = %self.common.id, "http header '{}' template warning: {}", k, warn);
            }
            substituted_headers.push((k.clone(), sub.output));
        }

        // 3. Build reqwest client
        let client = reqwest::Client::new();

        // 4. Build request
        let method_str = self.method.to_uppercase();
        // Validate against the standard set of HTTP methods.
        let known_methods = [
            "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "TRACE", "CONNECT",
        ];
        if !known_methods.contains(&method_str.as_str()) {
            return Err(StepError::Internal(format!(
                "unknown HTTP method: {}",
                self.method
            )));
        }
        let method = reqwest::Method::from_bytes(method_str.as_bytes())
            .map_err(|_| StepError::Internal(format!("unknown HTTP method: {}", self.method)))?;

        let mut builder = client.request(method, &url);
        for (k, v) in &substituted_headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        if let Some(ref b) = body {
            builder = builder.body(b.clone());
        }

        // 5. Apply timeout and send, racing against kill signal.
        let timeout_secs = self.common.timeout_secs.filter(|&s| s > 0);
        let kill_rx = ctx.kill_rx.clone();

        let resp = if let Some(secs) = timeout_secs {
            tokio::select! {
                result = tokio::time::timeout(
                    std::time::Duration::from_secs(secs),
                    builder.send(),
                ) => {
                    match result {
                        Ok(r) => r.map_err(|e| StepError::Internal(format!("http request failed: {}", e)))?,
                        Err(_) => return Err(StepError::Timeout(secs)),
                    }
                }
                _ = wait_for_kill(kill_rx) => {
                    tracing::info!(step_id = %self.common.id, "Kill signal received, cancelling HTTP request");
                    // Dropping the request future cancels the in-flight reqwest request.
                    return Err(StepError::Killed);
                }
            }
        } else {
            tokio::select! {
                result = builder.send() => {
                    result.map_err(|e| StepError::Internal(format!("http request failed: {}", e)))?
                }
                _ = wait_for_kill(kill_rx) => {
                    tracing::info!(step_id = %self.common.id, "Kill signal received, cancelling HTTP request");
                    return Err(StepError::Killed);
                }
            }
        };

        // 7. Capture response
        let status: u16 = resp.status().as_u16();
        let content_type: Option<String> = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let body_bytes = resp
            .bytes()
            .await
            .map_err(|e| StepError::Internal(format!("failed to read response body: {}", e)))?;

        // 8. Determine effective expect_status
        let effective_expect: Vec<u16> = if self.expect_status.is_empty() {
            (200u16..300u16).collect()
        } else {
            self.expect_status.clone()
        };

        // 9. Determine exit_code
        let exit_code: i32 = if effective_expect.contains(&status) {
            0
        } else {
            status as i32
        };

        // 10. Write response body to log_sink
        ctx.log_sink
            .write_chunk(&body_bytes)
            .await
            .map_err(StepError::Io)?;

        // 11. Emit step end marker
        let _ = ctx
            .log_sink
            .write_step_end(&self.common.id, Some(exit_code), Utc::now())
            .await
            .map_err(StepError::Io)?;

        // 12. Build StepOutput.stdout
        let stdout = parse_response_body(
            &body_bytes,
            content_type.as_deref(),
            self.common.capture.parser.as_deref(),
        );

        // 13. Return
        Ok(StepOutput {
            exit_code: Some(exit_code),
            stdout: Some(stdout),
            exports: HashMap::new(),
            cost: None,
        })
    }
}

/// Parse the response body into a `serde_json::Value`.
///
/// If the content-type indicates JSON, attempt to parse as JSON (fall back to string on error).
/// Otherwise, apply the capture spec parser.
fn parse_response_body(
    body: &[u8],
    content_type: Option<&str>,
    parser: Option<&str>,
) -> Value {
    // Check if content-type indicates JSON
    let is_json_content_type = content_type
        .map(|ct| ct.contains("application/json"))
        .unwrap_or(false);

    if is_json_content_type {
        match serde_json::from_slice(body) {
            Ok(v) => return v,
            Err(_) => {
                // Fall through to capture parser logic
            }
        }
    }

    // Apply capture parser
    match parser {
        Some("json") => match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(_) => Value::String(String::from_utf8_lossy(body).into_owned()),
        },
        Some("lines") => {
            let text = String::from_utf8_lossy(body);
            let lines: Vec<Value> = text
                .split('\n')
                .filter(|l| !l.is_empty())
                .map(|l| Value::String(l.to_owned()))
                .collect();
            Value::Array(lines)
        }
        Some("raw") | None => Value::String(String::from_utf8_lossy(body).into_owned()),
        Some(other) => {
            tracing::warn!("unknown capture parser '{}'; treating as raw", other);
            Value::String(String::from_utf8_lossy(body).into_owned())
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use axum::body::Body;
    use axum::extract::{Request, State};
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use axum::Router;
    use chrono::{DateTime, Utc};
    use serde_json::{json, Value};
    use tokio::net::TcpListener;
    use uuid::Uuid;

    use crate::models::workflow::{CaptureSpec, HttpStep, StepDefCommon};
    use crate::workflow::step::{LogSink, Step, StepContext, StepError};

    // ── Mock LogSink ──────────────────────────────────────────────────────────

    #[derive(Clone, Default)]
    struct MockLogSink {
        chunks: Arc<Mutex<Vec<u8>>>,
        events: Arc<Mutex<Vec<String>>>,
    }

    impl MockLogSink {
        fn events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl LogSink for MockLogSink {
        async fn write_step_start(
            &self,
            step_id: &str,
            _started_at: DateTime<Utc>,
        ) -> std::io::Result<u64> {
            self.events
                .lock()
                .unwrap()
                .push(format!("start:{}", step_id));
            Ok(0)
        }

        async fn write_chunk(&self, data: &[u8]) -> std::io::Result<()> {
            self.chunks.lock().unwrap().extend_from_slice(data);
            Ok(())
        }

        async fn write_step_end(
            &self,
            step_id: &str,
            exit_code: Option<i32>,
            _finished_at: DateTime<Utc>,
        ) -> std::io::Result<u64> {
            self.events.lock().unwrap().push(format!(
                "end:{}:{}",
                step_id,
                exit_code.map(|c| c.to_string()).unwrap_or("-1".to_string())
            ));
            Ok(0)
        }
    }

    // ── Helper: create a minimal StepContext ──────────────────────────────────

    fn make_ctx(sink: Arc<dyn LogSink>) -> StepContext {
        StepContext {
            workflow_id: Uuid::now_v7(),
            workflow_version: 1,
            run_id: Uuid::now_v7(),
            step_index: 0,
            input: json!({}),
            steps: HashMap::new(),
            log_sink: sink,
            working_dir: None,
            env: HashMap::new(),
            event_tx: None,
            kill_rx: None,
        }
    }

    fn make_step(id: &str, method: &str, url: &str) -> HttpStep {
        HttpStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            method: method.to_string(),
            url: url.to_string(),
            headers: HashMap::new(),
            body: None,
            expect_status: vec![],
        }
    }

    // ── Helper: spawn a local axum test server ────────────────────────────────

    async fn spawn_test_server(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let base_url = format!("http://127.0.0.1:{}", addr.port());
        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        // Give the server a brief moment to start listening
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        (base_url, handle)
    }

    // ── Test 1: GET 200 happy path with JSON response ─────────────────────────

    #[tokio::test]
    async fn test_http_step_get_200_json() {
        async fn handler() -> impl IntoResponse {
            (
                StatusCode::OK,
                [("content-type", "application/json")],
                r#"{"x":1}"#,
            )
        }

        let router = Router::new().route("/test", get(handler));
        let (base_url, _handle) = spawn_test_server(router).await;

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("h1", "GET", &format!("{}/test", base_url));
        let output = step.execute(&mut ctx).await.expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        match output.stdout {
            Some(Value::Object(ref map)) => {
                assert_eq!(map.get("x"), Some(&json!(1)));
            }
            other => panic!("expected JSON object, got {:?}", other),
        }
        // log sink markers
        let events = sink.events();
        assert_eq!(events[0], "start:h1");
        assert!(events[1].starts_with("end:h1:0"));
    }

    // ── Test 2: POST with body — server echoes the request body ──────────────

    #[tokio::test]
    async fn test_http_step_post_with_body() {
        #[derive(Clone)]
        struct EchoState {
            received: Arc<Mutex<Option<Vec<u8>>>>,
        }

        async fn echo_handler(
            State(state): State<EchoState>,
            body: Body,
        ) -> impl IntoResponse {
            let bytes = axum::body::to_bytes(body, 64 * 1024).await.unwrap();
            *state.received.lock().unwrap() = Some(bytes.to_vec());
            (StatusCode::OK, [("content-type", "text/plain")], "ok")
        }

        let received: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
        let state = EchoState {
            received: Arc::clone(&received),
        };
        let router = Router::new()
            .route("/echo", post(echo_handler))
            .with_state(state);
        let (base_url, _handle) = spawn_test_server(router).await;

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let mut step = make_step("h2", "POST", &format!("{}/echo", base_url));
        step.body = Some("hello".to_string());

        let output = step.execute(&mut ctx).await.expect("execute should succeed");
        assert_eq!(output.exit_code, Some(0));

        let body_received = received.lock().unwrap().clone().unwrap_or_default();
        assert_eq!(body_received, b"hello");
    }

    // ── Test 3: Header substitution ──────────────────────────────────────────

    #[tokio::test]
    async fn test_http_step_header_substitution() {
        #[derive(Clone)]
        struct HeaderState {
            auth: Arc<Mutex<Option<String>>>,
        }

        async fn header_handler(
            State(state): State<HeaderState>,
            req: Request,
        ) -> impl IntoResponse {
            let auth = req
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            *state.auth.lock().unwrap() = auth;
            (StatusCode::OK, [("content-type", "text/plain")], "ok")
        }

        let captured_auth: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let state = HeaderState {
            auth: Arc::clone(&captured_auth),
        };
        let router = Router::new()
            .route("/headers", get(header_handler))
            .with_state(state);
        let (base_url, _handle) = spawn_test_server(router).await;

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);
        ctx.input = json!({"token": "xyz"});

        let mut step = make_step("h3", "GET", &format!("{}/headers", base_url));
        step.headers
            .insert("Authorization".to_string(), "Bearer ${input.token}".to_string());

        let output = step.execute(&mut ctx).await.expect("execute should succeed");
        assert_eq!(output.exit_code, Some(0));

        let auth_received = captured_auth.lock().unwrap().clone().unwrap_or_default();
        assert_eq!(auth_received, "Bearer xyz");
    }

    // ── Test 4: URL substitution ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_http_step_url_substitution() {
        #[derive(Clone)]
        struct PathState {
            path: Arc<Mutex<Option<String>>>,
        }

        async fn path_handler(
            State(state): State<PathState>,
            req: Request,
        ) -> impl IntoResponse {
            *state.path.lock().unwrap() = Some(req.uri().path().to_string());
            (StatusCode::OK, [("content-type", "text/plain")], "ok")
        }

        let captured_path: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let state = PathState {
            path: Arc::clone(&captured_path),
        };
        let router = Router::new()
            .route("/items/{id}", get(path_handler))
            .with_state(state);
        let (base_url, _handle) = spawn_test_server(router).await;

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);
        ctx.input = json!({"id": "42"});

        let step = make_step(
            "h4",
            "GET",
            &format!("{}/items/${{input.id}}", base_url),
        );

        let output = step.execute(&mut ctx).await.expect("execute should succeed");
        assert_eq!(output.exit_code, Some(0));

        let path_received = captured_path.lock().unwrap().clone().unwrap_or_default();
        assert!(
            path_received.contains("42"),
            "expected path to contain '42', got: {}",
            path_received
        );
    }

    // ── Test 5: 404 not in expect_status default → exit_code=404 ─────────────

    #[tokio::test]
    async fn test_http_step_404_default_expect_exit_code_404() {
        async fn not_found_handler() -> impl IntoResponse {
            (StatusCode::NOT_FOUND, "not found")
        }

        let router = Router::new().route("/missing", get(not_found_handler));
        let (base_url, _handle) = spawn_test_server(router).await;

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        // expect_status is empty → effective default is 200..300
        let step = make_step("h5", "GET", &format!("{}/missing", base_url));
        let result = step.execute(&mut ctx).await;

        // Should NOT raise an error — exit_code = 404
        let output = result.expect("404 should not raise an error");
        assert_eq!(output.exit_code, Some(404));
    }

    // ── Test 6: Custom expect_status [404] → exit_code=0 ─────────────────────

    #[tokio::test]
    async fn test_http_step_custom_expect_status_404_is_success() {
        async fn not_found_handler() -> impl IntoResponse {
            (StatusCode::NOT_FOUND, "not found")
        }

        let router = Router::new().route("/missing2", get(not_found_handler));
        let (base_url, _handle) = spawn_test_server(router).await;

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let mut step = make_step("h6", "GET", &format!("{}/missing2", base_url));
        step.expect_status = vec![404];

        let output = step.execute(&mut ctx).await.expect("execute should succeed");
        assert_eq!(output.exit_code, Some(0));
    }

    // ── Test 7: Capture parser lines ─────────────────────────────────────────

    #[tokio::test]
    async fn test_http_step_capture_parser_lines() {
        async fn lines_handler() -> impl IntoResponse {
            (
                StatusCode::OK,
                [("content-type", "text/plain")],
                "a\nb\n",
            )
        }

        let router = Router::new().route("/lines", get(lines_handler));
        let (base_url, _handle) = spawn_test_server(router).await;

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let mut step = make_step("h7", "GET", &format!("{}/lines", base_url));
        step.common.capture.parser = Some("lines".to_string());

        let output = step.execute(&mut ctx).await.expect("execute should succeed");
        assert_eq!(output.exit_code, Some(0));

        match output.stdout {
            Some(Value::Array(arr)) => {
                assert!(arr.len() >= 2, "expected at least 2 lines, got {:?}", arr);
                assert_eq!(arr[0], json!("a"));
                assert_eq!(arr[1], json!("b"));
            }
            other => panic!("expected Array, got {:?}", other),
        }
    }

    // ── Test 8: Timeout ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_http_step_timeout() {
        async fn slow_handler() -> impl IntoResponse {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            (StatusCode::OK, "done")
        }

        let router = Router::new().route("/slow", get(slow_handler));
        let (base_url, _handle) = spawn_test_server(router).await;

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let mut step = make_step("h8", "GET", &format!("{}/slow", base_url));
        step.common.timeout_secs = Some(1);

        let result = step.execute(&mut ctx).await;
        match result {
            Err(StepError::Timeout(secs)) => {
                assert_eq!(secs, 1);
            }
            other => panic!("expected Timeout error, got {:?}", other),
        }
    }

    // ── Test 9: Unknown method → Err(StepError::Internal) ────────────────────

    #[tokio::test]
    async fn test_http_step_unknown_method() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("h9", "INVALID", "http://127.0.0.1:9999/noop");
        let result = step.execute(&mut ctx).await;

        match result {
            Err(StepError::Internal(msg)) => {
                assert!(
                    msg.contains("INVALID") || msg.contains("method"),
                    "expected error about method, got: {}",
                    msg
                );
            }
            other => panic!("expected Internal error, got {:?}", other),
        }
    }

    // ── Test: parse_response_body helper ─────────────────────────────────────

    #[test]
    fn test_parse_response_body_json_content_type() {
        let result = super::parse_response_body(
            b"{\"x\":1}",
            Some("application/json"),
            None,
        );
        assert_eq!(result, json!({"x": 1}));
    }

    #[test]
    fn test_parse_response_body_json_content_type_invalid_falls_back_to_capture() {
        // invalid JSON with application/json falls back to capture parser
        let result = super::parse_response_body(b"not json", Some("application/json"), None);
        // None parser → raw string
        assert_eq!(result, Value::String("not json".to_string()));
    }

    #[test]
    fn test_parse_response_body_lines_parser() {
        let result = super::parse_response_body(b"a\nb\n", Some("text/plain"), Some("lines"));
        assert_eq!(result, json!(["a", "b"]));
    }

    #[test]
    fn test_parse_response_body_raw_parser() {
        let result = super::parse_response_body(b"hello", None, Some("raw"));
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_parse_response_body_no_parser_no_content_type() {
        let result = super::parse_response_body(b"hello", None, None);
        assert_eq!(result, Value::String("hello".to_string()));
    }

    // ── Test: kind() ──────────────────────────────────────────────────────────

    #[test]
    fn test_http_step_kind() {
        let step = make_step("k", "GET", "http://example.com");
        assert_eq!(step.kind(), "http");
    }
}
