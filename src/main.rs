use clap::Parser;

use agent_cron_scheduler::cli::{self, Cli};

fn main() {
    let cli = Cli::parse();

    // Handle Windows Service mode before creating tokio runtime
    // The service dispatcher must be called early and creates its own runtime
    #[cfg(target_os = "windows")]
    if matches!(
        cli.command,
        Some(agent_cron_scheduler::cli::Commands::Service)
    ) {
        if let Err(e) = agent_cron_scheduler::daemon::windows_service::run() {
            eprintln!("Windows Service error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // Normal CLI mode - create tokio runtime
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    rt.block_on(async {
        // Set up tracing based on verbose flag
        if cli.verbose {
            tracing_subscriber::fmt().with_env_filter("debug").init();
        }

        if let Err(e) = cli::dispatch(&cli).await {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    });
}
