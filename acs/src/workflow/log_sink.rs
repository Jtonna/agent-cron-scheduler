use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::workflow::step::LogSink;

/// Concrete [`LogSink`] that writes to a single combined file.
///
/// Marker format:
/// ```text
/// ===== ACS-<VERSION>:STEP:<step_id>:START:<iso8601> =====\n
/// <chunks>
/// ===== ACS-<VERSION>:STEP:<step_id>:END:exit=<code or -1>:<iso8601> =====\n
/// ```
pub struct FileLogSink {
    file: Arc<Mutex<File>>,
    pos: Arc<Mutex<u64>>,
    path: PathBuf,
}

impl FileLogSink {
    /// Open (or create) the log file at `path` in append mode.
    pub async fn create(path: PathBuf) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        // Determine the current size so we track position correctly from
        // wherever we were appended to.
        let metadata = tokio::fs::metadata(&path).await?;
        let initial_pos = metadata.len();

        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            pos: Arc::new(Mutex::new(initial_pos)),
            path,
        })
    }

    /// Return the path this sink writes to.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

#[async_trait]
impl LogSink for FileLogSink {
    async fn write_step_start(
        &self,
        step_id: &str,
        started_at: DateTime<Utc>,
    ) -> std::io::Result<u64> {
        let version = env!("CARGO_PKG_VERSION");
        let marker = format!(
            "===== ACS-{}:STEP:{}:START:{} =====\n",
            version,
            step_id,
            started_at.to_rfc3339(),
        );
        let marker_bytes = marker.as_bytes();

        let mut file = self.file.lock().await;
        let mut pos = self.pos.lock().await;

        let offset_before = *pos;
        file.write_all(marker_bytes).await?;
        file.flush().await?;
        *pos += marker_bytes.len() as u64;

        Ok(offset_before)
    }

    async fn write_chunk(&self, data: &[u8]) -> std::io::Result<()> {
        let mut file = self.file.lock().await;
        let mut pos = self.pos.lock().await;

        file.write_all(data).await?;
        file.flush().await?;
        *pos += data.len() as u64;

        Ok(())
    }

    async fn write_step_end(
        &self,
        step_id: &str,
        exit_code: Option<i32>,
        finished_at: DateTime<Utc>,
    ) -> std::io::Result<u64> {
        let version = env!("CARGO_PKG_VERSION");
        let code_str = exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "-1".to_string());
        let marker = format!(
            "===== ACS-{}:STEP:{}:END:exit={}:{} =====\n",
            version,
            step_id,
            code_str,
            finished_at.to_rfc3339(),
        );
        let marker_bytes = marker.as_bytes();

        let mut file = self.file.lock().await;
        let mut pos = self.pos.lock().await;

        file.write_all(marker_bytes).await?;
        file.flush().await?;
        *pos += marker_bytes.len() as u64;

        // Return offset just AFTER the marker line ended
        Ok(*pos)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::NamedTempFile;

    /// Create a temporary file and a `FileLogSink` over it.
    async fn setup_sink() -> (FileLogSink, NamedTempFile) {
        let tmp = NamedTempFile::new().expect("create temp file");
        // Close the NamedTempFile handle so we can reopen for appending on
        // Windows, but keep the path alive. We'll reopen via FileLogSink.
        let path = tmp.path().to_path_buf();
        let sink = FileLogSink::create(path)
            .await
            .expect("create FileLogSink");
        (sink, tmp)
    }

    #[tokio::test]
    async fn test_markers_and_chunk_written_in_order() {
        let (sink, tmp) = setup_sink().await;
        let t1 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 5).unwrap();

        sink.write_step_start("my-step", t1)
            .await
            .expect("write start");
        sink.write_chunk(b"hello output\n")
            .await
            .expect("write chunk");
        sink.write_step_end("my-step", Some(0), t2)
            .await
            .expect("write end");

        let content = std::fs::read_to_string(tmp.path()).expect("read file");
        assert!(content.contains("ACS-"), "version marker present");
        assert!(content.contains(":STEP:my-step:START:"), "start marker");
        assert!(content.contains(":STEP:my-step:END:exit=0:"), "end marker");
        assert!(content.contains("hello output\n"), "chunk content");

        // Verify ordering: START before chunk before END
        let start_pos = content.find(":START:").expect("START exists");
        let chunk_pos = content.find("hello output").expect("chunk exists");
        let end_pos = content.find(":END:").expect("END exists");
        assert!(start_pos < chunk_pos, "START before chunk");
        assert!(chunk_pos < end_pos, "chunk before END");
    }

    #[tokio::test]
    async fn test_start_offset_is_position_before_marker() {
        let (sink, _tmp) = setup_sink().await;
        let t = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();

        // File starts empty — start offset should be 0
        let start_offset = sink
            .write_step_start("s1", t)
            .await
            .expect("write start");
        assert_eq!(start_offset, 0, "start offset should be 0 for fresh file");
    }

    #[tokio::test]
    async fn test_end_offset_is_position_after_marker() {
        let (sink, tmp) = setup_sink().await;
        let t1 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 1).unwrap();

        sink.write_step_start("s1", t1)
            .await
            .expect("write start");
        sink.write_chunk(b"data").await.expect("chunk");
        let end_offset = sink
            .write_step_end("s1", Some(0), t2)
            .await
            .expect("write end");

        let file_len = std::fs::metadata(tmp.path()).expect("metadata").len();
        assert_eq!(end_offset, file_len, "end offset should equal file length");
    }

    #[tokio::test]
    async fn test_write_chunk_preserves_binary_data() {
        let (sink, tmp) = setup_sink().await;

        // Include null bytes and arbitrary bytes
        let data: Vec<u8> = (0u8..=15).collect();
        sink.write_chunk(&data).await.expect("write chunk");

        let written = std::fs::read(tmp.path()).expect("read file");
        assert_eq!(written, data, "binary data must be preserved verbatim");
    }

    #[tokio::test]
    async fn test_no_exit_code_renders_minus_one() {
        let (sink, tmp) = setup_sink().await;
        let t1 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 1).unwrap();

        sink.write_step_start("s1", t1)
            .await
            .expect("write start");
        sink.write_step_end("s1", None, t2)
            .await
            .expect("write end");

        let content = std::fs::read_to_string(tmp.path()).expect("read");
        assert!(
            content.contains("exit=-1"),
            "None exit code should render as -1; got: {}",
            content
        );
    }

    #[tokio::test]
    async fn test_multiple_steps_sequential_offsets() {
        let (sink, _tmp) = setup_sink().await;
        let t = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();

        let start1 = sink.write_step_start("step-a", t).await.expect("start1");
        sink.write_chunk(b"step-a output\n")
            .await
            .expect("chunk1");
        let end1 = sink
            .write_step_end("step-a", Some(0), t)
            .await
            .expect("end1");

        let start2 = sink.write_step_start("step-b", t).await.expect("start2");
        let end2 = sink
            .write_step_end("step-b", Some(1), t)
            .await
            .expect("end2");

        // Offsets must be monotonically increasing
        assert!(start1 < end1, "s1: start < end");
        assert_eq!(end1, start2, "step-b starts where step-a ended");
        assert!(start2 < end2, "s2: start < end");
    }
}
