"use client";

import { useCallback } from "react";
import {
  CurrencyDollarIcon,
  CalendarIcon,
  ChartBarIcon,
} from "@heroicons/react/24/outline";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { useGlobalCostSummary } from "@/hooks/useGlobalCostSummary";
import { CostMetricCard } from "./CostMetricCard";

// ─── Skeleton ────────────────────────────────────────────────────────────────

function MetricCardSkeleton() {
  return (
    <Card>
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
  );
}

// ─── Main Component ───────────────────────────────────────────────────────────

export function CostOverviewSection() {
  const { summary, loading, error, refresh } = useGlobalCostSummary();

  const handleRetry = useCallback(() => refresh(), [refresh]);

  // ── Loading state ──
  if (loading) {
    return (
      <section aria-label="Cost Overview" className="space-y-6">
        <h2 className="text-xl font-semibold">Cost Overview</h2>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <MetricCardSkeleton />
          <MetricCardSkeleton />
          <MetricCardSkeleton />
        </div>
      </section>
    );
  }

  // ── Error state ──
  if (error) {
    const isMissingEndpoint =
      error.toLowerCase().includes("404") ||
      error.toLowerCase().includes("not found");

    if (isMissingEndpoint) {
      return (
        <section aria-label="Cost Overview" className="space-y-6">
          <h2 className="text-xl font-semibold">Cost Overview</h2>
          <div className="flex items-center justify-center rounded-lg border border-dashed border-muted-foreground/25 p-8 text-center">
            <p className="text-sm text-muted-foreground">
              Cost tracking not configured
            </p>
          </div>
        </section>
      );
    }

    return (
      <section aria-label="Cost Overview" className="space-y-6">
        <h2 className="text-xl font-semibold">Cost Overview</h2>
        <div className="flex flex-col items-center justify-center gap-3 rounded-lg border border-destructive/25 p-8 text-center">
          <p className="text-sm text-destructive">{error}</p>
          <Button variant="outline" size="sm" onClick={handleRetry}>
            Retry
          </Button>
        </div>
      </section>
    );
  }

  // ── Empty state ──
  if (!summary) {
    return (
      <section aria-label="Cost Overview" className="space-y-6">
        <h2 className="text-xl font-semibold">Cost Overview</h2>
        <div className="flex items-center justify-center rounded-lg border border-dashed border-muted-foreground/25 p-8 text-center">
          <p className="text-sm text-muted-foreground">
            No cost data available
          </p>
        </div>
      </section>
    );
  }

  // ── Render ──
  return (
    <section aria-label="Cost Overview" className="space-y-6">
      <h2 className="text-xl font-semibold">Cost Overview</h2>

      {/* Metric cards: Today / Week / Month */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        <CostMetricCard
          label="Today's Cost"
          amountUsd={summary.today_usd}
          tokens={summary.today_tokens}
          icon={CurrencyDollarIcon}
        />
        <CostMetricCard
          label="This Week"
          amountUsd={summary.week_usd}
          icon={CalendarIcon}
        />
        <CostMetricCard
          label="This Month"
          amountUsd={summary.month_usd}
          icon={ChartBarIcon}
        />
      </div>
    </section>
  );
}
