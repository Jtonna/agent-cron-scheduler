"use client";

import { useMemo } from "react";
import { AreaChart, Area, Tooltip, ResponsiveContainer, XAxis, YAxis } from "recharts";
import { TrendingUp } from "lucide-react";
import type { DailyCostBucket } from "@/apis/types";
import { fillCostWindow, type CostChartPoint } from "@/apis/format";

/**
 * CostTrendWidget
 *
 * System-wide 30-day daily cost area chart. "Pulse" pastel mesh persona
 * surface; the chart line + fill use the ink scale (black/grey) instead
 * of brand pink — the user explicitly approved the palette change.
 * The hover tooltip uses the noir-card visual treatment for consistency.
 */

interface CostTrendWidgetProps {
  data: DailyCostBucket[];
  /** Internal chart height in pixels. Defaults to 120. */
  height?: number;
  /** Window size in days. Defaults to 30. */
  windowDays?: number;
}

interface TooltipPayloadEntry {
  payload?: CostChartPoint;
}

function NoirTooltip({
  active,
  payload,
}: {
  active?: boolean;
  payload?: TooltipPayloadEntry[];
}) {
  if (!active || !payload || payload.length === 0) return null;
  const entry = payload[0]?.payload;
  if (!entry) return null;
  return (
    <div className="noir-card !p-2.5 !rounded-[12px] text-xs font-mono num">
      <div className="opacity-60 text-[10px] uppercase tracking-wider">{entry.date}</div>
      <div className="mt-0.5">${entry.total_usd.toFixed(2)}</div>
    </div>
  );
}

export function CostTrendWidget({ data, height = 140, windowDays = 30 }: CostTrendWidgetProps) {
  const series = useMemo(() => fillCostWindow(data, windowDays), [data, windowDays]);
  const hasAnyCost = series.some((p) => p.total_usd > 0);
  const total = series.reduce((sum, p) => sum + p.total_usd, 0);

  return (
    <div
      data-mesh="pulse"
      className="rounded-card p-6 flex flex-col text-[color:var(--color-ink-900)] border border-border-subtle"
    >
      <div className="flex items-baseline justify-between gap-3 mb-3">
        <div className="text-eyebrow !text-fg-tertiary inline-flex items-center gap-2">
          <TrendingUp size={12} />
          <span>30-day cost trend</span>
        </div>
        <div className="text-display text-2xl num text-[color:var(--color-ink-950)] leading-none">
          ${total.toFixed(2)}
        </div>
      </div>

      {!hasAnyCost ? (
        <div
          className="flex items-center justify-center text-fg-tertiary text-xs"
          style={{ height }}
        >
          No cost data yet
        </div>
      ) : (
        <div style={{ width: "100%", height }}>
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={series} margin={{ top: 4, right: 4, bottom: 4, left: 4 }}>
              <defs>
                <linearGradient id="cost-trend-gradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--color-ink-900)" stopOpacity={0.32} />
                  <stop offset="100%" stopColor="var(--color-ink-900)" stopOpacity={0} />
                </linearGradient>
              </defs>
              <XAxis dataKey="date" hide />
              <YAxis hide />
              <Tooltip
                content={<NoirTooltip />}
                cursor={{ stroke: "var(--color-ink-900)", strokeOpacity: 0.25, strokeWidth: 1 }}
              />
              <Area
                type="monotone"
                dataKey="total_usd"
                stroke="var(--color-ink-900)"
                strokeWidth={1.75}
                fill="url(#cost-trend-gradient)"
                isAnimationActive={false}
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
}
