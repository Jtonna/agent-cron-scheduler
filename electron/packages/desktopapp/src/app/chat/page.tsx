"use client";

import { Navbar } from "@/components/navbar/Navbar";
import { ChatBar } from "@/components/ui/ChatBar";

export default function ChatPage() {
  return (
    <div className="min-h-screen bg-surface text-fg flex flex-col">
      <Navbar />
      <div className="flex-1 flex flex-col px-16 py-14">
        <h1 className="text-3xl font-extrabold tracking-tight mb-2">Chat</h1>
        <p className="text-fg-muted text-[15px] mb-8">Ask a question, build or do something.</p>
        <div className="flex-1" />
        <div className="pb-8">
          <ChatBar />
        </div>
      </div>
    </div>
  );
}
