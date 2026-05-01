"use client";

import { useState } from "react";
import { TextField, Input, Button } from "react-aria-components";
import { SendHorizontal } from "lucide-react";

/**
 * ChatBar
 *
 * Pill-shaped input with a brand-colored send button. Used on the home
 * hero and on the chat page. Submits on Enter or send-button click;
 * trims whitespace and ignores empty submissions.
 */

interface ChatBarProps {
  placeholder?: string;
  onSend?: (message: string) => void;
}

export function ChatBar({
  placeholder = "Ask a question, build or do something",
  onSend,
}: ChatBarProps) {
  const [value, setValue] = useState("");

  function handleSend() {
    if (!value.trim()) return;
    onSend?.(value.trim());
    setValue("");
  }

  return (
    <div className="flex items-center border border-border rounded-pill overflow-hidden bg-surface-secondary focus-within:border-brand-ring transition-colors">
      <TextField
        aria-label="Chat input"
        value={value}
        onChange={setValue}
        onKeyDown={(e) => {
          if (e.key === "Enter") handleSend();
        }}
        className="flex-1"
      >
        <Input
          className="w-full bg-transparent px-6 py-4 text-base outline-none placeholder-fg-subtle"
          placeholder={placeholder}
        />
      </TextField>
      <Button
        onPress={handleSend}
        isDisabled={!value.trim()}
        className="w-[var(--width-send-btn)] h-[var(--height-send-btn)] mr-1.5 flex items-center justify-center bg-brand hover:bg-brand-hover disabled:bg-fg-faint rounded-full text-surface transition-colors shrink-0 outline-none focus-visible:ring-2 focus-visible:ring-brand-ring"
      >
        <SendHorizontal size={18} strokeWidth={2.5} />
      </Button>
    </div>
  );
}
