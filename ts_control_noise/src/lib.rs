//! Noise handshake and protocol used to communicate with the Tailscale control server.

mod cipher;
mod error;
mod handshake;
mod io;
mod messages;

pub use cipher::ChaCha20Poly1305BigEndian;
pub use error::Error;
pub use handshake::Handshake;
pub use io::NoiseIo;
