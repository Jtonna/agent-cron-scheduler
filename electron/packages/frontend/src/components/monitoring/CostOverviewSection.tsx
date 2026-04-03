"use client";

import { useState, useMemo, useCallback } from "react";
import {
  CurrencyDollarIcon,
  CalendarIcon,
  ChartBarIcon,
} from "@heroicons/react/24/outline";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { useGlobalCostSummary } from "@/hooks/useGlobalCostSummary";
import { CostMetricCard } from "./CostMetricCard";
import { TopCostJobsList } from "./TopCostJobsList";
import { TimeframeSelector } from "@/components/cost-per-run/TimeframeSelector";
import { LineChartContent } from "@/components/cost-per-run/LineChartContent";
import {
  DEFAULT_TIMEFRAME,
  CUSTOM_TIMEFRAME,
} from "@/components/cost-per-run/utils";
import type { DailyDataPoint } from "@/lib/types";

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

function ChartSkeleton() {
  return (
    <div className="h-[300px] w-full animate-pulse rounded-lg bg-muted" />
  );
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/**
 * Filter daily trend data by the selected timeframe.
 * The global cost summary always returns all available data, so we slice
 * client-side based on the number of days each preset represents.
 */
function filterTrendByTimeframe(
  data: DailyDataPoint[],
  timeframe: string,
  customStart?: string,
  customEnd?: string
): DailyDataPoint[] {
  if (timeframe === "all") return data;

  if (timeframe === CUSTOM_TIMEFRAME) {
    if (!customStart || !customEnd) return data;
    return data.filter((d) => d.date >= customStart && d.date <= customEnd);
  }

  const days = parseInt(timeframe, 10);
  if (isNaN(days)) return data;

  // Keep only the most recent `days` entries (data is assumed date-sorted asc)
  return data.slice(-days);
}

// ─── Main Component ───────────────────────────────────────────────────────────

export function CostOverviewSection() {
  const { summary, loading, error, refresh } = useGlobalCostSummary();

  // Timeframe state for the daily trend chart
  const [timeframe, setTimeframe] = useState(DEFAULT_TIMEFRAME);
  const [customStart, setCustomStart] = useState<string | undefined>();
  const [customEnd, setCustomEnd] = useState<string | undefined>();

  const handleTimeframeChange = useCallback(
    (tf: string, start?: string, end?: string) => {
      setTimeframe(tf);
      if (tf === CUSTOM_TIMEFRAME) {
        setCustomStart(start);
        setCustomEnd(end);
      } else {
        setCustomStart(undefined);
        setCustomEnd(undefined);
      }
    },
    []
  );

  // Map GlobalDailyTrend (with injected `cost`) to DailyDataPoint for LineChartContent.
  // The hook already adds `cost: item.cost_usd` to each entry; we supply the
  // remaining DailyDataPoint fields (`runs`) with a sensible default.
  const allTrendPoints: DailyDataPoint[] = useMemo(() => {
    if (!summary) return [];
    return summary.daily_trend.map((item) => ({
      date: item.date,
      runs: 0,
      // hook maps cost_usd → cost; cast via unknown since the type is extended at runtime
      cost: (item as unknown as { cost: number }).cost ?? item.cost_usd,
      input_tokens: item.input_tokens,
      output_tokens: item.output_tokens,
    }));
  }, [summary]);

  const filteredTrendPoints = useMemo(
    () =>
      filterTrendByTimeframe(allTrendPoints, timeframe, customStart, customEnd),
    [allTrendPoints, timeframe, customStart, customEnd]
  );

  // No-op day-click handler — global monitoring has no per-day drill-down
  const handleDayClick = useCallback((_date: string) => {}, []);

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
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-base">Daily Cost Trend</CardTitle>
          </CardHeader>
          <CardContent>
            <ChartSkeleton />
          </CardContent>
        </Card>
      </section>
    );
  }

  // ── Error state ──
  if (error) {
    // Graceful fallback when the cost endpoint is absent or not yet configured
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
          <Button variant="outline" size="sm" onClick={refresh}>
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

      {/* Daily cost trend chart */}
      <Card>
        <CardHeader className="pb-2">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <CardTitle className="text-base">Daily Cost Trend</CardTitle>
            <TimeframeSelector
              value={timeframe}
              onTimeframeChange={handleTimeframeChange}
            />
          </div>
        </CardHeader>
        <CardContent>
          {filteredTrendPoints.length === 0 ? (
            <div className="flex h-[200px] items-center justify-center">
              <p className="text-sm text-muted-foreground">
                No trend data in selected timeframe
              </p>
            </div>
          ) : (
            <div className="h-[300px] sm:h-[250px] md:h-[300px]">
              <LineChartContent
                data={filteredTrendPoints}
                onDayClick={handleDayClick}
                timeframe={timeframe}
              />
            </div>
          )}
        </CardContent>
      </Card>

      {/* Top-cost jobs table */}
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-base">Top Cost Jobs</CardTitle>
        </CardHeader>
        <CardContent>
          <TopCostJobsList topJobs={summary.top_jobs} />
        </CardContent>
      </Card>
    </section>
  );
}
