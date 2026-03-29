import React from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface CostTrendChartProps {
  runs: Array<{ total_cost_usd?: number | null; started_at: string; finished_at?: string | null }>;
}

export function CostTrendChart({ runs }: CostTrendChartProps) {
  // Filter to runs with actual cost data
  const qualifying = runs.filter(
    (r) => r.total_cost_usd != null && r.total_cost_usd > 0
  );

  if (qualifying.length < 2) {
    return null;
  }

  // Sort by started_at ascending (oldest first), take last 20
  const sorted = [...qualifying]
    .sort((a, b) => new Date(a.started_at).getTime() - new Date(b.started_at).getTime())
    .slice(-20);

  const maxCost = Math.max(...sorted.map((r) => r.total_cost_usd as number));

  // SVG dimensions (viewBox units)
  const viewWidth = 400;
  const viewHeight = 100;
  const paddingTop = 8;
  const paddingBottom = 16; // space for baseline
  const paddingLeft = 4;
  const paddingRight = 4;
  const chartHeight = viewHeight - paddingTop - paddingBottom;
  const chartWidth = viewWidth - paddingLeft - paddingRight;

  const barCount = sorted.length;
  const gap = 2;
  const barWidth = (chartWidth - gap * (barCount - 1)) / barCount;

  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
          Cost Trend
        </CardTitle>
      </CardHeader>
      <CardContent className="pb-4">
        <svg
          viewBox={`0 0 ${viewWidth} ${viewHeight}`}
          width="100%"
          aria-label="Cost trend bar chart"
          style={{ display: "block" }}
        >
          {/* Baseline */}
          <line
            x1={paddingLeft}
            y1={paddingTop + chartHeight}
            x2={paddingLeft + chartWidth}
            y2={paddingTop + chartHeight}
            stroke="currentColor"
            strokeOpacity={0.15}
            strokeWidth={1}
          />

          {/* Bars */}
          {sorted.map((run, i) => {
            const cost = run.total_cost_usd as number;
            const barH = (cost / maxCost) * chartHeight;
            const x = paddingLeft + i * (barWidth + gap);
            const y = paddingTop + chartHeight - barH;

            const date = new Date(run.started_at).toLocaleDateString("en-US", {
              month: "short",
              day: "numeric",
              year: "numeric",
              hour: "2-digit",
              minute: "2-digit",
            });
            const costLabel = `$${cost.toFixed(4)}`;

            return (
              <rect
                key={run.started_at + i}
                x={x}
                y={y}
                width={barWidth}
                height={barH}
                rx={1}
                fill="#3b82f6"
                fillOpacity={0.85}
              >
                <title>{`${date} — ${costLabel}`}</title>
              </rect>
            );
          })}
        </svg>
      </CardContent>
    </Card>
  );
}
