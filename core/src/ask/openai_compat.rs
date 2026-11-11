//! Ask backend: any endpoint that speaks the OpenAI chat-completions
//! API. The same URL, model and key as Refine's backend
//! ([`crate::refine::openai_compat`]), and the same error messages.
//!
//! What differs from Refine's one request per dictation:
//!
//! - **Streaming.** `stream: true`, read as server-sent events over
//!   the same `ureq` client: `data:` lines carrying
//!   `choices[0].delta.content`, ended by `data: [DONE]`. With
//!   `stream_options.include_usage` the last chunk carries the token
//!   counts. A server that ignores `stream` and answers with one JSON
//!   body is read as that.
//! - **Timeouts.** A connect timeout and a limit on how long the
//!   server may stay silent, instead of one deadline for the whole
//!   call: a local model writing a long answer at ten tokens a second
//!   is slow, not stuck. Every line the server sends counts as a sign
//!   of life, not only text: a model that streams its reasoning for a
//!   minute before the first word of the answer is working too, and
//!   so is a server that sends keep-alive comments.
//! - **Cancel.** The request runs on its own thread and hands what it
//!   reads over a channel, so a cancel is noticed within
//!   milliseconds, also before the first byte. A cancel or a timeout
//!   then closes the connection ([`Wire::cut`]), whatever the server
//!   is doing: the server sees it at once and stops working on an
//!   answer nobody will read.
//! - **A cut-off answer is not an answer.** The stream ends properly
//!   with `data: [DONE]`, or with a chunk carrying a `finish_reason`
//!   for the servers that never send `[DONE]`. A connection that
//!   closes before either, after some text, is an error; the text
//!   that arrived stays on show.
//! - **A conversation.** The messages so far are kept, so a further
//!   [`AskBackend::send`] continues it.
//! - **No web search.** There is no portable one in this API. When it
//!   is asked for, the question is answered without it and the reply
//!   says so ([`super::WEB_SEARCH_UNAVAILABLE`]).
//!
//! Verified against a local fake server only (2026-11-10): no real
//! endpoint was at hand.

use std::io::{self, BufRead, BufReader};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{Value, json};

use super::{
    AskBackend, AskError, AskEvent, AskInput, AskReply, AskSetup, CancelFlag, POLL, Turn, WEB_SEARCH_UNAVAILABLE,
};
use crate::refine::CallUsage;
use crate::refine::openai_compat::{self as refine_api, CallError};
use crate::settings::OpenAiSettings;

/// Backend id: the same backend as Refine's.
pub const ID: &str = refine_api::ID;

/// Longest wait for the TCP and TLS handshake.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Optional request fields a server may refuse; each is dropped and
/// the request sent again, once.
const OPTIONAL_PARAMS: &[&str] = &["stream_options", "reasoning_effort"];

/// An OpenAI-compatible endpoint as an [`AskBackend`].
pub struct OpenAiAsk {
    /// URL only: the model and effort of an ask come with the
    /// [`AskSetup`].
    settings: OpenAiSettings,
    api_key: Option<String>,
    /// The conversation so far, system message first. Empty before
    /// the first turn.
    messages: Vec<Value>,
    /// The setup `messages` belongs to.
    setup: Option<AskSetup>,
}

impl OpenAiAsk {
    /// `api_key` may be `None`: local endpoints usually need none.
    pub fn new(settings: OpenAiSettings, api_key: Option<String>) -> Self {
        Self {
            settings,
            api_key: api_key.filter(|k| !k.trim().is_empty()),
            messages: Vec::new(),
            setup: None,
        }
    }
}

/// What stands in a kept conversation where an image was, once the
/// request that carried it has returned ([`forget_images`]).
pub const IMAGE_NOT_KEPT: &str = "(A screenshot was attached to this message when it was sent. It is not kept.)";

/// Take the images out of a conversation: every image part is wiped
/// (its `data:` URL holds the whole image as text) and replaced by a
/// line that says an image was there.
///
/// Called as soon as the request that carried the image has returned.
/// The model has seen the image by then and its answer is in the
/// conversation; a later turn of the same conversation is sent
/// without the image and with that line in its place. An ask has at
/// most one turn after the one with the screenshot, and only when
/// the model asked for a screen it had already been sent.
fn forget_images(messages: &mut [Value]) {
    for message in messages {
        let Some(parts) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        for part in parts.iter_mut().filter(|part| part["type"] == "image_url") {
            super::wipe::json(part);
            *part = json!({ "type": "text", "text": IMAGE_NOT_KEPT });
        }
    }
}

/// The user message of one turn: plain text, or text and images as
/// content parts (each image a `data:` URL).
///
/// With images, the returned value holds each of them as base64 text.
/// [`OpenAiAsk::send`] keeps it in the conversation only until the
/// request has returned ([`forget_images`]).
pub fn user_message(input: &AskInput) -> Value {
    if input.images.is_empty() {
        return json!({ "role": "user", "content": input.text });
    }
    let mut parts = vec![json!({ "type": "text", "text": input.text })];
    for image in &input.images {
        let data = base64::engine::general_purpose::STANDARD.encode(&image.data);
        parts.push(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:{};base64,{data}", image.media_type) },
        }));
    }
    json!({ "role": "user", "content": parts })
}

/// What the request thread reports. Every message is a sign of life
/// from the server.
enum Msg {
    /// A piece of the answer.
    Delta(String),
    Usage(CallUsage),
    /// A line with nothing for the answer in it: reasoning, a
    /// keep-alive comment, a role chunk.
    Alive,
    /// The stream ended properly.
    Done,
    Failed(AskError),
}

