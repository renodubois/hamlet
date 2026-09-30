#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiError {
    InvalidCredentials,
    InvalidInput,
    Conflict,
    Unavailable,
    InvalidResponse,
    AlreadyInvalid,
    NotFound,
    ServerFailure,
}

impl ApiError {
    /// Operation-neutral fallback; features own authentication/write-specific feedback.
    pub fn description(&self) -> &'static str {
        match self {
            Self::AlreadyInvalid => "Session rejected. Please log in again.",
            Self::InvalidInput => "The server rejected the request.",
            Self::Conflict => "The request conflicts with existing server data.",
            Self::InvalidCredentials => "Credentials were rejected.",
            Self::InvalidResponse => "The server returned an invalid response.",
            Self::Unavailable => "Could not reach the server. Check the address and try again.",
            Self::NotFound => "The channel no longer exists on the server.",
            Self::ServerFailure => "The server could not complete the request. Try again later.",
        }
    }
}
