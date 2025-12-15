use clap::Parser;

use agent_cron_scheduler::cli::{self, Cli};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Set up tracing based on verbose flag
    if cli.verbose {
        tracing_subscriber::fmt().with_env_filter("debug").init();
    }

    if let Err(e) = cli::dispatch(&cli).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
