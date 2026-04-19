"use client";

import type { GlobalDailyTrend } from "@/lib/types";
import { formatCost } from "@/lib/format";

interface BarChartProps {
  dailyTrend: GlobalDailyTrend[];
}

/**
 * Format a date string (YYYY-MM-DD) to a short day label (e.g. "Mon", "Tue").
 * Falls back to the last 5 chars of the date string if parsing fails.
 */
function formatDayLabel(dateStr: string): string {
  try {
    const d = new Date(dateStr + "T00:00:00");
    return d.toLocaleDateString("en-US", { weekday: "short" });
  } catch {
    return dateStr.slice(-5);
  }
}

export function BarChart({ dailyTrend }: BarChartProps) {
  if (!dailyTrend || dailyTrend.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        No trend data available
      </div>
    );
  }

  const maxCost = Math.max(...dailyTrend.map((d) => d.cost_usd), 0.0001);

  // SVG layout constants
  const marginLeft = 48;
  const marginRight = 8;
  const marginTop = 12;
  const marginBottom = 24;
  const chartWidth = 260;
  const chartHeight = 140;
  const svgWidth = marginLeft + chartWidth + marginRight;
  const svgHeight = marginTop + chartHeight + marginBottom;

  const barCount = dailyTrend.length;
  const barGap = 4;
  const barWidth = Math.max(
    4,
    (chartWidth - barGap * (barCount - 1)) / barCount
  );

  // Y-axis ticks (3 ticks: 0, mid, max)
  const yTicks = [0, maxCost / 2, maxCost];

  return (
    <div className="flex flex-col items-center">
      <svg
        viewBox={`0 0 ${svgWidth} ${svgHeight}`}
        className="h-[160px] w-full max-w-[320px]"
        role="img"
        aria-label="Daily cost trend bar chart"
      >
        {/* Y-axis labels and grid lines */}
        {yTicks.map((tick, i) => {
          const y = marginTop + chartHeight - (tick / maxCost) * chartHeight;
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
        {dailyTrend.map((d, i) => {
          const barHeight = (d.cost_usd / maxCost) * chartHeight;
          const x = marginLeft + i * (barWidth + barGap);
          const y = marginTop + chartHeight - barHeight;
          return (
            <g key={d.date}>
              <rect
                x={x}
                y={y}
                width={barWidth}
                height={Math.max(barHeight, 1)}
                rx={2}
                fill="var(--chart-1)"
              />
              {/* X-axis label */}
              <text
                x={x + barWidth / 2}
                y={marginTop + chartHeight + 14}
                textAnchor="middle"
                className="fill-muted-foreground text-[7px]"
              >
                {formatDayLabel(d.date)}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}
