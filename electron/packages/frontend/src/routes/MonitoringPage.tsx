import React, { useState, useEffect, useCallback } from "react";
import { ChevronDownIcon, ExclamationTriangleIcon, ArrowPathIcon } from "@heroicons/react/24/outline";
import { useHealth } from "@/hooks/useHealth";
import { useSSEEvents } from "@/hooks/useSSE";
import { useSystemLogs } from "@/hooks/useSystemLogs";
import { DashboardGrid } from "@/components/dashboard/DashboardGrid";
import { CostOverviewSection } from "@/components/monitoring/CostOverviewSection";
import { LogViewer } from "@/components/LogViewer";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

// ─── Skeleton for health cards ────────────────────────────────────────────────

function SkeletonCards() {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 w-full">
      {Array.from({ length: 5 }).map((_, i) => (
        <Card key={i}>
          <CardHeader className="pb-2">
            <div className="h-3 w-24 animate-pulse rounded bg-muted" />
          </CardHeader>
          <CardContent>
            <div className="flex items-center gap-3">
              <div className="h-9 w-9 shrink-0 animate-pulse rounded-md bg-muted" />
              <div className="h-5 w-32 animate-pulse rounded bg-muted" />
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

// ─── System Logs collapsible section ─────────────────────────────────────────

function SystemLogsSection() {
  const [open, setOpen] = useState(false);
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
    <section aria-label="System Logs">
      {/* Collapsible header */}
      <button
        type="button"
        className="flex w-full items-center justify-between rounded-lg border bg-card px-4 py-3 text-left hover:bg-accent/50 transition-colors"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        <h2 className="text-xl font-semibold">System Logs</h2>
        <ChevronDownIcon
          className={`h-5 w-5 text-muted-foreground transition-transform duration-200 ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>

      {/* Collapsible content */}
      {open && (
        <div className="mt-4 flex flex-col gap-4">
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
                id="logs-auto-refresh"
                checked={autoRefresh}
                onCheckedChange={setAutoRefresh}
              />
              <Label htmlFor="logs-auto-refresh">Auto-refresh</Label>
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
      )}
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
    <div className="flex flex-col gap-8 w-full">
      {/* Page title */}
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Monitoring</h1>
        <p className="text-sm text-muted-foreground">
          System health, cost overview, and logs
        </p>
      </div>

      {/* Section 1: System Health */}
      <section aria-label="System Health" className="flex flex-col gap-4">
        <h2 className="text-xl font-semibold">System Health</h2>

        {loading && !health && <SkeletonCards />}

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

        {health && <DashboardGrid health={health} />}
      </section>

      {/* Section 2: Cost Overview */}
      <CostOverviewSection />

      {/* Section 3: System Logs (collapsible) */}
      <SystemLogsSection />
    </div>
  );
}
