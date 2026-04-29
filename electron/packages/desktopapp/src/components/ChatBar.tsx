"use client";

import { useState } from "react";
import { SendHorizontal } from "lucide-react";

interface ChatBarProps {
  placeholder?: string;
  onSend?: (message: string) => void;
}

export function ChatBar({ placeholder = "Ask a question, build or do something", onSend }: ChatBarProps) {
  const [value, setValue] = useState("");

  function handleSend() {
    if (!value.trim()) return;
    onSend?.(value.trim());
    setValue("");
  }

  return (
    <div className="flex items-center border border-gray-200 rounded-full overflow-hidden bg-gray-50 focus-within:border-pink-300 transition-colors">
      <input
        className="flex-1 bg-transparent px-6 py-4 text-base outline-none placeholder-gray-400"
        placeholder={placeholder}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Enter") handleSend(); }}
      />
      <button
        onClick={handleSend}
        disabled={!value.trim()}
        className="w-12 h-12 mr-1.5 flex items-center justify-center bg-pink-500 hover:bg-pink-600 disabled:bg-gray-300 rounded-full text-white transition-colors shrink-0"
      >
        <SendHorizontal size={18} strokeWidth={2.5} />
      </button>
    </div>
  );
}
