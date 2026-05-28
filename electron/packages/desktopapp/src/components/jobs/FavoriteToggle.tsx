"use client";

import { Star } from "lucide-react";
import { Button } from "react-aria-components";
import { useFavorite } from "@/apis/useFavorite";

/**
 * FavoriteToggle
 *
 * Small star toggle used on job cards (in the main jobs list) and on the
 * job detail header. Renders a filled star when `favorited`, outlined
 * otherwise. Clicking dispatches the matching favorite/unfavorite mutation
 * from `useFavorite()`; while a request is in flight the icon is dimmed.
 *
 * The button stops event propagation so it can be embedded inside a Link
 * row (e.g. JobsListRow) without triggering navigation.
 */

interface FavoriteToggleProps {
  jobId: string;
  favorited: boolean;
  /** Icon size in px. Defaults to 16. */
  size?: number;
  /** Extra classes for the outer button. */
  className?: string;
}

export function FavoriteToggle({
  jobId,
  favorited,
  size = 16,
  className = "",
}: FavoriteToggleProps) {
  const { favorite, unfavorite, isPending } = useFavorite();

  async function toggle() {
    if (favorited) {
      await unfavorite(jobId);
    } else {
      await favorite(jobId);
    }
  }

  return (
    <Button
      onPress={toggle}
      // React Aria's Button surfaces a native press; intercept the underlying
      // click so we don't navigate when this lives inside a Link row.
      onClick={(e) => {
        e.stopPropagation();
      }}
      aria-label={favorited ? "Unfavorite job" : "Favorite job"}
      aria-pressed={favorited}
      isDisabled={isPending}
      className={[
        "inline-flex items-center justify-center rounded-pill p-1 outline-none focus-visible:ring-2 focus-visible:ring-brand-ring transition-opacity",
        isPending ? "opacity-50" : "opacity-100 hover:bg-surface-hover",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <Star
        size={size}
        className={favorited ? "fill-brand text-brand" : "text-fg-subtle"}
      />
    </Button>
  );
}
