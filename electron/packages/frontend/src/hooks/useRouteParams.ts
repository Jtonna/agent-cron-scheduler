"use client";

import { useMemo } from "react";
import { useParams } from "next/navigation";

/**
 * Extract route params from `window.location.pathname` using a route pattern.
 * Returns null if the pattern doesn't match or window is unavailable.
 */
function extractFromPathname<T extends Record<string, string>>(
  pattern: string
): T | null {
  if (typeof window === "undefined") return null;

  const pathname = window.location.pathname;
  const patternSegments = pattern.split("/").filter(Boolean);
  const pathSegments = pathname.split("/").filter(Boolean);

  if (pathSegments.length !== patternSegments.length) return null;

  const extracted: Record<string, string> = {};
  for (let i = 0; i < patternSegments.length; i++) {
    const ps = patternSegments[i];
    if (ps.startsWith("[") && ps.endsWith("]")) {
      const paramName = ps.slice(1, -1);
      extracted[paramName] = decodeURIComponent(pathSegments[i]);
    } else if (ps !== pathSegments[i]) {
      return null;
    }
  }

  return Object.keys(extracted).length > 0 ? (extracted as T) : null;
}

/**
 * In a Next.js static export, `useParams()` returns the params baked into the
 * statically-generated HTML (e.g. `{ id: "_" }` from the placeholder used in
 * `generateStaticParams`). When the Electron static server serves that HTML for
 * a different URL (e.g. `/jobs/real-uuid`), `useParams()` still returns `_`.
 *
 * This hook reads the actual route params from `window.location.pathname` so
 * the client component sees the real IDs from the URL on the very first render.
 *
 * @param pattern - Route pattern like "/jobs/[id]" or "/jobs/[id]/runs/[runId]"
 * @returns An object mapping param names to their values from the actual URL.
 *          Falls back to `useParams()` if the pattern doesn't match.
 */
export function useRouteParams<T extends Record<string, string>>(
  pattern: string
): T {
  const nextParams = useParams() as T;

  return useMemo(() => {
    return extractFromPathname<T>(pattern) ?? nextParams;
  }, [pattern, nextParams]);
}
