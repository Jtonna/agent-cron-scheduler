"use client";

import { Plus } from "lucide-react";
import { CompactActionButton } from "@/components/ui/CompactActionButton";
import { SidebarSearchTrigger } from "./SidebarSearchTrigger";
import { SidebarSectionHeader } from "./SidebarSectionHeader";
import { SidebarRecentJobs } from "./SidebarRecentJobs";
import { SidebarFavoritedJobs } from "./SidebarFavoritedJobs";
import type { Job } from "@/apis/types";

/**
 * JobsSidebar
 *
 * Left rail used on `/workflows`. Follows the unified sidebar anatomy
 * shared with `JobDetailSidebar` and `RunDetailSidebar`:
 *
 *   1. (no back link — top-level page)
 *   2. Search trigger        — opens the global command palette
 *   3. Primary action row    — "New Workflow" (CompactActionButton)
 *   4. Favorited section     — eyebrow header + list (empty state inline)
 *   5. Recent section        — eyebrow header + list, capped at 6
 *
 * Vertical rhythm is one `gap-4` token between sections; section
 * headers and their content sit inside the same flex column with a
 * tighter `gap-1.5`.
 */

interface JobsSidebarProps {
  jobs: Job[];
}

const RECENT_LIMIT = 6;

export function JobsSidebar({ jobs }: JobsSidebarProps) {
  const favorited = jobs.filter((j) => j.is_favorited);

  return (
    <aside className="h-full flex flex-col">
      <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-4">
        <SidebarSearchTrigger placeholder="Search · ⌘K" />

        <CompactActionButton
          intent="primary"
          fullWidth
          href="/create"
          icon={<Plus size={12} aria-hidden />}
          ariaLabel="Create a new workflow"
        >
          New Workflow
        </CompactActionButton>

        <section className="flex flex-col gap-1.5">
          <SidebarSectionHeader
            title="Favorited"
            meta={favorited.length > 0 ? `(${favorited.length})` : undefined}
          />
          {favorited.length === 0 ? (
            <div className="px-2 py-2 text-xs text-fg-subtle italic">
              No favorites yet
            </div>
          ) : (
            <SidebarFavoritedJobs jobs={favorited} />
          )}
        </section>

        <section className="flex flex-col gap-1.5">
          <SidebarSectionHeader title="Recent" />
          <SidebarRecentJobs jobs={jobs} max={RECENT_LIMIT} />
        </section>
      </div>
    </aside>
  );
}
