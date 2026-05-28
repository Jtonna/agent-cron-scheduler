import Link from "next/link";
import { Sidebar } from "./Sidebar";
import { SidebarSection } from "./SidebarSection";
import { SidebarSearchTrigger } from "./SidebarSearchTrigger";
import { SidebarRecentJobs } from "./SidebarRecentJobs";
import { SidebarFavoritedJobs } from "./SidebarFavoritedJobs";
import { Plus } from "lucide-react";
import type { Job } from "@/apis/types";

/**
 * JobsSidebar
 *
 * Left rail used on `/workflows`. Top of the rail mirrors the workflow-
 * detail sidebar: a SidebarSearchTrigger (opens the global command
 * palette) sits above a compact-density primary "New Workflow" button.
 * Below those are the Favorited and Recent sections. The favorites
 * section filters the supplied `jobs` by `is_favorited` from the
 * workflow store.
 */

interface JobsSidebarProps {
  jobs: Job[];
}

export function JobsSidebar({ jobs }: JobsSidebarProps) {
  const favorited = jobs.filter((j) => j.is_favorited);
  return (
    <Sidebar>
      <SidebarSearchTrigger placeholder="Search · ⌘K" />
      <Link
        href="/create"
        aria-label="Create a new workflow"
        className={[
          "mt-1 flex w-full items-center justify-center gap-1.5",
          "px-3 py-1.5 text-xs font-semibold",
          "rounded-input",
          "bg-brand hover:bg-brand-hover text-surface",
          "outline-none focus-visible:ring-2 focus-visible:ring-brand-ring focus-visible:ring-offset-2",
          "cursor-pointer transition-colors",
        ].join(" ")}
      >
        <Plus size={12} aria-hidden />
        New Workflow
      </Link>
      {favorited.length === 0 ? (
        <SidebarSection title="Favorited" emptyText="No favorites yet" />
      ) : (
        <SidebarSection title="Favorited">
          <SidebarFavoritedJobs jobs={favorited} />
        </SidebarSection>
      )}
      <SidebarSection title="Recent">
        <SidebarRecentJobs jobs={jobs} />
      </SidebarSection>
    </Sidebar>
  );
}
