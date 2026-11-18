//! Refine backend: any endpoint that speaks the OpenAI
//! chat-completions API (OpenAI itself, or another provider, a
//! gateway, a local server such as Ollama or LM Studio).
//!
//! One `POST {base_url}/chat/completions` per dictation with two
//! messages: the composed system prompt and the transcript as the user
//! message. The URL and model come from `settings.json`; the API key
//! comes from the OS keychain (see [`super::secrets`]) and is only
//! ever held in memory here.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{BackendState, BackendStatus, CallUsage, Prompt, RefineError, Refiner, secrets};
use crate::settings::OpenAiSettings;

/// Backend id.
pub const ID: &str = "openai-compatible";

/// An OpenAI-compatible endpoint as a [`Refiner`].
pub struct OpenAiCompat {
    settings: OpenAiSettings,
    api_key: Option<String>,
    last_usage: Option<CallUsage>,
}

impl OpenAiCompat {
    /// `api_key` may be `None`: local endpoints usually need none.
    pub fn new(settings: OpenAiSettings, api_key: Option<String>) -> Self {
        Self {
            settings,
            api_key: api_key.filter(|k| !k.trim().is_empty()),
            last_usage: None,
        }
    }

    fn post(&self, body: &Value, timeout: Duration) -> Result<String, CallError> {
        let agent = ureq::AgentBuilder::new().timeout(timeout).build();
        let mut request = agent
            .post(&endpoint_url(&self.settings.base_url))
            .set("Content-Type", "application/json");
        if let Some(key) = &self.api_key {
            request = request.set("Authorization", &format!("Bearer {}", key.trim()));
        }
        match request.send_string(&body.to_string()) {
            Ok(response) => response
                .into_string()
                .map_err(|e| CallError::Other(RefineError::Backend(format!("could not read the answer: {e}")))),
            Err(ureq::Error::Status(code, response)) => {
                let text = response.into_string().unwrap_or_default();
                Err(CallError::Http(code, error_message(&text)))
            }
            Err(ureq::Error::Transport(t)) => Err(CallError::Other(transport_error(&t, &self.settings))),
        }
    }
}

/// Optional request fields a server may refuse.
const OPTIONAL_PARAMS: &[&str] = &["temperature", "reasoning_effort"];

/// `(endpoint, model, param)` combinations a server has refused in
/// this process. Remembered so that only the first dictation pays for
/// the extra round trip.
static REFUSED: Mutex<Option<HashSet<(String, String, &'static str)>>> = Mutex::new(None);

pub(crate) fn is_refused(url: &str, model: &str, param: &'static str) -> bool {
    REFUSED.lock().ok().is_some_and(|g| {
        g.as_ref()
            .is_some_and(|set| set.contains(&(url.to_string(), model.to_string(), param)))
    })
}

pub(crate) fn remember_refused(url: &str, model: &str, param: &'static str) {
    if let Ok(mut g) = REFUSED.lock() {
        g.get_or_insert_with(HashSet::new)
            .insert((url.to_string(), model.to_string(), param));
    }
}

/// Which optional field a 400 complains about, if any.
pub(crate) fn refused_param(message: &str) -> Option<&'static str> {
    let lower = message.to_lowercase();
    if lower.contains("reasoning_effort") || lower.contains("reasoning effort") {
        Some("reasoning_effort")
    } else if lower.contains("temperature") {
        Some("temperature")
    } else {
        None
    }
}

/// How one HTTP call failed. Shared with Ask, which posts to the same
/// endpoint and reports its errors in the same words.
pub(crate) enum CallError {
    Http(u16, String),
    Other(RefineError),
}

