"use client";

import { MessageCircle, Settings, Sparkles, ExternalLink } from "lucide-react";
import Link from "next/link";
import { Button } from "@/components/ui/Button";
import { ApiReferencesDropdown } from "./ApiReferencesDropdown";
import { InstanceDropdown } from "./InstanceDropdown";

/**
 * Navbar
 *
 * Top-of-page sticky navigation bar. Composes the brand mark, primary
 * nav links, the API references dropdown, the "Build a Job" CTA, the
 * chat/settings icon links, and the instance switcher dropdown.
 */
export function Navbar() {
  return (
    <nav className="sticky top-0 z-20 bg-surface flex items-center h-[var(--height-navbar)] px-8 border-b border-border-subtle">
      <Link href="/" className="text-[22px] font-extrabold tracking-tight mr-10 text-fg">
        ACS
      </Link>

      <div className="flex items-center gap-7 text-[14px] font-semibold text-fg-secondary">
        <Link href="/systemlogs" className="cursor-pointer hover:text-fg">
          System Logs
        </Link>

        <ApiReferencesDropdown />

        <a
          href="https://github.com/Jtonna/agent-cron-scheduler"
          target="_blank"
          rel="noopener noreferrer"
          className="cursor-pointer hover:text-fg flex items-center gap-1"
        >
          GitHub
          <ExternalLink size={11} className="text-fg-subtle" />
        </a>

        <a
          href="#"
          target="_blank"
          rel="noopener noreferrer"
          className="cursor-pointer hover:text-fg flex items-center gap-1"
        >
          Community
          <ExternalLink size={11} className="text-fg-subtle" />
        </a>
      </div>

      <div className="ml-auto flex items-center gap-5">
        <Button href="/create" icon={<Sparkles size={14} />}>
          Build a Job
        </Button>

        <Link
          href="/chat"
          className="text-fg-muted hover:text-fg-secondary transition-colors cursor-pointer"
        >
          <MessageCircle size={20} />
        </Link>

        <Link
          href="/settings"
          className="text-fg-muted hover:text-fg-secondary transition-colors cursor-pointer"
        >
          <Settings size={20} />
        </Link>

        <InstanceDropdown />
      </div>
    </nav>
  );
}
