"use client";

import { useState, useCallback, useRef, useMemo } from "react";
import { Button } from "react-aria-components";
import { RefreshCw, Wifi, WifiOff, ChevronDown } from "lucide-react";
import { Navbar } from "@/components/Navbar";
import { useSystemLogs } from "@/apis/useSystemLogs";
import { useSSEEvents, useSSEConnected } from "@/apis/sse";
import dynamic from "next/dynamic";

const LazyLog = dynamic(
  () =>
    import("@melloware/react-logviewer").then((mod) => mod.LazyLog),
  { ssr: false },
);

const TAIL_OPTIONS = [100, 250, 500, 1000, 5000];

export default function SystemLogsPage() {
  const [tail, setTail] = useState(500);
  const [tailOpen, setTailOpen] = useState(false);
  const { logs, loading, error, refresh, setLogs } = useSystemLogs(tail);
  const connected = useSSEConnected();
  const [refreshing, setRefreshing] = useState(false);
  const logsRef = useRef(logs);
  logsRef.current = logs;

  // Append new log output from SSE in real-time
  useSSEEvents(
    useCallback(
      (event) => {
        if (event.type === "output") {
          try {
            const parsed = JSON.parse(event.data);
            // System log output events have a "line" or "data" field
            const line =
              parsed.line ?? parsed.data ?? parsed.message ?? event.data;
            if (typeof line === "string" && line.length > 0) {
              setLogs((prev) => (prev ? prev + "\n" + line : line));
            }
          } catch {
            // If not JSON, append raw data
            if (event.data && event.data.length > 0) {
              setLogs((prev) =>
                prev ? prev + "\n" + event.data : event.data,
              );
            }
          }
        }
      },
      [setLogs],
    ),
  );

  const handleRefresh = useCallback(async () => {
    setRefreshing(true);
    await refresh();
    setRefreshing(false);
  }, [refresh]);

  // Memoize the log text so LazyLog doesn't re-render on every parent render
  const logText = useMemo(() => logs || " ", [logs]);

  return (
    <div className="min-h-screen h-screen bg-surface text-fg flex flex-col">
      <Navbar />

      {/* Header + Controls */}
      <div className="px-8 pt-6 pb-4 flex items-center justify-between border-b border-border-subtle">
        <div>
          <h1 className="text-2xl font-extrabold tracking-tight">
            System Logs
          </h1>
          <p className="text-fg-muted text-sm mt-0.5">
            Live daemon log output
          </p>
        </div>

        <div className="flex items-center gap-3">
          {/* Connection indicator */}
          <div
            className={`flex items-center gap-1.5 text-xs font-medium px-3 py-1.5 rounded-pill border ${
              connected
                ? "text-status-success border-status-success-border bg-status-success-bg"
                : "text-status-failed border-status-failed-border bg-status-failed-bg"
            }`}
          >
            {connected ? <Wifi size={12} /> : <WifiOff size={12} />}
            {connected ? "Connected" : "Disconnected"}
          </div>

          {/* Tail line count selector */}
          <div className="relative">
            <Button
              onPress={() => setTailOpen((o) => !o)}
              className="flex items-center gap-1.5 text-sm font-medium px-3 py-1.5 rounded-input border border-border bg-surface-secondary hover:bg-surface-tertiary cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-brand-ring"
            >
              {tail} lines
              <ChevronDown size={12} />
            </Button>
            {tailOpen && (
              <div className="absolute right-0 top-full mt-1 bg-surface border border-border rounded-input shadow-menu z-50 py-1 min-w-[120px]">
                {TAIL_OPTIONS.map((n) => (
                  <button
                    key={n}
                    onClick={() => {
                      setTail(n);
                      setTailOpen(false);
                    }}
                    className={`w-full text-left px-3 py-1.5 text-sm hover:bg-surface-secondary cursor-pointer ${
                      n === tail
                        ? "text-brand font-semibold"
                        : "text-fg-secondary"
                    }`}
                  >
                    {n} lines
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Refresh button */}
          <Button
            onPress={handleRefresh}
            isDisabled={refreshing}
            className="flex items-center gap-1.5 text-sm font-medium px-4 py-1.5 rounded-input bg-brand hover:bg-brand-hover text-surface cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-brand-ring disabled:opacity-50"
          >
            <RefreshCw
              size={14}
              className={refreshing ? "animate-spin" : ""}
            />
            Refresh
          </Button>
        </div>
      </div>

      {/* Log viewer area */}
      <div className="flex-1 min-h-0 relative">
        {loading && !logs && (
          <div className="absolute inset-0 flex items-center justify-center bg-surface z-10">
            <div className="flex flex-col items-center gap-2">
              <RefreshCw size={24} className="animate-spin text-fg-muted" />
              <span className="text-fg-muted text-sm">Loading logs...</span>
            </div>
          </div>
        )}

        {error && !logs && (
          <div className="absolute inset-0 flex items-center justify-center bg-surface z-10">
            <div className="flex flex-col items-center gap-3">
              <span className="text-status-failed text-sm font-medium">
                {error}
              </span>
              <Button
                onPress={handleRefresh}
                className="text-sm font-medium px-4 py-1.5 rounded-input bg-brand hover:bg-brand-hover text-surface cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-brand-ring"
              >
                Retry
              </Button>
            </div>
          </div>
        )}

        <LazyLog
          text={logText}
          follow
          enableSearch
          enableHotKeys
          selectableLines
          enableLineNumbers
          extraLines={1}
          height="auto"
          style={{
            background: "var(--color-surface)",
            color: "var(--color-fg)",
            fontFamily: "var(--font-geist-mono), monospace",
            fontSize: "13px",
          }}
          containerStyle={{
            overflow: "auto",
          }}
        />
      </div>
    </div>
  );
}
