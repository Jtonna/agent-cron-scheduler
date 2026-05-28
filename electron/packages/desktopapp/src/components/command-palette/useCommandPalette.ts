"use client";

/**
 * useCommandPalette
 *
 * Hook the rest of the app uses to interact with the global palette.
 * Read open state, open/close the modal, and register/unregister
 * scoped commands (the sidebar will lean on register/unregister in
 * phase 2). The provider supplies the implementation via React
 * context; calling outside the provider throws.
 */

import { useContext } from "react";
import {
  CommandPaletteContext,
  type CommandPaletteContextValue,
} from "./CommandPaletteContext";

export function useCommandPalette(): CommandPaletteContextValue {
  const ctx = useContext(CommandPaletteContext);
  if (!ctx) {
    throw new Error("useCommandPalette must be used inside <CommandPaletteProvider>");
  }
  return ctx;
}
