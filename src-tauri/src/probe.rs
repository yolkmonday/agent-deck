//! One tiny request against a provider, and a model list fetch.
//!
//! Every error string returned by this module is redacted against the key before it
//! leaves, and no request body or error is ever logged.

use crate::providers::HeaderStyle;
use serde::Serialize;
use serde_json::json;
use std::time::{Duration, Instant};

const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// A reasoning model can spend most of a small budget on hidden reasoning and return an
/// empty `content`. 512 is the floor that kept a real design probe from a false failure.
pub const PROBE_MAX_TOKENS: u32 = 512;

const PROBE_PROMPT: &str = "Reply with exactly: ok";

pub const REASONING_ONLY_ERROR: &str =
    "model menjawab hanya dengan reasoning (kemungkinan max_tokens terlalu kecil)";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelTestResult {
    pub ok: bool,
    pub status: Option<i64>,
    pub latency_ms: i64,
    pub reply: Option<String>,
    pub error: Option<String>,
    pub used_header: HeaderStyle,
}

/// Strips any occurrence of the key from a message. Belt and braces around every path
/// that could echo a provider response.
pub fn redact(text: &str, key: &str) -> String {
    if key.is_empty() {
        return text.to_string();
    }
    text.replace(key, "[redacted]")
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

fn apply_header(
    req: reqwest::RequestBuilder,
    key: &str,
    header: HeaderStyle,
    custom_name: &str,
) -> reqwest::RequestBuilder {
    if key.is_empty() {
        return req;
    }
    match header {
        HeaderStyle::Bearer => req.bearer_auth(key),
        HeaderStyle::Custom => req.header(custom_name, key),
    }
}

/// `GET {base}/models`, reading the ids out of `data[].id`.
pub async fn fetch_models(base_url: &str, key: &str, header: HeaderStyle) -> anyhow::Result<Vec<String>> {
    let client = reqwest::Client::builder().timeout(PROBE_TIMEOUT).build()?;
    let req = apply_header(client.get(join_url(base_url, "models")), key, header, "x-api-key");
    let res = req.send().await?;

    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("status {} from /models", status.as_u16());
    }
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| anyhow::anyhow!("invalid JSON from /models: {e}"))?;
    let ids = value
        .get("data")
        .and_then(|d| d.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.get("id").and_then(|i| i.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok(ids)
}

/// Sends one small chat completion and reports what came back. Never returns the key.
pub async fn test_model(
    base_url: &str,
    key: &str,
    header: HeaderStyle,
    model: &str,
) -> ModelTestResult {
    let first = attempt(base_url, key, header, "x-api-key", model).await;
    // Some gateways reject Bearer outright; a single retry with `x-api-key` covers them.
    if header == HeaderStyle::Bearer && first.status == Some(401) {
        let mut retry = attempt(base_url, key, HeaderStyle::Custom, "x-api-key", model).await;
        if retry.ok {
            retry.used_header = HeaderStyle::Custom;
            return retry;
        }
        if retry.status != Some(401) {
            return retry;
        }
    }
    first
}

async fn attempt(
    base_url: &str,
    key: &str,
    header: HeaderStyle,
    custom_name: &str,
    model: &str,
) -> ModelTestResult {
    let started = Instant::now();
    let mut used_header = header;

    let client = match reqwest::Client::builder().timeout(PROBE_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            return failure(redact(&e.to_string(), key), None, started, used_header);
        }
    };

    let body = json!({
        "model": model,
        "messages": [{ "role": "user", "content": PROBE_PROMPT }],
        "max_tokens": PROBE_MAX_TOKENS,
    });
    let req = apply_header(
        client.post(join_url(base_url, "chat/completions")).json(&body),
        key,
        header,
        custom_name,
    );

    let res = match req.send().await {
        Ok(r) => r,
        Err(e) => return failure(redact(&e.to_string(), key), None, started, used_header),
    };

    let status = res.status().as_u16() as i64;
    let text = res.text().await.unwrap_or_default();
    let latency_ms = started.elapsed().as_millis() as i64;

    if !(200..300).contains(&status) {
        let reason = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| text.clone());
        let message = redact(&format!("HTTP {status}: {reason}"), key);
        return failure(message, Some(status), started, used_header);
    }

    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            let message = redact(&format!("invalid JSON response: {e}"), key);
            return failure(message, Some(status), started, used_header);
        }
    };

    let choice = value.get("choices").and_then(|c| c.as_array()).and_then(|c| c.first());
    let content = choice
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let has_reasoning = choice
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("reasoning_content"))
        .and_then(|r| r.as_str())
        .is_some_and(|r| !r.trim().is_empty());

    if content.is_empty() && has_reasoning {
        return ModelTestResult {
            ok: true,
            status: Some(status),
            latency_ms,
            reply: None,
            error: Some(REASONING_ONLY_ERROR.to_string()),
            used_header,
        };
    }

    let reply = if content.is_empty() {
        None
    } else {
        Some(content.chars().take(200).collect())
    };
    ModelTestResult {
        ok: true,
        status: Some(status),
        latency_ms,
        reply,
        error: None,
        used_header,
    }
}

