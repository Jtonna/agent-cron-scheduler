// PTY module - Phase 2 implementation
// Cross-platform PTY abstraction with real and mock implementations.

use std::io;
use std::process::ExitStatus;

/// Trait for spawning PTY processes.
pub trait PtySpawner: Send + Sync {
    fn spawn(
        &self,
        cmd: portable_pty::CommandBuilder,
        rows: u16,
        cols: u16,
    ) -> anyhow::Result<Box<dyn PtyProcess>>;
}

/// Trait for interacting with a spawned PTY process.
pub trait PtyProcess: Send {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn kill(&mut self) -> io::Result<()>;
    fn wait(&mut self) -> io::Result<ExitStatus>;
}

// --- Real PTY implementation using portable-pty ---
//
// KNOWN ISSUE (Windows ConPTY):
// On Windows, the ConPTY subsystem has a well-known problem where the master-side
// reader pipe does not receive EOF after the child process exits. The background
// thread below attempts to work around this by:
//   1. Waiting for the child to exit via child.wait()
//   2. Dropping the master handle to try to signal EOF to the reader
//
// However, drop(master) does NOT reliably unblock the reader on Windows ConPTY.
// The internal ConPTY pipe handle can remain open even after the master is closed,
// leaving the reader blocked in a read() call indefinitely. This causes job runs
// to get stuck in "Running" status forever with 0 bytes of output.
//
// WORKAROUND: Use NoPtySpawner (see below) for production use. It uses plain
// std::process::Command with piped stdout/stderr, which properly handles EOF
// on all platforms including Windows. The daemon's start_daemon() function
// uses NoPtySpawner by default for this reason.
//
// If ConPTY support is needed in the future, possible approaches include:
//   - Using a timeout on the reader with CancelIoEx on the pipe handle
//   - Polling the child exit status from the reader side
//   - Using a different PTY library that handles ConPTY cleanup correctly

pub struct RealPtySpawner;

impl PtySpawner for RealPtySpawner {
    fn spawn(
        &self,
        cmd: portable_pty::CommandBuilder,
        rows: u16,
        cols: u16,
    ) -> anyhow::Result<Box<dyn PtyProcess>> {
        use portable_pty::{native_pty_system, PtySize};

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut child = pair.slave.spawn_command(cmd)?;

        // Drop slave to avoid holding the other end of the PTY
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let master = pair.master;

        // Channel to receive exit status from background thread
        let (exit_tx, exit_rx) = std::sync::mpsc::sync_channel(1);

        // Background thread: wait for child to exit, then drop master to unblock reader
        std::thread::spawn(move || {
            let status = child.wait();
            let result = match status {
                Ok(pty_status) => {
                    let code = if pty_status.success() { 0 } else { 1 };
                    Ok(exit_status_from_code(code))
                }
                Err(e) => Err(io::Error::other(format!("Child wait failed: {}", e))),
            };
            let _ = exit_tx.send(result);
            // Drop master AFTER sending exit status.
            // On Windows ConPTY, this unblocks the reader so the read loop can exit.
            drop(master);
        });

        Ok(Box::new(RealPtyProcess { reader, exit_rx }))
    }
}

struct RealPtyProcess {
    reader: Box<dyn std::io::Read + Send>,
    exit_rx: std::sync::mpsc::Receiver<io::Result<ExitStatus>>,
}

impl PtyProcess for RealPtyProcess {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }

    fn kill(&mut self) -> io::Result<()> {
        // The child process is managed by the background thread.
        // When the executor abandons this PtyProcess (drops it),
        // the reader is dropped, and the background thread will
        // eventually clean up when the child exits.
        Ok(())
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        self.exit_rx
            .recv()
            .map_err(|_| io::Error::other("Child wait channel closed unexpectedly"))?
    }
}

// --- NoPty fallback implementation using std::process::Command ---

