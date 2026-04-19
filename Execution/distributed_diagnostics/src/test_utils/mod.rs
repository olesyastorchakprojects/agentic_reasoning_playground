//! Internal test-only helpers. Not compiled in production builds.
//!
//! Provides:
//! - `MockHttpServer` – single-use loopback HTTP/1.1 server for unit tests.
//! - `TempArtifactDir` – temporary directory helpers for artifact-loading tests.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

// ─── MockHttpServer ──────────────────────────────────────────────────────────

/// A response the mock server will send for one request.
pub struct MockResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl MockResponse {
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self { status: 200, body: body.into() }
    }
    pub fn status(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self { status, body: body.into() }
    }
}

/// Minimal async HTTP/1.1 mock server that:
/// - listens on `127.0.0.1` on an ephemeral port;
/// - serves one preconfigured response per accepted connection;
/// - records the raw request body for later assertion.
pub struct MockHttpServer {
    addr: SocketAddr,
    recorded_bodies: Arc<Mutex<Vec<Vec<u8>>>>,
    _handle: tokio::task::JoinHandle<()>,
}

impl MockHttpServer {
    /// Spawn a mock server that will respond with `responses` in order.
    /// Panics if the server cannot bind.
    pub async fn new(responses: Vec<MockResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock server");
        let addr = listener.local_addr().expect("local_addr");
        let recorded_bodies = Arc::new(Mutex::new(Vec::new()));
        let recorded_clone = Arc::clone(&recorded_bodies);

        let handle = tokio::spawn(async move {
            let mut queue: VecDeque<MockResponse> = responses.into();
            while let Some(resp) = queue.pop_front() {
                if let Ok((stream, _)) = listener.accept().await {
                    let body_store = Arc::clone(&recorded_clone);
                    handle_connection(stream, resp, body_store).await;
                }
            }
        });

        Self { addr, recorded_bodies, _handle: handle }
    }

    pub fn addr(&self) -> SocketAddr { self.addr }

    pub fn base_url(&self) -> String { format!("http://{}", self.addr) }

    /// Return all request bodies received so far.
    pub async fn take_bodies(&self) -> Vec<Vec<u8>> {
        self.recorded_bodies.lock().await.clone()
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    resp: MockResponse,
    bodies: Arc<Mutex<Vec<Vec<u8>>>>,
) {
    // Read request into buffer (up to 1 MiB).
    let mut buf = vec![0u8; 1024 * 1024];
    let n = stream.read(&mut buf).await.unwrap_or(0);
    buf.truncate(n);

    // Extract HTTP body (everything after \r\n\r\n).
    let body = if let Some(pos) = find_body_start(&buf) {
        buf[pos..].to_vec()
    } else {
        Vec::new()
    };
    bodies.lock().await.push(body);

    // Write HTTP/1.1 response.
    let status_text = match resp.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Unknown",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        resp.status, status_text, resp.body.len()
    );
    let _ = stream.write_all(header.as_bytes()).await;
    let _ = stream.write_all(&resp.body).await;
}

fn find_body_start(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

// ─── TempArtifactDir ─────────────────────────────────────────────────────────

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex as StdMutex};
use tempfile::TempDir;
use tokenizers::models::wordlevel::WordLevel;
use tokenizers::pre_tokenizers::whitespace::Whitespace;
use tokenizers::Tokenizer;

use crate::utils::tokenizer::tokenizer_cache_root;

static POPULATED_CACHES: LazyLock<StdMutex<HashSet<String>>> =
    LazyLock::new(|| StdMutex::new(HashSet::new()));

/// Pre-populate the tokenizer cache for `source` at the same path `HfTokenizer::load` uses.
/// Safe to call from multiple threads — writes are serialized and each source is written once.
pub fn populate_tokenizer_cache(source: &str) {
    let mut populated = POPULATED_CACHES.lock().unwrap();
    if populated.contains(source) {
        return;
    }
    let cache = tokenizer_cache_root().join(source).join("tokenizer.json");
    std::fs::create_dir_all(cache.parent().unwrap()).expect("create tokenizer cache dir");
    let model = WordLevel::builder()
        .vocab(
            [
                ("[UNK]".to_string(), 0u32),
                ("service".to_string(), 1u32),
                ("down".to_string(), 2u32),
                ("query".to_string(), 3u32),
                ("text".to_string(), 4u32),
                ("consensus".to_string(), 5u32),
                ("fault".to_string(), 6u32),
            ]
            .into_iter()
            .collect(),
        )
        .unk_token("[UNK]".to_string())
        .build()
        .expect("build wordlevel tokenizer");
    let mut tokenizer = Tokenizer::new(model);
    tokenizer.with_pre_tokenizer(Some(Whitespace));
    tokenizer.save(&cache, false).expect("save tokenizer to cache");
    populated.insert(source.to_string());
}

/// Wraps a `tempfile::TempDir` and exposes helpers for writing JSON artifacts.
pub struct TempArtifactDir {
    inner: TempDir,
}

impl TempArtifactDir {
    pub fn new() -> Self {
        Self { inner: TempDir::new().expect("tempdir") }
    }

    pub fn path(&self) -> &std::path::Path { self.inner.path() }

    /// Write `content` to `filename` inside the temp dir; return full path.
    pub fn write_json(&self, filename: &str, content: &str) -> PathBuf {
        let path = self.inner.path().join(filename);
        std::fs::write(&path, content).expect("write artifact");
        path
    }

    /// Write a minimal Hugging Face-compatible tokenizer artifact that uses
    /// whitespace tokenization and a small fixed vocabulary.
    pub fn write_basic_tokenizer(&self, filename: &str) -> PathBuf {
        let path = self.inner.path().join(filename);

        let model = WordLevel::builder()
            .vocab(
                [
                    ("[UNK]".to_string(), 0u32),
                    ("service".to_string(), 1u32),
                    ("down".to_string(), 2u32),
                    ("query".to_string(), 3u32),
                    ("text".to_string(), 4u32),
                    ("consensus".to_string(), 5u32),
                    ("fault".to_string(), 6u32),
                ]
                .into_iter()
                .collect(),
            )
            .unk_token("[UNK]".to_string())
            .build()
            .expect("build wordlevel tokenizer");

        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_pre_tokenizer(Some(Whitespace));
        tokenizer.save(&path, false).expect("save tokenizer artifact");

        path
    }
}
