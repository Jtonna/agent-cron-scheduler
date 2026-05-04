pub mod daemon;
pub mod workflows;

use clap::{Parser, Subcommand};
use std::collections::HashMap;

/// Agent Cron Scheduler - A cross-platform cron scheduler daemon
#[derive(Parser, Debug)]
#[command(
    name = "agentcronsystem",
    version,
    about = "Agent Cron Scheduler - A cross-platform cron scheduler daemon"
)]
pub struct Cli {
    /// Daemon host
    #[arg(long, default_value = "127.0.0.1", global = true)]
    pub host: String,

    /// Daemon port
    #[arg(long, default_value_t = 8377, global = true)]
    pub port: u16,

    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(short = 'f', long)]
        foreground: bool,

        /// Path to configuration file
        #[arg(short = 'c', long = "config")]
        config: Option<String>,

        /// Port to listen on (overrides config)
        #[arg(short = 'p', long)]
        port: Option<u16>,

        /// Data directory path
        #[arg(long = "data-dir")]
        data_dir: Option<String>,
    },

    /// Stop the daemon
    Stop {
        /// Force kill the daemon process
        #[arg(long)]
        force: bool,
    },

    /// Show daemon status
    Status,

    /// Remove system service registration
    Uninstall {
        /// Also remove all data (workflows, logs)
        #[arg(long)]
        purge: bool,
    },

    /// Restart the daemon
    Restart,

    /// Update to the latest version
    Update {
        /// Target version (default: latest)
        #[arg(long)]
        version: Option<String>,
        /// Force update even if already on latest
        #[arg(long)]
        force: bool,
    },

    /// Manage workflows
    Workflows(workflows::WorkflowsCmd),
}

/// Build the base URL for the daemon HTTP API.
pub fn base_url(host: &str, port: u16) -> String {
    format!("http://{}:{}", host, port)
}

/// Parse environment variable arguments from "KEY=VALUE" format into a HashMap.
pub fn parse_env_vars(env_args: &[String]) -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    for arg in env_args {
        if let Some((key, value)) = arg.split_once('=') {
            if key.is_empty() {
                return Err(format!("Invalid environment variable: '{}'", arg));
            }
            map.insert(key.to_string(), value.to_string());
        } else {
            return Err(format!(
                "Invalid environment variable format: '{}'. Expected KEY=VALUE",
                arg
            ));
        }
    }
    Ok(map)
}

/// Format a connection error message for when the daemon is not reachable.
pub fn connection_error_message(host: &str, port: u16) -> String {
    format!(
        "Could not connect to daemon at {}:{}. Is it running? (try: agentcronsystem start)",
        host, port
    )
}

