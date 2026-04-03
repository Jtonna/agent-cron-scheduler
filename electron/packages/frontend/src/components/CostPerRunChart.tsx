"use client";

import { useState, useCallback } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { JobRun } from "@/lib/types";
import { useCostSummary } from "@/hooks/useCostSummary";
import { DEFAULT_TIMEFRAME, CUSTOM_TIMEFRAME } from "./cost-per-run/utils";
import { TimeframeSelector } from "./cost-per-run/TimeframeSelector";
import { LineChartContent } from "./cost-per-run/LineChartContent";
import { DayRunsModal } from "./cost-per-run/DayRunsModal";

interface CostPerRunChartProps {
  jobId: string;
  allRuns: JobRun[];
  totalRuns: number;  // total runs available (from useRuns)
}

export function CostPerRunChart({ jobId, allRuns, totalRuns }: CostPerRunChartProps) {
  // State
  const [timeframe, setTimeframe] = useState(DEFAULT_TIMEFRAME);
  const [customStart, setCustomStart] = useState<string | undefined>();
  const [customEnd, setCustomEnd] = useState<string | undefined>();
  const [selectedDate, setSelectedDate] = useState<string | null>(null);

  // Determine API params
  const isCustom = timeframe === CUSTOM_TIMEFRAME;
  const { data, loading, error } = useCostSummary(
    jobId,
    isCustom ? undefined : timeframe,    // pass undefined for custom so it uses start/end
    isCustom ? customStart : undefined,
    isCustom ? customEnd : undefined
  );

  // Timeframe change handler
  const handleTimeframeChange = useCallback((tf: string, start?: string, end?: string) => {
    setTimeframe(tf);
    if (tf === CUSTOM_TIMEFRAME) {
      setCustomStart(start);
      setCustomEnd(end);
    } else {
      setCustomStart(undefined);
      setCustomEnd(undefined);
    }
  }, []);

  // Day click → filter allRuns by date → open modal
  const handleDayClick = useCallback((date: string) => {
    setSelectedDate(date);
  }, []);

  // Filter runs for the selected date
  const runsForDate = selectedDate
    ? allRuns.filter(r => r.started_at?.startsWith(selectedDate))
    : [];

  // Render
  return (
    <Card>
      <CardHeader className="pb-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <CardTitle className="text-base">Cost per Run</CardTitle>
          <TimeframeSelector value={timeframe} onTimeframeChange={handleTimeframeChange} />
        </div>
      </CardHeader>
      <CardContent>
        {/* Loading state */}
        {loading && (
          <div className="flex h-[200px] items-center justify-center">
            <div className="text-sm text-muted-foreground">Loading cost data...</div>
          </div>
        )}

        {/* Error state */}
        {error && (
          <div className="flex h-[200px] items-center justify-center">
            <div className="text-sm text-destructive">{error}</div>
          </div>
        )}

        {/* Empty state: no runs at all */}
        {!loading && !error && data && data.summary.total_runs === 0 && (
          <div className="flex h-[200px] items-center justify-center">
            <div className="text-center text-sm text-muted-foreground">
              <p>No cost data available</p>
              <p className="mt-1 text-xs">Cost data requires runs with Claude CLI cost tracking</p>
            </div>
          </div>
        )}

        {/* Empty state: has runs but no data in this timeframe */}
        {!loading && !error && data && data.summary.total_runs > 0 && data.data.length === 0 && (
          <div className="flex h-[200px] items-center justify-center">
            <div className="text-center text-sm text-muted-foreground">
              <p>No cost data in selected timeframe</p>
              <p className="mt-1 text-xs">Try a wider date range</p>
            </div>
          </div>
        )}

        {/* Chart */}
        {!loading && !error && data && data.data.length > 0 && (
          <div className="h-[300px] sm:h-[250px] md:h-[300px]">
            <LineChartContent
              data={data.data}
              onDayClick={handleDayClick}
              timeframe={timeframe}
            />
          </div>
        )}
      </CardContent>

      {/* Drill-down modal */}
      <DayRunsModal
        open={selectedDate !== null}
        onClose={() => setSelectedDate(null)}
        date={selectedDate ?? ""}
        runs={runsForDate}
        jobId={jobId}
        totalRunsLoaded={allRuns.length}
        totalRunsAvailable={totalRuns}
      />
    </Card>
  );
}
