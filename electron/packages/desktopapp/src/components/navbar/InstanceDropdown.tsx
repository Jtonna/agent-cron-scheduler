"use client";

import { Button, MenuTrigger, Menu, MenuItem, Popover, Separator } from "react-aria-components";
import { Plug, Bot } from "lucide-react";

/**
 * InstanceDropdown
 *
 * Avatar-style trigger in the top-right of the navbar that opens a menu
 * to switch between ACS instances. The button itself is the bot avatar;
 * the popover lists "New connection" plus recent instances. Currently
 * the recent-instances list is static placeholder data.
 */
export function InstanceDropdown() {
  return (
    <MenuTrigger>
      <Button className="w-[var(--height-avatar)] h-[var(--height-avatar)] rounded-full bg-gradient-to-br from-gradient-bot-from to-gradient-bot-to cursor-pointer ring-2 ring-transparent hover:ring-border transition flex items-center justify-center outline-none focus-visible:ring-brand-ring">
        <Bot size={18} className="text-surface" />
      </Button>
      <Popover
        placement="bottom end"
        className="w-64 bg-surface border border-border rounded-menu shadow-menu py-2 z-50 outline-none entering:animate-in entering:fade-in entering:zoom-in-95 exiting:animate-out exiting:fade-out exiting:zoom-out-95"
      >
        <Menu className="outline-none">
          <MenuItem
            className="px-4 py-2 text-xs font-semibold text-fg-subtle uppercase tracking-wider outline-none"
            isDisabled
          >
            Connect to instance
          </MenuItem>
          <MenuItem className="w-full flex items-center gap-2.5 px-4 py-2.5 text-sm text-fg-secondary hover:bg-surface-secondary outline-none cursor-pointer">
            <Plug size={14} className="text-fg-subtle" />
            New connection...
          </MenuItem>
          <Separator className="h-px bg-border-subtle my-1" />
          <MenuItem
            className="px-4 py-2 text-xs font-semibold text-fg-subtle uppercase tracking-wider outline-none"
            isDisabled
          >
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
