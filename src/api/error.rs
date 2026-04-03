pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const GENERAL_ERROR: i32 = 1;
    pub const CONFIG_ERROR: i32 = 2;
    pub const AUTH_ERROR: i32 = 3;
    pub const NOT_FOUND: i32 = 4;
    pub const API_ERROR: i32 = 5;
    pub const CONFLICT: i32 = 6;
    pub const TIMEOUT: i32 = 7;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("{0} not found")]
    NotFound(String),

    #[error("API error {status}: {message}")]
    Api { status: u16, message: String },

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Task failed: {0}")]
    TaskFailed(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Returns a machine-readable error kind string for structured error output.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Config(_) => "config",
            Error::Auth(_) => "auth",
            Error::NotFound(_) => "not_found",
            Error::Api { .. } | Error::TaskFailed(_) => "api",
            Error::Conflict(_) => "conflict",
            Error::Timeout(_) => "timeout",
            Error::Http(_) | Error::Other(_) => "other",
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Config(_) => exit_codes::CONFIG_ERROR,
            Error::Auth(_) => exit_codes::AUTH_ERROR,
            Error::NotFound(_) => exit_codes::NOT_FOUND,
            Error::Api { .. } | Error::TaskFailed(_) => exit_codes::API_ERROR,
            Error::Conflict(_) => exit_codes::CONFLICT,
            Error::Timeout(_) => exit_codes::TIMEOUT,
            Error::Http(_) | Error::Other(_) => exit_codes::GENERAL_ERROR,
        }
    }

    pub fn from_status(status: u16, message: String) -> Self {
        match status {
            401 | 403 => Error::Auth(message),
            404 => Error::NotFound(message),
            _ => Error::Api { status, message },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_code_config() {
        let err = Error::Config("missing field".to_string());
        assert_eq!(err.exit_code(), exit_codes::CONFIG_ERROR);
    }

    #[test]
    fn test_exit_code_auth() {
        let err = Error::Auth("invalid credentials".to_string());
        assert_eq!(err.exit_code(), exit_codes::AUTH_ERROR);
    }

    #[test]
    fn test_exit_code_not_found() {
        let err = Error::NotFound("VM 100".to_string());
        assert_eq!(err.exit_code(), exit_codes::NOT_FOUND);
    }

    #[test]
    fn test_exit_code_api() {
        let err = Error::Api {
            status: 500,
            message: "internal server error".to_string(),
        };
        assert_eq!(err.exit_code(), exit_codes::API_ERROR);
    }

    #[test]
    fn test_exit_code_conflict() {
        let err = Error::Conflict("VM already exists".to_string());
        assert_eq!(err.exit_code(), exit_codes::CONFLICT);
    }

    #[test]
    fn test_exit_code_timeout() {
        let err = Error::Timeout("operation timed out".to_string());
        assert_eq!(err.exit_code(), exit_codes::TIMEOUT);
    }

    #[test]
    fn test_exit_code_task_failed() {
        let err = Error::TaskFailed("backup failed".to_string());
        assert_eq!(err.exit_code(), exit_codes::API_ERROR);
    }

    #[test]
    fn test_exit_code_other() {
        let err = Error::Other("unexpected".to_string());
        assert_eq!(err.exit_code(), exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn test_from_status_401_maps_to_auth() {
        let err = Error::from_status(401, "unauthorized".to_string());
        assert!(matches!(err, Error::Auth(_)));
        assert_eq!(err.exit_code(), exit_codes::AUTH_ERROR);
    }

    #[test]
    fn test_from_status_403_maps_to_auth() {
        let err = Error::from_status(403, "forbidden".to_string());
        assert!(matches!(err, Error::Auth(_)));
        assert_eq!(err.exit_code(), exit_codes::AUTH_ERROR);
    }

    #[test]
    fn test_from_status_404_maps_to_not_found() {
        let err = Error::from_status(404, "node/pve not found".to_string());
        assert!(matches!(err, Error::NotFound(_)));
        assert_eq!(err.exit_code(), exit_codes::NOT_FOUND);
    }

    #[test]
    fn test_from_status_500_maps_to_api() {
        let err = Error::from_status(500, "server error".to_string());
        assert!(matches!(err, Error::Api { status: 500, .. }));
        assert_eq!(err.exit_code(), exit_codes::API_ERROR);
    }

    #[test]
    fn test_from_status_409_maps_to_api() {
        let err = Error::from_status(409, "conflict".to_string());
        assert!(matches!(err, Error::Api { status: 409, .. }));
        assert_eq!(err.exit_code(), exit_codes::API_ERROR);
    }

    #[test]
    fn test_from_status_preserves_message() {
        let msg = "detailed error message".to_string();
        let err = Error::from_status(500, msg.clone());
        if let Error::Api { message, .. } = err {
            assert_eq!(message, msg);
        } else {
            panic!("expected Api variant");
        }
    }

    #[test]
    fn test_exit_code_constants() {
        assert_eq!(exit_codes::SUCCESS, 0);
        assert_eq!(exit_codes::GENERAL_ERROR, 1);
        assert_eq!(exit_codes::CONFIG_ERROR, 2);
        assert_eq!(exit_codes::AUTH_ERROR, 3);
        assert_eq!(exit_codes::NOT_FOUND, 4);
        assert_eq!(exit_codes::API_ERROR, 5);
        assert_eq!(exit_codes::CONFLICT, 6);
        assert_eq!(exit_codes::TIMEOUT, 7);
    }

    #[test]
    fn test_kind_maps_correctly() {
        assert_eq!(Error::Config("x".into()).kind(), "config");
        assert_eq!(Error::Auth("x".into()).kind(), "auth");
        assert_eq!(Error::NotFound("x".into()).kind(), "not_found");
        assert_eq!(
            Error::Api {
                status: 500,
                message: "x".into()
            }
            .kind(),
            "api"
        );
        assert_eq!(Error::TaskFailed("x".into()).kind(), "api");
        assert_eq!(Error::Conflict("x".into()).kind(), "conflict");
        assert_eq!(Error::Timeout("x".into()).kind(), "timeout");
        assert_eq!(Error::Other("x".into()).kind(), "other");
    }

    #[test]
    fn test_error_display_config() {
        let err = Error::Config("missing host".to_string());
        assert_eq!(err.to_string(), "Configuration error: missing host");
    }

    #[test]
    fn test_error_display_auth() {
        let err = Error::Auth("bad token".to_string());
        assert_eq!(err.to_string(), "Authentication failed: bad token");
    }

    #[test]
    fn test_error_display_not_found() {
        let err = Error::NotFound("VM 100".to_string());
        assert_eq!(err.to_string(), "VM 100 not found");
    }

    #[test]
    fn test_error_display_api() {
        let err = Error::Api {
            status: 422,
            message: "unprocessable entity".to_string(),
        };
        assert_eq!(err.to_string(), "API error 422: unprocessable entity");
    }
}
