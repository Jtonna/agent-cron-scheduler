"use client";

import React, { useState, useEffect } from "react";
import { ArrowPathIcon } from "@heroicons/react/24/outline";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { LogViewer } from "@/components/LogViewer";
import { useSystemLogs } from "@/hooks/useSystemLogs";

export default function SystemLogsPage() {
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
    <div className="flex flex-col gap-5 w-full">
      <h1 className="text-2xl font-semibold tracking-tight">System Logs</h1>

      {/* Controls */}
      <div className="flex flex-wrap items-center gap-3 w-full">
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
            id="auto-refresh"
            checked={autoRefresh}
            onCheckedChange={setAutoRefresh}
          />
          <Label htmlFor="auto-refresh">Auto-refresh</Label>
        </div>
      </div>

      {/* Error state */}
      {error && (
        <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
          <p className="text-sm text-destructive">{error}</p>
        </div>
      )}

      {/* Log viewer */}
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
          maxHeight="calc(100vh - 260px)"
        />
      )}
    </div>
  );
}
