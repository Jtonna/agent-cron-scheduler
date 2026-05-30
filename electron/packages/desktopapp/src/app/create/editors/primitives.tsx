"use client";

/**
 * editors/primitives
 *
 * Tiny shared form bits used by every per-kind step editor body:
 *   - FieldLabel (eyebrow-styled <label>)
 *   - TextInput, MonoTextInput, NumberInput, TextAreaInput
 *   - Segmented (pill-style multi-button)
 *   - ChipInput (token entry; Enter / comma to commit)
 *   - TemplateChips (clickable template-string suggestions that insert
 *     at the cursor of the most-recently-focused field)
 *
 * These are deliberately local to the create-editor — not promoted to
 * `components/ui` — because their density, palette, and behaviours are
 * tuned for the palette-style step editor and would distract elsewhere.
 */

import {
  forwardRef,
  useCallback,
  useEffect,
  useRef,
  useState,
  type InputHTMLAttributes,
  type ReactNode,
  type TextareaHTMLAttributes,
} from "react";
import { X } from "lucide-react";

/* ── Label ───────────────────────────────────────────────────────────── */

export function FieldLabel({ children }: { children: ReactNode }) {
  return (
    <label className="text-eyebrow !text-fg-tertiary block mb-1.5">
      {children}
    </label>
  );
}

/* ── Inputs ──────────────────────────────────────────────────────────── */

const INPUT_BASE =
  "w-full px-2.5 py-1.5 text-sm bg-surface border border-border rounded-input " +
  "text-fg placeholder:text-fg-subtle focus:outline-none focus:border-fg " +
  "focus:ring-2 focus:ring-brand-ring/30";

export const TextInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  function TextInput(props, ref) {
    return (
      <input
        {...props}
        ref={ref}
        className={[INPUT_BASE, props.className ?? ""].join(" ")}
      />
    );
  },
);

export const MonoTextInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  function MonoTextInput(props, ref) {
    return (
      <input
        {...props}
        ref={ref}
        className={[INPUT_BASE, "font-mono text-[12.5px]", props.className ?? ""].join(" ")}
      />
    );
  },
);

export const NumberInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  function NumberInput(props, ref) {
    return (
      <input
        type="number"
        {...props}
        ref={ref}
        className={[INPUT_BASE, "font-mono text-[12.5px]", props.className ?? ""].join(" ")}
      />
    );
  },
);

export const TextAreaInput = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(
  function TextAreaInput(props, ref) {
    return (
      <textarea
        {...props}
        ref={ref}
        className={[
          INPUT_BASE,
          "font-mono text-[12.5px] resize-y min-h-[80px] whitespace-pre",
          props.className ?? "",
        ].join(" ")}
        style={{ tabSize: 2, ...props.style }}
      />
    );
  },
);

/* ── Segmented control ───────────────────────────────────────────────── */

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
}

interface SegmentedProps<T extends string> {
  value: T;
  onChange: (next: T) => void;
  options: SegmentedOption<T>[];
  ariaLabel?: string;
}

