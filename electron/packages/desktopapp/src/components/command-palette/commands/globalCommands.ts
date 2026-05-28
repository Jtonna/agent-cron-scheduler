/**
 * globalCommands
 *
 * Static "Global" group entries for the global command palette. These
 * are always present regardless of which page the user is on.
 */

import type { PaletteCommand } from "../types";

export const GLOBAL_GROUP = "Global";

export function buildGlobalCommands(navigate: (href: string) => void): PaletteCommand[] {
  return [
    {
      id: "global:new-workflow",
      group: GLOBAL_GROUP,
      label: "New Workflow",
      hint: "Create a workflow",
      keywords: ["create", "new", "workflow", "build"],
      action: () => navigate("/workflows/new"),
    },
    {
      id: "global:view-all-workflows",
      group: GLOBAL_GROUP,
      label: "View All Workflows",
      hint: "Browse every workflow",
      keywords: ["workflows", "all", "list", "browse"],
      action: () => navigate("/workflows"),
    },
  ];
}
