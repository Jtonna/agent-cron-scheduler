// PTY module - Process spawning abstraction.
// Provides NoPtySpawner (piped I/O) for production and MockPtySpawner for testing.

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

    /// Spawn a process from a plain argv array without any shell wrapper.
    ///
    /// `argv[0]` is the program; `argv[1..]` are its arguments. No shell
    /// (`cmd /C` or `sh -c`) is involved — the OS receives each argument as a
    /// discrete string, eliminating all shell-escaping concerns.
    ///
    /// `cwd` sets the working directory when present. `env` variables are
    /// layered on top of the inherited environment.
    fn spawn_argv(
        &self,
        argv: &[String],
        cwd: Option<&std::path::Path>,
        env: &std::collections::HashMap<String, String>,
        rows: u16,
        cols: u16,
    ) -> anyhow::Result<Box<dyn PtyProcess>>;
}

/// Trait for interacting with a spawned PTY process.
pub trait PtyProcess: Send {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn kill(&mut self) -> io::Result<()>;
    fn wait(&mut self) -> io::Result<ExitStatus>;
    /// Write data to the process's stdin. Default is no-op.
    fn write_stdin(&mut self, _data: &[u8]) -> io::Result<()> {
        Ok(())
    }
    /// Close the stdin handle, signaling EOF to the process. Default is no-op.
    fn close_stdin(&mut self) {}
    /// Return the OS process ID of the spawned process, if available.
    fn pid(&self) -> Option<u32> {
        None
    }
}

// This module provides NoPtySpawner as the production process spawner.
// It uses piped I/O via std::process::Command, which reliably handles EOF
// on all platforms. PTY emulation is intentionally not used.

// --- NoPty implementation using std::process::Command ---

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

        #[cfg(target_os = "windows")]
        {
            // On Windows, cmd.exe /C needs the command string passed without
            // Rust's automatic re-quoting, otherwise embedded quotes get mangled.
            // Rust's Command::arg() uses MSVC C runtime escaping (backslash-escaping
            // internal quotes), but cmd.exe does not recognize backslash as an escape
            // character — it uses its own parsing rules. Using raw_arg bypasses
            // Rust's automatic quoting and sends the string to CreateProcessW as-is.
            use std::os::windows::process::CommandExt;
            let raw_args: String = args[1..]
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" ");
            command.raw_arg(raw_args);
        }

        #[cfg(not(target_os = "windows"))]
        {
            for arg in &args[1..] {
                command.arg(arg);
            }
        }

        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());

        // Forward working directory and environment variables from CommandBuilder.
        // Previously these were silently dropped, causing jobs with working_dir
        // or env_vars to ignore those settings.
        if let Some(cwd) = cmd.get_cwd() {
            command.current_dir(cwd);
        }
        for (key, val) in cmd.iter_extra_env_as_str() {
            command.env(key, val);
        }

        // Create a new process group so that kill signals can be sent to the
        // entire process tree (not just the immediate child).
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
            command.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }

        let child = command.spawn()?;
        finish_spawn(child)
    }

    fn spawn_argv(
        &self,
        argv: &[String],
        cwd: Option<&std::path::Path>,
        env: &std::collections::HashMap<String, String>,
        _rows: u16,
        _cols: u16,
    ) -> anyhow::Result<Box<dyn PtyProcess>> {
        use std::process::{Command, Stdio};

        if argv.is_empty() {
            return Err(anyhow::anyhow!("spawn_argv: empty argv"));
        }

        let mut command = Command::new(&argv[0]);
        command.args(&argv[1..]);
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());

        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        for (k, v) in env {
            command.env(k, v);
        }

        // Create a new process group so that kill signals propagate to the
        // entire process tree (not just the immediate child).
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
            command.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }

        let child = command.spawn()?;
        finish_spawn(child)
    }
}

/// Wire stdout/stderr reader threads and wrap a freshly-spawned `Child` into a
/// `Box<dyn PtyProcess>`. This is the shared post-spawn machinery extracted
/// from `NoPtySpawner::spawn` and `NoPtySpawner::spawn_argv`.
fn finish_spawn(mut child: std::process::Child) -> anyhow::Result<Box<dyn PtyProcess>> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let (tx, rx) = std::sync::mpsc::channel::<io::Result<Vec<u8>>>();

    if let Some(stdout) = stdout {
        let tx_stdout = tx.clone();
        std::thread::Builder::new()
            .name("stdout-reader".to_string())
            .spawn(move || {
                use std::io::Read;
                let mut reader = stdout;
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if tx_stdout.send(Ok(buf[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(e)
                            if e.kind() == io::ErrorKind::BrokenPipe
                                || e.kind() == io::ErrorKind::UnexpectedEof =>
                        {
                            break;
                        }
                        Err(e) => {
                            let _ = tx_stdout.send(Err(e));
                            break;
                        }
                    }
                }
            })?;
    }

    if let Some(stderr) = stderr {
        std::thread::Builder::new()
            .name("stderr-reader".to_string())
            .spawn(move || {
                use std::io::Read;
                let mut reader = stderr;
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if tx.send(Ok(buf[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(e)
                            if e.kind() == io::ErrorKind::BrokenPipe
                                || e.kind() == io::ErrorKind::UnexpectedEof =>
                        {
                            break;
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e));
                            break;
                        }
                    }
                }
            })?;
    }

    Ok(Box::new(NoPtyProcess {
        child,
        rx,
        leftover: Vec::new(),
    }))
}

