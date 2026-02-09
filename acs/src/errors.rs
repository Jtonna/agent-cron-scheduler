use thiserror::Error;

#[derive(Debug, Error)]
pub enum AcsError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Cron error: {0}")]
    Cron(String),

    #[error("PTY error: {0}")]
    Pty(String),

    #[error("Timeout: {0}")]
    Timeout(String),
}

impl From<std::io::Error> for AcsError {
    fn from(err: std::io::Error) -> Self {
        AcsError::Storage(err.to_string())
    }
}

impl From<serde_json::Error> for AcsError {
    fn from(err: serde_json::Error) -> Self {
        AcsError::Storage(err.to_string())
    }
}

impl From<uuid::Error> for AcsError {
    fn from(err: uuid::Error) -> Self {
        AcsError::Validation(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_display() {
        let err = AcsError::NotFound("job xyz".to_string());
        assert_eq!(err.to_string(), "Not found: job xyz");
    }

    #[test]
    fn test_conflict_display() {
        let err = AcsError::Conflict("duplicate name".to_string());
        assert_eq!(err.to_string(), "Conflict: duplicate name");
    }

    #[test]
    fn test_validation_display() {
        let err = AcsError::Validation("invalid cron".to_string());
        assert_eq!(err.to_string(), "Validation error: invalid cron");
    }

    #[test]
    fn test_storage_display() {
        let err = AcsError::Storage("disk full".to_string());
        assert_eq!(err.to_string(), "Storage error: disk full");
    }

    #[test]
    fn test_internal_display() {
        let err = AcsError::Internal("unexpected".to_string());
        assert_eq!(err.to_string(), "Internal error: unexpected");
    }

    #[test]
    fn test_cron_display() {
        let err = AcsError::Cron("bad expression".to_string());
        assert_eq!(err.to_string(), "Cron error: bad expression");
    }

    #[test]
    fn test_pty_display() {
        let err = AcsError::Pty("spawn failed".to_string());
        assert_eq!(err.to_string(), "PTY error: spawn failed");
    }

    #[test]
    fn test_timeout_display() {
        let err = AcsError::Timeout("30s exceeded".to_string());
        assert_eq!(err.to_string(), "Timeout: 30s exceeded");
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let acs_err: AcsError = io_err.into();
        match acs_err {
            AcsError::Storage(msg) => assert!(msg.contains("file missing")),
            other => panic!("Expected Storage, got: {:?}", other),
        }
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<String>("not valid json").unwrap_err();
        let acs_err: AcsError = json_err.into();
        match acs_err {
            AcsError::Storage(_) => {}
            other => panic!("Expected Storage, got: {:?}", other),
        }
    }

    #[test]
    fn test_from_uuid_error() {
        let uuid_err = "not-a-uuid".parse::<uuid::Uuid>().unwrap_err();
        let acs_err: AcsError = uuid_err.into();
        match acs_err {
            AcsError::Validation(_) => {}
            other => panic!("Expected Validation, got: {:?}", other),
        }
    }
}
