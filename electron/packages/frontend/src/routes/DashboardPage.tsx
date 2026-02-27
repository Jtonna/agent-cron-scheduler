import React, { useCallback } from "react";
import { ExclamationTriangleIcon } from "@heroicons/react/24/outline";
import { useHealth } from "@/hooks/useHealth";
import { DashboardGrid } from "@/components/dashboard/DashboardGrid";
import { useSSEEvents } from "@/hooks/useSSE";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

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

export function DashboardPage() {
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
    <div className="flex flex-col gap-6 w-full">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Dashboard</h1>
        <p className="text-sm text-muted-foreground">System health overview</p>
      </div>

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
    </div>
  );
}
