//! Windows Service implementation using the windows-service crate.
//!
//! This module handles the Service Control Manager (SCM) integration, allowing
//! the daemon to run as a proper Windows Service that can be started/stopped
//! via services.msc or `sc` commands.

use std::ffi::OsString;
use std::sync::mpsc;
use std::time::Duration;

use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

/// Service name constant (must match registration)
const SERVICE_NAME: &str = "AgentCronScheduler";

// Generate the FFI service main function
define_windows_service!(ffi_service_main, service_main);

/// Entry point called by the CLI when running as a Windows Service.
/// This function calls the service dispatcher which connects to SCM.
pub fn run() -> windows_service::Result<()> {
    // This call blocks until the service is stopped
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

/// Service main function called by SCM after dispatcher connects.
fn service_main(_arguments: Vec<OsString>) {
    // Initialize tracing for service mode (logs to file since no console)
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .try_init();

    if let Err(e) = run_service() {
        tracing::error!("Service failed: {:?}", e);
    }
}

/// Main service logic - registers control handler, reports status, runs daemon.
fn run_service() -> windows_service::Result<()> {
    // Channel for receiving shutdown signal from SCM
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    // Event handler for service control events
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                tracing::info!("Received SERVICE_CONTROL_STOP from SCM");
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    // Register our control handler with SCM
    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    // Report that we're starting
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(10),
        process_id: None,
    })?;

    // Create Tokio runtime for async daemon code
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("Failed to create Tokio runtime: {}", e);
            status_handle.set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Stopped,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: ServiceExitCode::Win32(1),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            })?;
            return Ok(());
        }
    };

    // Report that we're running
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    tracing::info!("Windows Service started successfully");

    // Run the daemon, blocking until shutdown signal received
    let exit_code = rt.block_on(async {
        match super::run_daemon_until_shutdown(shutdown_rx).await {
            Ok(()) => 0u32,
            Err(e) => {
                tracing::error!("Daemon error: {}", e);
                1u32
            }
        }
    });

    // Report that we're stopping
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StopPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(exit_code),
        checkpoint: 0,
        wait_hint: Duration::from_secs(10),
        process_id: None,
    })?;

    tracing::info!("Windows Service stopping");

    // Report that we've stopped
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(exit_code),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_name_matches() {
        // Verify that our constant matches the service registration name
        assert_eq!(SERVICE_NAME, "AgentCronScheduler");
    }
}
