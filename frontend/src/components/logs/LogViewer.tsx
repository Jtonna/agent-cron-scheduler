"use client";

import React, { useEffect, useRef, useState, useCallback } from "react";
import { Column, Row, Text } from "@once-ui-system/core";
import { useRunLog } from "@/hooks/useRunLog";
import { useSSEEvents } from "@/hooks/useSSE";
import { Spinner } from "@/components/ui/Spinner";

interface LogViewerProps {
  runId: string | null;
  jobId?: string;
}

export function LogViewer({ runId, jobId }: LogViewerProps) {
  const { log, loading, error, refresh } = useRunLog(runId);
  const [streamedContent, setStreamedContent] = useState<string>("");
  const preRef = useRef<HTMLPreElement>(null);

  // Reset streamed content when runId changes
  useEffect(() => {
    setStreamedContent("");
  }, [runId]);

  // SSE streaming for running jobs
  useSSEEvents(
    useCallback(
      (event) => {
        if (event.type === "output" && runId) {
          try {
            const data = JSON.parse(event.data);
            if (data.run_id === runId) {
              setStreamedContent((prev) => prev + (data.data || ""));
            }
          } catch {
            // Ignore parse errors
          }
        }
        // Refresh log when job completes
        if (
          (event.type === "completed" || event.type === "failed" || event.type === "killed") &&
          jobId
        ) {
          try {
            const data = JSON.parse(event.data);
            if (data.job_id === jobId) {
              refresh();
            }
          } catch {
            // Ignore
          }
        }
      },
      [runId, jobId, refresh]
    )
  );

  // Auto-scroll to bottom
  useEffect(() => {
    if (preRef.current) {
      preRef.current.scrollTop = preRef.current.scrollHeight;
    }
  }, [log, streamedContent]);

  if (!runId) {
    return (
      <Column horizontal="center" paddingY="32">
        <Text variant="body-default-s" onBackground="neutral-weak">
          Select a run to view its log output.
        </Text>
      </Column>
    );
  }

  if (loading) {
    return (
      <Row horizontal="center" paddingY="32">
        <Spinner size="md" />
      </Row>
    );
  }

  if (error) {
    return (
      <Column horizontal="center" paddingY="32">
        <Text variant="body-default-s" onBackground="danger-strong">
          Error loading log: {error}
        </Text>
      </Column>
    );
  }

  const displayContent = log + streamedContent;

  return (
    <pre
      ref={preRef}
      className="log-pre"
      style={{ maxHeight: "384px" }}
    >
      {displayContent || "No output."}
    </pre>
  );
}
