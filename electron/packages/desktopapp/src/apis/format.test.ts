import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { formatDuration, formatTimeAgo, formatTimeUntil, formatUptime } from "./format";

describe("formatDuration", () => {
  it("formats 0ms as 0m 00s", () => {
    expect(formatDuration(0)).toBe("0m 00s");
  });

  it("formats sub-minute durations with zero-padded seconds", () => {
    expect(formatDuration(5_000)).toBe("0m 05s");
    expect(formatDuration(45_000)).toBe("0m 45s");
  });

  it("formats multi-minute durations", () => {
    expect(formatDuration(60_000)).toBe("1m 00s");
    expect(formatDuration(125_000)).toBe("2m 05s");
    expect(formatDuration(3_600_000)).toBe("60m 00s");
  });

  it("floors fractional seconds", () => {
    expect(formatDuration(1_999)).toBe("0m 01s");
  });
});

describe("formatTimeAgo", () => {
  // Pin "now" to a fixed instant so the tests are deterministic.
  const NOW = new Date("2026-01-15T12:00:00Z");

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns 'just now' for the current instant", () => {
    expect(formatTimeAgo(NOW.toISOString())).toBe("just now");
  });

  it("returns 'just now' for sub-minute differences", () => {
    const iso = new Date(NOW.getTime() - 30 * 1000).toISOString();
    expect(formatTimeAgo(iso)).toBe("just now");
  });

  it("returns 'just now' for future timestamps", () => {
    const iso = new Date(NOW.getTime() + 60 * 1000).toISOString();
    expect(formatTimeAgo(iso)).toBe("just now");
  });

  it("formats minutes ago", () => {
    const iso = new Date(NOW.getTime() - 5 * 60 * 1000).toISOString();
    expect(formatTimeAgo(iso)).toBe("5 min ago");
  });

  it("formats hours ago", () => {
    const iso = new Date(NOW.getTime() - 3 * 60 * 60 * 1000).toISOString();
    expect(formatTimeAgo(iso)).toBe("3 hr ago");
  });

  it("formats days ago", () => {
    const iso = new Date(NOW.getTime() - 2 * 24 * 60 * 60 * 1000).toISOString();
    expect(formatTimeAgo(iso)).toBe("2d ago");
  });
});

describe("formatTimeUntil", () => {
  const NOW = new Date("2026-01-15T12:00:00Z");

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns 'now' for the current instant", () => {
    expect(formatTimeUntil(NOW.toISOString())).toBe("now");
  });

  it("returns 'now' for past timestamps", () => {
    const iso = new Date(NOW.getTime() - 60_000).toISOString();
    expect(formatTimeUntil(iso)).toBe("now");
  });

  it("returns 'in <1m' for sub-minute futures", () => {
    const iso = new Date(NOW.getTime() + 30_000).toISOString();
    expect(formatTimeUntil(iso)).toBe("in <1m");
  });

  it("formats minutes from now", () => {
    const iso = new Date(NOW.getTime() + 5 * 60_000).toISOString();
    expect(formatTimeUntil(iso)).toBe("in 5m");
  });

  it("formats hours from now", () => {
    const iso = new Date(NOW.getTime() + 3 * 60 * 60_000).toISOString();
    expect(formatTimeUntil(iso)).toBe("in 3h");
  });

  it("formats days from now", () => {
    const iso = new Date(NOW.getTime() + 2 * 24 * 60 * 60_000).toISOString();
    expect(formatTimeUntil(iso)).toBe("in 2d");
  });
});

describe("formatUptime", () => {
  it("formats zero as 0d 0h 0m", () => {
    expect(formatUptime(0)).toBe("0d 0h 0m");
  });

  it("formats minutes only", () => {
    expect(formatUptime(5 * 60)).toBe("0d 0h 5m");
  });

  it("formats hours and minutes", () => {
    expect(formatUptime(2 * 3600 + 30 * 60)).toBe("0d 2h 30m");
  });

  it("formats days, hours, and minutes", () => {
    const seconds = 3 * 86400 + 4 * 3600 + 15 * 60;
    expect(formatUptime(seconds)).toBe("3d 4h 15m");
  });

  it("ignores leftover seconds below a minute", () => {
    expect(formatUptime(59)).toBe("0d 0h 0m");
  });
});
