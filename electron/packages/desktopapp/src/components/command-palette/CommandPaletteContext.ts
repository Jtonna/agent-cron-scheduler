"use client";

/**
 * CommandPaletteContext
 *
 * Internal context shared by the provider and the public
 * `useCommandPalette` hook. Lives in its own file so the hook stays a
 * thin re-export and so Storybook can mock the context in stories
 * without dragging in the real provider.
 */

import { createContext } from "react";
import type { CommandRegistrationId, PaletteCommand } from "./types";

export interface CommandPaletteContextValue {
  isOpen: boolean;
  open: () => void;
  close: () => void;
  toggle: () => void;
  /** Register a batch of commands; returns an id to pass to `unregisterCommands`. */
  registerCommands: (commands: PaletteCommand[]) => CommandRegistrationId;
  unregisterCommands: (id: CommandRegistrationId) => void;
}

export const CommandPaletteContext = createContext<CommandPaletteContextValue | null>(null);
