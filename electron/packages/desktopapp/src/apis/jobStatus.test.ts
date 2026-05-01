import { describe, expect, it } from "vitest";
import { groupRunsByJob, isRunning } from "./jobStatus";
import type { JobRun } from "./types";

describe("isRunning", () => {
  it("returns true when any run has status 'Running'", () => {
    expect(isRunning([{ status: "Running" }])).toBe(true);
    expect(isRunning([{ status: "Completed" }, { status: "Running" }])).toBe(true);
  });

  it("returns false when no run is 'Running'", () => {
    expect(isRunning([{ status: "Completed" }])).toBe(false);
    expect(isRunning([{ status: "Completed" }, { status: "Failed" }])).toBe(false);
  });

  it("returns false for an empty list", () => {
    expect(isRunning([])).toBe(false);
  });
});

describe("groupRunsByJob", () => {
  function makeRun(id: string, jobId: string): JobRun {
    return {
      run_id: id,
      job_id: jobId,
      started_at: "2026-01-15T12:00:00Z",
      finished_at: null,
      status: "Completed",
      exit_code: 0,
    };
  }

  it("returns an empty map for no runs", () => {
    const grouped = groupRunsByJob([]);
    expect(grouped.size).toBe(0);
  });

  it("groups a single run under its job", () => {
    const runs = [makeRun("r1", "job-1")];
    const grouped = groupRunsByJob(runs);
    expect(grouped.size).toBe(1);
    expect(grouped.get("job-1")).toEqual(runs);
  });

  it("groups multiple runs across jobs preserving original order", () => {
    const r1 = makeRun("r1", "job-1");
    const r2 = makeRun("r2", "job-2");
    const r3 = makeRun("r3", "job-1");
    const r4 = makeRun("r4", "job-2");
    const grouped = groupRunsByJob([r1, r2, r3, r4]);

    expect(grouped.size).toBe(2);
    expect(grouped.get("job-1")).toEqual([r1, r3]);
    expect(grouped.get("job-2")).toEqual([r2, r4]);
  });
});
