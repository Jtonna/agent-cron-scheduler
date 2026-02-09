use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
    #[serde(default = "default_max_log_files_per_job")]
    pub max_log_files_per_job: usize,
    #[serde(default = "default_max_log_file_size")]
    pub max_log_file_size: u64,
    #[serde(default = "default_timeout_secs")]
    pub default_timeout_secs: u64,
    #[serde(default = "default_broadcast_capacity")]
    pub broadcast_capacity: usize,
    #[serde(default = "default_pty_rows")]
    pub pty_rows: u16,
    #[serde(default = "default_pty_cols")]
    pub pty_cols: u16,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8377
}

fn default_max_log_files_per_job() -> usize {
    50
}

fn default_max_log_file_size() -> u64 {
    10_485_760 // 10MB
}

fn default_timeout_secs() -> u64 {
    0
}

fn default_broadcast_capacity() -> usize {
    4096
}

fn default_pty_rows() -> u16 {
    24
}

fn default_pty_cols() -> u16 {
    80
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            data_dir: None,
            max_log_files_per_job: default_max_log_files_per_job(),
            max_log_file_size: default_max_log_file_size(),
            default_timeout_secs: default_timeout_secs(),
            broadcast_capacity: default_broadcast_capacity(),
            pty_rows: default_pty_rows(),
            pty_cols: default_pty_cols(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_config_defaults() {
        let config = DaemonConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8377);
        assert!(config.data_dir.is_none());
        assert_eq!(config.max_log_files_per_job, 50);
        assert_eq!(config.max_log_file_size, 10_485_760);
        assert_eq!(config.default_timeout_secs, 0);
        assert_eq!(config.broadcast_capacity, 4096);
        assert_eq!(config.pty_rows, 24);
        assert_eq!(config.pty_cols, 80);
    }

    #[test]
    fn test_daemon_config_serde_roundtrip() {
        let config = DaemonConfig::default();
        let json = serde_json::to_string(&config).expect("serialize");
        let deserialized: DaemonConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.host, config.host);
        assert_eq!(deserialized.port, config.port);
        assert_eq!(
            deserialized.max_log_files_per_job,
            config.max_log_files_per_job
        );
        assert_eq!(deserialized.max_log_file_size, config.max_log_file_size);
        assert_eq!(
            deserialized.default_timeout_secs,
            config.default_timeout_secs
        );
        assert_eq!(deserialized.broadcast_capacity, config.broadcast_capacity);
        assert_eq!(deserialized.pty_rows, config.pty_rows);
        assert_eq!(deserialized.pty_cols, config.pty_cols);
    }

    #[test]
    fn test_daemon_config_partial_deserialization_empty() {
        let json = "{}";
        let config: DaemonConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8377);
        assert!(config.data_dir.is_none());
        assert_eq!(config.max_log_files_per_job, 50);
        assert_eq!(config.max_log_file_size, 10_485_760);
        assert_eq!(config.default_timeout_secs, 0);
        assert_eq!(config.broadcast_capacity, 4096);
        assert_eq!(config.pty_rows, 24);
        assert_eq!(config.pty_cols, 80);
    }

    #[test]
    fn test_daemon_config_partial_deserialization_some_fields() {
        let json = r#"{"port": 9000, "pty_rows": 40}"#;
        let config: DaemonConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(config.host, "127.0.0.1"); // default
        assert_eq!(config.port, 9000); // overridden
        assert_eq!(config.pty_rows, 40); // overridden
        assert_eq!(config.pty_cols, 80); // default
        assert_eq!(config.max_log_files_per_job, 50); // default
    }

    #[test]
    fn test_daemon_config_with_data_dir() {
        let json = r#"{"data_dir": "/custom/path"}"#;
        let config: DaemonConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(config.data_dir, Some(PathBuf::from("/custom/path")));
    }

    #[test]
    fn test_daemon_config_all_fields_overridden() {
        let json = r#"{
            "host": "0.0.0.0",
            "port": 9999,
            "data_dir": "/data",
            "max_log_files_per_job": 100,
            "max_log_file_size": 52428800,
            "default_timeout_secs": 300,
            "broadcast_capacity": 8192,
            "pty_rows": 48,
            "pty_cols": 120
        }"#;
        let config: DaemonConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 9999);
        assert_eq!(config.data_dir, Some(PathBuf::from("/data")));
        assert_eq!(config.max_log_files_per_job, 100);
        assert_eq!(config.max_log_file_size, 52428800);
        assert_eq!(config.default_timeout_secs, 300);
        assert_eq!(config.broadcast_capacity, 8192);
        assert_eq!(config.pty_rows, 48);
        assert_eq!(config.pty_cols, 120);
    }
}