fn failure(error: String, status: Option<i64>, started: Instant, used_header: HeaderStyle) -> ModelTestResult {
    ModelTestResult {
        ok: false,
        status,
        latency_ms: started.elapsed().as_millis() as i64,
        reply: None,
        error: Some(error),
        used_header,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use tiny_http::{Header, Response, Server, StatusCode};

    /// A local server that answers each request with the next canned reply. Returns the
    /// base URL plus a receiver of the request headers it saw, so a test can assert which
    /// auth header arrived. No test here reaches a real provider.
    struct TestServer {
        base: String,
        seen: mpsc::Receiver<Vec<(String, String)>>,
    }

    type Canned = (u16, String);

    fn spawn(responses: Vec<Canned>) -> TestServer {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr());
        let (tx, seen) = mpsc::channel();

        std::thread::spawn(move || {
            for (status, body) in responses {
                let request = match server.recv() {
                    Ok(r) => r,
                    Err(_) => return,
                };
                let headers: Vec<(String, String)> = request
                    .headers()
                    .iter()
                    .map(|h| (h.field.as_str().as_str().to_lowercase(), h.value.as_str().to_string()))
                    .collect();
                let _ = tx.send(headers);

                let response = Response::from_string(body)
                    .with_status_code(StatusCode(status))
                    .with_header(
                        Header::from_bytes("Content-Type", "application/json").unwrap(),
                    );
                let _ = request.respond(response);
            }
        });

        TestServer { base, seen }
    }

    fn completion(content: &str) -> String {
        json!({
            "choices": [{ "message": { "role": "assistant", "content": content } }]
        })
        .to_string()
    }

    #[tokio::test]
    async fn fetch_models_parses_data_ids() {
        let s = spawn(vec![(
            200,
            json!({ "data": [{ "id": "acme-large" }, { "id": "acme-small" }, { "object": "x" }] })
                .to_string(),
        )]);

        let ids = fetch_models(&s.base, "sk-test-key-123456", HeaderStyle::Bearer)
            .await
            .unwrap();
        assert_eq!(ids, vec!["acme-large", "acme-small"]);
    }

    #[tokio::test]
    async fn test_model_reports_ok_and_latency() {
        let s = spawn(vec![(200, completion("ok"))]);

        let result = test_model(&s.base, "sk-test-key-123456", HeaderStyle::Bearer, "m").await;
        assert!(result.ok);
        assert_eq!(result.reply.as_deref(), Some("ok"));
        assert_eq!(result.status, Some(200));
        assert!(result.error.is_none());
        assert!(result.latency_ms > 0);
        assert_eq!(result.used_header, HeaderStyle::Bearer);
    }

    #[tokio::test]
    async fn test_model_retries_with_custom_header_on_401() {
        let unauthorized = json!({ "error": { "message": "bad bearer" } }).to_string();
        let s = spawn(vec![
            (401, unauthorized),
            (200, completion("ok")),
        ]);

        let result = test_model(&s.base, "sk-test-key-123456", HeaderStyle::Bearer, "m").await;
        assert!(result.ok, "expected the retry to succeed: {result:?}");
        assert_eq!(result.used_header, HeaderStyle::Custom);

        let first = s.seen.recv().unwrap();
        let second = s.seen.recv().unwrap();
        assert!(first.iter().any(|(k, _)| k == "authorization"));
        assert!(second.iter().any(|(k, _)| k == "x-api-key"));
        assert!(!second.iter().any(|(k, _)| k == "authorization"));
    }

    #[tokio::test]
    async fn test_model_reports_reasoning_only_reply() {
        let body = json!({
            "choices": [{
                "message": { "role": "assistant", "content": "", "reasoning_content": "thinking..." }
            }]
        })
        .to_string();
        let s = spawn(vec![(200, body)]);

        let result = test_model(&s.base, "sk-test-key-123456", HeaderStyle::Bearer, "m").await;
        assert!(result.ok);
        assert!(result.reply.is_none());
        assert_eq!(result.error.as_deref(), Some(REASONING_ONLY_ERROR));
    }

    #[tokio::test]
    async fn test_model_error_never_contains_the_key() {
        let key = "sk-verysecret-abcdefghijkl";
        let body = json!({ "error": { "message": format!("invalid key {key} supplied") } }).to_string();
        let s = spawn(vec![(500, body)]);

        let result = test_model(&s.base, key, HeaderStyle::Bearer, "m").await;
        assert!(!result.ok);
        let error = result.error.unwrap();
        assert!(!error.contains(key), "key leaked into the error: {error}");
        assert!(error.contains("[redacted]"));
    }

    #[tokio::test]
    async fn test_model_reports_http_status_on_failure() {
        let s = spawn(vec![(404, json!({ "error": { "message": "no such model" } }).to_string())]);

        let result = test_model(&s.base, "sk-test-key-123456", HeaderStyle::Bearer, "m").await;
        assert!(!result.ok);
        assert_eq!(result.status, Some(404));
        assert!(result.reply.is_none());
    }

    #[test]
    fn redaction_strips_only_the_key() {
        assert_eq!(redact("bad sk-abc here", "sk-abc"), "bad [redacted] here");
        assert_eq!(redact("no secret", ""), "no secret");
    }

    #[test]
    fn probe_budget_is_at_least_512_tokens() {
        assert!(PROBE_MAX_TOKENS >= 512);
    }
}