impl AskBackend for OpenAiAsk {
    fn id(&self) -> &'static str {
        ID
    }

    /// Nothing to start ahead: the connection is made with the
    /// request.
    fn warm(&mut self, _setup: &AskSetup) {}

    fn send(
        &mut self,
        setup: &AskSetup,
        input: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
        timeout: Duration,
    ) -> Result<AskReply, AskError> {
        let settings = OpenAiSettings { model: setup.model.clone(), effort: setup.effort, ..self.settings.clone() };
        if let Some(problem) = refine_api::config_problem(&settings) {
            return Err(AskError::Unavailable(problem));
        }
        let started = Instant::now();
        // A conversation belongs to one setup.
        if self.setup.as_ref() != Some(setup) {
            self.messages = vec![json!({ "role": "system", "content": setup.system })];
            self.setup = Some(setup.clone());
        }
        self.messages.push(user_message(input));

        let url = refine_api::endpoint_url(&settings.base_url);
        let mut body = json!({
            "model": settings.model,
            "messages": self.messages,
            "stream": true,
        });
        if let Some(map) = body.as_object_mut() {
            if !refine_api::is_refused(&url, &settings.model, "stream_options") {
                map.insert("stream_options".into(), json!({ "include_usage": true }));
            }
            if let Some(effort) = settings.effort
                && !refine_api::is_refused(&url, &settings.model, "reasoning_effort")
            {
                map.insert("reasoning_effort".into(), json!(effort.as_str()));
            }
        }

        let (tx, rx) = mpsc::channel();
        let wire = Wire::default();
        {
            let settings = settings.clone();
            let api_key = self.api_key.clone();
            let wire = wire.clone();
            // The body is a copy of the conversation, image included.
            // The request thread owns it and wipes it when the
            // request has been sent, or when it gives up.
            let body = super::wipe::Json(body);
            thread::spawn(move || request(&settings, api_key.as_deref(), body, timeout, &wire, &tx));
        }

        let mut turn = Turn::new(sink, started);
        let mut usage = None;
        let mut last_sign = Instant::now();
        let result = loop {
            if cancel.is_cancelled() {
                break Err(AskError::Cancelled);
            }
            if last_sign.elapsed() >= timeout {
                break Err(AskError::Timeout);
            }
            let msg = match rx.recv_timeout(POLL) {
                Ok(msg) => msg,
                Err(RecvTimeoutError::Timeout) => continue,
                // The connection closed before the stream said it was
                // over. Text that arrived is on show already, but it
                // is not the whole answer.
                Err(RecvTimeoutError::Disconnected) if turn.has_text() => {
                    break Err(AskError::Backend(format!(
                        "The answer was cut off: {} closed the connection before the end.",
                        refine_api::host(&settings.base_url)
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    break Err(AskError::Backend(format!(
                        "{} closed the connection without an answer",
                        refine_api::host(&settings.base_url)
                    )));
                }
            };
            // Whatever it says, the server is alive.
            last_sign = Instant::now();
            match msg {
                Msg::Delta(text) => turn.text(&text),
                Msg::Usage(u) => usage = Some(u),
                Msg::Alive => {}
                Msg::Done => break Ok(()),
                Msg::Failed(e) => break Err(e),
            }
        };
        // Close the connection, whichever way the turn ended: after a
        // cancel or a timeout this is what tells the server to stop,
        // and what ends the request thread's blocked read.
        wire.cut();
        drop(rx);
        // The request that carried the image has returned, whichever
        // way: the image leaves the conversation now. What is left of
        // it is the copy inside the HTTP client, which ends with the
        // connection that was just closed.
        forget_images(&mut self.messages);
        if let Err(e) = result {
            // The turn did not happen: a retry must not find the
            // question in the conversation twice.
            self.messages.pop();
            return Err(e);
        }
        let notice = if setup.web_search { WEB_SEARCH_UNAVAILABLE } else { "" };
        let reply = turn.finish(usage, None, notice);
        self.messages.push(json!({ "role": "assistant", "content": reply.text }));
        Ok(reply)
    }
}

/// Post `body` and report the stream on `tx`. Runs on its own thread
/// and ends when the stream does, on an error, as soon as nobody
/// receives any more, or when `wire` is cut (which ends a read that
/// is waiting for the server).
fn request(
    settings: &OpenAiSettings,
    api_key: Option<&str>,
    mut body: super::wipe::Json,
    silence: Duration,
    wire: &Wire,
    tx: &Sender<Msg>,
) {
    let url = refine_api::endpoint_url(&settings.base_url);
    let connect_timeout = CONNECT_TIMEOUT.min(silence);
    let agent = {
        let wire = wire.clone();
        ureq::AgentBuilder::new()
            // Every connection goes over the wire, so it can be cut.
            .resolver(move |netloc: &str| wire.route(netloc, connect_timeout))
            .timeout_connect(connect_timeout)
            // The caller gives up first and cuts the wire; this only
            // makes sure the thread cannot outlive a dead connection.
            .timeout_read(silence + Duration::from_secs(1))
            .timeout_write(CONNECT_TIMEOUT)
            .build()
    };
    let failed = |e: CallError| Msg::Failed(e.into_refine(settings).into());

    // A server may refuse an optional field. Drop it and ask again
    // rather than failing every ask, and remember it for the next one.
    let mut attempts_left = OPTIONAL_PARAMS.len() + 1;
    let response = loop {
        attempts_left -= 1;
        let mut request = agent
            .post(&url)
            .set("Content-Type", "application/json")
            .set("Accept", "text/event-stream");
        if let Some(key) = api_key {
            request = request.set("Authorization", &format!("Bearer {}", key.trim()));
        }
        // The request as text: one more copy of an image in it, wiped
        // as soon as the client has sent it.
        let text = super::wipe::Text(body.0.to_string());
        let sent = request.send_string(&text.0);
        drop(text);
        match sent {
            Ok(response) => break response,
            Err(ureq::Error::Status(400, response)) if attempts_left > 0 => {
                let message = refine_api::error_message(&response.into_string().unwrap_or_default());
                let dropped = refused_param(&message)
                    .filter(|param| body.0.as_object_mut().is_some_and(|map| map.remove(*param).is_some()));
                match dropped {
                    Some(param) => refine_api::remember_refused(&url, &settings.model, param),
                    None => {
                        let _ = tx.send(failed(CallError::Http(400, message)));
                        return;
                    }
                }
            }
            Err(ureq::Error::Status(code, response)) => {
                let message = refine_api::error_message(&response.into_string().unwrap_or_default());
                let _ = tx.send(failed(CallError::Http(code, message)));
                return;
            }
            Err(ureq::Error::Transport(t)) => {
                let _ = tx.send(failed(CallError::Other(refine_api::transport_error(&t, settings))));
                return;
            }
        }
    };

    // The request is out: this thread's copy of the conversation, and
    // of an image in it, is not needed while the answer streams.
    drop(body);

    // Lines that are not events, in case the server answered with a
    // plain JSON body instead of a stream.
    let mut plain = String::new();
    let mut streamed = false;
    // The model said it was done (`finish_reason`).
    let mut finished = false;
    for line in BufReader::new(response.into_reader()).lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                let lower = e.to_string().to_lowercase();
                let error = if lower.contains("timed out") || lower.contains("timeout") {
                    AskError::Timeout
                } else {
                    AskError::Backend(format!("{}: {e}", refine_api::host(&settings.base_url)))
                };
                let _ = tx.send(Msg::Failed(error));
                return;
            }
        };
        // Every line is reported, also the ones with nothing for the
        // answer in them: the caller counts them as signs of life,
        // and a send that fails is how this thread learns that the
        // ask was cancelled or timed out.
        let sent = match read_event(&line) {
            Event::Delta(text) => {
                streamed = true;
                tx.send(Msg::Delta(text))
            }
            Event::Finish(text) => {
                finished = true;
                if text.is_empty() {
                    tx.send(Msg::Alive)
                } else {
                    streamed = true;
                    tx.send(Msg::Delta(text))
                }
            }
            Event::Usage(usage) => tx.send(Msg::Usage(usage)),
            Event::Done => {
                let _ = tx.send(Msg::Done);
                return;
            }
            Event::Failed(message) => {
                let _ = tx.send(Msg::Failed(AskError::Backend(format!(
                    "{}: {message}",
                    refine_api::host(&settings.base_url)
                ))));
                return;
            }
            Event::NotAnEvent => {
                plain.push_str(&line);
                plain.push('\n');
                tx.send(Msg::Alive)
            }
            Event::Nothing => tx.send(Msg::Alive),
        };
        if sent.is_err() {
            // The ask was cancelled or timed out: stop reading, which
            // closes the connection.
            return;
        }
    }
    // The connection closed without `[DONE]`.
    if finished {
        // Some servers never send it. The model said it was done, so
        // what arrived is the whole answer.
        let _ = tx.send(Msg::Done);
    } else if !streamed && !plain.trim().is_empty() {
        match refine_api::extract_content(&plain) {
            Ok(text) => {
                let _ = tx.send(Msg::Delta(text));
                if let Some(usage) = refine_api::parse_usage(&plain) {
                    let _ = tx.send(Msg::Usage(usage));
                }
                let _ = tx.send(Msg::Done);
            }
            Err(e) => {
                let _ = tx.send(Msg::Failed(e.into()));
            }
        }
    }
    // Otherwise the channel closes without `Done`, and the caller
    // reports an answer that was cut off (or none at all).
}

