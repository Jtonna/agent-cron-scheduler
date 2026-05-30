"use client";

/**
 * HttpStepBody
 *
 * Editor body for an HTTP step. Method segmented control, URL input,
 * key/value header editor, body textarea with a JSON/text display hint
 * pill, expect-status chip input, and a timeout number input.
 */

import { useMemo, useState } from "react";
import type { NewHttpStep } from "../types";
import {
  ChipInput,
  FieldLabel,
  KeyValueEditor,
  MonoTextInput,
  NumberInput,
  Segmented,
  TextAreaInput,
} from "./primitives";

interface HttpStepBodyProps {
  value: NewHttpStep;
  onChange: (next: NewHttpStep) => void;
}

const METHODS = [
  { value: "GET", label: "GET" },
  { value: "POST", label: "POST" },
  { value: "PUT", label: "PUT" },
  { value: "PATCH", label: "PATCH" },
  { value: "DELETE", label: "DELETE" },
];

export function HttpStepBody({ value, onChange }: HttpStepBodyProps) {
  const [bodyKind, setBodyKind] = useState<"json" | "text">(() =>
    looksLikeJson(value.body ?? "") ? "json" : "text",
  );

  const headerEntries = useMemo(
    () => Object.entries(value.headers ?? {}).map(([k, v]) => ({ key: k, value: v })),
    [value.headers],
  );

  return (
    <div className="space-y-4">
      <div>
        <FieldLabel>HTTP method</FieldLabel>
        <Segmented
          value={value.method}
          onChange={(next) => onChange({ ...value, method: next })}
          options={METHODS}
          ariaLabel="HTTP method"
        />
      </div>

      <div>
        <FieldLabel>URL</FieldLabel>
        <MonoTextInput
          value={value.url}
          onChange={(e) => onChange({ ...value, url: e.target.value })}
          placeholder="https://api.example.com/v1/ping"
        />
      </div>

      <div>
        <FieldLabel>Headers</FieldLabel>
        <KeyValueEditor
          entries={headerEntries}
          onChange={(next) => {
            const obj: Record<string, string> = {};
            for (const { key, value: v } of next) {
              if (!key) continue;
              obj[key] = v;
            }
            onChange({
              ...value,
              headers: Object.keys(obj).length > 0 ? obj : undefined,
            });
          }}
          keyPlaceholder="Header-Name"
          valuePlaceholder="value"
          addLabel="+ Add header"
        />
      </div>

      <div>
        <FieldLabel>Body</FieldLabel>
        <div className="relative">
          <button
            type="button"
            onClick={() => setBodyKind((k) => (k === "json" ? "text" : "json"))}
            className="absolute top-1.5 right-1.5 z-10 text-[10px] font-semibold bg-brand-muted text-brand-hover px-2 py-0.5 rounded-pill cursor-pointer border border-transparent hover:border-brand"
            aria-label="Toggle body display mode"
          >
            {bodyKind === "json" ? "JSON" : "text"}
          </button>
          <TextAreaInput
            value={value.body ?? ""}
            onChange={(e) =>
              onChange({
                ...value,
                body: e.target.value.length > 0 ? e.target.value : null,
              })
            }
            placeholder={bodyKind === "json" ? '{ "key": "value" }' : "raw body…"}
            spellCheck={false}
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div>
          <FieldLabel>Expected status codes</FieldLabel>
          <ChipInput
            values={(value.expect_status ?? []).map((n) => n.toString())}
            onChange={(next) =>
              onChange({
                ...value,
                expect_status: next
                  .map((s) => parseInt(s, 10))
                  .filter((n) => !Number.isNaN(n)),
              })
            }
            numeric
            placeholder="200"
            ariaLabel="Expected status codes"
          />
        </div>
        <div>
          <FieldLabel>Timeout (seconds)</FieldLabel>
          <NumberInput
            value={value.timeout_secs ?? ""}
            onChange={(e) =>
              onChange({
                ...value,
                timeout_secs: e.target.value.length > 0 ? parseInt(e.target.value, 10) : null,
              })
            }
            placeholder="30"
          />
        </div>
      </div>
    </div>
  );
}

function looksLikeJson(s: string): boolean {
  const t = s.trim();
  return t.startsWith("{") || t.startsWith("[");
}
