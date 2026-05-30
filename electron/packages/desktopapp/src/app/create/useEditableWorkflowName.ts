"use client";

/**
 * useEditableWorkflowName
 *
 * Tiny hook that hosts the workflow-name state for pages that render a
 * `CanvasBreadcrumb` with an editable name crumb feeding back into
 * `WorkflowGraphEditor` via its controlled-name (`name` + `onNameChange`)
 * surface.
 *
 * Two consumers exist today:
 *
 *   - `/create` — `initialName` is whatever the seed workflow comes with
 *     (usually `""`). The hook initialises state to `initialName` and
 *     just tracks user edits.
 *
 *   - `/workflows/[id]/edit` — `initialName` arrives asynchronously
 *     after the job fetch. The hook starts with `null` ("user hasn't
 *     touched the field yet") and resolves to `initialName ?? ""` until
 *     the user types anything, after which the user's value (including
 *     `""`) is authoritative.
 *
 * Returning `[displayedName, onChange]` lets both call sites stay tiny
 * and avoids duplicating the "first edit replaces the fetched value"
 * subtlety in two places.
 */

import { useCallback, useState } from "react";

export interface UseEditableWorkflowNameOptions {
  /** When provided, used as the displayed name until the user types. */
  initialName?: string | null;
}

export function useEditableWorkflowName(
  options: UseEditableWorkflowNameOptions = {},
): [string, (next: string) => void] {
  const { initialName } = options;
  // `null` = user hasn't touched the field yet; fall back to initialName.
  // Anything else (including "") = user-edited value.
  const [editedName, setEditedName] = useState<string | null>(null);
  const onChange = useCallback((n: string) => setEditedName(n), []);
  const displayedName = editedName ?? initialName ?? "";
  return [displayedName, onChange];
}