/// Dispatch the CLI command to the appropriate handler.
pub async fn dispatch(cli: &Cli) -> anyhow::Result<()> {
    match &cli.command {
        Some(Commands::Start {
            foreground,
            config,
            port,
            data_dir,
        }) => {
            daemon::cmd_start(
                &cli.host,
                cli.port,
                *foreground,
                config.as_deref(),
                *port,
                data_dir.as_deref(),
            )
            .await
        }
        Some(Commands::Stop { force }) => daemon::cmd_stop(&cli.host, cli.port, *force).await,
        Some(Commands::Restart) => daemon::cmd_restart(&cli.host, cli.port).await,
        Some(Commands::Status) => daemon::cmd_status(&cli.host, cli.port, cli.verbose).await,
        Some(Commands::Uninstall { purge }) => {
            daemon::cmd_uninstall(&cli.host, cli.port, *purge).await
        }
        Some(Commands::Update { version, force }) => {
            daemon::cmd_update(version.as_deref(), *force).await
        }
        Some(Commands::Workflows(cmd)) => workflows::dispatch(cmd, &cli.host, cli.port).await,
        None => {
            // No subcommand provided -- print help
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}

// ===========================================================================
// Tests (Phase 6: job CLI commands removed; only daemon / workflow commands remain)
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    // -----------------------------------------------------------------------
    // CLI parsing: `acs --version` produces version string
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_version_flag() {
        let result = Cli::try_parse_from(["agentcronsystem", "--version"]);
        // --version causes clap to exit with an error containing the version
        assert!(result.is_err());
        let err = result.unwrap_err();
        // The error kind should be DisplayVersion
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
        let output = err.to_string();
        let expected = env!("CARGO_PKG_VERSION");
        assert!(
            output.contains(expected),
            "Expected version {} in output: {}",
            expected,
            output
        );
    }

    // -----------------------------------------------------------------------
    // CLI parsing: global --host and --port flags parse correctly
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_global_host_port() {
        let cli =
            Cli::try_parse_from(["acs", "--host", "192.168.1.100", "--port", "9999", "status"])
                .expect("Should parse global host/port");

        assert_eq!(cli.host, "192.168.1.100");
        assert_eq!(cli.port, 9999);
        assert!(matches!(cli.command, Some(Commands::Status)));
    }

    // -----------------------------------------------------------------------
    // Connection error message format
    // -----------------------------------------------------------------------
    #[test]
    fn test_connection_error_message() {
        let msg = connection_error_message("127.0.0.1", 8377);
        assert_eq!(
            msg,
            "Could not connect to daemon at 127.0.0.1:8377. Is it running? (try: agentcronsystem start)"
        );
    }

    // -----------------------------------------------------------------------
    // Additional: default host and port
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_default_host_port() {
        let cli = Cli::try_parse_from(["acs", "status"]).expect("Should parse with defaults");
        assert_eq!(cli.host, "127.0.0.1");
        assert_eq!(cli.port, 8377);
    }

    // -----------------------------------------------------------------------
    // parse_env_vars helper
    // -----------------------------------------------------------------------
    #[test]
    fn test_parse_env_vars_valid() {
        let args = vec!["FOO=bar".to_string(), "BAZ=qux".to_string()];
        let result = parse_env_vars(&args).unwrap();
        assert_eq!(result.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(result.get("BAZ"), Some(&"qux".to_string()));
    }

    #[test]
    fn test_parse_env_vars_empty_key_rejected() {
        let args = vec!["=value".to_string()];
        let result = parse_env_vars(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_env_vars_no_equals_rejected() {
        let args = vec!["NOEQUALS".to_string()];
        let result = parse_env_vars(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_env_vars_value_with_equals() {
        let args = vec!["KEY=val=ue".to_string()];
        let result = parse_env_vars(&args).unwrap();
        assert_eq!(result.get("KEY"), Some(&"val=ue".to_string()));
    }

    // -----------------------------------------------------------------------
    // start with all flags
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_start_all_flags() {
        let cli = Cli::try_parse_from([
            "agentcronsystem",
            "start",
            "--foreground",
            "--config",
            "/etc/acs/config.json",
            "--port",
            "9000",
            "--data-dir",
            "/var/acs",
        ])
        .expect("Should parse start with all flags");

        match &cli.command {
            Some(Commands::Start {
                foreground,
                config,
                port,
                data_dir,
            }) => {
                assert!(foreground);
                assert_eq!(config.as_deref(), Some("/etc/acs/config.json"));
                assert_eq!(*port, Some(9000));
                assert_eq!(data_dir.as_deref(), Some("/var/acs"));
            }
            other => panic!("Expected Start command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: start with short flags
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_start_short_flags() {
        let cli = Cli::try_parse_from(["acs", "start", "-f", "-c", "/etc/acs.json", "-p", "8080"])
            .expect("Should parse start with short flags");

        match &cli.command {
            Some(Commands::Start {
                foreground,
                config,
                port,
                ..
            }) => {
                assert!(foreground);
                assert_eq!(config.as_deref(), Some("/etc/acs.json"));
                assert_eq!(*port, Some(8080));
            }
            other => panic!("Expected Start command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: stop with --force
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_stop_force() {
        let cli =
            Cli::try_parse_from(["acs", "stop", "--force"]).expect("Should parse stop --force");

        match &cli.command {
            Some(Commands::Stop { force }) => {
                assert!(force);
            }
            other => panic!("Expected Stop command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: uninstall with --purge
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_uninstall_purge() {
        let cli = Cli::try_parse_from(["acs", "uninstall", "--purge"])
            .expect("Should parse uninstall --purge");

        match &cli.command {
            Some(Commands::Uninstall { purge }) => {
                assert!(purge);
            }
            other => panic!("Expected Uninstall command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: verbose flag
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_verbose_flag() {
        let cli = Cli::try_parse_from(["acs", "-v", "status"]).expect("Should parse -v flag");
        assert!(cli.verbose);
    }

    // -----------------------------------------------------------------------
    // base_url helper
    // -----------------------------------------------------------------------
    #[test]
    fn test_base_url() {
        assert_eq!(base_url("127.0.0.1", 8377), "http://127.0.0.1:8377");
        assert_eq!(base_url("0.0.0.0", 9000), "http://0.0.0.0:9000");
    }

    // -----------------------------------------------------------------------
    // restart command parses
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_restart_parses() {
        let cli = Cli::try_parse_from(["acs", "restart"]).expect("Should parse restart");
        assert!(matches!(cli.command, Some(Commands::Restart)));
    }

    // -----------------------------------------------------------------------
    // Additional: global options with subcommand placed after
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_global_options_after_subcommand() {
        let cli = Cli::try_parse_from(["acs", "status", "--host", "10.0.0.1", "--port", "1234"])
            .expect("Should parse global options after subcommand");

        assert_eq!(cli.host, "10.0.0.1");
        assert_eq!(cli.port, 1234);
        assert!(matches!(cli.command, Some(Commands::Status)));
    }

    // -----------------------------------------------------------------------
    // Update command parsing
    // -----------------------------------------------------------------------

    /// `agentcronsystem update` with no flags parses successfully and both
    /// optional fields default to their zero-values.
    #[test]
    fn test_cli_update_no_flags() {
        let cli = Cli::try_parse_from(["agentcronsystem", "update"]).expect("Should parse update");

        match &cli.command {
            Some(Commands::Update { version, force }) => {
                assert!(version.is_none(), "version should be None by default");
                assert!(!force, "force should be false by default");
            }
            other => panic!("Expected Update command, got: {:?}", other),
        }
    }

    /// `agentcronsystem update --version 1.5.0` sets the version field.
    #[test]
    fn test_cli_update_with_version() {
        let cli = Cli::try_parse_from(["agentcronsystem", "update", "--version", "1.5.0"])
            .expect("Should parse update --version");

        match &cli.command {
            Some(Commands::Update { version, force }) => {
                assert_eq!(version.as_deref(), Some("1.5.0"));
                assert!(!force);
            }
            other => panic!("Expected Update command, got: {:?}", other),
        }
    }

    /// `agentcronsystem update --force` sets the force flag.
    #[test]
    fn test_cli_update_with_force() {
        let cli = Cli::try_parse_from(["agentcronsystem", "update", "--force"])
            .expect("Should parse update --force");

        match &cli.command {
            Some(Commands::Update { version, force }) => {
                assert!(version.is_none());
                assert!(force);
            }
            other => panic!("Expected Update command, got: {:?}", other),
        }
    }

    /// `agentcronsystem update --version 2.0.0 --force` sets both fields.
    #[test]
    fn test_cli_update_version_and_force() {
        let cli =
            Cli::try_parse_from(["agentcronsystem", "update", "--version", "2.0.0", "--force"])
                .expect("Should parse update with version and force");

        match &cli.command {
            Some(Commands::Update { version, force }) => {
                assert_eq!(version.as_deref(), Some("2.0.0"));
                assert!(force);
            }
            other => panic!("Expected Update command, got: {:?}", other),
        }
    }

}


