"use client";

/**
 * WorkflowNameInline
 *
 * Inline editable workflow name rendered in the top-left of the canvas
 * (no card chrome). Looks like a heading until clicked, then becomes a
 * styled input. Shows a subtle "Workflow name" placeholder when empty.
 */

import { useEffect, useRef, useState } from "react";

interface WorkflowNameInlineProps {
  value: string;
  onChange: (next: string) => void;
  /** When provided, rendered as a tiny mesh dot before the name. */
  dotMesh?: "mist" | "lemon" | "peach" | "fog" | "ember" | "pulse";
}

export function WorkflowNameInline({ value, onChange, dotMesh = "ember" }: WorkflowNameInlineProps) {
  const [editing, setEditing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);

  return (
    <div className="inline-flex items-center gap-2.5">
      <span
        data-mesh={dotMesh}
        aria-hidden
        className="inline-block h-2 w-2 rounded-full"
      />
      {editing ? (
        <input
          ref={inputRef}
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onBlur={() => setEditing(false)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === "Escape") {
              e.preventDefault();
              setEditing(false);
            }
          }}
          placeholder="workflow-name"
          className="text-[20px] font-semibold tracking-tight text-fg bg-surface-hover px-1.5 py-0.5 rounded-input outline-none w-[320px] max-w-full"
        />
      ) : (
        <button
          type="button"
          onClick={() => setEditing(true)}
          className="text-[20px] font-semibold tracking-tight text-fg hover:bg-surface-hover px-1.5 py-0.5 rounded-input cursor-text text-left max-w-[420px] truncate"
          aria-label="Edit workflow name"
        >
          {value || <span className="text-fg-subtle font-normal">Workflow name</span>}
        </button>
      )}
    </div>
  );
}