impl Refiner for OpenAiCompat {
    fn id(&self) -> &'static str {
        ID
    }

    fn last_usage(&self) -> Option<CallUsage> {
        self.last_usage.clone()
    }

    fn refine(
        &mut self,
        prompt: &Prompt,
        transcript: &str,
        timeout: Duration,
    ) -> Result<String, RefineError> {
        if let Some(problem) = config_problem(&self.settings) {
            return Err(RefineError::Unavailable(problem));
        }
        let started = Instant::now();
        let url = endpoint_url(&self.settings.base_url);
        let model = self.settings.model.clone();
        let mut body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": prompt.system() },
                { "role": "user", "content": prompt.user_message(transcript) },
            ],
            "stream": false,
        });
        if let Some(map) = body.as_object_mut() {
            // Cleanup, not creative writing.
            if !is_refused(&url, &model, "temperature") {
                map.insert("temperature".into(), json!(0));
            }
            if let Some(effort) = self.settings.effort {
                if !is_refused(&url, &model, "reasoning_effort") {
                    map.insert("reasoning_effort".into(), json!(effort.as_str()));
                }
            }
        }

        // Some models take only their default temperature, many take
        // no reasoning effort. When a server refuses one of those
        // optional fields, drop it and ask again rather than failing
        // every dictation, and remember it for the next one.
        let mut attempts_left = OPTIONAL_PARAMS.len() + 1;
        let answer = loop {
            attempts_left -= 1;
            let left = timeout.saturating_sub(started.elapsed());
            if left.is_zero() {
                return Err(RefineError::Timeout);
            }
            match self.post(&body, left) {
                Ok(text) => break text,
                Err(CallError::Http(400, message)) if attempts_left > 0 => {
                    let dropped = refused_param(&message).filter(|param| {
                        body.as_object_mut()
                            .is_some_and(|map| map.remove(*param).is_some())
                    });
                    match dropped {
                        Some(param) => remember_refused(&url, &model, param),
                        None => {
                            return Err(CallError::Http(400, message).into_refine(&self.settings));
                        }
                    }
                }
                Err(e) => return Err(e.into_refine(&self.settings)),
            }
        };
        self.last_usage = parse_usage(&answer);
        extract_content(&answer)
    }
}

impl CallError {
    pub(crate) fn into_refine(self, settings: &OpenAiSettings) -> RefineError {
        match self {
            Self::Other(e) => e,
            Self::Http(401 | 403, message) => RefineError::Unavailable(format!(
                "{} refused the API key ({message}). Check the key in settings.",
                host(&settings.base_url)
            )),
            Self::Http(404, message) => RefineError::Backend(format!(
                "{} answered 404 ({message}). Check the URL and the model name.",
                host(&settings.base_url)
            )),
            Self::Http(code, message) => {
                RefineError::Backend(format!("{} answered {code}: {message}", host(&settings.base_url)))
            }
        }
    }
}

pub(crate) fn transport_error(t: &ureq::Transport, settings: &OpenAiSettings) -> RefineError {
    let text = t.to_string();
    let lower = text.to_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        return RefineError::Timeout;
    }
    match t.kind() {
        ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed => RefineError::Unavailable(format!(
            "Could not reach {}. Check the URL and your connection.",
            host(&settings.base_url)
        )),
        ureq::ErrorKind::InvalidUrl | ureq::ErrorKind::UnknownScheme => {
            RefineError::Unavailable(format!("\"{}\" is not a valid URL.", settings.base_url))
        }
        _ => RefineError::Backend(format!("{}: {text}", host(&settings.base_url))),
    }
}

/// Full chat-completions URL for a base URL. Accepts a base
/// (`https://api.openai.com/v1`) or the full endpoint.
pub fn endpoint_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

/// Host part of a URL, for messages ("api.openai.com").
pub fn host(base_url: &str) -> String {
    let rest = base_url.trim();
    let rest = rest.split_once("://").map_or(rest, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    // Drop credentials in the URL, if someone put them there.
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    if host.is_empty() {
        "the endpoint".to_string()
    } else {
        host.to_string()
    }
}

/// Where the text goes with this backend. Shown in settings.
pub fn destination(settings: &OpenAiSettings) -> String {
    if settings.base_url.trim().is_empty() {
        "the server you configure below".to_string()
    } else {
        format!("the server at {}", host(&settings.base_url))
    }
}

pub(crate) fn config_problem(settings: &OpenAiSettings) -> Option<String> {
    let url = settings.base_url.trim();
    if url.is_empty() {
        return Some("No endpoint URL is set.".to_string());
    }
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Some(format!("\"{url}\" is not a valid URL. It must start with https:// or http://."));
    }
    if settings.model.trim().is_empty() {
        return Some("No model name is set.".to_string());
    }
    None
}

/// Configuration check only; no network call.
pub fn status(settings: &OpenAiSettings) -> BackendStatus {
    let has_api_key = secrets::has_api_key();
    let problem = config_problem(settings);
    let ready = problem.is_none();
    BackendStatus {
        backend: ID.to_string(),
        state: if ready {
            BackendState::Ready
        } else {
            BackendState::NotConfigured
        },
        ready,
        message: problem.unwrap_or_else(|| {
            if has_api_key {
                format!("Set up for {}. Use the connection test to check it.", settings.model)
            } else {
                format!(
                    "Set up for {} without an API key (fine for a local server). \
Use the connection test to check it.",
                    settings.model
                )
            }
        }),
        destination: destination(settings),
        path: None,
        version: None,
        auth_method: None,
        plan: None,
        has_api_key,
    }
}

