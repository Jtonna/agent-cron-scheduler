"use client";

import React from "react";
import { MinusIcon } from "@heroicons/react/24/outline";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";

interface EnvVarsEditorProps {
  value: Record<string, string>;
  onChange: (vars: Record<string, string>) => void;
}

export function EnvVarsEditor({ value, onChange }: EnvVarsEditorProps) {
  const entries = Object.entries(value);

  const updateKey = (oldKey: string, newKey: string) => {
    const result: Record<string, string> = {};
    for (const [k, v] of entries) {
      if (k === oldKey) {
        result[newKey] = v;
      } else {
        result[k] = v;
      }
    }
    onChange(result);
  };

  const updateValue = (key: string, newValue: string) => {
    onChange({ ...value, [key]: newValue });
  };

  const addEntry = () => {
    const key = `VAR_${entries.length + 1}`;
    onChange({ ...value, [key]: "" });
  };

  const removeEntry = (key: string) => {
    const result = { ...value };
    delete result[key];
    onChange(result);
  };

  return (
    <div className="flex flex-col gap-2 w-full">
      <Label>Environment Variables</Label>
      {entries.length === 0 && (
        <p className="text-sm text-muted-foreground py-2">No environment variables defined.</p>
      )}
      {entries.map(([key, val], index) => (
        <div key={index} className="flex items-center gap-2 w-full">
          <Input
            placeholder="KEY"
            value={key}
            onChange={(e) => updateKey(key, e.target.value)}
            className="font-mono"
          />
          <span className="text-muted-foreground" aria-hidden="true">=</span>
          <Input
            placeholder="value"
            value={val}
            onChange={(e) => updateValue(key, e.target.value)}
            className="font-mono"
          />
          <Button
            variant="ghost"
            size="icon"
            className="h-9 w-9 shrink-0"
            onClick={() => removeEntry(key)}
            type="button"
            aria-label={`Remove variable ${key}`}
          >
            <MinusIcon className="h-4 w-4" />
          </Button>
        </div>
      ))}
      <Button
        variant="secondary"
        size="sm"
        onClick={addEntry}
        type="button"
      >
        + Add Variable
      </Button>
    </div>
  );
}
