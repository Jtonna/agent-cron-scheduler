import { DailyDataPoint } from "@/lib/types";

// Timeframe options mapping UI labels to API values
export const TIMEFRAME_OPTIONS = [
  { label: "1W", value: "7d" },
  { label: "1M", value: "30d" },
  { label: "6M", value: "180d" },
  { label: "1Y", value: "365d" },
  { label: "All", value: "all" },
] as const;

export const CUSTOM_TIMEFRAME = "custom" as const;
export const DEFAULT_TIMEFRAME = "30d";

export interface ChartPoint {
  x: number;
  y: number;
  data: DailyDataPoint;
}

export interface ChartPadding {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

const SHORT_RANGE_TIMEFRAMES = new Set(["7d", "30d"]);

// Format date for X-axis labels based on timeframe.
// Short ranges (7d, 30d): "Mar 2"
// Long ranges (180d, 365d, all, custom): "Mar '25"
export function formatAxisDate(dateStr: string, timeframe: string): string {
  // Parse YYYY-MM-DD without timezone conversion by treating it as local midnight
  const [year, month, day] = dateStr.split("-").map(Number);
  const date = new Date(year, month - 1, day);

  const monthLabel = date.toLocaleString("en-US", { month: "short" });

  if (SHORT_RANGE_TIMEFRAMES.has(timeframe)) {
    return `${monthLabel} ${day}`;
  }

  const shortYear = String(year).slice(2);
  return `${monthLabel} '${shortYear}`;
}

// Map data points to SVG coordinates.
// X: evenly spaced across available width.
// Y: scaled from 0 to maxCost, inverted for SVG (y=0 is top).
export function computeChartPoints(
  data: DailyDataPoint[],
  width: number,
  height: number,
  padding: ChartPadding
): ChartPoint[] {
  if (data.length === 0) return [];

  const drawWidth = width - padding.left - padding.right;
  const drawHeight = height - padding.top - padding.bottom;

  const maxCost = Math.max(...data.map((d) => d.cost), 0);

  if (data.length === 1) {
    // Single point: centre horizontally, place on baseline if cost is 0
    const x = padding.left + drawWidth / 2;
    const y =
      maxCost === 0
        ? padding.top + drawHeight
        : padding.top + drawHeight * (1 - data[0].cost / maxCost);
    return [{ x, y, data: data[0] }];
  }

  return data.map((point, i) => {
    const x = padding.left + (i / (data.length - 1)) * drawWidth;
    const y =
      maxCost === 0
        ? padding.top + drawHeight
        : padding.top + drawHeight * (1 - point.cost / maxCost);
    return { x, y, data: point };
  });
}

// Build SVG path d attribute for a polyline: M x0,y0 L x1,y1 ...
export function buildPathD(points: { x: number; y: number }[]): string {
  if (points.length === 0) return "";

  const [first, ...rest] = points;
  const start = `M ${first.x},${first.y}`;
  const segments = rest.map((p) => `L ${p.x},${p.y}`).join(" ");
  return segments.length > 0 ? `${start} ${segments}` : start;
}

// Build SVG path d attribute for the area fill (closed path under the line).
// M x0,baselineY L x0,y0 L x1,y1 ... L xN,yN L xN,baselineY Z
export function buildAreaPathD(
  points: { x: number; y: number }[],
  baselineY: number
): string {
  if (points.length === 0) return "";

  const first = points[0];
  const last = points[points.length - 1];

  const lineSegments = points.map((p) => `L ${p.x},${p.y}`).join(" ");
  return `M ${first.x},${baselineY} ${lineSegments} L ${last.x},${baselineY} Z`;
}

// Nice round step magnitudes used for Y-axis ticks
const NICE_STEPS = [
  0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50,
  100, 200, 500, 1000,
];

// Compute sensible Y-axis tick values (3-5 ticks, starting at 0).
export function getYAxisTicks(maxCost: number): number[] {
  if (maxCost <= 0) return [0];

  // Target ~4 intervals (5 ticks including 0)
  const TARGET_INTERVALS = 4;
  const rawStep = maxCost / TARGET_INTERVALS;

  // Find the smallest nice step that is >= rawStep
  const step =
    NICE_STEPS.find((s) => s >= rawStep) ??
    NICE_STEPS[NICE_STEPS.length - 1];

  const niceMax = Math.ceil(maxCost / step) * step;
  const count = Math.round(niceMax / step);

  return Array.from({ length: count + 1 }, (_, i) =>
    parseFloat((i * step).toFixed(10))
  );
}

// Determine how many X-axis labels to skip to avoid overlap.
// Returns step size: 1 = show all, 2 = every other, etc.
export function getXAxisLabelStep(
  totalPoints: number,
  availableWidth: number
): number {
  const MIN_LABEL_WIDTH_PX = 50;
  const maxLabels = Math.floor(availableWidth / MIN_LABEL_WIDTH_PX);
  if (maxLabels <= 0 || totalPoints <= 0) return 1;
  return Math.ceil(totalPoints / maxLabels);
}
