"use client";

import { useState, useEffect, useRef } from "react";
import {
  Button,
  Input,
  TextField,
  MenuTrigger,
  Menu,
  MenuItem,
  Popover,
} from "react-aria-components";
import { Search, ChevronDown, Loader2 } from "lucide-react";
import { JobsListRow } from "./JobsListRow";
import type { Job, RecentRunEntry } from "@/apis/types";
import { useRecentRuns } from "@/apis/useRecentRuns";
import { useJobRuns } from "@/apis/useJobRuns";
import { groupRunsByJob, isRunning } from "@/apis/jobStatus";

/**
 * JobsList
 *
 * The full jobs table on `/jobs` — toolbar (sort dropdown + search),
 * column header, and a virtualized-via-IntersectionObserver list of
 * JobsListRow rows. Pulls a wide window of recent runs so each row can
 * render its last-7 dot history.
 */

/**
 * Per-row wrapper so each rendered row gets its own per-job run history
 * (parallels the sidebar's SidebarItemWithRuns pattern). Each call to
 * useJobRuns is keyed by job.id, so TanStack Query dedupes/caches across
 * the sidebar and this list when the same job appears in both.
 */
function JobsListRowWithRuns({ job }: { job: Job }) {
  const { runs } = useJobRuns(job.id, 7);
  return <JobsListRow job={job} runs={runs} />;
}

interface JobsListProps {
  jobs: Job[];
  loading: boolean;
}

type SortKey = "name" | "lastRan";

const SORT_LABELS: Record<SortKey, string> = {
  name: "Name (A–Z)",
  lastRan: "Recent",
};

const PAGE = 15;

function compareJobs(
  a: Job,
  b: Job,
  key: SortKey,
  runsByJob: Map<string, RecentRunEntry[]>,
): number {
  if (key === "name") {
    return a.name.localeCompare(b.name);
  }
  // "Recent": currently-running jobs first, then by last_run_at desc.
  const aRunning = isRunning(runsByJob.get(a.id) ?? []);
  const bRunning = isRunning(runsByJob.get(b.id) ?? []);
  if (aRunning !== bRunning) return aRunning ? -1 : 1;
  const aT = a.last_run_at ? new Date(a.last_run_at).getTime() : 0;
  const bT = b.last_run_at ? new Date(b.last_run_at).getTime() : 0;
  return bT - aT;
}

export function JobsList({ jobs, loading }: JobsListProps) {
  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [visibleCount, setVisibleCount] = useState(PAGE);
  const sentinelRef = useRef<HTMLDivElement>(null);

  // Pull a wide window of recent activity so we can show last-7 dots per row
  // and detect currently-running jobs for "Recent" sort.
  const { runs: recentRuns } = useRecentRuns(200);
  const runsByJob = groupRunsByJob(recentRuns);

  const needle = search.trim().toLowerCase();
  const list = needle ? jobs.filter((j) => j.name.toLowerCase().includes(needle)) : jobs;
  const filtered = [...list].sort((a, b) => compareJobs(a, b, sortKey, runsByJob));

  // Clamp naturally: if the user just searched and visibleCount > filtered.length,
  // slice() already returns at most filtered.length items. No reset effect needed.
  const visible = filtered.slice(0, visibleCount);

  const hasMore = visible.length < filtered.length;

  // IntersectionObserver to lazy-reveal more rows as the user scrolls
  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel || !hasMore) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          setVisibleCount((c) => c + PAGE);
        }
      },
      { rootMargin: "200px" },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [hasMore]);

  return (
    <div className="flex flex-col gap-4">
      {/* Toolbar — TabBar style: sort dropdown + tab on the left, search on the right */}
      <div className="border-y border-border-subtle">
        <div className="h-[var(--height-tab-bar)] flex items-center gap-6 text-sm">
          <MenuTrigger>
            <Button className="text-fg font-semibold cursor-pointer flex items-center gap-1 outline-none focus-visible:ring-2 focus-visible:ring-brand-ring rounded px-1">
              {SORT_LABELS[sortKey]}
              <ChevronDown size={12} />
            </Button>
            <Popover
              placement="bottom start"
              className="w-52 bg-surface border border-border rounded-menu shadow-menu py-1 z-50 outline-none entering:animate-in entering:fade-in entering:zoom-in-95 exiting:animate-out exiting:fade-out exiting:zoom-out-95"
            >
              <Menu className="outline-none">
                {(Object.keys(SORT_LABELS) as SortKey[]).map((key) => (
                  <MenuItem
                    key={key}
                    onAction={() => setSortKey(key)}
                    className={`px-3 py-2 text-sm hover:bg-surface-secondary outline-none cursor-pointer ${
                      key === sortKey ? "text-fg font-semibold" : "text-fg-secondary"
                    }`}
                  >
                    {SORT_LABELS[key]}
                  </MenuItem>
                ))}
              </Menu>
            </Popover>
          </MenuTrigger>

          <div className="w-px h-5 bg-border" />

          <span className="text-fg font-semibold">Jobs</span>

          <div className="ml-auto">
            <TextField aria-label="Search jobs" value={search} onChange={setSearch}>
              <div className="relative">
                <Search
                  size={14}
                  className="absolute left-3 top-1/2 -translate-y-1/2 text-fg-subtle pointer-events-none"
                />
                <Input
                  placeholder="Search jobs..."
                  className="w-64 h-[var(--height-input)] pl-9 pr-3 text-sm border border-border rounded-input bg-surface outline-none focus:border-brand-ring transition-colors"
                />
              </div>
            </TextField>
          </div>
        </div>
      </div>

      {/* Header row */}
      <div className="grid grid-cols-[20px_minmax(0,1.6fr)_120px_minmax(0,1.4fr)_minmax(0,1fr)_minmax(0,1fr)] gap-4 px-4 text-[11px] font-semibold text-fg-subtle uppercase tracking-wider">
        <span aria-hidden />
        <span>Name</span>
        <span>Recent runs</span>
        <span>Schedule</span>
        <span>Last run</span>
        <span>Next run</span>
      </div>

      {/* Body */}
      {loading && jobs.length === 0 ? (
        <div className="flex items-center justify-center py-12">
          <Loader2 size={20} className="animate-spin text-fg-subtle" />
        </div>
      ) : filtered.length === 0 ? (
        <div className="text-center py-12 text-fg-subtle text-sm">
          {jobs.length === 0 ? "No jobs yet" : "No jobs match your search"}
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          {visible.map((job) => (
            <JobsListRowWithRuns key={job.id} job={job} />
          ))}
          {hasMore && (
            <div ref={sentinelRef} className="flex items-center justify-center py-4">
              <Loader2 size={16} className="animate-spin text-fg-subtle" />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
