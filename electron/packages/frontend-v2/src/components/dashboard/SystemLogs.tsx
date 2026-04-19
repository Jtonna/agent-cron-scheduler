"use client";

import { useState, useEffect } from "react";
import { ArrowPathIcon } from "@heroicons/react/24/outline";
import { useSystemLogs } from "@/hooks/useSystemLogs";
import { LogViewer } from "@/components/LogViewer";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";

export function SystemLogs() {
  const [tail, setTail] = useState("200");
  const [autoRefresh, setAutoRefresh] = useState(true);
  const { logs, loading, error, refresh } = useSystemLogs(Number(tail));

  // Auto-refresh polling
  useEffect(() => {
    if (!autoRefresh) return;
    const timer = setInterval(refresh, 3000);
    return () => clearInterval(timer);
  }, [autoRefresh, refresh]);

  return (
    <section
      id="system-logs"
      aria-label="System Logs"
      className="flex flex-col gap-3 flex-1 min-h-0"
    >
      {/* Card container */}
      <div className="rounded-xl bg-card shadow-[var(--shadow-card)] flex flex-col flex-1 min-h-0 overflow-hidden">
        {/* Header + controls */}
        <div className="flex flex-wrap items-center justify-between gap-3 px-5 py-4 border-b border-border">
          <h2 className="text-lg font-semibold" id="system-logs">
            System Logs
          </h2>
          <div className="flex flex-wrap items-center gap-3">
            <Tabs value={tail} onValueChange={setTail}>
              <TabsList>
                <TabsTrigger value="100">100</TabsTrigger>
                <TabsTrigger value="200">200</TabsTrigger>
                <TabsTrigger value="500">500</TabsTrigger>
                <TabsTrigger value="1000">1K</TabsTrigger>
              </TabsList>
            </Tabs>
            <div className="flex items-center gap-2">
              <Switch
                id="logs-auto-refresh"
                checked={autoRefresh}
                onCheckedChange={setAutoRefresh}
              />
              <Label htmlFor="logs-auto-refresh">Auto-refresh</Label>
            </div>
          </div>
        </div>

        {/* Error state */}
        {error && (
          <div className="mx-5 mt-3 rounded-lg border border-destructive/50 bg-destructive/10 p-4">
            <p className="text-sm text-destructive">{error}</p>
          </div>
        )}

        {/* Log viewer */}
        <div className="flex-1 min-h-0">
          {loading && !logs ? (
            <div className="flex items-center justify-center py-12">
              <ArrowPathIcon className="h-8 w-8 animate-spin text-muted-foreground" />
            </div>
          ) : (
            <LogViewer
              content={logs || ""}
              loading={false}
              error={null}
              live={autoRefresh}
              maxHeight="100%"
              className="border-0 rounded-none"
            />
          )}
        </div>
      </div>
    </section>
  );
}
