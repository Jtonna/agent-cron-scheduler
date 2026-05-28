/**
 * Command palette types — shared by the modal, provider, hook, and
 * the static command builders. Phase 2 (sidebar) will register
 * workflow-scoped commands through `useCommandPalette`.
 */

export interface PaletteCommand {
  /** Stable id; used as React key and cmdk `value`. */
  id: string;
  /** Group label rendered as the cmdk heading. */
  group: string;
  /** Primary label shown to the user. */
  label: string;
  /** Optional secondary hint rendered on the right. */
  hint?: string;
  /** Optional icon rendered before the label. */
  icon?: React.ReactNode;
  /** Extra strings folded into fuzzy search. */
  keywords?: string[];
  /** Run when the user activates the item. */
  action: () => void;
}

/** Opaque handle returned from registerCommands; pass to unregisterCommands. */
export type CommandRegistrationId = string;
