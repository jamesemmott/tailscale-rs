//! HTTP/1.1 client implementation, and utilities to establish an HTTP/1.1 connection over TCP or
//! TLS.

use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};

use http::{Request, Response};
use hyper::{
    body::{Body, Incoming},
    client::conn::http1::{self, SendRequest},
};
use hyper_util::rt::tokio::WithHyperIo;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Mutex,
    task::JoinSet,
};

use crate::{Client, Error};

/// An HTTP/1.1 client that can connect to a server and send HTTP requests/receive HTTP responses.
/// Supports the [HTTP/1.1 protocol upgrade mechanism].
///
/// [HTTP/1.1 protocol upgrade mechanism]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Protocol_upgrade_mechanism
#[derive(Clone)]
pub struct Http1<B> {
    inner: Arc<Inner<B>>,
}

impl<B> Debug for Http1<B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Http1").finish_non_exhaustive()
    }
}

struct Inner<B> {
    client: Mutex<SendRequest<B>>,
    _runner: JoinSet<()>,
}

impl<B> Client<B> for Http1<B>
where
    B: Body + Send + 'static,
    B::Data: Send,
    B::Error: Send + Sync + 'static,
{
    async fn send(&self, req: Request<B>) -> Result<Response<Incoming>, Error> {
        let mut client = self.inner.client.lock().await;

        client
            .send_request(req)
            .await
            .inspect_err(|e| {
                tracing::error!(error = %e, "sending request");
            })
            .map_err(From::from)
    }
}

/// Establish a connection to an HTTP/1.1 server over an existing connection.
pub async fn connect<B>(
    lower: impl AsyncRead + AsyncWrite + Unpin + Send + 'static,
) -> Result<Http1<B>, Error>
where
    B: Body + Send + 'static,
    B::Data: Send,
    B::Error: core::error::Error + Send + Sync + 'static,
{
    let (client, conn) = http1::handshake(WithHyperIo::new(lower))
        .await
        .inspect_err(|e| {
            tracing::error!(error = %e, "sending request");
        })
        .map_err(Error::from)?;

    let mut joinset = JoinSet::new();

    joinset.spawn(async move {
        if let Err(e) = conn.with_upgrades().await {
            tracing::error!(?e, "error in http/1.1 connection; closing connection");
        }
    });

    Ok(Http1 {
        inner: Arc::new(Inner {
            client: Mutex::new(client),
            _runner: joinset,
        }),
    })
}

/// Establish an HTTP/1.1 connection to the server at the given `url` over plaintext TCP.
pub async fn connect_tcp<B>(url: &url::Url) -> Result<Http1<B>, Error>
where
    B: Body + Send + 'static,
    B::Data: Send,
    B::Error: core::error::Error + Send + Sync + 'static,
{
    let conn = crate::dial_tcp(url).await?;
    connect(conn).await
}

/// Establish an HTTP/1.1 connection to the server at the given `url` over encrypted TLS.
pub async fn connect_tls<B>(url: &url::Url) -> Result<Http1<B>, Error>
where
    B: Body + Send + 'static,
    B::Data: Send,
    B::Error: core::error::Error + Send + Sync + 'static,
{
    let conn = crate::dial_tls(url, [b"http/1.1".to_vec()]).await?;
    connect(conn).await
}
