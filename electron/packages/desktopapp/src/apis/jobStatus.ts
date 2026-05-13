/**
 * Shared utilities for working with job runs across the app.
 *
 * Visual concerns (colors, labels, icons, dot sizes) live in
 * `@/components/ui/JobStateIndicator` — import from there.
 */

import type { JobRun, RecentRunEntry } from "./types";

/** True if any run in the list is currently running. */
export function isRunning(runs: { status: JobRun["status"] }[]): boolean {
  return runs.some((r) => r.status === "Running");
}

/** Group recent runs by workflow_id, preserving original order (newest first). */
export function groupRunsByWorkflow<T extends { workflow_id: string }>(
  runs: T[],
): Map<string, T[]> {
  const map = new Map<string, T[]>();
  for (const run of runs) {
    const arr = map.get(run.workflow_id) ?? [];
    arr.push(run);
    map.set(run.workflow_id, arr);
  }
  return map;
}

export type AnyRun = JobRun | RecentRunEntry;
