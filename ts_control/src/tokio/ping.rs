use alloc::string::String;

use bytes::Bytes;
use tokio::sync::watch;
use ts_control_serde::PingType;
use ts_http_util::{BytesBody, ClientExt, Http2};
use url::Url;

use crate::StateUpdate;

#[derive(Debug, thiserror::Error)]
pub enum PingError {
    #[error(transparent)]
    Http(#[from] ts_http_util::Error),
    #[error(transparent)]
    JoinFailed(#[from] tokio::task::JoinError),
    #[error("C2N ping request is missing payload")]
    MissingPayload,
    #[error(transparent)]
    WatchRecv(#[from] watch::error::RecvError),
}

/// Extracts the body portion of an HTTP/1.1 request with a `Transfer-Encoding` header of
/// `chunked`. Used with C2N echo requests.
fn extract_chunked_http_body(payload: &str) -> Option<String> {
    let mut parts = payload.split("\r\n\r\n");
    // Extract and move past the method and headers.
    let _ = parts.next();

    // The next part is the body; extract it, then handle the chunked encoding.
    let body = parts.next()?;
    tracing::trace!(body, "extracted ping request body");

    // TODO (dylan): this needs to handle _all_ chunks, not just the first one.
    // TODO (dylan): this needs to check if the transfer-encoding header is chunked before
    // trying to process the body as chunked.
    let mut chunk = body.split("\r\n");
    let len = match chunk.next() {
        Some("0") => return None,
        Some(len) => usize::from_str_radix(len, 16),
        None => return None,
    }
    .ok()?;

    let content = chunk.next()?;
    tracing::trace!(
        payload_len = len,
        payload = content,
        "extracted payload from ping request body"
    );
    if content.len() != len {
        return None;
    }

    Some(content.to_string())
}

/// Handles [`PingRequest`]s from the control plane to this Tailscale node. Currently only handles
/// C2N pings.
pub async fn handle_ping(
    state: &StateUpdate,
    control_url: &Url,
    http2_client: &Http2<BytesBody>,
) -> Result<(), PingError> {
    let Some(request) = &state.ping else {
        return Ok(());
    };

    tracing::trace!(request = ?request, "handling ping request");
    for typ in &request.types {
        if typ != &PingType::C2N {
            tracing::warn!(ping_type = ?typ, "ignoring unsupported ping type");
            continue;
        }

        let payload = request.payload.clone().ok_or(PingError::MissingPayload)?;
        let body = match extract_chunked_http_body(&payload) {
            Some(body) => body,
            None => {
                tracing::warn!("ignoring malformed ping request");
                continue;
            }
        };
        tracing::debug!(body = %body, "extracted ping request echo body");

        let resp_body = format!("HTTP/1.1 200 OK\r\n\r\n{}", body);
        tracing::debug!(?body, "sending ping response embedded in POST");
        let response = http2_client
            .post(control_url, None, Bytes::from(resp_body).into())
            .await?;
        if !response.status().is_success() {
            tracing::error!("error responding to ping: {}", response.status());
        } else {
            tracing::debug!("ping response sent");
        }
    }

    Ok(())
}
