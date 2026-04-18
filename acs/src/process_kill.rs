// process_kill module — platform-specific process tree termination.
//
// On Unix, child processes are spawned with `setsid()` (see pty/mod.rs),
// which creates a new session and process group with PGID == child PID.
// This means we can send signals to the entire process tree by targeting
// the process group via `killpg(pid, signal)`.
//
// On Windows, child processes are spawned with CREATE_NEW_PROCESS_GROUP.
// We use `taskkill /T /F /PID` to terminate the whole tree.

use tracing::{debug, warn};

/// Gracefully terminate a process tree, falling back to force-kill after 5 s.
///
/// On Unix: sends SIGTERM to the process group, polls for up to 5 seconds,
/// then sends SIGKILL if the process is still alive.
///
/// On Windows: immediately calls [`force_kill_process_tree`] because there is
/// no reliable graceful-termination equivalent on that platform.
pub async fn kill_process_tree(pid: u32) {
    #[cfg(unix)]
    {
        debug!(pid, "Sending SIGTERM to process group");

        // SAFETY: killpg is async-signal-safe; pid fits in i32 for any
        // realistic process ID value.
        let ret = unsafe { libc::killpg(pid as i32, libc::SIGTERM) };
        if ret != 0 {
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if errno == libc::ESRCH {
                // Process group already gone — nothing to do.
                debug!(pid, "Process group already gone (SIGTERM skipped)");
                return;
            }
            warn!(pid, errno, "killpg(SIGTERM) failed");
        }

        // Poll for up to 5 s (50 × 100 ms) to see whether the process exits
        // on its own after receiving SIGTERM.
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
            if !alive {
                debug!(pid, "Process exited after SIGTERM");
                return;
            }
        }

        warn!(
            pid,
            "Process did not exit within 5 s of SIGTERM; escalating to SIGKILL"
        );
        force_kill_process_tree(pid).await;
    }

    #[cfg(windows)]
    {
        // Windows has no reliable SIGTERM equivalent; go straight to force-kill.
        force_kill_process_tree(pid).await;
    }
}

/// Immediately and forcefully terminate a process tree.
///
/// On Unix: sends SIGKILL to the process group; if that fails with ESRCH the
/// process is already gone.  Falls back to killing just the root PID if the
/// group kill fails for any other reason.
///
/// On Windows: runs `taskkill /T /F /PID <pid>`, which kills the process and
/// all of its descendants.
pub async fn force_kill_process_tree(pid: u32) {
    #[cfg(unix)]
    {
        debug!(pid, "Sending SIGKILL to process group");

        // SAFETY: same as above.
        let ret = unsafe { libc::killpg(pid as i32, libc::SIGKILL) };
        if ret != 0 {
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if errno == libc::ESRCH {
                debug!(pid, "Process group already gone (SIGKILL skipped)");
                return;
            }
            // killpg failed for some other reason (e.g. the child was never
            // made a session leader).  Fall back to killing the root PID only.
            warn!(
                pid,
                errno, "killpg(SIGKILL) failed; falling back to kill(SIGKILL)"
            );
            let ret2 = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
            if ret2 != 0 {
                let errno2 = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
                if errno2 != libc::ESRCH {
                    warn!(pid, errno = errno2, "kill(SIGKILL) also failed");
                }
            }
        }
    }

    #[cfg(windows)]
    {
        debug!(pid, "Running taskkill /T /F /PID to terminate process tree");

        // `taskkill /T` kills the entire process tree rooted at the given PID.
        // `/F` forces termination of processes that do not respond to messages.
        let output = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);

                if out.status.success() {
                    debug!(pid, "taskkill succeeded");
                } else {
                    // taskkill exits with a non-zero code when the process
                    // does not exist — that is acceptable.
                    let combined = format!("{stdout}{stderr}");
                    if combined.to_lowercase().contains("not found")
                        || combined.to_lowercase().contains("no running instance")
                    {
                        debug!(pid, "Process already gone (taskkill reported not found)");
                    } else {
                        warn!(pid, ?stdout, ?stderr, "taskkill reported an error");
                    }
                }
            }
            Err(e) => {
                warn!(pid, error = %e, "Failed to spawn taskkill");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A non-existent PID that is extremely unlikely to be live on any system.
    const DEAD_PID: u32 = 999_999;

    #[tokio::test]
    async fn test_kill_already_dead_process() {
        // Should complete without panicking even though the PID does not exist.
        kill_process_tree(DEAD_PID).await;
    }

    #[tokio::test]
    async fn test_force_kill_already_dead_process() {
        // Should complete without panicking even though the PID does not exist.
        force_kill_process_tree(DEAD_PID).await;
    }
}
