//! A small local HTTP server that lets a media player read a file while it
//! is still downloading. The engine fetches the parts being read first.

use std::io::SeekFrom;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::engine::EngineSlot;

/// Where the server listens, and the secret that guards it.
#[derive(Clone)]
pub struct StreamServer {
    port: u16,
    token: String,
}

impl StreamServer {
    pub fn url(&self, torrent_id: usize, file_index: usize, file_name: &str) -> String {
        use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
        let name = utf8_percent_encode(file_name, NON_ALPHANUMERIC);
        format!(
            "http://127.0.0.1:{}/{}/{torrent_id}/{file_index}/{name}",
            self.port, self.token
        )
    }
}

#[derive(Clone)]
struct Shared {
    engine: Arc<EngineSlot>,
    token: String,
}

/// Starts the server on a free local port. Only requests carrying the token
/// are answered, so other programs and web pages cannot read your files.
pub async fn start(engine: Arc<EngineSlot>) -> anyhow::Result<StreamServer> {
    let token = uuid::Uuid::new_v4().simple().to_string();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let app = Router::new()
        .route("/{token}/{torrent}/{file}/{*name}", get(serve))
        .with_state(Shared {
            engine,
            token: token.clone(),
        });
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(StreamServer { port, token })
}

/// The byte range a request asks for, as start and exclusive end.
fn requested_range(headers: &HeaderMap, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(value) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) else {
        return Ok(None);
    };
    let (start, end) = value
        .strip_prefix("bytes=")
        .and_then(|v| v.split_once('-'))
        .ok_or(())?;
    let start: u64 = start.parse().map_err(|_| ())?;
    let end = match end {
        "" => len,
        end => end.parse::<u64>().map_err(|_| ())?.saturating_add(1).min(len),
    };
    if start >= len || end <= start {
        return Err(());
    }
    Ok(Some((start, end)))
}

async fn serve(
    State(shared): State<Shared>,
    Path((token, torrent, file, _name)): Path<(String, usize, usize, String)>,
    headers: HeaderMap,
) -> Response {
    if token != shared.token {
        return StatusCode::NOT_FOUND.into_response();
    }
    let api = shared.engine.get().api.clone();
    let mut stream = match api.api_stream(torrent.into(), file).await {
        Ok(stream) => stream,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let len = stream.len();

    let mut out = HeaderMap::new();
    out.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Ok(mime) = api.torrent_file_mime_type(torrent.into(), file) {
        out.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    }

    let (status, start, end) = match requested_range(&headers, len) {
        Ok(Some((start, end))) => (StatusCode::PARTIAL_CONTENT, start, end),
        Ok(None) => (StatusCode::OK, 0, len),
        Err(()) => return StatusCode::RANGE_NOT_SATISFIABLE.into_response(),
    };
    if status == StatusCode::PARTIAL_CONTENT {
        if stream.seek(SeekFrom::Start(start)).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        if let Ok(value) = HeaderValue::from_str(&format!("bytes {start}-{}/{len}", end - 1)) {
            out.insert(header::CONTENT_RANGE, value);
        }
    }
    out.insert(header::CONTENT_LENGTH, HeaderValue::from(end - start));

    let reader: Box<dyn AsyncRead + Send + Unpin> = Box::new(stream.take(end - start));
    (status, out, Body::from_stream(ReaderStream::new(reader))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(value: &str, len: u64) -> Result<Option<(u64, u64)>, ()> {
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_str(value).unwrap());
        requested_range(&headers, len)
    }

    #[test]
    fn reads_byte_ranges() {
        assert_eq!(requested_range(&HeaderMap::new(), 100), Ok(None));
        assert_eq!(range("bytes=0-", 100), Ok(Some((0, 100))));
        assert_eq!(range("bytes=10-19", 100), Ok(Some((10, 20))));
        // Players ask past the end when probing; give them what there is.
        assert_eq!(range("bytes=90-500", 100), Ok(Some((90, 100))));
        assert_eq!(range("bytes=100-", 100), Err(()));
        assert_eq!(range("bytes=20-10", 100), Err(()));
        assert_eq!(range("lines=1-2", 100), Err(()));
    }
}
