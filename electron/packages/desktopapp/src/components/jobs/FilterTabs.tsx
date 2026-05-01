"use client";

import { usePathname } from "next/navigation";
import { LayoutDashboard, List, DatabaseBackup, ScrollText } from "lucide-react";
import { Pill } from "@/components/ui/Pill";

/**
 * FilterTabs
 *
 * Top-level navigation pills shown on the home hero (Dashboard, All
 * Jobs, Backups, System Logs). Active state is derived from the current
 * pathname. Pass a custom `tabs` array to override the default set.
 */

interface FilterTab {
  label: string;
  icon: React.ReactNode;
  href: string;
}

const DEFAULT_TABS: FilterTab[] = [
  { label: "Dashboard", icon: <LayoutDashboard size={14} strokeWidth={2.5} />, href: "/" },
  { label: "All Jobs", icon: <List size={14} />, href: "/jobs" },
  { label: "Backups", icon: <DatabaseBackup size={14} />, href: "/backups" },
  { label: "System Logs", icon: <ScrollText size={14} />, href: "/systemlogs" },
];

interface FilterTabsProps {
  tabs?: FilterTab[];
}

export function FilterTabs({ tabs = DEFAULT_TABS }: FilterTabsProps) {
  const pathname = usePathname();

  return (
    <div className="flex items-center gap-1.5">
      {tabs.map((tab) => {
        const active = pathname === tab.href;
        return (
          <Pill
            key={tab.label}
            href={tab.href}
            label={tab.label}
            icon={tab.icon}
            active={active}
            bordered
            size="md"
          />
        );
      })}
    </div>
  );
}