// --- the wire -------------------------------------------------------

/// The connections of one request, with a switch that closes them
/// from another thread.
///
/// ureq 2 keeps its socket to itself, and a read that is waiting for
/// the server cannot be interrupted: not while the response headers
/// are still to come, and not in the body either (there is one read
/// timeout for both, and its chunk decoder cannot pick up again after
/// a timeout, so a short timeout used as a poll is not an option). So
/// the request does not connect to the server itself. [`Wire::route`]
/// is its resolver: it opens the connection to the server and sends
/// ureq to a socket on loopback instead, and two threads pass the
/// bytes on unchanged in both directions. TLS is not touched: it runs
/// between ureq and the server, through the hop.
///
/// [`Wire::cut`] closes both sockets. The server sees that at once,
/// and ureq's read ends.
#[derive(Clone, Default)]
struct Wire(Arc<Mutex<WireState>>);

#[derive(Default)]
struct WireState {
    cut: bool,
    /// Every socket of every connection made over this wire.
    sockets: Vec<TcpStream>,
}

impl Wire {
    fn state(&self) -> std::sync::MutexGuard<'_, WireState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Close every connection, now and from here on.
    fn cut(&self) {
        let mut state = self.state();
        state.cut = true;
        for socket in state.sockets.drain(..) {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }

    /// Keep a handle on both sockets of a connection. False when the
    /// wire was cut meanwhile (or the sockets cannot be held): the
    /// caller drops them, which closes them.
    fn hold(&self, a: &TcpStream, b: &TcpStream) -> bool {
        let mut state = self.state();
        let (Ok(a), Ok(b)) = (a.try_clone(), b.try_clone()) else {
            return false;
        };
        if state.cut {
            return false;
        }
        state.sockets.extend([a, b]);
        true
    }

    /// The resolver of the request: connect to `netloc` (the server's
    /// `host:port`) and answer with the loopback address that leads
    /// there. Errors read like ureq's own: a name that does not
    /// resolve and a server that does not answer both come out as
    /// "could not reach".
    fn route(&self, netloc: &str, connect_timeout: Duration) -> io::Result<Vec<SocketAddr>> {
        if self.state().cut {
            return Err(io::Error::other("the ask was stopped"));
        }
        let mut server = Err(io::Error::new(io::ErrorKind::NotFound, format!("no address for {netloc}")));
        for addr in netloc.to_socket_addrs()? {
            server = TcpStream::connect_timeout(&addr, connect_timeout);
            if server.is_ok() {
                break;
            }
        }
        let server = server?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let hop = listener.local_addr()?;
        let wire = self.clone();
        thread::spawn(move || wire.pass(&listener, server));
        Ok(vec![hop])
    }

    /// Take the one connection ureq makes to `listener` and pass the
    /// bytes between it and `server` until one side closes or the
    /// wire is cut.
    fn pass(&self, listener: &TcpListener, server: TcpStream) {
        let Some(client) = accept_one(listener) else {
            return;
        };
        // Small pieces, each as soon as it is there.
        let _ = client.set_nodelay(true);
        let _ = server.set_nodelay(true);
        let (Ok(client_out), Ok(server_in)) = (client.try_clone(), server.try_clone()) else {
            return;
        };
        if !self.hold(&client, &server) {
            return;
        }
        let answers = thread::spawn(move || {
            let _ = io::copy(&mut &server_in, &mut &client_out);
            // The server is done sending: ureq reads the end.
            let _ = client_out.shutdown(Shutdown::Write);
        });
        let _ = io::copy(&mut &client, &mut &server);
        // ureq let go of its socket (or the wire was cut): the
        // conversation is over, in both directions.
        let _ = server.shutdown(Shutdown::Both);
        let _ = client.shutdown(Shutdown::Both);
        let _ = answers.join();
    }
}

/// The connection ureq makes right after [`Wire::route`] answered.
/// Does not wait for long: a connection that never comes must not
/// leave a thread behind.
fn accept_one(listener: &TcpListener) -> Option<TcpStream> {
    listener.set_nonblocking(true).ok()?;
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((client, _)) => {
                // Some systems hand the listener's mode on.
                client.set_nonblocking(false).ok()?;
                return Some(client);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(1));
            }
            Err(_) => return None,
        }
    }
}

