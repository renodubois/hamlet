use serde::Deserialize;

// Decode only the error fields used by the client.
#[derive(Deserialize)]
pub(super) struct ErrorResponse {
    pub error: ErrorInfo,
}

#[derive(Deserialize)]
pub(super) struct ErrorInfo {
    pub code: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiError {
    /// The client refused to build the request: invalid server URL, identifier or credential.
    InvalidRequest,
    /// Login/signup credentials were rejected.
    InvalidCredentials,
    /// The server rejected a bearer credential (401); the session is no longer valid.
    SessionRejected,
    InvalidInput,
    Conflict,
    NotFound,
    Forbidden,
    ServerFailure,
    /// The server answered with a status this operation does not define, including redirects.
    UnexpectedStatus,
    /// The response was malformed or violated the protocol contract.
    InvalidResponse,
    /// No complete response: connection failure or deadline.
    Unavailable,
}

impl ApiError {
    /// Operation-neutral fallback; features own authentication/write-specific feedback.
    pub fn description(&self) -> &'static str {
        match self {
            Self::InvalidRequest => {
                "The request could not be sent: the address or identifier is invalid."
            }
            Self::InvalidCredentials => "Credentials were rejected.",
            Self::SessionRejected => "Session rejected. Please log in again.",
            Self::InvalidInput => "The server rejected the request.",
            Self::Conflict => "The request conflicts with existing server data.",
            Self::NotFound => "The requested item no longer exists on the server.",
            Self::Forbidden => "You do not have permission to perform this action.",
            Self::ServerFailure => "The server could not complete the request. Try again later.",
            Self::UnexpectedStatus => "The server returned an unexpected response.",
            Self::InvalidResponse => "The server returned an invalid response.",
            Self::Unavailable => "Could not reach the server. Check the address and try again.",
        }
    }
}
