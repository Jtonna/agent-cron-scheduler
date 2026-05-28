"use client";

/**
 * Toggle
 *
 * Minimal accessible switch primitive (the "switch" role) used inline
 * inside property rows — e.g. STATUS · Enabled. Uses React Aria's
 * `Switch` for keyboard + ARIA correctness and the design-token palette
 * for visuals (brand fill when on, surface-tertiary track when off).
 *
 * The component is uncontrolled-from-its-own-state perspective: the
 * caller owns `checked` and updates it in `onChange`. A pending state
 * disables interaction without changing the visual checked position
 * (so the user sees the optimistic state while the request flies).
 */

import { Switch } from "react-aria-components";

export interface ToggleProps {
  checked: boolean;
  onChange: (next: boolean) => void;
  /** Used for the accessible name. Required because the switch has no visible label of its own. */
  ariaLabel: string;
  /** Visually + interactively disables the switch (used while a mutation is pending). */
  disabled?: boolean;
  /** Size variant. `sm` is the default sidebar size; `md` is for denser composites. */
  size?: "sm" | "md";
}

export function Toggle({
  checked,
  onChange,
  ariaLabel,
  disabled = false,
  size = "sm",
}: ToggleProps) {
  const trackW = size === "sm" ? "w-7" : "w-9";
  const trackH = size === "sm" ? "h-4" : "h-5";
  const thumb = size === "sm" ? "h-3 w-3" : "h-4 w-4";
  const translateOn = size === "sm" ? "translate-x-3" : "translate-x-4";

  return (
    <Switch
      isSelected={checked}
      onChange={onChange}
      isDisabled={disabled}
      aria-label={ariaLabel}
      className={[
        "group inline-flex items-center outline-none cursor-pointer",
        disabled ? "opacity-50 cursor-not-allowed" : "",
      ].join(" ")}
    >
      <span
        className={[
          "inline-flex items-center rounded-pill p-0.5 transition-colors",
          trackW,
          trackH,
          "outline-none group-focus-visible:ring-2 group-focus-visible:ring-brand-ring",
          checked ? "bg-brand" : "bg-surface-tertiary",
        ].join(" ")}
      >
        <span
          className={[
            "inline-block rounded-pill bg-surface shadow-sm transition-transform",
            thumb,
            checked ? translateOn : "translate-x-0",
          ].join(" ")}
        />
      </span>
    </Switch>
  );
}
