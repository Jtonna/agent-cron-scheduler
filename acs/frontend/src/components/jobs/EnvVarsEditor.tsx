"use client";

import React from "react";
import { Column, Row, Text, Input, Button, IconButton } from "@once-ui-system/core";

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
    <Column gap="8" fillWidth>
      <Text variant="label-default-s" onBackground="neutral-strong">
        Environment Variables
      </Text>
      {entries.map(([key, val], index) => (
        <Row key={index} gap="8" vertical="center" fillWidth>
          <Input
            id={`env-key-${index}`}
            placeholder="KEY"
            value={key}
            onChange={(e) => updateKey(key, e.target.value)}
            style={{ fontFamily: "var(--font-code)" }}
          />
          <Text onBackground="neutral-weak">=</Text>
          <Input
            id={`env-val-${index}`}
            placeholder="value"
            value={val}
            onChange={(e) => updateValue(key, e.target.value)}
            style={{ fontFamily: "var(--font-code)" }}
          />
          <IconButton
            icon="minus"
            variant="tertiary"
            size="s"
            onClick={() => removeEntry(key)}
            tooltip="Remove"
          />
        </Row>
      ))}
      <Button
        variant="secondary"
        size="s"
        onClick={addEntry}
        type="button"
      >
        + Add Variable
      </Button>
    </Column>
  );
}
