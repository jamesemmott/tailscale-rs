use kameo::error::SendError;

/// Runtime errors.
#[derive(Debug, Copy, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The runtime encountered an internal error due to an invalid state.
    ///
    /// An internal component has panicked, or the runtime is in the midst of
    /// shutting down. It's unlikely this operation can be retried successfully.
    #[error("ts_runtime in invalid state")]
    RuntimeState,

    /// An operation timed out.
    #[error("operation timed out")]
    Timeout,
}

impl<M, E> From<SendError<M, E>> for Error {
    fn from(err: SendError<M, E>) -> Self {
        match err {
            SendError::ActorNotRunning(_)
            | SendError::ActorStopped
            | SendError::HandlerError(_)
            | SendError::MailboxFull(_) => Error::RuntimeState,
            SendError::Timeout(_) => Error::Timeout,
        }
    }
}
