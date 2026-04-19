"use client";

import { PaperAirplaneIcon } from "@heroicons/react/24/solid";

export function ChatInput() {
  return (
    <div className="flex items-center gap-2 p-4">
      <div className="flex-1 flex items-center gap-2 bg-card border border-border rounded-full px-4 py-3 shadow-sm focus-within:ring-1 focus-within:ring-primary/40">
        <input
          type="text"
          placeholder="Ask your agent assistant..."
          className="flex-1 bg-transparent text-foreground placeholder-muted-foreground outline-none text-sm"
          disabled
          readOnly
        />
      </div>
      <button
        type="button"
        disabled
        className="inline-flex items-center justify-center size-10 bg-primary text-primary-foreground rounded-full disabled:opacity-50 disabled:cursor-not-allowed hover:bg-primary/90 transition-colors"
        aria-label="Send message"
      >
        <PaperAirplaneIcon className="size-5" />
      </button>
    </div>
  );
}
