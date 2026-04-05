//! Support for the [`axum`] http server wrapping [`TcpListener`].
//!
//! # Example
//!
//! ```rust,no_run
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn core::error::Error>> {
//! let dev = tailscale::Device::new(
//!     Default::default(),
//!     Some("YOUR_AUTH_KEY".to_owned()),
//!     Default::default(),
//! ).await?;
//!
//! let listener = dev.tcp_listen((dev.ipv4().await?, 80).into()).await?;
//!
//! async fn index() -> &'static str { "Hello world!" }
//! let router = axum::Router::new().route("/", axum::routing::get(index));
//!
//! axum::serve(tailscale::axum::Listener(listener), router).await?;
//! #   Ok(())
//! # }
//! ```

use std::net::SocketAddr;

use crate::{TcpListener, TcpStream};

/// Wrapper type implementing [`axum::serve::Listener`] on [`TcpListener`].
#[derive(Debug)]
pub struct Listener(pub TcpListener);

impl From<TcpListener> for Listener {
    fn from(listener: TcpListener) -> Self {
        Self(listener)
    }
}

impl axum::serve::Listener for Listener {
    type Io = TcpStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        let stream = loop {
            match self.0.accept().await {
                Ok(stream) => break stream,
                Err(e) => tracing::error!(err = %e, "tcp accept"),
            }
        };

        let addr = stream.remote_endpoint();

        (stream, addr)
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.0.local_endpoint())
    }
}