/// A PTY spawner that uses plain std::process::Command with piped I/O
/// instead of a real PTY. Useful for testing and environments where
/// PTY is not available.
pub struct NoPtySpawner;

impl PtySpawner for NoPtySpawner {
    fn spawn(
        &self,
        cmd: portable_pty::CommandBuilder,
        _rows: u16,
        _cols: u16,
    ) -> anyhow::Result<Box<dyn PtyProcess>> {
        use std::process::{Command, Stdio};

        let args = cmd.get_argv();
        if args.is_empty() {
            return Err(anyhow::anyhow!("Empty command"));
        }

        let program = args[0].to_string_lossy().to_string();
        let mut command = Command::new(&program);

        for arg in &args[1..] {
            command.arg(arg);
        }

        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        let child = command.spawn()?;

        Ok(Box::new(NoPtyProcess { child }))
    }
}

struct NoPtyProcess {
    child: std::process::Child,
}

impl PtyProcess for NoPtyProcess {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(ref mut stdout) = self.child.stdout {
            use std::io::Read;
            stdout.read(buf)
        } else {
            Ok(0)
        }
    }

    fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }
}

// --- Mock implementations for testing ---

use std::sync::{Arc, Mutex};

/// Configuration for creating a MockPtyProcess.
#[derive(Clone, Default)]
pub struct MockPtyConfig {
    /// Output data the mock process will produce
    pub output: Vec<Vec<u8>>,
    /// Exit code to return
    pub exit_code: i32,
    /// Whether spawn should fail with an error
    pub spawn_error: Option<String>,
    /// Delay between output chunks in milliseconds (for timeout testing)
    pub chunk_delay_ms: u64,
}

/// Mock PTY spawner for testing.
pub struct MockPtySpawner {
    config: Arc<Mutex<MockPtyConfig>>,
}

impl MockPtySpawner {
    pub fn new(config: MockPtyConfig) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
        }
    }

    /// Create a MockPtySpawner that produces the given output and exits with the given code.
    pub fn with_output_and_exit(output: Vec<Vec<u8>>, exit_code: i32) -> Self {
        Self::new(MockPtyConfig {
            output,
            exit_code,
            spawn_error: None,
            chunk_delay_ms: 0,
        })
    }

    /// Create a MockPtySpawner with a delay between chunks (for timeout testing).
    pub fn with_slow_output(output: Vec<Vec<u8>>, exit_code: i32, chunk_delay_ms: u64) -> Self {
        Self::new(MockPtyConfig {
            output,
            exit_code,
            spawn_error: None,
            chunk_delay_ms,
        })
    }

    /// Create a MockPtySpawner that fails to spawn with the given error.
    pub fn with_spawn_error(error: &str) -> Self {
        Self::new(MockPtyConfig {
            spawn_error: Some(error.to_string()),
            ..Default::default()
        })
    }
}

impl PtySpawner for MockPtySpawner {
    fn spawn(
        &self,
        _cmd: portable_pty::CommandBuilder,
        _rows: u16,
        _cols: u16,
    ) -> anyhow::Result<Box<dyn PtyProcess>> {
        let config = self.config.lock().unwrap().clone();

        if let Some(error) = config.spawn_error {
            return Err(anyhow::anyhow!(error));
        }

        Ok(Box::new(MockPtyProcess {
            output_chunks: config.output,
            chunk_index: 0,
            exit_code: config.exit_code,
            chunk_delay_ms: config.chunk_delay_ms,
        }))
    }
}

/// Mock PTY process for testing.
pub struct MockPtyProcess {
    output_chunks: Vec<Vec<u8>>,
    chunk_index: usize,
    exit_code: i32,
    chunk_delay_ms: u64,
}

impl PtyProcess for MockPtyProcess {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.chunk_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(self.chunk_delay_ms));
        }

        if self.chunk_index >= self.output_chunks.len() {
            // Simulate EOF
            return Ok(0);
        }

        let chunk = &self.output_chunks[self.chunk_index];
        let len = std::cmp::min(buf.len(), chunk.len());
        buf[..len].copy_from_slice(&chunk[..len]);
        self.chunk_index += 1;
        Ok(len)
    }

    fn kill(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        Ok(exit_status_from_code(self.exit_code))
    }
}

