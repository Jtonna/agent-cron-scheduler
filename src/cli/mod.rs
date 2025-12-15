pub mod daemon;
pub mod jobs;
pub mod logs;

use clap::{Parser, Subcommand};
use std::collections::HashMap;

/// Agent Cron Scheduler - A cross-platform cron scheduler daemon
#[derive(Parser, Debug)]
#[command(
    name = "acs",
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
        /// Also remove all data (jobs, logs)
        #[arg(long)]
        purge: bool,
    },

    /// Add a new scheduled job
    Add {
        /// Job name (must be unique)
        #[arg(short = 'n', long)]
        name: String,

        /// Cron schedule expression (5-field)
        #[arg(short = 's', long)]
        schedule: String,

        /// Shell command to execute
        #[arg(short = 'c', long = "cmd", conflicts_with = "script")]
        cmd: Option<String>,

        /// Script file to execute (relative to data_dir/scripts/)
        #[arg(long, conflicts_with = "cmd")]
        script: Option<String>,

        /// IANA timezone (default: UTC)
        #[arg(long)]
        timezone: Option<String>,

        /// Working directory for the command
        #[arg(long = "working-dir")]
        working_dir: Option<String>,

        /// Environment variables (KEY=VALUE)
        #[arg(short = 'e', long = "env", value_name = "KEY=VALUE")]
        env: Vec<String>,

        /// Create the job in disabled state
        #[arg(long)]
        disabled: bool,
    },

    /// Remove a scheduled job
    Remove {
        /// Job name or UUID
        job: String,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// List all scheduled jobs
    List {
        /// Show only enabled jobs
        #[arg(long, conflicts_with = "disabled")]
        enabled: bool,

        /// Show only disabled jobs
        #[arg(long, conflicts_with = "enabled")]
        disabled: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Enable a scheduled job
    Enable {
        /// Job name or UUID
        job: String,
    },

    /// Disable a scheduled job
    Disable {
        /// Job name or UUID
        job: String,
    },

    /// Manually trigger a job run
    Trigger {
        /// Job name or UUID
        job: String,

        /// Follow the job output (stream via SSE)
        #[arg(long)]
        follow: bool,
    },

    /// View job run logs
    Logs {
        /// Job name or UUID
        job: String,

        /// Follow live output (stream via SSE)
        #[arg(long)]
        follow: bool,

        /// Specific run ID to view
        #[arg(long)]
        run: Option<String>,

        /// Show last N runs
        #[arg(long)]
        last: Option<usize>,

        /// Show last N lines of log output
        #[arg(long)]
        tail: Option<usize>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
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
        "Could not connect to daemon at {}:{}. Is it running? (try: acs start)",
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
        Some(Commands::Status) => daemon::cmd_status(&cli.host, cli.port, cli.verbose).await,
        Some(Commands::Uninstall { purge }) => {
            daemon::cmd_uninstall(&cli.host, cli.port, *purge).await
        }
        Some(Commands::Add {
            name,
            schedule,
            cmd,
            script,
            timezone,
            working_dir,
            env,
            disabled,
        }) => {
            jobs::cmd_add(
                &cli.host,
                cli.port,
                name,
                schedule,
                cmd.as_deref(),
                script.as_deref(),
                timezone.as_deref(),
                working_dir.as_deref(),
                env,
                *disabled,
            )
            .await
        }
        Some(Commands::Remove { job, yes }) => {
            jobs::cmd_remove(&cli.host, cli.port, job, *yes).await
        }
        Some(Commands::List {
            enabled,
            disabled,
            json,
        }) => jobs::cmd_list(&cli.host, cli.port, *enabled, *disabled, *json).await,
        Some(Commands::Enable { job }) => jobs::cmd_enable(&cli.host, cli.port, job).await,
        Some(Commands::Disable { job }) => jobs::cmd_disable(&cli.host, cli.port, job).await,
        Some(Commands::Trigger { job, follow }) => {
            jobs::cmd_trigger(&cli.host, cli.port, job, *follow).await
        }
        Some(Commands::Logs {
            job,
            follow,
            run,
            last,
            tail,
            json,
        }) => {
            logs::cmd_logs(
                &cli.host,
                cli.port,
                job,
                *follow,
                run.as_deref(),
                *last,
                *tail,
                *json,
            )
            .await
        }
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
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    // -----------------------------------------------------------------------
    // 1. CLI parsing: `acs --version` produces version string
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_version_flag() {
        let result = Cli::try_parse_from(["acs", "--version"]);
        // --version causes clap to exit with an error containing the version
        assert!(result.is_err());
        let err = result.unwrap_err();
        // The error kind should be DisplayVersion
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
        let output = err.to_string();
        assert!(
            output.contains("0.1.0"),
            "Expected version 0.1.0 in output: {}",
            output
        );
    }

    // -----------------------------------------------------------------------
    // 2. CLI parsing: `acs add -n test -s "* * * * *" -c "echo hi"` parses
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_add_parses_correctly() {
        let cli = Cli::try_parse_from([
            "acs",
            "add",
            "-n",
            "test",
            "-s",
            "* * * * *",
            "-c",
            "echo hi",
        ])
        .expect("Should parse add command");

        match &cli.command {
            Some(Commands::Add {
                name,
                schedule,
                cmd,
                script,
                disabled,
                ..
            }) => {
                assert_eq!(name, "test");
                assert_eq!(schedule, "* * * * *");
                assert_eq!(cmd.as_deref(), Some("echo hi"));
                assert!(script.is_none());
                assert!(!disabled);
            }
            other => panic!("Expected Add command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // 3. CLI parsing: `acs list --json` sets json flag
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_list_json_flag() {
        let cli = Cli::try_parse_from(["acs", "list", "--json"]).expect("Should parse list --json");

        match &cli.command {
            Some(Commands::List {
                json,
                enabled,
                disabled,
            }) => {
                assert!(json);
                assert!(!enabled);
                assert!(!disabled);
            }
            other => panic!("Expected List command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // 4. CLI parsing: `acs remove test --yes` sets yes flag
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_remove_yes_flag() {
        let cli = Cli::try_parse_from(["acs", "remove", "test", "--yes"])
            .expect("Should parse remove --yes");

        match &cli.command {
            Some(Commands::Remove { job, yes }) => {
                assert_eq!(job, "test");
                assert!(yes);
            }
            other => panic!("Expected Remove command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // 5. CLI parsing: global --host and --port flags parse correctly
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
    // 6. CLI parsing: `acs logs test --follow --tail 100` parses correctly
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_logs_follow_tail() {
        let cli = Cli::try_parse_from(["acs", "logs", "test", "--follow", "--tail", "100"])
            .expect("Should parse logs --follow --tail");

        match &cli.command {
            Some(Commands::Logs {
                job,
                follow,
                tail,
                run,
                last,
                json,
            }) => {
                assert_eq!(job, "test");
                assert!(follow);
                assert_eq!(*tail, Some(100));
                assert!(run.is_none());
                assert!(last.is_none());
                assert!(!json);
            }
            other => panic!("Expected Logs command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // 7. Connection error message format
    // -----------------------------------------------------------------------
    #[test]
    fn test_connection_error_message() {
        let msg = connection_error_message("127.0.0.1", 8377);
        assert_eq!(
            msg,
            "Could not connect to daemon at 127.0.0.1:8377. Is it running? (try: acs start)"
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
    // Additional: add with --script flag
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_add_with_script() {
        let cli = Cli::try_parse_from([
            "acs",
            "add",
            "-n",
            "script-job",
            "-s",
            "0 * * * *",
            "--script",
            "deploy.sh",
        ])
        .expect("Should parse add with --script");

        match &cli.command {
            Some(Commands::Add { cmd, script, .. }) => {
                assert!(cmd.is_none());
                assert_eq!(script.as_deref(), Some("deploy.sh"));
            }
            other => panic!("Expected Add command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: add with environment variables
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_add_with_env_vars() {
        let cli = Cli::try_parse_from([
            "acs",
            "add",
            "-n",
            "env-job",
            "-s",
            "* * * * *",
            "-c",
            "echo $FOO",
            "-e",
            "FOO=bar",
            "-e",
            "BAZ=qux",
        ])
        .expect("Should parse add with env vars");

        match &cli.command {
            Some(Commands::Add { env, .. }) => {
                assert_eq!(env.len(), 2);
                assert_eq!(env[0], "FOO=bar");
                assert_eq!(env[1], "BAZ=qux");
            }
            other => panic!("Expected Add command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: parse_env_vars helper
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
    // Additional: add with --disabled flag
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_add_with_disabled() {
        let cli = Cli::try_parse_from([
            "acs",
            "add",
            "-n",
            "disabled-job",
            "-s",
            "* * * * *",
            "-c",
            "echo hi",
            "--disabled",
        ])
        .expect("Should parse add --disabled");

        match &cli.command {
            Some(Commands::Add { disabled, .. }) => {
                assert!(disabled);
            }
            other => panic!("Expected Add command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: trigger with --follow
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_trigger_follow() {
        let cli = Cli::try_parse_from(["acs", "trigger", "my-job", "--follow"])
            .expect("Should parse trigger --follow");

        match &cli.command {
            Some(Commands::Trigger { job, follow }) => {
                assert_eq!(job, "my-job");
                assert!(follow);
            }
            other => panic!("Expected Trigger command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: start with all flags
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_start_all_flags() {
        let cli = Cli::try_parse_from([
            "acs",
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
    // Additional: list --enabled filter
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_list_enabled_filter() {
        let cli =
            Cli::try_parse_from(["acs", "list", "--enabled"]).expect("Should parse list --enabled");

        match &cli.command {
            Some(Commands::List {
                enabled,
                disabled,
                json,
            }) => {
                assert!(enabled);
                assert!(!disabled);
                assert!(!json);
            }
            other => panic!("Expected List command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: list --disabled filter
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_list_disabled_filter() {
        let cli = Cli::try_parse_from(["acs", "list", "--disabled"])
            .expect("Should parse list --disabled");

        match &cli.command {
            Some(Commands::List {
                enabled,
                disabled,
                json,
            }) => {
                assert!(!enabled);
                assert!(disabled);
                assert!(!json);
            }
            other => panic!("Expected List command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: list --enabled and --disabled conflict
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_list_enabled_disabled_conflict() {
        let result = Cli::try_parse_from(["acs", "list", "--enabled", "--disabled"]);
        assert!(result.is_err(), "--enabled and --disabled should conflict");
    }

    // -----------------------------------------------------------------------
    // Additional: add requires either -c or --script
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_add_cmd_and_script_conflict() {
        let result = Cli::try_parse_from([
            "acs",
            "add",
            "-n",
            "test",
            "-s",
            "* * * * *",
            "-c",
            "echo hi",
            "--script",
            "test.sh",
        ]);
        assert!(result.is_err(), "-c and --script should conflict");
    }

    // -----------------------------------------------------------------------
    // Additional: logs with --run
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_logs_with_run_id() {
        let cli = Cli::try_parse_from([
            "acs",
            "logs",
            "my-job",
            "--run",
            "550e8400-e29b-41d4-a716-446655440000",
        ])
        .expect("Should parse logs --run");

        match &cli.command {
            Some(Commands::Logs { job, run, .. }) => {
                assert_eq!(job, "my-job");
                assert_eq!(run.as_deref(), Some("550e8400-e29b-41d4-a716-446655440000"));
            }
            other => panic!("Expected Logs command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: logs with --last
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_logs_with_last() {
        let cli = Cli::try_parse_from(["acs", "logs", "my-job", "--last", "5"])
            .expect("Should parse logs --last");

        match &cli.command {
            Some(Commands::Logs { job, last, .. }) => {
                assert_eq!(job, "my-job");
                assert_eq!(*last, Some(5));
            }
            other => panic!("Expected Logs command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: base_url helper
    // -----------------------------------------------------------------------
    #[test]
    fn test_base_url() {
        assert_eq!(base_url("127.0.0.1", 8377), "http://127.0.0.1:8377");
        assert_eq!(base_url("0.0.0.0", 9000), "http://0.0.0.0:9000");
    }

    // -----------------------------------------------------------------------
    // Additional: add with --timezone and --working-dir
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_add_with_timezone_and_working_dir() {
        let cli = Cli::try_parse_from([
            "acs",
            "add",
            "-n",
            "tz-job",
            "-s",
            "0 9 * * *",
            "-c",
            "echo morning",
            "--timezone",
            "America/New_York",
            "--working-dir",
            "/home/user/project",
        ])
        .expect("Should parse add with --timezone and --working-dir");

        match &cli.command {
            Some(Commands::Add {
                timezone,
                working_dir,
                ..
            }) => {
                assert_eq!(timezone.as_deref(), Some("America/New_York"));
                assert_eq!(working_dir.as_deref(), Some("/home/user/project"));
            }
            other => panic!("Expected Add command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: enable and disable parse job argument
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_enable_parses_job() {
        let cli = Cli::try_parse_from(["acs", "enable", "my-job"]).expect("Should parse enable");

        match &cli.command {
            Some(Commands::Enable { job }) => {
                assert_eq!(job, "my-job");
            }
            other => panic!("Expected Enable command, got: {:?}", other),
        }
    }

    #[test]
    fn test_cli_disable_parses_job() {
        let cli = Cli::try_parse_from(["acs", "disable", "my-job"]).expect("Should parse disable");

        match &cli.command {
            Some(Commands::Disable { job }) => {
                assert_eq!(job, "my-job");
            }
            other => panic!("Expected Disable command, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Additional: logs --json flag
    // -----------------------------------------------------------------------
    #[test]
    fn test_cli_logs_json_flag() {
        let cli = Cli::try_parse_from(["acs", "logs", "my-job", "--json"])
            .expect("Should parse logs --json");

        match &cli.command {
            Some(Commands::Logs { job, json, .. }) => {
                assert_eq!(job, "my-job");
                assert!(json);
            }
            other => panic!("Expected Logs command, got: {:?}", other),
        }
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
}
