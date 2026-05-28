import { ReactNode } from "react";

/**
 * NoirCallout
 *
 * A black, slightly-grainy callout panel — adopted from acs-ui-refresh's
 * `.noir-card` composition pattern. Sits inside pastel persona surfaces
 * (mesh widgets, gradient slides) to host the headline numeral and an
 * eyebrow label, giving the surface dramatic contrast against the soft
 * background.
 *
 * Renders as a self-contained block; layout (sizing, position) is the
 * caller's responsibility. Pass an optional eyebrow label and any
 * children for the body.
 */

interface NoirCalloutProps {
  /** Mono uppercase micro-label rendered above the body. */
  eyebrow?: string;
  /** Optional inline icon shown next to the eyebrow. */
  icon?: ReactNode;
  /** Main body — typically a `.text-display` numeral + supporting copy. */
  children: ReactNode;
  className?: string;
}

export function NoirCallout({ eyebrow, icon, children, className = "" }: NoirCalloutProps) {
  return (
    <div className={`noir-card ${className}`}>
      {eyebrow && (
        <div className="text-eyebrow inline-flex items-center gap-2 mb-2">
          {icon && <span className="shrink-0 opacity-70">{icon}</span>}
          <span>{eyebrow}</span>
        </div>
      )}
      {children}
    </div>
  );
}