/// Which optional field a 400 complains about, if any.
fn refused_param(message: &str) -> Option<&'static str> {
    let lower = message.to_lowercase();
    if lower.contains("stream_options") || lower.contains("include_usage") {
        Some("stream_options")
    } else {
        // Refine's list; `temperature` is never sent by an ask, so a
        // complaint about it finds nothing to drop and fails as it
        // should.
        refine_api::refused_param(message)
    }
}

/// One line of a server-sent event stream.
#[derive(Debug, PartialEq)]
enum Event {
    Delta(String),
    /// The model ended the answer in this chunk (`finish_reason`),
    /// which may carry the last piece of text.
    Finish(String),
    Usage(CallUsage),
    Done,
    /// The server reported an error inside the stream.
    Failed(String),
    /// A `data:` line with nothing for an ask, a comment, a blank.
    Nothing,
    /// Not an event line at all.
    NotAnEvent,
}

fn read_event(line: &str) -> Event {
    let line = line.trim();
    if line.is_empty() || line.starts_with(':') || line.starts_with("event:") || line.starts_with("id:") {
        return Event::Nothing;
    }
    let Some(data) = line.strip_prefix("data:") else {
        return Event::NotAnEvent;
    };
    let data = data.trim();
    if data == "[DONE]" {
        return Event::Done;
    }
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return Event::Nothing;
    };
    if v.get("error").is_some_and(|e| !e.is_null()) {
        return Event::Failed(refine_api::error_message(data));
    }
    let content = &v["choices"][0]["delta"]["content"];
    let text = match content {
        Value::String(text) => text.clone(),
        // Content as a list of parts.
        Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join(""),
        _ => String::new(),
    };
    if v["choices"][0]["finish_reason"].is_string() {
        return Event::Finish(text);
    }
    if !text.is_empty() {
        return Event::Delta(text);
    }
    // The usage chunk comes last, with an empty `choices` list.
    if v.get("usage").is_some_and(|u| !u.is_null())
        && let Some(usage) = refine_api::parse_usage(data)
    {
        return Event::Usage(usage);
    }
    Event::Nothing
}

#[cfg(test)]
mod tests {
    use super::super::AskImage;
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::Receiver;

    /// What the mock server received.
    struct Seen {
        path: String,
        headers: String,
        body: Value,
    }

    /// How the mock server answers one request.
    enum Answer {
        /// A 200 event stream: each chunk is written, flushed, then
        /// the pause is slept. The connection closes after the last.
        Stream(Vec<(String, Duration)>),
        /// A complete response with this status and JSON body.
        Body(u16, String),
        /// Never answers; holds the connection open this long.
        Hang(Duration),
    }

    /// One-thread HTTP server answering each request with the next
    /// canned [`Answer`].
    fn serve(answers: Vec<Answer>) -> (String, Receiver<Seen>) {
        let (url, seen, _closed) = serve_watched(answers);
        (url, seen)
    }

