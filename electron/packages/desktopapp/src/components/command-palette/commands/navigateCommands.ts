/**
 * navigateCommands
 *
 * Static "Navigate" group entries for the global command palette. Pure
 * data — the palette consumes these to render a cmdk group. Pages /
 * subviews are reached via Next.js navigation rather than the more
 * elaborate cmdk sub-view pattern (kept simple for v1; see report
 * notes for phase 2 follow-ups).
 */

import type { PaletteCommand } from "../types";

export const NAVIGATE_GROUP = "Navigate";

export function buildNavigateCommands(navigate: (href: string) => void): PaletteCommand[] {
  return [
    {
      id: "navigate:workflows",
      group: NAVIGATE_GROUP,
      label: "Go to workflow…",
      hint: "Open workflows list",
      keywords: ["workflow", "jobs", "list"],
      action: () => navigate("/workflows"),
    },
    {
      id: "navigate:runs",
      group: NAVIGATE_GROUP,
      label: "Go to run…",
      hint: "Open recent runs",
      keywords: ["run", "history", "recent"],
      action: () => navigate("/"),
    },
    {
      id: "navigate:dashboard",
      group: NAVIGATE_GROUP,
      label: "Dashboard",
      keywords: ["home", "overview"],
      action: () => navigate("/"),
    },
    {
      id: "navigate:system-logs",
      group: NAVIGATE_GROUP,
      label: "System Logs",
      keywords: ["logs", "system", "debug"],
      action: () => navigate("/systemlogs"),
    },
  ];
}