/// Helper to create an ExitStatus from a raw exit code.
/// On Windows, uses a direct approach; on Unix, encodes the exit code.
fn exit_status_from_code(code: i32) -> ExitStatus {
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(code as u32)
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(code << 8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_pty_spawner_returns_configured_output_and_exit_code() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"hello\n".to_vec()], 0);
        let cmd = portable_pty::CommandBuilder::new("echo");
        let mut process = spawner.spawn(cmd, 24, 80).expect("spawn");

        let mut buf = [0u8; 1024];
        let n = process.read(&mut buf).expect("read");
        assert_eq!(&buf[..n], b"hello\n");

        // Next read should return EOF (0)
        let n = process.read(&mut buf).expect("read");
        assert_eq!(n, 0);

        let status = process.wait().expect("wait");
        assert!(status.success());
    }

    #[test]
    fn test_mock_pty_spawner_nonzero_exit() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"error output\n".to_vec()], 1);
        let cmd = portable_pty::CommandBuilder::new("fail");
        let mut process = spawner.spawn(cmd, 24, 80).expect("spawn");

        let mut buf = [0u8; 1024];
        let n = process.read(&mut buf).expect("read");
        assert_eq!(&buf[..n], b"error output\n");

        let status = process.wait().expect("wait");
        assert!(!status.success());
    }

    #[test]
    fn test_mock_pty_spawner_spawn_error() {
        let spawner = MockPtySpawner::with_spawn_error("PTY not available");
        let cmd = portable_pty::CommandBuilder::new("echo");
        let result = spawner.spawn(cmd, 24, 80);
        assert!(result.is_err());
        let err = result.err().expect("should be an error");
        assert!(err.to_string().contains("PTY not available"));
    }

    #[test]
    fn test_mock_pty_process_multiple_chunks() {
        let spawner = MockPtySpawner::with_output_and_exit(
            vec![
                b"chunk1\n".to_vec(),
                b"chunk2\n".to_vec(),
                b"chunk3\n".to_vec(),
            ],
            0,
        );
        let cmd = portable_pty::CommandBuilder::new("echo");
        let mut process = spawner.spawn(cmd, 24, 80).expect("spawn");

        let mut buf = [0u8; 1024];

        let n = process.read(&mut buf).expect("read");
        assert_eq!(&buf[..n], b"chunk1\n");

        let n = process.read(&mut buf).expect("read");
        assert_eq!(&buf[..n], b"chunk2\n");

        let n = process.read(&mut buf).expect("read");
        assert_eq!(&buf[..n], b"chunk3\n");

        // EOF
        let n = process.read(&mut buf).expect("read");
        assert_eq!(n, 0);
    }

    #[test]
    fn test_mock_pty_process_empty_output() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![], 0);
        let cmd = portable_pty::CommandBuilder::new("true");
        let mut process = spawner.spawn(cmd, 24, 80).expect("spawn");

        let mut buf = [0u8; 1024];
        let n = process.read(&mut buf).expect("read");
        assert_eq!(n, 0);

        let status = process.wait().expect("wait");
        assert!(status.success());
    }

    #[test]
    fn test_mock_pty_process_kill() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![], 0);
        let cmd = portable_pty::CommandBuilder::new("sleep");
        let mut process = spawner.spawn(cmd, 24, 80).expect("spawn");
        assert!(process.kill().is_ok());
    }

    #[test]
    fn test_exit_status_from_code_zero() {
        let status = exit_status_from_code(0);
        assert!(status.success());
    }

    #[test]
    fn test_exit_status_from_code_nonzero() {
        let status = exit_status_from_code(1);
        assert!(!status.success());
    }
}
