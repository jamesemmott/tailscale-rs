//! A small set of utility functions for establishing Transport Layer Security
//! (TLS) streams over existing transport-layer connections. Built on top of [`rustls`],
//! [`tokio_rustls`], and [`webpki_roots`] for use with [`tailscale`].
//!
//! This crate is tailored for our needs, and is probably a poor fit for other use cases. If you're
//! looking for a general-purpose TLS crate, we recommend using [`rustls`] and/or [`tokio_rustls`]
//! directly, or [`native-tls`].
//!
//! [`native-tls`]: https://docs.rs/native-tls/latest
//! [`rustls`]: https://docs.rs/rustls/latest
//! [`tailscale`]: https://docs.rs/tailscale/latest
//!
//! ## Root Certificates
//!
//! `ts_tls_util` uses [`webpki_roots::TLS_SERVER_ROOTS`] as the set of root certificates to form
//! a root-of-trust. These are the same root certificates trusted by Mozilla. This crate currently
//! **does not** check Certificate Revocation Lists (CRLs) to determine if any of the root
//! certificates have been revoked.

use std::sync::{Arc, LazyLock};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore},
};
pub use tokio_rustls::{client::TlsStream, rustls::pki_types::ServerName};
use url::Url;

static ROOT_CERT_STORE: LazyLock<Arc<RootCertStore>> = LazyLock::new(|| {
    Arc::new(RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.into(),
    })
});

/// Establishes a TLS stream with a server over an existing connection.
///
/// See module-level documentation for information on root certificates.
pub async fn connect<Io>(server_name: ServerName<'_>, io: Io) -> tokio::io::Result<TlsStream<Io>>
where
    Io: AsyncRead + AsyncWrite + Unpin,
{
    connect_alpn::<Io>(server_name, io, []).await
}

/// Establishes a TLS stream with a server over an existing connection, with an optional set of
/// ALPN protocols to negotiate.
///
/// See module-level documentation for information on root certificates.
pub async fn connect_alpn<Io>(
    server_name: ServerName<'_>,
    io: Io,
    alpn: impl IntoIterator<Item = Vec<u8>>,
) -> tokio::io::Result<TlsStream<Io>>
where
    Io: AsyncRead + AsyncWrite + Unpin,
{
    // TODO(npry): custom tls cert verifier to support commonname overrides and self-signed certs
    let mut rustls_config = ClientConfig::builder()
        .with_root_certificates(ROOT_CERT_STORE.clone())
        .with_no_client_auth();

    rustls_config
        .alpn_protocols
        .extend(alpn.into_iter().map(|x| x.to_owned()));

    let connector = TlsConnector::from(Arc::new(rustls_config));

    let stream = connector.connect(server_name.to_owned(), io).await?;

    Ok(stream)
}

/// If possible, converts the host portion of the given [`Url`] to a [`ServerName`] for establishing
/// TLS streams.
pub fn server_name(url: &Url) -> Option<ServerName<'_>> {
    ServerName::try_from(url.host_str()?).ok()
}