/// Token counts from the response's `usage` block, when the server
/// sends one. No cost: prices are the provider's business.
pub(crate) fn parse_usage(body: &str) -> Option<CallUsage> {
    let v: Value = serde_json::from_str(body).ok()?;
    let usage = v.get("usage")?;
    let tokens_in = usage["prompt_tokens"].as_u64()?;
    Some(CallUsage {
        tokens_in,
        tokens_cached: usage["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap_or(0),
        tokens_out: usage["completion_tokens"].as_u64().unwrap_or(0),
        cost_usd: None,
        plan: None,
    })
}

/// `choices[0].message.content`, as a string or as a list of text
/// parts.
pub(crate) fn extract_content(body: &str) -> Result<String, RefineError> {
    let v: Value = serde_json::from_str(body)
        .map_err(|_| RefineError::Backend("the endpoint did not answer with JSON".into()))?;
    let content = &v["choices"][0]["message"]["content"];
    if let Some(text) = content.as_str() {
        return Ok(text.to_string());
    }
    if let Some(parts) = content.as_array() {
        let text: String = parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("");
        if !text.is_empty() {
            return Ok(text);
        }
    }
    let reason = error_message(body);
    Err(RefineError::Backend(if reason.is_empty() {
        "the endpoint answered without any text".to_string()
    } else {
        reason
    }))
}

/// The human part of an error body: `error.message`, `error` as a
/// string, `message`, or the start of the raw body.
pub(crate) fn error_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        for candidate in [&v["error"]["message"], &v["error"], &v["message"]] {
            if let Some(text) = candidate.as_str() {
                return cap(text);
            }
        }
        if v.get("choices").is_some() {
            return String::new();
        }
    }
    cap(body)
}

