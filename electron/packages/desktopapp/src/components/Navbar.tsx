"use client";

import { ReactNode } from "react";
import { Button, MenuTrigger, Menu, MenuItem, Popover, Separator } from "react-aria-components";
import { ChevronDown, MessageCircle, Settings, Sparkles, ExternalLink, Terminal, Globe, Cable, Plug, Bot, BookOpen } from "lucide-react";
import Link from "next/link";

/* ── NavButton (generic, reusable) ── */

interface NavButtonProps {
  children: ReactNode;
  icon?: ReactNode;
  className?: string;
  onPress?: () => void;
}

export function NavButton({ children, icon, className = "", onPress }: NavButtonProps) {
  return (
    <Button
      onPress={onPress}
      className={`bg-brand hover:bg-brand-hover text-surface text-sm font-semibold px-5 py-2.5 rounded-pill transition-colors cursor-pointer flex items-center gap-2 outline-none focus-visible:ring-2 focus-visible:ring-brand-ring focus-visible:ring-offset-2 ${className}`}
    >
      {icon}
      {children}
    </Button>
  );
}

/* ── API References dropdown ── */

function ApiReferencesDropdown() {
  return (
    <MenuTrigger>
      <Button className="cursor-pointer hover:text-fg flex items-center gap-1 outline-none text-[14px] font-semibold">
        API References
        <ChevronDown size={12} />
      </Button>
      <Popover placement="bottom start" className="w-48 bg-surface border border-border rounded-menu shadow-menu py-2 z-50 outline-none entering:animate-in entering:fade-in entering:zoom-in-95 exiting:animate-out exiting:fade-out exiting:zoom-out-95">
        <Menu className="outline-none">
          <MenuItem href="/docs" className="flex items-center gap-2.5 px-4 py-2 text-sm text-fg-tertiary hover:bg-surface-secondary hover:text-fg outline-none cursor-pointer">
            <BookOpen size={14} /> All docs
          </MenuItem>
          <Separator className="h-px bg-border-subtle my-1" />
          <MenuItem href="/docs/cli" className="flex items-center gap-2.5 px-4 py-2 text-sm text-fg-tertiary hover:bg-surface-secondary hover:text-fg outline-none cursor-pointer">
            <Terminal size={14} /> CLI
          </MenuItem>
          <MenuItem href="/docs/rest" className="flex items-center gap-2.5 px-4 py-2 text-sm text-fg-tertiary hover:bg-surface-secondary hover:text-fg outline-none cursor-pointer">
            <Globe size={14} /> REST
          </MenuItem>
          <MenuItem href="/docs/mcp" className="flex items-center gap-2.5 px-4 py-2 text-sm text-fg-tertiary hover:bg-surface-secondary hover:text-fg outline-none cursor-pointer">
            <Cable size={14} /> MCP
          </MenuItem>
        </Menu>
      </Popover>
    </MenuTrigger>
  );
}

/* ── Instance selector dropdown ── */

function InstanceDropdown() {
  return (
    <MenuTrigger>
      <Button className="w-[var(--height-avatar)] h-[var(--height-avatar)] rounded-full bg-gradient-to-br from-gradient-bot-from to-gradient-bot-to cursor-pointer ring-2 ring-transparent hover:ring-border transition flex items-center justify-center outline-none focus-visible:ring-brand-ring">
        <Bot size={18} className="text-surface" />
      </Button>
      <Popover placement="bottom end" className="w-64 bg-surface border border-border rounded-menu shadow-menu py-2 z-50 outline-none entering:animate-in entering:fade-in entering:zoom-in-95 exiting:animate-out exiting:fade-out exiting:zoom-out-95">
        <Menu className="outline-none">
          <MenuItem className="px-4 py-2 text-xs font-semibold text-fg-subtle uppercase tracking-wider outline-none" isDisabled>
            Connect to instance
          </MenuItem>
          <MenuItem className="w-full flex items-center gap-2.5 px-4 py-2.5 text-sm text-fg-secondary hover:bg-surface-secondary outline-none cursor-pointer">
            <Plug size={14} className="text-fg-subtle" />
            New connection...
          </MenuItem>
          <Separator className="h-px bg-border-subtle my-1" />
          <MenuItem className="px-4 py-2 text-xs font-semibold text-fg-subtle uppercase tracking-wider outline-none" isDisabled>
            Recent instances
          </MenuItem>
          <MenuItem className="w-full flex items-center gap-2.5 px-4 py-2.5 text-sm text-fg-secondary hover:bg-surface-secondary outline-none cursor-pointer">
            <span className="w-[var(--size-status-dot)] h-[var(--size-status-dot)] rounded-full bg-status-success-dot shrink-0" />
            <span className="flex-1 text-left truncate">localhost:9090</span>
            <span className="text-xs text-fg-subtle">local</span>
          </MenuItem>
          <MenuItem className="w-full flex items-center gap-2.5 px-4 py-2.5 text-sm text-fg-secondary hover:bg-surface-secondary outline-none cursor-pointer">
            <span className="w-[var(--size-status-dot)] h-[var(--size-status-dot)] rounded-full bg-fg-faint shrink-0" />
            <span className="flex-1 text-left truncate">acs-prod.example.com</span>
            <span className="text-xs text-fg-subtle">remote</span>
          </MenuItem>
          <MenuItem className="w-full flex items-center gap-2.5 px-4 py-2.5 text-sm text-fg-secondary hover:bg-surface-secondary outline-none cursor-pointer">
            <span className="w-[var(--size-status-dot)] h-[var(--size-status-dot)] rounded-full bg-fg-faint shrink-0" />
            <span className="flex-1 text-left truncate">192.168.1.50:9090</span>
            <span className="text-xs text-fg-subtle">remote</span>
          </MenuItem>
        </Menu>
      </Popover>
    </MenuTrigger>
  );
}

/* ── Navbar ── */

export function Navbar() {
  return (
    <nav className="flex items-center h-[var(--height-navbar)] px-8 border-b border-border-subtle">
      <Link href="/" className="text-[22px] font-extrabold tracking-tight mr-10 text-fg">ACS</Link>

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
        <Link href="/create">
          <NavButton icon={<Sparkles size={14} />}>Build a Job</NavButton>
        </Link>

        <Link href="/chat" className="text-fg-muted hover:text-fg-secondary transition-colors cursor-pointer">
          <MessageCircle size={20} />
        </Link>

        <Link href="/settings" className="text-fg-muted hover:text-fg-secondary transition-colors cursor-pointer">
          <Settings size={20} />
        </Link>

        <InstanceDropdown />
      </div>
    </nav>
  );
}
