use netstack::netcore;

/// Errors that may occur while interacting with a device.
#[derive(Debug, Copy, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Internal operation failed, likely a bug.
    #[error("internal operation failed, likely a bug")]
    InternalFailure,

    /// An operation timed out.
    #[error("operation timed out")]
    Timeout,

    /// A connection was reset.
    #[error("connection reset")]
    ConnectionReset,
}

impl From<ts_runtime::Error> for Error {
    fn from(value: ts_runtime::Error) -> Self {
        match value {
            ts_runtime::Error::Timeout => Error::Timeout,
            ts_runtime::Error::RuntimeState => Error::InternalFailure,
        }
    }
}

impl From<netcore::Error> for Error {
    fn from(value: netcore::Error) -> Self {
        match value {
            netcore::Error::WrongType
            | netcore::Error::ChannelClosed
            | netcore::Error::BadRequest
            | netcore::Error::InvariantViolated => Error::InternalFailure,

            netcore::Error::TcpStream(netcore::tcp::stream::Error::Reset) => Error::ConnectionReset,
        }
    }
}
