"use client";

import { useState, useMemo, useId } from "react";
import { DailyDataPoint } from "@/lib/types";
import { formatCost } from "@/lib/format";
import {
  buildPathD,
  buildAreaPathD,
  getYAxisTicks,
  getXAxisLabelStep,
  formatAxisDate,
  ChartPoint,
  ChartPadding,
} from "./utils";

interface LineChartContentProps {
  data: DailyDataPoint[];
  onDayClick: (date: string) => void;
  timeframe: string;
}

const PADDING: ChartPadding = { top: 16, right: 16, bottom: 32, left: 56 };
const VIEWBOX_WIDTH = 600;
const VIEWBOX_HEIGHT = 300;

/**
 * Map data points to SVG coordinates using the Y-axis tick max as the scale
 * ceiling. This ensures gridlines and data points share the same coordinate
 * system (the tick max may be larger than the data max due to rounding up to
 * nice values).
 */
function computeScaledPoints(
  data: DailyDataPoint[],
  scaleMax: number
): ChartPoint[] {
  if (data.length === 0) return [];

  const drawWidth = VIEWBOX_WIDTH - PADDING.left - PADDING.right;
  const drawHeight = VIEWBOX_HEIGHT - PADDING.top - PADDING.bottom;

  if (data.length === 1) {
    const x = PADDING.left + drawWidth / 2;
    const y =
      scaleMax === 0
        ? PADDING.top + drawHeight
        : PADDING.top + drawHeight * (1 - data[0].cost / scaleMax);
    return [{ x, y, data: data[0] }];
  }

  return data.map((point, i) => {
    const x = PADDING.left + (i / (data.length - 1)) * drawWidth;
    const y =
      scaleMax === 0
        ? PADDING.top + drawHeight
        : PADDING.top + drawHeight * (1 - point.cost / scaleMax);
    return { x, y, data: point };
  });
}

/** Compute the Y coordinate for a given cost value on the shared scale. */
function costToY(value: number, scaleMax: number): number {
  const drawHeight = VIEWBOX_HEIGHT - PADDING.top - PADDING.bottom;
  if (scaleMax === 0) return PADDING.top + drawHeight;
  return PADDING.top + drawHeight * (1 - value / scaleMax);
}

/** Clamp the tooltip X so the rect stays inside the SVG viewBox. */
function clampTooltipX(
  cx: number,
  halfWidth: number
): number {
  const minX = PADDING.left + halfWidth;
  const maxX = VIEWBOX_WIDTH - PADDING.right - halfWidth;
  return Math.max(minX, Math.min(maxX, cx));
}

