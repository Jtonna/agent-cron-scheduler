"use client";

/**
 * SidebarSearchTrigger
 *
 * A button styled to look like a search input. Lives at the top of the
 * workflow-detail sidebar (and any future sidebar that wants palette
 * access). Clicking it opens the global command palette via
 * `useCommandPalette().open()`. The user can also hit Cmd/Ctrl+K from
 * anywhere — the chip on the right hints at that shortcut.
 *
 * It is visually a search input — but behaviorally it is a button. We
 * intentionally do NOT mount a real <input>; typing should land in the
 * palette's own search field, not here. This sidesteps focus-shuffling
 * and ensures Cmd+K and click-to-open are exactly equivalent.
 */

import { Search } from "lucide-react";
import { useCommandPalette } from "@/components/command-palette/useCommandPalette";

export interface SidebarSearchTriggerProps {
  placeholder?: string;
}

function shortcutLabel(): string {
  if (typeof navigator === "undefined") return "⌘K";
  const mac = /Mac|iPhone|iPad|iPod/.test(navigator.platform);
  return mac ? "⌘K" : "Ctrl K";
}

export function SidebarSearchTrigger({
  placeholder = "Search",
}: SidebarSearchTriggerProps) {
  const palette = useCommandPalette();

  return (
    <button
      type="button"
      onClick={palette.open}
      aria-label="Open command palette"
      className={[
        "w-full flex items-center gap-2 px-2.5 h-8",
        "rounded-input bg-surface-secondary border border-border-subtle",
        "text-left text-xs text-fg-muted",
        "hover:bg-surface-hover hover:border-border transition-colors",
        "outline-none focus-visible:ring-2 focus-visible:ring-brand-ring",
        "cursor-pointer",
      ].join(" ")}
    >
      <Search size={14} className="shrink-0 text-fg-subtle" aria-hidden />
      <span className="flex-1 truncate">{placeholder}</span>
      <kbd
        className={[
          "shrink-0 inline-flex items-center px-1.5 py-0.5",
          "rounded-input bg-surface-tertiary border border-border-subtle",
          "text-[10px] font-medium text-fg-tertiary",
          "font-mono",
        ].join(" ")}
      >
        {shortcutLabel()}
      </kbd>
    </button>
  );
}
