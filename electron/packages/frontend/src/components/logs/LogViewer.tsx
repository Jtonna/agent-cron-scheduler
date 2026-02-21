"use client";

import React, { useEffect, useRef, useState, useCallback } from "react";
import { ArrowPathIcon } from "@heroicons/react/24/outline";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useRunLog } from "@/hooks/useRunLog";
import { useSSEEvents } from "@/hooks/useSSE";

interface LogViewerProps {
  runId: string | null;
  jobId?: string;
}

export function LogViewer({ runId, jobId }: LogViewerProps) {
  const { log, loading, error, refresh } = useRunLog(runId);
  const [streamedContent, setStreamedContent] = useState<string>("");
  const bottomRef = useRef<HTMLDivElement>(null);

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
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [log, streamedContent]);

  if (!runId) {
    return (
      <div className="flex items-center justify-center py-8">
        <p className="text-sm text-muted-foreground">
          Select a run to view its log output.
        </p>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8">
        <ArrowPathIcon className="h-6 w-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
        <p className="text-sm text-destructive">
          Error loading log: {error}
        </p>
      </div>
    );
  }

  const displayContent = log + streamedContent;

  return (
    <ScrollArea className="h-96 rounded-md border">
      <pre className="font-mono text-[13px] leading-relaxed bg-muted text-foreground p-4 whitespace-pre-wrap break-words">
        {displayContent || "No output."}
        <div ref={bottomRef} />
      </pre>
    </ScrollArea>
  );
}
