import { Sidebar } from "./Sidebar";
import { SidebarSection } from "./SidebarSection";
import { Button } from "@/components/ui/Button";
import { SidebarRecentJobs } from "./SidebarRecentJobs";
import { Plus } from "lucide-react";
import type { Job } from "@/apis/types";

/**
 * JobsSidebar
 *
 * Left rail used on `/jobs`. Composes the "Quick actions", "Favorited",
 * and "Recent" sections. Currently the favorites section is a stub
 * empty state until the favorites API lands.
 */

interface JobsSidebarProps {
  jobs: Job[];
}

export function JobsSidebar({ jobs }: JobsSidebarProps) {
  return (
    <Sidebar>
      <SidebarSection title="Quick actions">
        <Button href="/create" icon={<Plus size={14} />} fullWidth size="sm">
          New job
        </Button>
      </SidebarSection>
      <SidebarSection title="Favorited" emptyText="No favorites yet" />
      <SidebarSection title="Recent">
        <SidebarRecentJobs jobs={jobs} />
      </SidebarSection>
    </Sidebar>
  );
}
