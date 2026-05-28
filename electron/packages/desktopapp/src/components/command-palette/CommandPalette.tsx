"use client";

/**
 * CommandPalette
 *
 * The visible modal that renders the cmdk Command primitives. The
 * provider owns open/close state and the consumer-registered command
 * list — this component is dumb: given a list of commands and an open
 * flag, it renders the modal and forwards activation through the
 * supplied `onSelect` callback. Storybook stories drive it directly
 * without the provider.
 *
 * Visual: max-w-xl, anchored ~1/4 from the top of the viewport with a
 * semi-transparent ink backdrop. Search input uses ink-900 text on a
 * surface-secondary background; results live in a rounded-input shell.
 *
 * Accessibility: cmdk handles arrow-key navigation, focus trap, and
 * Escape. We add a visually-hidden label, restore focus on close, and
 * mark the backdrop as a click-to-dismiss surface.
 */

import { useEffect, useId, useMemo, useRef } from "react";
import { Command } from "cmdk";
import { Search } from "lucide-react";
import type { PaletteCommand } from "./types";

export interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  commands: PaletteCommand[];
  /** Override group display order. Unknown groups go to the bottom in the order they appear. */
  groupOrder?: string[];
}

const DEFAULT_GROUP_ORDER = ["Workflow Actions", "Navigate", "Global"];

export function CommandPalette({ isOpen, onClose, commands, groupOrder }: CommandPaletteProps) {
  const labelId = useId();
  const previouslyFocusedRef = useRef<HTMLElement | null>(null);

  // Restore focus to whatever was focused when the palette opened.
  useEffect(() => {
    if (isOpen) {
      previouslyFocusedRef.current = (document.activeElement as HTMLElement | null) ?? null;
    } else if (previouslyFocusedRef.current) {
      previouslyFocusedRef.current.focus?.();
      previouslyFocusedRef.current = null;
    }
  }, [isOpen]);

  const grouped = useMemo(() => {
    const order = groupOrder ?? DEFAULT_GROUP_ORDER;
    const buckets = new Map<string, PaletteCommand[]>();
    for (const cmd of commands) {
      const arr = buckets.get(cmd.group) ?? [];
      arr.push(cmd);
      buckets.set(cmd.group, arr);
    }
    const ordered: { group: string; items: PaletteCommand[] }[] = [];
    for (const g of order) {
      const items = buckets.get(g);
      if (items && items.length) {
        ordered.push({ group: g, items });
        buckets.delete(g);
      }
    }
    for (const [g, items] of buckets) {
      if (items.length) ordered.push({ group: g, items });
    }
    return ordered;
  }, [commands, groupOrder]);

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-ink-900/60 pt-[20vh] px-4"
      onMouseDown={(e) => {
        // Only close when the click actually started on the backdrop.
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <span id={labelId} className="sr-only">
        Command palette
      </span>
      <Command
        label="Command palette"
        loop
        aria-labelledby={labelId}
        className="w-full max-w-xl overflow-hidden rounded-menu border border-border bg-surface-secondary shadow-menu"
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            onClose();
          }
        }}
      >
        <div className="flex items-center gap-2 border-b border-border px-3">
          <Search className="size-[var(--size-icon-md)] text-fg-muted" aria-hidden />
          <Command.Input
            autoFocus
            placeholder="Type a command or search…"
            className="h-11 flex-1 bg-transparent text-sm text-ink-900 placeholder:text-fg-subtle outline-none"
          />
        </div>
        <Command.List className="max-h-[60vh] overflow-y-auto p-2">
          <Command.Empty className="px-3 py-6 text-center text-sm text-fg-muted">
            No matching commands.
          </Command.Empty>
          {grouped.map(({ group, items }) => (
            <Command.Group
              key={group}
              heading={group}
              className="text-fg-muted [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-eyebrow"
            >
              {items.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={[cmd.label, cmd.hint, ...(cmd.keywords ?? [])].filter(Boolean).join(" ")}
                  onSelect={() => {
                    cmd.action();
                    onClose();
                  }}
                  className="flex cursor-pointer items-center gap-2 rounded-input px-2 py-2 text-sm text-fg aria-selected:bg-surface-hover aria-selected:text-fg"
                >
                  {cmd.icon ? <span className="text-fg-muted">{cmd.icon}</span> : null}
                  <span className="flex-1 truncate">{cmd.label}</span>
                  {cmd.hint ? (
                    <span className="text-xs text-fg-subtle">{cmd.hint}</span>
                  ) : null}
                </Command.Item>
              ))}
            </Command.Group>
          ))}
        </Command.List>
      </Command>
    </div>
  );
}