export function LineChartContent({
  data,
  onDayClick,
  timeframe,
}: LineChartContentProps) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const uniqueId = useId();
  const gradientId = `areaGradient-${uniqueId.replace(/:/g, "")}`;

  // 1. Compute Y-axis ticks and derive the scale ceiling
  const maxCost = useMemo(() => Math.max(...data.map((d) => d.cost), 0), [data]);
  const yTicks = useMemo(() => getYAxisTicks(maxCost), [maxCost]);
  const scaleMax = useMemo(
    () => (yTicks.length > 0 ? yTicks[yTicks.length - 1] : maxCost),
    [yTicks, maxCost]
  );

  // 2. Compute chart points using the tick-based scale
  const points = useMemo(
    () => computeScaledPoints(data, scaleMax),
    [data, scaleMax]
  );

  // 3. Compute X-axis label step
  const labelStep = useMemo(
    () =>
      getXAxisLabelStep(
        points.length,
        VIEWBOX_WIDTH - PADDING.left - PADDING.right
      ),
    [points.length]
  );

  // 4. Build paths
  const linePath = useMemo(() => buildPathD(points), [points]);
  const areaPath = useMemo(() => {
    const baselineY = VIEWBOX_HEIGHT - PADDING.bottom;
    return buildAreaPathD(points, baselineY);
  }, [points]);

  // Tooltip dimensions
  const tooltipW = 110;
  const tooltipH = 24;
  const tooltipHalf = tooltipW / 2;

  return (
    <svg
      viewBox={`0 0 ${VIEWBOX_WIDTH} ${VIEWBOX_HEIGHT}`}
      width="100%"
      height="100%"
      aria-label="Cost per run line chart"
    >
      {/* Gradient definition for area fill */}
      <defs>
        <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#3b82f6" stopOpacity={0.3} />
          <stop offset="100%" stopColor="#3b82f6" stopOpacity={0.05} />
        </linearGradient>
      </defs>

      {/* Y-axis gridlines + labels */}
      {yTicks.map((tick) => {
        const y = costToY(tick, scaleMax);
        return (
          <g key={tick}>
            <line
              x1={PADDING.left}
              y1={y}
              x2={VIEWBOX_WIDTH - PADDING.right}
              y2={y}
              stroke="currentColor"
              strokeOpacity={0.1}
              strokeDasharray="4 4"
            />
            <text
              x={PADDING.left - 8}
              y={y}
              textAnchor="end"
              dominantBaseline="middle"
              fontSize="10"
              fill="currentColor"
              opacity={0.5}
            >
              {formatCost(tick)}
            </text>
          </g>
        );
      })}

      {/* Area fill */}
      {points.length > 0 && (
        <path d={areaPath} fill={`url(#${gradientId})`} />
      )}

      {/* Line path */}
      {points.length > 0 && (
        <path
          d={linePath}
          fill="none"
          stroke="#3b82f6"
          strokeWidth="2"
          strokeLinejoin="round"
          strokeLinecap="round"
        />
      )}

      {/* X-axis date labels */}
      {points.map(
        (point, i) =>
          i % labelStep === 0 && (
            <text
              key={`xlabel-${i}`}
              x={point.x}
              y={VIEWBOX_HEIGHT - PADDING.bottom + 16}
              textAnchor="middle"
              fontSize="10"
              fill="currentColor"
              opacity={0.5}
            >
              {formatAxisDate(point.data.date, timeframe)}
            </text>
          )
      )}

      {/* Clickable data points */}
      {points.map((point, i) => (
        <g key={`point-${i}`}>
          {/* Larger invisible hit area for easier clicking */}
          <circle
            cx={point.x}
            cy={point.y}
            r={12}
            fill="transparent"
            cursor="pointer"
            onClick={() => onDayClick(point.data.date)}
            onMouseEnter={() => setHoveredIndex(i)}
            onMouseLeave={() => setHoveredIndex(null)}
          />
          {/* Visible dot */}
          <circle
            cx={point.x}
            cy={point.y}
            r={hoveredIndex === i ? 5 : 3}
            fill={hoveredIndex === i ? "#2563eb" : "#3b82f6"}
            cursor="pointer"
            pointerEvents="none"
          />
        </g>
      ))}

      {/* Tooltip for hovered point */}
      {hoveredIndex !== null && points[hoveredIndex] && (() => {
        const hp = points[hoveredIndex];
        const tooltipX = clampTooltipX(hp.x, tooltipHalf);
        const tooltipY = Math.max(tooltipH, hp.y - 32);

        return (
          <g pointerEvents="none">
            {/* Vertical indicator line */}
            <line
              x1={hp.x}
              y1={PADDING.top}
              x2={hp.x}
              y2={VIEWBOX_HEIGHT - PADDING.bottom}
              stroke="#3b82f6"
              strokeOpacity={0.3}
              strokeDasharray="4 4"
            />
            {/* Tooltip background */}
            <rect
              x={tooltipX - tooltipHalf}
              y={tooltipY - tooltipH}
              width={tooltipW}
              height={tooltipH}
              rx={4}
              fill="hsl(var(--popover))"
              stroke="hsl(var(--border))"
            />
            {/* Tooltip text */}
            <text
              x={tooltipX}
              y={tooltipY - tooltipH / 2 + 1}
              textAnchor="middle"
              dominantBaseline="middle"
              fontSize="10"
              fill="currentColor"
            >
              {hp.data.date} &middot; {formatCost(hp.data.cost)}
            </text>
          </g>
        );
      })()}
    </svg>
  );
}
