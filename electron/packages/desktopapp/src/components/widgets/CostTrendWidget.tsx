"use client";

import { AreaChart, Area, Tooltip, ResponsiveContainer, XAxis, YAxis } from "recharts";
import { TrendingUp } from "lucide-react";
import { StatWidget } from "@/components/widgets/StatWidget";
import type { GlobalDailyTrend } from "@/apis/types";

/**
 * CostTrendWidget
 *
 * Dashboard widget showing daily cost as an area chart over the last 30
 * days. Includes its own StatWidget shell with title and icon, so the
 * page just drops it in alongside the other widgets without manual
 * wrapping.
 *
 * Hovering a point on the chart shows a small tooltip with that day's
 * date and cost. Renders a gentle empty state when `data` is empty.
 */

interface CostTrendWidgetProps {
  data: GlobalDailyTrend[];
  /** Internal chart height in pixels. Defaults to 96. */
  height?: number;
}

interface TooltipPayloadEntry {
  payload?: GlobalDailyTrend;
}

function SparklineTooltip({
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
    <div className="bg-surface border border-border rounded-input shadow-menu px-2.5 py-1.5 text-xs">
      <div className="text-fg-muted">{entry.date}</div>
      <div className="font-mono text-fg">${entry.cost_usd.toFixed(2)}</div>
    </div>
  );
}

export function CostTrendWidget({ data, height = 96 }: CostTrendWidgetProps) {
  return (
    <StatWidget title="30-day cost trend" icon={<TrendingUp size={14} />}>
      {data.length === 0 ? (
        <div className="flex items-center justify-center text-fg-subtle text-xs" style={{ height }}>
          No cost data yet
        </div>
      ) : (
        <div style={{ width: "100%", height }}>
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={data} margin={{ top: 4, right: 4, bottom: 4, left: 4 }}>
              <defs>
                <linearGradient id="cost-trend-gradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--color-brand)" stopOpacity={0.5} />
                  <stop offset="100%" stopColor="var(--color-brand)" stopOpacity={0} />
                </linearGradient>
              </defs>
              <XAxis dataKey="date" hide />
              <YAxis hide />
              <Tooltip
                content={<SparklineTooltip />}
                cursor={{
                  stroke: "var(--color-border)",
                  strokeWidth: 1,
                }}
              />
              <Area
                type="monotone"
                dataKey="cost_usd"
                stroke="var(--color-brand)"
                strokeWidth={2}
                fill="url(#cost-trend-gradient)"
                isAnimationActive={false}
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      )}
    </StatWidget>
  );
}
