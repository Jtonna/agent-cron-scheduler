"use client";

import type { GlobalTopJob } from "@/lib/types";
import { formatCost } from "@/lib/format";

const CHART_COLORS = [
  "var(--chart-1)",
  "var(--chart-2)",
  "var(--chart-3)",
  "var(--chart-4)",
  "var(--chart-5)",
];

interface DonutChartProps {
  topJobs: GlobalTopJob[];
}

/**
 * Compute the SVG arc path for a donut segment.
 * Uses stroke-based arcs on a circle with given radius.
 */
function describeArc(
  cx: number,
  cy: number,
  r: number,
  startAngle: number,
  endAngle: number
): string {
  // Clamp to avoid full-circle path issues
  const sweep = Math.min(endAngle - startAngle, 359.999);
  const startRad = ((startAngle - 90) * Math.PI) / 180;
  const endRad = ((startAngle + sweep - 90) * Math.PI) / 180;

  const x1 = cx + r * Math.cos(startRad);
  const y1 = cy + r * Math.sin(startRad);
  const x2 = cx + r * Math.cos(endRad);
  const y2 = cy + r * Math.sin(endRad);

  const largeArc = sweep > 180 ? 1 : 0;

  return `M ${x1} ${y1} A ${r} ${r} 0 ${largeArc} 1 ${x2} ${y2}`;
}

export function DonutChart({ topJobs }: DonutChartProps) {
  if (!topJobs || topJobs.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        No cost data available
      </div>
    );
  }

  const totalCost = topJobs.reduce((sum, j) => sum + j.total_cost, 0);

  // Build segments
  const segments: { job: GlobalTopJob; startAngle: number; sweep: number; color: string }[] = [];
  let currentAngle = 0;

  topJobs.forEach((job, i) => {
    const fraction = totalCost > 0 ? job.total_cost / totalCost : 0;
    const sweep = fraction * 360;
    segments.push({
      job,
      startAngle: currentAngle,
      sweep,
      color: CHART_COLORS[i % CHART_COLORS.length],
    });
    currentAngle += sweep;
  });

  const cx = 65;
  const cy = 65;
  const r = 50;
  const strokeWidth = 18;

  return (
    <div className="flex flex-col items-center gap-3">
      <svg viewBox="0 0 130 130" className="h-[120px] w-[120px]">
        {/* Background ring */}
        <circle
          cx={cx}
          cy={cy}
          r={r}
          fill="none"
          stroke="var(--muted)"
          strokeWidth={strokeWidth}
        />
        {/* Segments */}
        {segments.map((seg, i) => {
          if (seg.sweep < 0.1) return null;
          // For a single job that takes 100%, draw a near-full circle
          const path = describeArc(cx, cy, r, seg.startAngle, seg.startAngle + seg.sweep);
          return (
            <path
              key={seg.job.job_id || i}
              d={path}
              fill="none"
              stroke={seg.color}
              strokeWidth={strokeWidth}
              strokeLinecap="round"
            />
          );
        })}
        {/* Center text */}
        <text
          x={cx}
          y={cy - 6}
          textAnchor="middle"
          className="fill-foreground text-[11px] font-semibold"
        >
          {formatCost(totalCost)}
        </text>
        <text
          x={cx}
          y={cy + 8}
          textAnchor="middle"
          className="fill-muted-foreground text-[8px]"
        >
          total
        </text>
      </svg>

      {/* Legend */}
      <div className="flex flex-wrap justify-center gap-x-4 gap-y-1">
        {segments.map((seg, i) => (
          <div key={seg.job.job_id || i} className="flex items-center gap-1.5 text-[11px]">
            <span
              className="inline-block h-2.5 w-2.5 rounded-full"
              style={{ backgroundColor: seg.color }}
            />
            <span className="max-w-[100px] truncate text-foreground">
              {seg.job.job_name}
            </span>
            <span className="text-muted-foreground">{formatCost(seg.job.total_cost)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
