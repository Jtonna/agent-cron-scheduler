"use client";

import { useState } from "react";
import type { DailyDataPoint } from "@/lib/types";
import { formatCost } from "@/lib/format";

interface CostPerRunChartProps {
  data: DailyDataPoint[];
  loading?: boolean;
  timeframe: string;
  onTimeframeChange: (tf: string) => void;
}

/**
 * Format a date string (YYYY-MM-DD) to a short date label (e.g. "Jan 15").
 * Falls back to the last 5 chars of the date string if parsing fails.
 */
function formatDateLabel(dateStr: string): string {
  try {
    const d = new Date(dateStr + "T00:00:00");
    return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
  } catch {
    return dateStr.slice(-5);
  }
}

/**
 * Compute the cost per run for a data point.
 */
function costPerRun(point: DailyDataPoint): number {
  return point.runs > 0 ? point.cost / point.runs : 0;
}

const TIMEFRAME_OPTIONS = ["1W", "2W", "1M", "3M", "All"];

export function CostPerRunChart({
  data,
  loading = false,
  timeframe,
  onTimeframeChange,
}: CostPerRunChartProps) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);

  if (loading) {
    return (
      <div className="flex flex-col gap-4">
        <div className="flex gap-2">
          {TIMEFRAME_OPTIONS.map((tf) => (
            <button
              key={tf}
              disabled
              className="rounded px-3 py-1.5 text-xs font-medium text-muted-foreground opacity-50"
            >
              {tf}
            </button>
          ))}
        </div>
        <div className="flex h-[220px] items-center justify-center text-sm text-muted-foreground">
          Loading...
        </div>
      </div>
    );
  }

  if (!data || data.length === 0) {
    return (
      <div className="flex flex-col gap-4">
        <div className="flex gap-2">
          {TIMEFRAME_OPTIONS.map((tf) => (
            <button
              key={tf}
              onClick={() => onTimeframeChange(tf)}
              className={`rounded px-3 py-1.5 text-xs font-medium transition-colors ${
                timeframe === tf
                  ? "bg-accent text-accent-foreground"
                  : "bg-muted text-muted-foreground hover:bg-muted hover:text-foreground"
              }`}
            >
              {tf}
            </button>
          ))}
        </div>
        <div className="flex h-[220px] items-center justify-center text-sm text-muted-foreground">
          No cost data available
        </div>
      </div>
    );
  }

  // Calculate cost per run for all data points
  const costPerRunValues = data.map(costPerRun);
  const maxCostPerRun = Math.max(...costPerRunValues, 0.0001);

  // Chart dimensions (SVG)
  const marginLeft = 48;
  const marginRight = 8;
  const marginTop = 12;
  const marginBottom = 40;
  const chartWidth = 380;
  const chartHeight = 160;
  const svgWidth = marginLeft + chartWidth + marginRight;
  const svgHeight = marginTop + chartHeight + marginBottom;

  const barCount = data.length;
  const barGap = 3;
  const barWidth = Math.max(
    2,
    (chartWidth - barGap * (barCount - 1)) / barCount
  );

  // Y-axis ticks (3 ticks: 0, mid, max)
  const yTicks = [0, maxCostPerRun / 2, maxCostPerRun];

  return (
    <div className="flex flex-col gap-4">
      {/* Timeframe selector */}
      <div className="flex gap-2">
        {TIMEFRAME_OPTIONS.map((tf) => (
          <button
            key={tf}
            onClick={() => onTimeframeChange(tf)}
            className={`rounded px-3 py-1.5 text-xs font-medium transition-colors ${
              timeframe === tf
                ? "bg-accent text-accent-foreground"
                : "bg-muted text-muted-foreground hover:bg-muted hover:text-foreground"
            }`}
          >
            {tf}
          </button>
        ))}
      </div>

      {/* Chart */}
      <div className="relative w-full">
        <svg
          viewBox={`0 0 ${svgWidth} ${svgHeight}`}
          className="h-[200px] w-full"
          role="img"
          aria-label="Cost per run bar chart"
        >
          {/* Y-axis labels and grid lines */}
          {yTicks.map((tick, i) => {
            const y = marginTop + chartHeight - (tick / maxCostPerRun) * chartHeight;
            return (
              <g key={i}>
                <line
                  x1={marginLeft}
                  x2={marginLeft + chartWidth}
                  y1={y}
                  y2={y}
                  stroke="var(--border)"
                  strokeDasharray="3,3"
                />
                <text
                  x={marginLeft - 6}
                  y={y + 3}
                  textAnchor="end"
                  className="fill-muted-foreground text-[8px]"
                >
                  {formatCost(tick)}
                </text>
              </g>
            );
          })}

          {/* Bars */}
          {data.map((point, i) => {
            const cpr = costPerRun(point);
            const barHeight = (cpr / maxCostPerRun) * chartHeight;
            const x = marginLeft + i * (barWidth + barGap);
            const y = marginTop + chartHeight - barHeight;
            const isHovered = hoveredIndex === i;

            return (
              <g
                key={point.date}
                onMouseEnter={() => setHoveredIndex(i)}
                onMouseLeave={() => setHoveredIndex(null)}
                style={{ cursor: "pointer" }}
              >
                {/* Bar */}
                <rect
                  x={x}
                  y={y}
                  width={barWidth}
                  height={Math.max(barHeight, 1)}
                  rx={2}
                  fill="var(--chart-1)"
                  opacity={isHovered ? 0.8 : 1}
                  style={{ transition: "opacity 150ms ease-out" }}
                />

                {/* Date label */}
                <text
                  x={x + barWidth / 2}
                  y={marginTop + chartHeight + 14}
                  textAnchor="middle"
                  className="fill-muted-foreground text-[7px]"
                >
                  {formatDateLabel(point.date)}
                </text>
              </g>
            );
          })}
        </svg>

        {/* Tooltip */}
        {hoveredIndex !== null && (
          <div className="absolute bottom-full right-0 mb-2 rounded bg-foreground/90 px-2.5 py-1.5 text-[11px] text-background shadow-lg">
            <div className="font-medium">{formatDateLabel(data[hoveredIndex].date)}</div>
            <div className="text-xs opacity-90">
              Cost per run: {formatCost(costPerRun(data[hoveredIndex]))}
            </div>
            <div className="text-xs opacity-75">
              {data[hoveredIndex].runs} run{data[hoveredIndex].runs !== 1 ? "s" : ""}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
