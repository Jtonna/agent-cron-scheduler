"use client";

import { usePathname } from "next/navigation";
import { LayoutDashboard, List, DatabaseBackup, ScrollText } from "lucide-react";
import Link from "next/link";

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
          <Link
            key={tab.label}
            href={tab.href}
            className={`flex items-center gap-2 px-4 py-2 text-sm font-semibold rounded-full transition-colors ${
              active
                ? "text-white border border-gray-900 bg-gray-900"
                : "text-gray-700 hover:text-gray-900 hover:bg-gray-100"
            }`}
          >
            {tab.icon}
            {tab.label}
          </Link>
        );
      })}
    </div>
  );
}