export function Segmented<T extends string>({
  value,
  onChange,
  options,
  ariaLabel,
}: SegmentedProps<T>) {
  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className="inline-flex p-0.5 bg-surface-tertiary border border-border rounded-input"
    >
      {options.map((opt) => {
        const on = opt.value === value;
        return (
          <button
            key={opt.value}
            type="button"
            role="radio"
            aria-checked={on}
            onClick={() => onChange(opt.value)}
            className={[
              "px-2.5 py-1 text-[11px] font-mono font-medium rounded-[5px] cursor-pointer transition-colors",
              on
                ? "bg-surface text-fg shadow-sm"
                : "text-fg-muted hover:text-fg",
            ].join(" ")}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}

/* ── Chip input (string or number) ───────────────────────────────────── */

interface ChipInputProps {
  values: string[];
  onChange: (next: string[]) => void;
  placeholder?: string;
  /** When true, only digits are accepted and values are stored as strings of integers. */
  numeric?: boolean;
  ariaLabel?: string;
}

export function ChipInput({
  values,
  onChange,
  placeholder = "add…",
  numeric = false,
  ariaLabel,
}: ChipInputProps) {
  const [draft, setDraft] = useState("");

  const commit = useCallback(() => {
    const v = draft.trim();
    if (!v) return;
    if (numeric && !/^-?\d+$/.test(v)) return;
    if (values.includes(v)) {
      setDraft("");
      return;
    }
    onChange([...values, v]);
    setDraft("");
  }, [draft, numeric, onChange, values]);

  return (
    <div
      aria-label={ariaLabel}
      className="flex flex-wrap gap-1.5 items-center border border-border rounded-input bg-surface px-1.5 py-1 min-h-[34px]"
    >
      {values.map((v) => (
        <span
          key={v}
          className="inline-flex items-center gap-1 bg-brand-muted text-brand-hover font-mono text-[11px] font-medium rounded-pill pl-2 pr-1 py-0.5"
        >
          {v}
          <button
            type="button"
            onClick={() => onChange(values.filter((x) => x !== v))}
            className="inline-flex items-center justify-center h-3.5 w-3.5 rounded-full hover:bg-brand-muted/60 cursor-pointer"
            aria-label={`Remove ${v}`}
          >
            <X size={10} />
          </button>
        </span>
      ))}
      <input
        type="text"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === ",") {
            e.preventDefault();
            commit();
          } else if (e.key === "Backspace" && draft === "" && values.length > 0) {
            onChange(values.slice(0, -1));
          }
        }}
        onBlur={commit}
        placeholder={placeholder}
        className="flex-1 min-w-[60px] bg-transparent outline-none text-[12px] font-mono text-fg placeholder:text-fg-subtle px-1 py-0.5"
      />
    </div>
  );
}

/* ── Template chips ──────────────────────────────────────────────────── */

const DEFAULT_TEMPLATE_CHIPS = [
  "${input.*}",
  "${steps.<id>.stdout}",
  "${steps.<id>.exports.*}",
];

interface TemplateChipsProps {
  /** Override the default chip set. */
  chips?: string[];
  /** Called with the selected chip's template string. */
  onInsert: (template: string) => void;
}

export function TemplateChips({ chips = DEFAULT_TEMPLATE_CHIPS, onInsert }: TemplateChipsProps) {
  return (
    <div className="flex flex-wrap gap-1 mt-1.5">
      {chips.map((c) => (
        <button
          key={c}
          type="button"
          onClick={() => onInsert(c)}
          className="font-mono text-[10.5px] text-fg-muted hover:text-fg bg-surface-tertiary hover:bg-surface-hover border border-transparent hover:border-border px-2 py-0.5 rounded-pill cursor-pointer"
        >
          {c}
        </button>
      ))}
    </div>
  );
}

/**
 * useCursorInsert
 *
 * Hook that wires up a ref to a text input/textarea and returns
 * `(template) => void` that inserts the template at the current cursor
 * (or appends at the end if the field isn't focused).
 */
export function useCursorInsert<T extends HTMLInputElement | HTMLTextAreaElement>(
  value: string,
  onChange: (next: string) => void,
) {
  const elRef = useRef<T | null>(null);
  // Track most-recent selection so insertion still works after the chip click steals focus.
  const selectionRef = useRef<{ start: number; end: number }>({ start: value.length, end: value.length });

  useEffect(() => {
    selectionRef.current = { start: value.length, end: value.length };
  }, [value.length]);

  // Callback ref — accepted by both input and textarea props.
  const attach = useCallback((node: T | null) => {
    elRef.current = node;
  }, []);

  const onSelect = useCallback(() => {
    const el = elRef.current;
    if (!el) return;
    selectionRef.current = {
      start: el.selectionStart ?? value.length,
      end: el.selectionEnd ?? value.length,
    };
  }, [value.length]);

  const insert = useCallback(
    (template: string) => {
      const { start, end } = selectionRef.current;
      const next = value.slice(0, start) + template + value.slice(end);
      onChange(next);
      // After the state lands, push the caret to just past the inserted text.
      requestAnimationFrame(() => {
        const el = elRef.current;
        if (!el) return;
        const caret = start + template.length;
        el.focus();
        try {
          el.setSelectionRange(caret, caret);
        } catch {
          /* selection isn't supported on some input types — ignore */
        }
      });
    },
    [onChange, value],
  );

  return [attach, onSelect, insert] as const;
}

/* ── Key-value editor (small table of [key, value] rows) ─────────────── */

interface KeyValueRow {
  key: string;
  value: string;
}

interface KeyValueEditorProps {
  entries: KeyValueRow[];
  onChange: (next: KeyValueRow[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  addLabel?: string;
}

export function KeyValueEditor({
  entries,
  onChange,
  keyPlaceholder = "key",
  valuePlaceholder = "value",
  addLabel = "+ Add row",
}: KeyValueEditorProps) {
  return (
    <div className="space-y-1.5">
      {entries.map((row, i) => (
        <div key={i} className="grid grid-cols-[1fr_1.4fr_24px] gap-1.5 items-center">
          <MonoTextInput
            value={row.key}
            onChange={(e) => {
              const next = [...entries];
              next[i] = { ...row, key: e.target.value };
              onChange(next);
            }}
            placeholder={keyPlaceholder}
          />
          <MonoTextInput
            value={row.value}
            onChange={(e) => {
              const next = [...entries];
              next[i] = { ...row, value: e.target.value };
              onChange(next);
            }}
            placeholder={valuePlaceholder}
          />
          <button
            type="button"
            onClick={() => onChange(entries.filter((_, j) => j !== i))}
            className="text-fg-subtle hover:text-fg p-1 rounded-input cursor-pointer flex items-center justify-center"
            aria-label="Remove row"
          >
            <X size={14} />
          </button>
        </div>
      ))}
      <button
        type="button"
        onClick={() => onChange([...entries, { key: "", value: "" }])}
        className="w-full text-left bg-transparent border border-dashed border-border text-fg-muted hover:text-fg hover:border-fg text-[11px] px-2.5 py-1.5 rounded-input cursor-pointer transition-colors"
      >
        {addLabel}
      </button>
    </div>
  );
}