fn cap(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut out: String = line.chars().take(200).collect();
    if line.chars().count() > 200 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver};
    use std::thread;

    /// What the mock server received.
    struct Seen {
        path: String,
        headers: String,
        body: Value,
    }

    /// One-thread HTTP server answering each request with the next
    /// canned `(status, body, delay)`.
    fn serve(responses: Vec<(u16, String, Duration)>) -> (String, Receiver<Seen>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for (status, body, delay) in responses {
                let Ok((mut stream, _)) = listener.accept() else { return };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                let mut headers = String::new();
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                    headers.push_str(&line);
                }
                let mut raw = vec![0u8; length];
                reader.read_exact(&mut raw).unwrap();
                let _ = tx.send(Seen {
                    path: request_line.split_whitespace().nth(1).unwrap_or("").to_string(),
                    headers,
                    body: serde_json::from_slice(&raw).unwrap_or(Value::Null),
                });
                thread::sleep(delay);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (url, rx)
    }

    fn ok_body(text: &str) -> String {
        json!({ "choices": [{ "message": { "role": "assistant", "content": text } }] }).to_string()
    }

    fn backend(url: &str, key: Option<&str>) -> OpenAiCompat {
        OpenAiCompat::new(
            OpenAiSettings {
                base_url: url.to_string(),
                model: "test-model".into(),
                effort: None,
            },
            key.map(str::to_string),
        )
    }

    const NO_DELAY: Duration = Duration::from_millis(0);

    #[test]
    fn sends_model_system_user_and_bearer_key() {
        let (url, seen) = serve(vec![(200, ok_body("Hello there."), NO_DELAY)]);
        let prompt = Prompt::compose("", &["PayProff".to_string()]);
        let out = backend(&url, Some("sk-test-123"))
            .refine(&prompt, "hello there", Duration::from_secs(5))
            .unwrap();
        assert_eq!(out, "Hello there.");

        let seen = seen.recv().unwrap();
        assert_eq!(seen.path, "/v1/chat/completions");
        assert!(
            seen.headers.to_lowercase().contains("authorization: bearer sk-test-123"),
            "{}",
            seen.headers
        );
        assert_eq!(seen.body["model"], "test-model");
        assert_eq!(seen.body["messages"][0]["role"], "system");
        assert_eq!(seen.body["messages"][0]["content"], prompt.system());
        assert_eq!(seen.body["messages"][1]["role"], "user");
        assert_eq!(
            seen.body["messages"][1]["content"],
            "<transcript>\nhello there\n</transcript>"
        );
        assert_eq!(seen.body["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn reports_token_counts_when_the_server_sends_them() {
        let body = json!({
            "choices": [{ "message": { "content": "Hi there." } }],
            "usage": { "prompt_tokens": 512, "completion_tokens": 9,
                       "prompt_tokens_details": { "cached_tokens": 384 } },
        })
        .to_string();
        let (url, _seen) = serve(vec![(200, body, NO_DELAY), (200, ok_body("Hi."), NO_DELAY)]);
        let mut b = backend(&url, None);
        b.refine(&Prompt::compose("", &[]), "hi there", Duration::from_secs(5)).unwrap();
        let usage = b.last_usage().unwrap();
        assert_eq!((usage.tokens_in, usage.tokens_cached, usage.tokens_out), (512, 384, 9));
        assert_eq!(usage.cost_usd, None);
        assert_eq!(usage.plan, None);
        // A server that sends no usage block: no numbers, no error.
        b.refine(&Prompt::compose("", &[]), "hi", Duration::from_secs(5)).unwrap();
        assert_eq!(b.last_usage(), None);
    }

    #[test]
    fn no_key_means_no_authorization_header() {
        let (url, seen) = serve(vec![(200, ok_body("Hi."), NO_DELAY)]);
        backend(&url, None)
            .refine(&Prompt::compose("", &[]), "hi there", Duration::from_secs(5))
            .unwrap();
        assert!(!seen.recv().unwrap().headers.to_lowercase().contains("authorization"));
    }

    #[test]
    fn content_as_parts_is_joined() {
        let body = json!({ "choices": [{ "message": { "content": [
            { "type": "text", "text": "Hello " }, { "type": "text", "text": "there." }
        ] } }] })
        .to_string();
        let (url, _seen) = serve(vec![(200, body, NO_DELAY)]);
        let out = backend(&url, None)
            .refine(&Prompt::compose("", &[]), "hello there", Duration::from_secs(5))
            .unwrap();
        assert_eq!(out, "Hello there.");
    }

    #[test]
    fn rejected_key_is_unavailable_and_names_the_host() {
        let body = json!({ "error": { "message": "Incorrect API key provided" } }).to_string();
        let (url, _seen) = serve(vec![(401, body, NO_DELAY)]);
        let err = backend(&url, Some("bad"))
            .refine(&Prompt::compose("", &[]), "hello there", Duration::from_secs(5))
            .unwrap_err();
        match err {
            RefineError::Unavailable(m) => {
                assert!(m.contains("127.0.0.1"), "{m}");
                assert!(m.contains("Incorrect API key provided"), "{m}");
                // The key itself never appears in a message.
                assert!(!m.contains("bad\""), "{m}");
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn server_error_is_a_backend_error() {
        let (url, _seen) = serve(vec![(500, "{\"error\":\"boom\"}".into(), NO_DELAY)]);
        let err = backend(&url, None)
            .refine(&Prompt::compose("", &[]), "hello there", Duration::from_secs(5))
            .unwrap_err();
        match err {
            RefineError::Backend(m) => assert!(m.contains("500") && m.contains("boom"), "{m}"),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn slow_server_times_out() {
        let (url, _seen) = serve(vec![(200, ok_body("late"), Duration::from_millis(1500))]);
        let started = Instant::now();
        let err = backend(&url, None)
            .refine(&Prompt::compose("", &[]), "hello there", Duration::from_millis(300))
            .unwrap_err();
        assert_eq!(err, RefineError::Timeout);
        assert!(started.elapsed() < Duration::from_millis(1400));
    }

    #[test]
    fn malformed_answer_is_a_backend_error() {
        let (url, _seen) = serve(vec![(200, "<html>gateway</html>".into(), NO_DELAY)]);
        let err = backend(&url, None)
            .refine(&Prompt::compose("", &[]), "hello there", Duration::from_secs(5))
            .unwrap_err();
        assert!(matches!(err, RefineError::Backend(_)), "{err:?}");
    }

    #[test]
    fn retries_once_without_temperature_when_the_model_rejects_it() {
        let rejected = json!({ "error": { "message":
            "Unsupported value: 'temperature' does not support 0 with this model." } })
        .to_string();
        let (url, seen) = serve(vec![(400, rejected, NO_DELAY), (200, ok_body("Hi there."), NO_DELAY)]);
        let out = backend(&url, None)
            .refine(&Prompt::compose("", &[]), "hi there", Duration::from_secs(5))
            .unwrap();
        assert_eq!(out, "Hi there.");
        assert_eq!(seen.recv().unwrap().body["temperature"], 0);
        assert!(seen.recv().unwrap().body.get("temperature").is_none());
    }

    #[test]
    fn effort_is_sent_only_when_set() {
        let (url, seen) = serve(vec![
            (200, ok_body("Hi there."), NO_DELAY),
            (200, ok_body("Hi there."), NO_DELAY),
        ]);
        backend(&url, None)
            .refine(&Prompt::compose("", &[]), "hi there", Duration::from_secs(5))
            .unwrap();
        assert!(seen.recv().unwrap().body.get("reasoning_effort").is_none());

        let mut with_effort = OpenAiCompat::new(
            OpenAiSettings {
                base_url: url.clone(),
                model: "effort-model".into(),
                effort: Some(crate::settings::RefineEffort::Low),
            },
            None,
        );
        with_effort
            .refine(&Prompt::compose("", &[]), "hi there", Duration::from_secs(5))
            .unwrap();
        assert_eq!(seen.recv().unwrap().body["reasoning_effort"], "low");
    }

    #[test]
    fn refused_effort_is_dropped_and_remembered() {
        let refused = json!({ "error": { "message":
            "Unrecognized request argument supplied: reasoning_effort" } })
        .to_string();
        let (url, seen) = serve(vec![
            (400, refused, NO_DELAY),
            (200, ok_body("Hi there."), NO_DELAY),
            (200, ok_body("Hi again."), NO_DELAY),
        ]);
        let settings = OpenAiSettings {
            base_url: url,
            model: "no-effort-model".into(),
            effort: Some(crate::settings::RefineEffort::Medium),
        };
        let out = OpenAiCompat::new(settings.clone(), None)
            .refine(&Prompt::compose("", &[]), "hi there", Duration::from_secs(5))
            .unwrap();
        assert_eq!(out, "Hi there.");
        assert_eq!(seen.recv().unwrap().body["reasoning_effort"], "medium");
        assert!(seen.recv().unwrap().body.get("reasoning_effort").is_none());

        // The next dictation does not pay for the refusal again.
        let out = OpenAiCompat::new(settings, None)
            .refine(&Prompt::compose("", &[]), "hi again", Duration::from_secs(5))
            .unwrap();
        assert_eq!(out, "Hi again.");
        assert!(seen.recv().unwrap().body.get("reasoning_effort").is_none());
    }

    #[test]
    fn unreachable_server_is_unavailable() {
        // Bind then drop: the port is closed.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let err = backend(&format!("http://127.0.0.1:{port}/v1"), None)
            .refine(&Prompt::compose("", &[]), "hello there", Duration::from_secs(2))
            .unwrap_err();
        assert!(matches!(err, RefineError::Unavailable(_)), "{err:?}");
    }

    #[test]
    fn unconfigured_backend_makes_no_request() {
        let mut no_model = OpenAiCompat::new(
            OpenAiSettings {
                base_url: "https://api.openai.com/v1".into(),
                model: String::new(),
                effort: None,
            },
            None,
        );
        assert_eq!(
            no_model.refine(&Prompt::compose("", &[]), "hello there", Duration::from_secs(1)),
            Err(RefineError::Unavailable("No model name is set.".into()))
        );
        let mut bad_url = OpenAiCompat::new(
            OpenAiSettings {
                base_url: "localhost:11434".into(),
                model: "m".into(),
                effort: None,
            },
            None,
        );
        assert!(matches!(
            bad_url.refine(&Prompt::compose("", &[]), "hello there", Duration::from_secs(1)),
            Err(RefineError::Unavailable(_))
        ));
    }

    #[test]
    fn endpoint_url_accepts_base_or_full() {
        assert_eq!(
            endpoint_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint_url("http://localhost:11434/v1/chat/completions"),
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn host_is_extracted_for_messages() {
        assert_eq!(host("https://api.openai.com/v1"), "api.openai.com");
        assert_eq!(host("http://localhost:11434/v1"), "localhost:11434");
        assert_eq!(host("https://user:pw@example.com/x"), "example.com");
        assert_eq!(host(""), "the endpoint");
        assert_eq!(
            destination(&OpenAiSettings {
                base_url: "https://api.openai.com/v1".into(),
                model: String::new(),
                effort: None,
            }),
            "the server at api.openai.com"
        );
    }
}
