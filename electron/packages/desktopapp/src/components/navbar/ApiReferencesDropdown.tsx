"use client";

import { Button, MenuTrigger, Menu, MenuItem, Popover, Separator } from "react-aria-components";
import { ChevronDown, Terminal, Globe, Cable, BookOpen } from "lucide-react";

/**
 * ApiReferencesDropdown
 *
 * Navbar dropdown linking to the docs landing pages (CLI, REST, MCP).
 * Rendered as a React Aria MenuTrigger so it gets keyboard navigation and
 * proper popover dismissal for free.
 */
export function ApiReferencesDropdown() {
  return (
    <MenuTrigger>
      <Button className="cursor-pointer hover:text-fg flex items-center gap-1 outline-none text-[14px] font-semibold">
        API References
        <ChevronDown size={12} />
      </Button>
      <Popover
        placement="bottom start"
        className="w-48 bg-surface border border-border rounded-menu shadow-menu py-2 z-50 outline-none entering:animate-in entering:fade-in entering:zoom-in-95 exiting:animate-out exiting:fade-out exiting:zoom-out-95"
      >
        <Menu className="outline-none">
          <MenuItem
            href="/docs"
            className="flex items-center gap-2.5 px-4 py-2 text-sm text-fg-tertiary hover:bg-surface-secondary hover:text-fg outline-none cursor-pointer"
          >
            <BookOpen size={14} /> All docs
          </MenuItem>
          <Separator className="h-px bg-border-subtle my-1" />
          <MenuItem
            href="/docs/cli"
            className="flex items-center gap-2.5 px-4 py-2 text-sm text-fg-tertiary hover:bg-surface-secondary hover:text-fg outline-none cursor-pointer"
          >
            <Terminal size={14} /> CLI
          </MenuItem>
          <MenuItem
            href="/docs/rest"
            className="flex items-center gap-2.5 px-4 py-2 text-sm text-fg-tertiary hover:bg-surface-secondary hover:text-fg outline-none cursor-pointer"
          >
            <Globe size={14} /> REST
          </MenuItem>
          <MenuItem
            href="/docs/mcp"
            className="flex items-center gap-2.5 px-4 py-2 text-sm text-fg-tertiary hover:bg-surface-secondary hover:text-fg outline-none cursor-pointer"
          >
            <Cable size={14} /> MCP
          </MenuItem>
        </Menu>
      </Popover>
    </MenuTrigger>
  );
}
