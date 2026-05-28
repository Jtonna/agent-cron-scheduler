"use client";

/**
 * HeroDailyCostSlide
 *
 * Hero-sized slide rendering the system's daily cost trend as a large
 * area chart over the last 30 days. Uses `fillCostWindow` so days with
 * no runs still anchor the chart timeline.
 */

import { useMemo } from "react";
import { AreaChart, Area, ResponsiveContainer, XAxis, YAxis, Tooltip } from "recharts";
import { TrendingUp } from "lucide-react";
import type { DailyCostBucket } from "@/apis/types";
import { fillCostWindow, type CostChartPoint } from "@/apis/format";

interface HeroDailyCostSlideProps {
  data: DailyCostBucket[];
  loading?: boolean;
}

interface TooltipPayloadEntry {
  payload?: CostChartPoint;
}

function HeroTooltip({
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
      <div className="font-mono text-fg">${entry.total_usd.toFixed(2)}</div>
    </div>
  );
}

export function HeroDailyCostSlide({ data, loading = false }: HeroDailyCostSlideProps) {
  const series = useMemo(() => fillCostWindow(data, 30), [data]);
  const total = series.reduce((sum, p) => sum + p.total_usd, 0);
  const hasAnyCost = total > 0;

  return (
    <div className="w-full h-full bg-gradient-to-br from-gradient-hero-from via-gradient-hero-via to-gradient-hero-to rounded-card flex flex-col px-8 py-7">
      <div className="flex items-center gap-2 text-fg-muted text-xs uppercase tracking-wider mb-2">
        <TrendingUp size={14} />
        <span>Daily cost trend · last 30 days</span>
      </div>
      <div className="flex items-baseline gap-2 mb-1">
        <span className="text-[32px] font-extrabold text-fg leading-none tracking-tight">
          ${total.toFixed(2)}
        </span>
        <span className="text-fg-muted text-xs">total</span>
      </div>
      <div className="flex-1 min-h-0">
        {loading ? (
          <div className="w-full h-full rounded bg-surface-tertiary/40 animate-pulse" />
        ) : !hasAnyCost ? (
          <div className="w-full h-full flex items-center justify-center text-fg-subtle text-sm">
            No cost data yet
          </div>
        ) : (
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={series} margin={{ top: 8, right: 4, bottom: 4, left: 4 }}>
              <defs>
                <linearGradient id="hero-cost-gradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--color-brand)" stopOpacity={0.55} />
                  <stop offset="100%" stopColor="var(--color-brand)" stopOpacity={0} />
                </linearGradient>
              </defs>
              <XAxis dataKey="date" hide />
              <YAxis hide />
              <Tooltip
                content={<HeroTooltip />}
                cursor={{ stroke: "var(--color-border)", strokeWidth: 1 }}
              />
              <Area
                type="monotone"
                dataKey="total_usd"
                stroke="var(--color-brand)"
                strokeWidth={2}
                fill="url(#hero-cost-gradient)"
                isAnimationActive={false}
              />
            </AreaChart>
          </ResponsiveContainer>
        )}
      </div>
    </div>
  );
}