struct NoPtyProcess {
    child: std::process::Child,
    rx: std::sync::mpsc::Receiver<io::Result<Vec<u8>>>,
    leftover: Vec<u8>,
}

impl PtyProcess for NoPtyProcess {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Drain leftover bytes from a previous oversized chunk first.
        if !self.leftover.is_empty() {
            let n = std::cmp::min(self.leftover.len(), buf.len());
            buf[..n].copy_from_slice(&self.leftover[..n]);
            self.leftover.drain(..n);
            return Ok(n);
        }

        match self.rx.recv() {
            Ok(Ok(data)) => {
                let n = std::cmp::min(data.len(), buf.len());
                buf[..n].copy_from_slice(&data[..n]);
                if data.len() > n {
                    self.leftover.extend_from_slice(&data[n..]);
                }
                Ok(n)
            }
            Ok(Err(e)) => Err(e),
            // Both senders dropped — EOF
            Err(_) => Ok(0),
        }
    }

    fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }

    fn write_stdin(&mut self, data: &[u8]) -> io::Result<()> {
        if let Some(ref mut stdin) = self.child.stdin {
            use std::io::Write;
            stdin.write_all(data)?;
            stdin.flush()?;
        }
        Ok(())
    }

    fn close_stdin(&mut self) {
        self.child.stdin.take();
    }

    fn pid(&self) -> Option<u32> {
        Some(self.child.id())
    }
}

// --- Mock implementations for testing ---

use std::sync::atomic::{AtomicBool, Ordering};
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
    /// Records the last argv passed to spawn_argv for test assertions.
    pub last_argv: Arc<Mutex<Option<Vec<String>>>>,
}

impl MockPtySpawner {
    pub fn new(config: MockPtyConfig) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            last_argv: Arc::new(Mutex::new(None)),
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

    /// Return the argv recorded from the most recent `spawn_argv` call, if any.
    pub fn recorded_argv(&self) -> Option<Vec<String>> {
        self.last_argv.lock().unwrap().clone()
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
            killed: Arc::new(AtomicBool::new(false)),
        }))
    }

    fn spawn_argv(
        &self,
        argv: &[String],
        _cwd: Option<&std::path::Path>,
        _env: &std::collections::HashMap<String, String>,
        _rows: u16,
        _cols: u16,
    ) -> anyhow::Result<Box<dyn PtyProcess>> {
        // Record argv for test assertions.
        *self.last_argv.lock().unwrap() = Some(argv.to_vec());

        let config = self.config.lock().unwrap().clone();

        if let Some(error) = config.spawn_error {
            return Err(anyhow::anyhow!(error));
        }

        Ok(Box::new(MockPtyProcess {
            output_chunks: config.output,
            chunk_index: 0,
            exit_code: config.exit_code,
            chunk_delay_ms: config.chunk_delay_ms,
            killed: Arc::new(AtomicBool::new(false)),
        }))
    }
}

/// Mock PTY process for testing.
pub struct MockPtyProcess {
    output_chunks: Vec<Vec<u8>>,
    chunk_index: usize,
    exit_code: i32,
    chunk_delay_ms: u64,
    killed: Arc<AtomicBool>,
}

impl PtyProcess for MockPtyProcess {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // If killed, return EOF immediately.
        if self.killed.load(Ordering::SeqCst) {
            return Ok(0);
        }

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
        self.killed.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        if self.killed.load(Ordering::SeqCst) {
            return Ok(exit_status_from_code(self.exit_code));
        }
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

    /// Helper: read all output from a PtyProcess until EOF, returning the collected bytes.
    fn read_all(process: &mut Box<dyn PtyProcess>) -> Vec<u8> {
        let mut output = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match process.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => output.extend_from_slice(&buf[..n]),
                Err(e) => panic!("read error: {e}"),
            }
        }
        output
    }

    #[test]
    fn test_nopty_stderr_captured() {
        let spawner = NoPtySpawner;
        let (shell, flag) = if cfg!(windows) {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };
        let mut cmd = portable_pty::CommandBuilder::new(shell);
        cmd.arg(flag);
        cmd.arg("echo stderr_test_output 1>&2");
        let mut process = spawner.spawn(cmd, 24, 80).expect("spawn");

        let output = read_all(&mut process);
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("stderr_test_output"),
            "expected 'stderr_test_output' in output, got: {text:?}"
        );
    }

    #[test]
    fn test_nopty_stdout_and_stderr_merged() {
        let spawner = NoPtySpawner;
        let (shell, flag) = if cfg!(windows) {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };
        let mut cmd = portable_pty::CommandBuilder::new(shell);
        cmd.arg(flag);
        cmd.arg("echo stdout_output && echo stderr_output 1>&2");
        let mut process = spawner.spawn(cmd, 24, 80).expect("spawn");

        let output = read_all(&mut process);
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("stdout_output"),
            "expected 'stdout_output' in merged output, got: {text:?}"
        );
        assert!(
            text.contains("stderr_output"),
            "expected 'stderr_output' in merged output, got: {text:?}"
        );
    }

    #[test]
    fn test_nopty_eof_after_both_streams_close() {
        let spawner = NoPtySpawner;
        let (shell, flag) = if cfg!(windows) {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };
        let mut cmd = portable_pty::CommandBuilder::new(shell);
        cmd.arg(flag);
        cmd.arg("echo hello");
        let mut process = spawner.spawn(cmd, 24, 80).expect("spawn");

        // Read until EOF — this must terminate cleanly (not hang or error).
        let output = read_all(&mut process);
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("hello"),
            "expected 'hello' in output before EOF, got: {text:?}"
        );
    }
}
