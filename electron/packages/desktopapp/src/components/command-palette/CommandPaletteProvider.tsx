"use client";

/**
 * CommandPaletteProvider
 *
 * App-shell-level provider that owns:
 *   - open/close state for the global command palette
 *   - the Cmd+K / Ctrl+K global keyboard listener
 *   - a registry of scoped commands (e.g. workflow actions added by
 *     the sidebar in phase 2)
 *   - rendering the modal itself, so any consumer can simply call
 *     `useCommandPalette().open()` without rendering markup
 *
 * Mounted once near the root (`src/app/layout.tsx` via
 * `src/apis/providers.tsx`).
 */

import {
  ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useRouter } from "next/navigation";
import { CommandPalette } from "./CommandPalette";
import {
  CommandPaletteContext,
  type CommandPaletteContextValue,
} from "./CommandPaletteContext";
import { buildGlobalCommands } from "./commands/globalCommands";
import { buildNavigateCommands } from "./commands/navigateCommands";
import type { CommandRegistrationId, PaletteCommand } from "./types";

export function CommandPaletteProvider({ children }: { children: ReactNode }) {
  const router = useRouter();
  const [isOpen, setIsOpen] = useState(false);
  const [registrations, setRegistrations] = useState<Record<string, PaletteCommand[]>>({});
  const nextRegistrationId = useRef(0);

  const open = useCallback(() => setIsOpen(true), []);
  const close = useCallback(() => setIsOpen(false), []);
  const toggle = useCallback(() => setIsOpen((v) => !v), []);

  const registerCommands = useCallback((commands: PaletteCommand[]): CommandRegistrationId => {
    const id = `reg-${nextRegistrationId.current++}`;
    setRegistrations((prev) => ({ ...prev, [id]: commands }));
    return id;
  }, []);

  const unregisterCommands = useCallback((id: CommandRegistrationId) => {
    setRegistrations((prev) => {
      if (!(id in prev)) return prev;
      const next = { ...prev };
      delete next[id];
      return next;
    });
  }, []);

  // Global Cmd+K / Ctrl+K toggle. Capture mode so it fires even when
  // focus is inside an input.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setIsOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const navigate = useCallback(
    (href: string) => {
      router.push(href);
    },
    [router],
  );

  const staticCommands = useMemo<PaletteCommand[]>(
    () => [...buildNavigateCommands(navigate), ...buildGlobalCommands(navigate)],
    [navigate],
  );

  const allCommands = useMemo<PaletteCommand[]>(() => {
    const registered = Object.values(registrations).flat();
    return [...registered, ...staticCommands];
  }, [registrations, staticCommands]);

  const value = useMemo<CommandPaletteContextValue>(
    () => ({
      isOpen,
      open,
      close,
      toggle,
      registerCommands,
      unregisterCommands,
    }),
    [isOpen, open, close, toggle, registerCommands, unregisterCommands],
  );

  return (
    <CommandPaletteContext.Provider value={value}>
      {children}
      <CommandPalette isOpen={isOpen} onClose={close} commands={allCommands} />
    </CommandPaletteContext.Provider>
  );
}
