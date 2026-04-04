"use client";

import React, { useEffect, useLayoutEffect, useRef, useState, useCallback } from "react";
import { ArrowPathIcon, ArrowDownIcon } from "@heroicons/react/24/outline";
import { Button } from "@/components/ui/button";
import { useRunLog } from "@/hooks/useRunLog";
import { useSSEEvents } from "@/hooks/useSSE";

interface LogViewerProps {
  runId: string | null;
  jobId?: string;
}

export function LogViewer({ runId, jobId }: LogViewerProps) {
  const { log, loading, error, refresh } = useRunLog(runId);
  const [streamedContent, setStreamedContent] = useState<string>("");
  const [userScrolled, setUserScrolled] = useState(false);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  // Stores scroll state captured before a render so we can restore it after
  const prevScrollRef = useRef<{ scrollTop: number; scrollHeight: number } | null>(null);

  // Reset streamed content and scroll state when runId changes
  useEffect(() => {
    setStreamedContent("");
    setUserScrolled(false);
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

  // Scroll tracking
  const handleScroll = useCallback(() => {
    const el = scrollContainerRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
    setUserScrolled(!atBottom);
  }, []);

  // Capture scroll position before every render so we can restore it after
  // if the user has scrolled up (prevents viewport shift from layout reflows).
  if (scrollContainerRef.current && userScrolled) {
    const el = scrollContainerRef.current;
    prevScrollRef.current = { scrollTop: el.scrollTop, scrollHeight: el.scrollHeight };
  } else {
    prevScrollRef.current = null;
  }

  // After render: restore scroll position when user has scrolled up so that
  // appended content (or content mutations) don't shift the visible viewport.
  useLayoutEffect(() => {
    const el = scrollContainerRef.current;
    if (!el || !userScrolled || !prevScrollRef.current) return;
    const { scrollTop: prevScrollTop, scrollHeight: prevScrollHeight } = prevScrollRef.current;
    const scrollHeightDelta = el.scrollHeight - prevScrollHeight;
    if (scrollHeightDelta !== 0) {
      el.scrollTop = prevScrollTop + scrollHeightDelta;
    }
  });

  // Auto-scroll to bottom (instant, no animation to fight rapid content updates)
  useEffect(() => {
    if (!userScrolled) {
      const el = scrollContainerRef.current;
      if (el) {
        el.scrollTop = el.scrollHeight;
      }
    }
  }, [log, streamedContent, userScrolled]);

  const jumpToBottom = useCallback(() => {
    const el = scrollContainerRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
    setUserScrolled(false);
  }, []);

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
    <div className="flex flex-col rounded-md border">
      <div
        ref={scrollContainerRef}
        onScroll={handleScroll}
        className="h-96 overflow-auto"
      >
        <pre className="font-mono text-[13px] leading-relaxed bg-muted text-foreground p-4 whitespace-pre-wrap break-words">
          {displayContent || "No output."}
        </pre>
      </div>
      {userScrolled && (
        <div className="flex justify-center py-1 border-t bg-muted/30">
          <Button
            variant="ghost"
            size="sm"
            className="h-6 text-xs gap-1"
            onClick={jumpToBottom}
          >
            <ArrowDownIcon className="h-3 w-3" />
            Jump to bottom
          </Button>
        </div>
      )}
    </div>
  );
}
