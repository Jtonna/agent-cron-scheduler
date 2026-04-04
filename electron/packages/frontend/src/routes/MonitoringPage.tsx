import React, { useState, useEffect, useCallback } from "react";
import { ExclamationTriangleIcon, ArrowPathIcon } from "@heroicons/react/24/outline";
import { useHealth } from "@/hooks/useHealth";
import { useSSEEvents } from "@/hooks/useSSE";
import { useSystemLogs } from "@/hooks/useSystemLogs";
import { CostOverviewSection } from "@/components/monitoring/CostOverviewSection";
import { LogViewer } from "@/components/LogViewer";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { formatUptime } from "@/lib/format";
import type { HealthResponse } from "@/lib/types";

// ─── Skeleton for health strip ────────────────────────────────────────────────

function SkeletonHealthStrip() {
  return (
    <Card>
      <CardContent className="py-2 px-4">
        <div className="flex flex-wrap items-center gap-4">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="flex items-center gap-2">
              <div className="h-3 w-14 animate-pulse rounded bg-muted" />
              <div className="h-3 w-20 animate-pulse rounded bg-muted" />
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

// ─── Compact System Health strip ──────────────────────────────────────────────

function HealthStrip({ health }: { health: HealthResponse }) {
  return (
    <Card>
      <CardContent className="py-2 px-4">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm">
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground">Status:</span>
            <Badge variant={health.status === "ok" ? "success" : "error"} className="text-xs">
              {health.status}
            </Badge>
          </div>
          <span className="hidden sm:inline text-border">|</span>
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground">Uptime:</span>
            <span className="font-medium">{formatUptime(health.uptime_seconds)}</span>
          </div>
          <span className="hidden sm:inline text-border">|</span>
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground">Jobs:</span>
            <span className="font-medium">{health.active_jobs}/{health.total_jobs} active</span>
          </div>
          <span className="hidden sm:inline text-border">|</span>
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground">Version:</span>
            <span className="font-medium">{health.version}</span>
          </div>
          <span className="hidden sm:inline text-border">|</span>
          <div className="flex items-center gap-1.5 min-w-0">
            <span className="text-muted-foreground shrink-0">Data:</span>
            <span className="font-medium truncate" title={health.data_dir}>{health.data_dir}</span>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

// ─── System Logs section (always visible) ────────────────────────────────────

function SystemLogsSection() {
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
    <section aria-label="System Logs" className="flex flex-col gap-3 flex-1 min-h-0">
      {/* Header + controls */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-xl font-semibold">System Logs</h2>
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
        <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
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
          />
        )}
      </div>
    </section>
  );
}

// ─── MonitoringPage ───────────────────────────────────────────────────────────

export function MonitoringPage() {
  const { health, loading, error, refresh } = useHealth(5000);

  useSSEEvents(
    useCallback(
      (event) => {
        if (event.type === "job_changed") {
          refresh();
        }
      },
      [refresh]
    )
  );

  return (
    <div className="flex flex-col gap-4 w-full h-full">
      {/* Page title */}
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Monitoring</h1>
        <p className="text-sm text-muted-foreground">
          System health, cost overview, and logs
        </p>
      </div>

      {/* Section 1: Compact System Health strip */}
      <section aria-label="System Health">
        {loading && !health && <SkeletonHealthStrip />}

        {error && !health && (
          <Card className="border-destructive/50 bg-destructive/10">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-destructive">
                <ExclamationTriangleIcon className="h-5 w-5" />
                Connection Error
              </CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-destructive/90">
                Failed to connect to server: {error}
              </p>
            </CardContent>
          </Card>
        )}

        {health && <HealthStrip health={health} />}
      </section>

      {/* Section 2: Cost Overview */}
      <CostOverviewSection />

      {/* Section 3: System Logs (always visible, dominant) */}
      <SystemLogsSection />
    </div>
  );
}
