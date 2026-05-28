import { Sidebar } from "./Sidebar";
import { SidebarSection } from "./SidebarSection";
import { Button } from "@/components/ui/Button";
import { SidebarRecentJobs } from "./SidebarRecentJobs";
import { SidebarFavoritedJobs } from "./SidebarFavoritedJobs";
import { Plus } from "lucide-react";
import type { Job } from "@/apis/types";

/**
 * JobsSidebar
 *
 * Left rail used on `/workflows`. Composes the "Quick actions", "Favorited",
 * and "Recent" sections. The favorites section filters the supplied
 * `jobs` by `is_favorited` from the workflow store.
 */

interface JobsSidebarProps {
  jobs: Job[];
}

export function JobsSidebar({ jobs }: JobsSidebarProps) {
  const favorited = jobs.filter((j) => j.is_favorited);
  return (
    <Sidebar>
      <SidebarSection title="Quick actions">
        <Button href="/create" icon={<Plus size={14} />} fullWidth size="sm">
          New workflow
        </Button>
      </SidebarSection>
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