    /// Wait up to `time` for the client to close the connection.
    fn closed_within(stream: &mut std::net::TcpStream, time: Duration) -> bool {
        let deadline = Instant::now() + time;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            stream.set_read_timeout(Some(left)).unwrap();
            match stream.read(&mut [0u8; 64]) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => return false,
                // Reset by the client.
                Err(_) => return true,
            }
        }
    }

    /// [`serve`], also reporting the moment the server saw the client
    /// close the connection while it was still answering (during a
    /// pause, a hang, or on a write).
    fn serve_watched(answers: Vec<Answer>) -> (String, Receiver<Seen>, Receiver<Instant>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let (closed_tx, closed) = mpsc::channel();
        thread::spawn(move || {
            for answer in answers {
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
                let gone = match answer {
                    Answer::Stream(chunks) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
                        );
                        let _ = stream.flush();
                        let mut gone = false;
                        for (chunk, pause) in chunks {
                            gone = stream.write_all(chunk.as_bytes()).and_then(|()| stream.flush()).is_err()
                                || closed_within(&mut stream, pause);
                            if gone {
                                break;
                            }
                        }
                        gone
                    }
                    Answer::Body(status, body) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        false
                    }
                    Answer::Hang(time) => closed_within(&mut stream, time),
                };
                if gone {
                    let _ = closed_tx.send(Instant::now());
                }
            }
        });
        (url, rx, closed)
    }

    const NO_PAUSE: Duration = Duration::from_millis(0);

    fn delta(text: &str) -> String {
        format!("data: {}\n\n", json!({ "choices": [{ "index": 0, "delta": { "content": text } }] }))
    }

    /// A chunk of the model's reasoning: no text for the answer.
    fn reasoning(text: &str) -> String {
        format!("data: {}\n\n", json!({ "choices": [{ "index": 0, "delta": { "reasoning_content": text } }] }))
    }

    /// Reasoning chunks and keep-alive comments in turn, `pause`
    /// apart.
    fn thinking(lines: usize, pause: Duration) -> Vec<(String, Duration)> {
        (0..lines)
            .map(|i| if i % 2 == 0 { reasoning("hm ") } else { ": keep-alive\n\n".to_string() })
            .map(|line| (line, pause))
            .collect()
    }

    /// Send the question and cancel it `after` this long. Returns the
    /// result, the events, and the moment of the cancel.
    fn send_and_cancel(url: &str, after: Duration) -> (Result<AskReply, AskError>, Vec<AskEvent>, Instant) {
        let cancel = CancelFlag::new();
        let canceller = {
            let cancel = cancel.clone();
            thread::spawn(move || {
                thread::sleep(after);
                cancel.cancel();
                Instant::now()
            })
        };
        let mut events = Vec::new();
        let result = backend(url, None).send(&setup(), &question(), &mut |e| events.push(e), &cancel, LONG);
        (result, events, canceller.join().unwrap())
    }

    /// The server saw the connection close within half a second of
    /// `since`.
    fn assert_closed_soon(closed: &Receiver<Instant>, since: Instant, what: &str) {
        let at = closed
            .recv_timeout(Duration::from_secs(4))
            .unwrap_or_else(|_| panic!("{what}: the server never saw the connection close"));
        let took = at.saturating_duration_since(since);
        assert!(took < Duration::from_millis(500), "{what}: the connection closed {took:?} later");
    }

    /// A stream the way OpenAI sends it: a role chunk, the text, a
    /// finish chunk, the usage chunk, `[DONE]`.
    fn stream_of(pieces: &[&str]) -> Vec<(String, Duration)> {
        let mut chunks = vec![(
            format!("data: {}\n\n", json!({ "choices": [{ "delta": { "role": "assistant", "content": "" } }] })),
            NO_PAUSE,
        )];
        chunks.extend(pieces.iter().map(|p| (delta(p), Duration::from_millis(20))));
        chunks.push((
            format!("data: {}\n\n", json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] })),
            NO_PAUSE,
        ));
        chunks.push((
            format!(
                "data: {}\n\n",
                json!({ "choices": [], "usage": { "prompt_tokens": 512, "completion_tokens": 9,
                        "prompt_tokens_details": { "cached_tokens": 384 } } })
            ),
            NO_PAUSE,
        ));
        chunks.push(("data: [DONE]\n\n".to_string(), NO_PAUSE));
        chunks
    }

    fn backend(url: &str, key: Option<&str>) -> OpenAiAsk {
        OpenAiAsk::new(
            OpenAiSettings { base_url: url.to_string(), model: "refine-model".into(), effort: None },
            key.map(str::to_string),
        )
    }

    fn setup() -> AskSetup {
        AskSetup { system: "You answer questions.".into(), model: "test-model".into(), effort: None, web_search: false }
    }

    fn question() -> AskInput {
        AskInput::text("<question>\nwhat is the capital of norway\n</question>")
    }

    fn send(
        backend: &mut OpenAiAsk,
        setup: &AskSetup,
        input: &AskInput,
        timeout: Duration,
    ) -> (Result<AskReply, AskError>, Vec<AskEvent>) {
        let mut events = Vec::new();
        let result = backend.send(setup, input, &mut |e| events.push(e), &CancelFlag::new(), timeout);
        (result, events)
    }

    const LONG: Duration = Duration::from_secs(10);

    #[test]
    fn streams_the_answer_and_stops_at_done() {
        let mut chunks = stream_of(&["The capital ", "of Norway ", "is Oslo."]);
        // Anything after `[DONE]` is not part of the answer, and the
        // call does not wait for the connection to close.
        chunks.push((delta(" And more."), Duration::from_secs(3)));
        let (url, seen) = serve(vec![Answer::Stream(chunks)]);
        let started = Instant::now();
        let (reply, events) = send(&mut backend(&url, Some("sk-test-123")), &setup(), &question(), LONG);
        let reply = reply.unwrap();
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
        assert_eq!(
            events,
            vec![
                AskEvent::Text("The capital ".into()),
                AskEvent::Text("of Norway ".into()),
                AskEvent::Text("is Oslo.".into()),
            ]
        );
        assert_eq!(reply.text, "The capital of Norway is Oslo.");
        assert!(reply.first_text_ms.unwrap() <= reply.elapsed_ms);
        let usage = reply.usage.unwrap();
        assert_eq!((usage.tokens_in, usage.tokens_cached, usage.tokens_out), (512, 384, 9));
        assert_eq!(reply.notice, "");

        let seen = seen.recv().unwrap();
        assert_eq!(seen.path, "/v1/chat/completions");
        assert!(seen.headers.to_lowercase().contains("authorization: bearer sk-test-123"), "{}", seen.headers);
        // The ask's model, not the one in the Refine block.
        assert_eq!(seen.body["model"], "test-model");
        assert_eq!(seen.body["stream"], true);
        assert_eq!(seen.body["stream_options"]["include_usage"], true);
        assert!(seen.body.get("temperature").is_none());
        assert!(seen.body.get("reasoning_effort").is_none());
        assert!(seen.body.get("tools").is_none());
        assert_eq!(seen.body["messages"][0], json!({ "role": "system", "content": "You answer questions." }));
        assert_eq!(seen.body["messages"][1]["role"], "user");
        assert_eq!(seen.body["messages"][1]["content"], "<question>\nwhat is the capital of norway\n</question>");
    }

    #[test]
    fn a_second_turn_carries_the_conversation() {
        let (url, seen) = serve(vec![Answer::Stream(stream_of(&["Oslo."])), Answer::Stream(stream_of(&["Stockholm."]))]);
        let mut b = backend(&url, None);
        assert_eq!(send(&mut b, &setup(), &question(), LONG).0.unwrap().text, "Oslo.");
        assert_eq!(send(&mut b, &setup(), &AskInput::text("and of sweden"), LONG).0.unwrap().text, "Stockholm.");
        let _first = seen.recv().unwrap();
        let second = seen.recv().unwrap();
        assert!(!second.headers.to_lowercase().contains("authorization"));
        let messages = second.body["messages"].as_array().unwrap();
        let roles: Vec<&str> = messages.iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["system", "user", "assistant", "user"]);
        assert_eq!(messages[2]["content"], "Oslo.");
        assert_eq!(messages[3]["content"], "and of sweden");
    }

    #[test]
    fn a_stream_that_finished_without_done_or_usage_is_still_an_answer() {
        // Some servers never send `[DONE]`: the finish chunk is the
        // end, on its own or with the last piece of text in it.
        let finish = format!("data: {}\n\n", json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] }));
        let (url, _seen) = serve(vec![Answer::Stream(vec![(delta("Oslo."), NO_PAUSE), (finish, NO_PAUSE)])]);
        let reply = send(&mut backend(&url, None), &setup(), &question(), LONG).0.unwrap();
        assert_eq!(reply.text, "Oslo.");
        assert_eq!(reply.usage, None);

        let last = format!(
            "data: {}\n\n",
            json!({ "choices": [{ "delta": { "content": "is Oslo." }, "finish_reason": "stop" }] })
        );
        let (url, _seen) = serve(vec![Answer::Stream(vec![(delta("It "), NO_PAUSE), (last, NO_PAUSE)])]);
        let (reply, events) = send(&mut backend(&url, None), &setup(), &question(), LONG);
        assert_eq!(reply.unwrap().text, "It is Oslo.");
        assert_eq!(events, vec![AskEvent::Text("It ".into()), AskEvent::Text("is Oslo.".into())]);
    }

    #[test]
    fn a_stream_cut_off_before_the_end_is_an_error_that_keeps_what_arrived() {
        // Text, then the connection closes: no `[DONE]`, no finish.
        let chunks = vec![(delta("The capital "), NO_PAUSE), (delta("of Norway"), NO_PAUSE)];
        let (url, seen) = serve(vec![Answer::Stream(chunks), Answer::Stream(stream_of(&["Oslo."]))]);
        let mut b = backend(&url, None);
        let (reply, events) = send(&mut b, &setup(), &question(), LONG);
        match reply.unwrap_err() {
            AskError::Backend(m) => assert!(m.contains("cut off") && m.contains("127.0.0.1"), "{m}"),
            other => panic!("expected Backend, got {other:?}"),
        }
        // What arrived was shown, and stays with whoever showed it.
        assert_eq!(events, vec![AskEvent::Text("The capital ".into()), AskEvent::Text("of Norway".into())]);
        // The half answer is not part of the conversation.
        assert_eq!(send(&mut b, &setup(), &question(), LONG).0.unwrap().text, "Oslo.");
        let _cut = seen.recv().unwrap();
        assert_eq!(seen.recv().unwrap().body["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_server_that_does_not_stream_is_read_as_one_answer() {
        let body = json!({
            "choices": [{ "message": { "role": "assistant", "content": "Oslo." } }],
            "usage": { "prompt_tokens": 40, "completion_tokens": 3 },
        })
        .to_string();
        let (url, _seen) = serve(vec![Answer::Body(200, body)]);
        let (reply, events) = send(&mut backend(&url, None), &setup(), &question(), LONG);
        let reply = reply.unwrap();
        assert_eq!(reply.text, "Oslo.");
        assert_eq!(events, vec![AskEvent::Text("Oslo.".into())]);
        assert_eq!(reply.usage.unwrap().tokens_in, 40);
    }

    #[test]
    fn http_errors_are_reported_plainly() {
        let body = json!({ "error": { "message": "Incorrect API key provided" } }).to_string();
        let (url, _seen) = serve(vec![Answer::Body(401, body)]);
        match send(&mut backend(&url, Some("bad")), &setup(), &question(), LONG).0.unwrap_err() {
            AskError::Unavailable(m) => {
                assert!(m.contains("127.0.0.1") && m.contains("Incorrect API key provided"), "{m}");
                assert!(m.contains("Check the key in settings"), "{m}");
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }

        let (url, _seen) = serve(vec![Answer::Body(500, "{\"error\":\"boom\"}".into())]);
        match send(&mut backend(&url, None), &setup(), &question(), LONG).0.unwrap_err() {
            AskError::Backend(m) => assert!(m.contains("500") && m.contains("boom"), "{m}"),
            other => panic!("expected Backend, got {other:?}"),
        }

        let (url, _seen) = serve(vec![Answer::Body(404, "{\"error\":{\"message\":\"model not found\"}}".into())]);
        match send(&mut backend(&url, None), &setup(), &question(), LONG).0.unwrap_err() {
            AskError::Backend(m) => assert!(m.contains("404") && m.contains("model name"), "{m}"),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn an_error_inside_the_stream_fails_the_ask() {
        let chunks = vec![
            (delta("The capital "), NO_PAUSE),
            (format!("data: {}\n\n", json!({ "error": { "message": "rate limit reached" } })), NO_PAUSE),
        ];
        let (url, _seen) = serve(vec![Answer::Stream(chunks)]);
        match send(&mut backend(&url, None), &setup(), &question(), LONG).0.unwrap_err() {
            AskError::Backend(m) => assert!(m.contains("rate limit reached"), "{m}"),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn a_failed_turn_is_not_left_in_the_conversation() {
        let (url, seen) = serve(vec![Answer::Body(500, "{\"error\":\"boom\"}".into()), Answer::Stream(stream_of(&["Oslo."]))]);
        let mut b = backend(&url, None);
        assert!(send(&mut b, &setup(), &question(), LONG).0.is_err());
        assert_eq!(send(&mut b, &setup(), &question(), LONG).0.unwrap().text, "Oslo.");
        let _failed = seen.recv().unwrap();
        // One system message and the question once.
        assert_eq!(seen.recv().unwrap().body["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_silent_server_times_out_and_the_connection_is_closed() {
        let silent: [(&str, Answer); 3] = [
            ("before the headers", Answer::Hang(Duration::from_secs(5))),
            ("after the headers", Answer::Stream(vec![(String::new(), Duration::from_secs(5))])),
            ("after some text", Answer::Stream(vec![(delta("The capital "), Duration::from_secs(5))])),
        ];
        for (what, answer) in silent {
            let (url, _seen, closed) = serve_watched(vec![answer]);
            let started = Instant::now();
            let (reply, _) = send(&mut backend(&url, None), &setup(), &question(), Duration::from_millis(300));
            let gave_up = Instant::now();
            assert_eq!(reply.unwrap_err(), AskError::Timeout, "{what}");
            assert!(started.elapsed() < Duration::from_millis(2000), "{what}: {:?}", started.elapsed());
            assert_closed_soon(&closed, gave_up, what);
        }
    }

    #[test]
    fn cancel_closes_the_connection_while_the_server_is_silent() {
        let silent: [(&str, Answer); 2] = [
            ("before the headers", Answer::Hang(Duration::from_secs(5))),
            ("after the headers", Answer::Stream(vec![(String::new(), Duration::from_secs(5))])),
        ];
        for (what, answer) in silent {
            let (url, _seen, closed) = serve_watched(vec![answer]);
            let started = Instant::now();
            let (reply, events, cancelled) = send_and_cancel(&url, Duration::from_millis(200));
            assert_eq!(reply.unwrap_err(), AskError::Cancelled, "{what}");
            assert!(events.is_empty(), "{what}: {events:?}");
            assert!(started.elapsed() < Duration::from_millis(1000), "{what}: {:?}", started.elapsed());
            assert_closed_soon(&closed, cancelled, what);
        }
    }

    #[test]
    fn cancel_closes_the_connection_while_only_reasoning_and_keep_alives_arrive() {
        // Three seconds of lines with nothing for the answer in them.
        let (url, _seen, closed) = serve_watched(vec![Answer::Stream(thinking(100, Duration::from_millis(30)))]);
        let (reply, events, cancelled) = send_and_cancel(&url, Duration::from_millis(200));
        assert_eq!(reply.unwrap_err(), AskError::Cancelled);
        assert!(events.is_empty(), "{events:?}");
        assert_closed_soon(&closed, cancelled, "reasoning");
    }

    #[test]
    fn a_long_reasoning_stream_is_not_a_timeout() {
        // 1.2 s of reasoning and keep-alives before the first word,
        // against a limit of 400 ms on silence.
        let mut chunks = thinking(12, Duration::from_millis(100));
        chunks.extend(stream_of(&["Oslo."]));
        let (url, _seen) = serve(vec![Answer::Stream(chunks)]);
        let (reply, events) = send(&mut backend(&url, None), &setup(), &question(), Duration::from_millis(400));
        let reply = reply.unwrap();
        assert_eq!(reply.text, "Oslo.");
        assert_eq!(events, vec![AskEvent::Text("Oslo.".into())]);
        assert!(reply.first_text_ms.unwrap() >= 1000, "{:?}", reply.first_text_ms);
    }

    #[test]
    fn steady_progress_is_not_a_timeout() {
        // Six pieces, 150 ms apart: 900 ms in all, against a limit of
        // 500 ms on silence.
        let chunks: Vec<(String, Duration)> =
            (0..6).map(|i| (delta(&format!("{i} ")), Duration::from_millis(150))).chain([("data: [DONE]\n\n".to_string(), NO_PAUSE)]).collect();
        let (url, _seen) = serve(vec![Answer::Stream(chunks)]);
        let (reply, _) = send(&mut backend(&url, None), &setup(), &question(), Duration::from_millis(500));
        let reply = reply.unwrap();
        assert_eq!(reply.text, "0 1 2 3 4 5");
        assert!(reply.elapsed_ms >= 700, "{}", reply.elapsed_ms);
    }

    #[test]
    fn cancel_is_noticed_between_chunks_and_before_the_first() {
        // Mid-stream.
        let chunks = vec![(delta("The capital "), Duration::from_secs(5)), (delta("is Oslo."), NO_PAUSE)];
        let (url, _seen) = serve(vec![Answer::Stream(chunks)]);
        let cancel = CancelFlag::new();
        let mut events = Vec::new();
        let started = Instant::now();
        let err = {
            let flag = cancel.clone();
            backend(&url, None)
                .send(
                    &setup(),
                    &question(),
                    &mut |e| {
                        events.push(e);
                        flag.cancel();
                    },
                    &cancel,
                    LONG,
                )
                .unwrap_err()
        };
        assert_eq!(err, AskError::Cancelled);
        assert_eq!(events, vec![AskEvent::Text("The capital ".into())]);
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());

        // While the server has not said anything yet.
        let (url, _seen) = serve(vec![Answer::Hang(Duration::from_secs(3))]);
        let cancel = CancelFlag::new();
        let canceller = {
            let cancel = cancel.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(200));
                cancel.cancel();
            })
        };
        let started = Instant::now();
        let err = backend(&url, None).send(&setup(), &question(), &mut |_| {}, &cancel, LONG).unwrap_err();
        canceller.join().unwrap();
        assert_eq!(err, AskError::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    }

    #[test]
    fn web_search_is_ignored_with_a_notice() {
        let (url, seen) = serve(vec![Answer::Stream(stream_of(&["Oslo."]))]);
        let mut with_search = setup();
        with_search.web_search = true;
        let reply = send(&mut backend(&url, None), &with_search, &question(), LONG).0.unwrap();
        assert_eq!(reply.text, "Oslo.");
        assert_eq!(reply.notice, WEB_SEARCH_UNAVAILABLE);
        let body = seen.recv().unwrap().body;
        assert!(body.get("tools").is_none() && body.get("web_search_options").is_none(), "{body}");
    }

    #[test]
    fn effort_is_sent_only_when_set_and_a_refused_field_is_dropped_and_remembered() {
        let refused = json!({ "error": { "message": "Unrecognized request argument supplied: stream_options" } }).to_string();
        let (url, seen) = serve(vec![
            Answer::Body(400, refused),
            Answer::Stream(stream_of(&["Oslo."])),
            Answer::Stream(stream_of(&["Oslo again."])),
        ]);
        let mut with_effort = setup();
        with_effort.model = "no-usage-model".into();
        with_effort.effort = Some(crate::settings::RefineEffort::Low);
        let reply = send(&mut backend(&url, None), &with_effort, &question(), LONG).0.unwrap();
        assert_eq!(reply.text, "Oslo.");
        let first = seen.recv().unwrap().body;
        assert_eq!(first["reasoning_effort"], "low");
        assert_eq!(first["stream_options"]["include_usage"], true);
        let second = seen.recv().unwrap().body;
        assert!(second.get("stream_options").is_none());
        assert_eq!(second["reasoning_effort"], "low");
        assert_eq!(second["stream"], true);

        // The next ask does not pay for the refusal again.
        let reply = send(&mut backend(&url, None), &with_effort, &question(), LONG).0.unwrap();
        assert_eq!(reply.text, "Oslo again.");
        assert!(seen.recv().unwrap().body.get("stream_options").is_none());
    }

    #[test]
    fn an_image_travels_as_a_data_url_part() {
        let input = AskInput {
            text: "what is this".into(),
            images: vec![AskImage { media_type: "image/png".into(), data: vec![0x89, b'P', b'N', b'G'] }],
        };
        let message = user_message(&input);
        assert_eq!(message["role"], "user");
        assert_eq!(message["content"][0], json!({ "type": "text", "text": "what is this" }));
        assert_eq!(message["content"][1]["type"], "image_url");
        assert_eq!(message["content"][1]["image_url"]["url"], "data:image/png;base64,iVBORw==");
        // Without images the content is a plain string, which every
        // server takes.
        assert_eq!(user_message(&AskInput::text("hi")), json!({ "role": "user", "content": "hi" }));
    }

    /// The image is sent once and then leaves the conversation the
    /// backend keeps: the kept messages hold no base64 of it, and a
    /// further turn goes out with a line in its place.
    #[test]
    fn the_image_leaves_the_conversation_when_its_request_has_returned() {
        let with_image = AskInput {
            text: "what is this".into(),
            images: vec![AskImage { media_type: "image/png".into(), data: vec![0x89, b'P', b'N', b'G'] }],
        };
        let (url, seen) = serve(vec![Answer::Stream(stream_of(&["A PNG."])), Answer::Stream(stream_of(&["Still a PNG."]))]);
        let mut b = backend(&url, None);
        assert_eq!(send(&mut b, &setup(), &with_image, LONG).0.unwrap().text, "A PNG.");
        // The request carried it.
        let first = seen.recv().unwrap().body;
        assert_eq!(first["messages"][1]["content"][1]["image_url"]["url"], "data:image/png;base64,iVBORw==");
        // What the backend keeps does not.
        let kept = serde_json::to_string(&b.messages).unwrap();
        assert!(!kept.contains("iVBORw") && !kept.contains("image_url") && !kept.contains("base64"), "{kept}");
        assert_eq!(
            b.messages[1]["content"],
            json!([{ "type": "text", "text": "what is this" }, { "type": "text", "text": IMAGE_NOT_KEPT }])
        );
        assert_eq!(b.messages[2], json!({ "role": "assistant", "content": "A PNG." }));
        // A further turn is sent without the image.
        assert_eq!(send(&mut b, &setup(), &AskInput::text("are you sure"), LONG).0.unwrap().text, "Still a PNG.");
        let second = serde_json::to_string(&seen.recv().unwrap().body).unwrap();
        assert!(!second.contains("iVBORw") && second.contains(IMAGE_NOT_KEPT), "{second}");

        // A turn that fails takes its image with it too.
        let (url, _seen) = serve(vec![Answer::Body(500, r#"{"error":{"message":"the model is busy"}}"#.into())]);
        let mut b = backend(&url, None);
        assert!(send(&mut b, &setup(), &with_image, LONG).0.is_err());
        let kept = serde_json::to_string(&b.messages).unwrap();
        assert!(!kept.contains("iVBORw") && !kept.contains("what is this"), "{kept}");
    }

    #[test]
    fn forgetting_images_touches_nothing_else() {
        let mut messages = vec![
            json!({ "role": "system", "content": "You answer questions." }),
            json!({ "role": "user", "content": "plain text" }),
            json!({ "role": "user", "content": [
                { "type": "text", "text": "what is this" },
                { "type": "image_url", "image_url": { "url": "data:image/png;base64,iVBORw==" } },
                { "type": "image_url", "image_url": { "url": "data:image/png;base64,AAAA" } },
            ] }),
            json!({ "role": "assistant", "content": "A PNG." }),
        ];
        forget_images(&mut messages);
        assert_eq!(messages[0], json!({ "role": "system", "content": "You answer questions." }));
        assert_eq!(messages[1], json!({ "role": "user", "content": "plain text" }));
        assert_eq!(
            messages[2]["content"],
            json!([
                { "type": "text", "text": "what is this" },
                { "type": "text", "text": IMAGE_NOT_KEPT },
                { "type": "text", "text": IMAGE_NOT_KEPT },
            ])
        );
        assert_eq!(messages[3], json!({ "role": "assistant", "content": "A PNG." }));
        assert!(!IMAGE_NOT_KEPT.contains('\u{2014}') && !IMAGE_NOT_KEPT.contains('\u{2013}'));
    }

    #[test]
    fn an_unconfigured_or_unreachable_endpoint_is_unavailable() {
        let mut no_model = setup();
        no_model.model = String::new();
        let (reply, _) = send(&mut backend("https://api.openai.com/v1", None), &no_model, &question(), LONG);
        assert_eq!(reply.unwrap_err(), AskError::Unavailable("No model name is set.".into()));

        // Bind then drop: the port is closed.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let (reply, _) = send(&mut backend(&format!("http://127.0.0.1:{port}/v1"), None), &setup(), &question(), LONG);
        assert!(matches!(reply.unwrap_err(), AskError::Unavailable(_)));
    }

    #[test]
    fn event_lines_are_told_apart() {
        assert_eq!(read_event(""), Event::Nothing);
        assert_eq!(read_event(": keep-alive"), Event::Nothing);
        assert_eq!(read_event("event: message"), Event::Nothing);
        assert_eq!(read_event("data: [DONE]"), Event::Done);
        assert_eq!(read_event("data:[DONE]"), Event::Done);
        assert_eq!(read_event(&delta("Hi").trim_end().to_string()), Event::Delta("Hi".into()));
        assert_eq!(
            read_event(r#"data: {"choices":[{"delta":{"content":[{"type":"text","text":"Hi "},{"type":"text","text":"there"}]}}]}"#),
            Event::Delta("Hi there".into())
        );
        assert_eq!(read_event(r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#), Event::Nothing);
        assert_eq!(read_event(r#"data: {"choices":[{"delta":{"content":null}}],"usage":null}"#), Event::Nothing);
        assert_eq!(read_event(r#"data: {"choices":[{"delta":{"reasoning_content":"hm"}}]}"#), Event::Nothing);
        assert_eq!(
            read_event(r#"data: {"choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}"#),
            Event::Delta("Hi".into())
        );
        assert_eq!(read_event(r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#), Event::Finish(String::new()));
        assert_eq!(
            read_event(r#"data: {"choices":[{"delta":{"content":"Hi"},"finish_reason":"stop"}]}"#),
            Event::Finish("Hi".into())
        );
        assert_eq!(read_event(r#"data: {"error":{"message":"boom"}}"#), Event::Failed("boom".into()));
        assert_eq!(read_event(r#"{"choices":[{"message":{"content":"Hi"}}]}"#), Event::NotAnEvent);
        assert!(matches!(
            read_event(r#"data: {"choices":[],"usage":{"prompt_tokens":5,"completion_tokens":2}}"#),
            Event::Usage(_)
        ));
    }
}
