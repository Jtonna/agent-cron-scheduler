"use client";

import { ReactNode, useState } from "react";
import { ChevronDown, MessageCircle, Settings, Sparkles, ExternalLink, Terminal, Globe, Cable, Plug, Bot, BookOpen } from "lucide-react";
import Link from "next/link";

/* ── NavButton (generic, reusable) ── */

interface NavButtonProps {
  children: ReactNode;
  icon?: ReactNode;
  className?: string;
  onClick?: () => void;
}

export function NavButton({ children, icon, className = "", onClick }: NavButtonProps) {
  return (
    <button
      onClick={onClick}
      className={`bg-pink-500 hover:bg-pink-600 text-white text-sm font-semibold px-5 py-2.5 rounded-full transition-colors cursor-pointer flex items-center gap-2 ${className}`}
    >
      {icon}
      {children}
    </button>
  );
}

/* ── API References dropdown ── */

function ApiReferencesDropdown() {
  const [open, setOpen] = useState(false);

  return (
    <div className="relative" onMouseEnter={() => setOpen(true)} onMouseLeave={() => setOpen(false)}>
      <span className="cursor-pointer hover:text-gray-900 flex items-center gap-1">
        API References
        <ChevronDown size={12} />
      </span>
      {open && (
        <div className="absolute top-full left-0 -ml-2 pt-2 z-50">
          <div className="w-48 bg-white border border-gray-200 rounded-xl shadow-lg py-2">
            <Link href="/docs" className="flex items-center gap-2.5 px-4 py-2 text-sm text-gray-600 hover:bg-gray-50 hover:text-gray-900">
              <BookOpen size={14} /> All docs
            </Link>
            <div className="h-px bg-gray-100 my-1" />
            <Link href="/docs/cli" className="flex items-center gap-2.5 px-4 py-2 text-sm text-gray-600 hover:bg-gray-50 hover:text-gray-900">
              <Terminal size={14} /> CLI
            </Link>
            <Link href="/docs/rest" className="flex items-center gap-2.5 px-4 py-2 text-sm text-gray-600 hover:bg-gray-50 hover:text-gray-900">
              <Globe size={14} /> REST
            </Link>
            <Link href="/docs/mcp" className="flex items-center gap-2.5 px-4 py-2 text-sm text-gray-600 hover:bg-gray-50 hover:text-gray-900">
              <Cable size={14} /> MCP
            </Link>
          </div>
        </div>
      )}
    </div>
  );
}

/* ── Instance selector dropdown ── */

function InstanceDropdown() {
  const [open, setOpen] = useState(false);

  return (
    <div className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="w-9 h-9 rounded-full bg-gradient-to-br from-violet-500 to-fuchsia-500 cursor-pointer ring-2 ring-transparent hover:ring-gray-200 transition flex items-center justify-center"
      >
        <Bot size={18} className="text-white" />
      </button>
      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div className="absolute top-full right-0 mt-2 w-64 bg-white border border-gray-200 rounded-xl shadow-lg py-2 z-50">
            <div className="px-4 py-2 text-xs font-semibold text-gray-400 uppercase tracking-wider">Connect to instance</div>
            <button className="w-full flex items-center gap-2.5 px-4 py-2.5 text-sm text-gray-700 hover:bg-gray-50">
              <Plug size={14} className="text-gray-400" />
              <span>New connection...</span>
            </button>
            <div className="h-px bg-gray-100 my-1" />
            <div className="px-4 py-2 text-xs font-semibold text-gray-400 uppercase tracking-wider">Recent instances</div>
            <button className="w-full flex items-center gap-2.5 px-4 py-2.5 text-sm text-gray-700 hover:bg-gray-50">
              <span className="w-2 h-2 rounded-full bg-green-400 shrink-0" />
              <span className="flex-1 text-left truncate">localhost:9090</span>
              <span className="text-xs text-gray-400">local</span>
            </button>
            <button className="w-full flex items-center gap-2.5 px-4 py-2.5 text-sm text-gray-700 hover:bg-gray-50">
              <span className="w-2 h-2 rounded-full bg-gray-300 shrink-0" />
              <span className="flex-1 text-left truncate">acs-prod.example.com</span>
              <span className="text-xs text-gray-400">remote</span>
            </button>
            <button className="w-full flex items-center gap-2.5 px-4 py-2.5 text-sm text-gray-700 hover:bg-gray-50">
              <span className="w-2 h-2 rounded-full bg-gray-300 shrink-0" />
              <span className="flex-1 text-left truncate">192.168.1.50:9090</span>
              <span className="text-xs text-gray-400">remote</span>
            </button>
          </div>
        </>
      )}
    </div>
  );
}

/* ── Navbar ── */

export function Navbar() {
  return (
    <nav className="flex items-center h-16 px-8 border-b border-gray-100">
      <Link href="/" className="text-[22px] font-extrabold tracking-tight mr-10">ACS</Link>

      <div className="flex items-center gap-7 text-[14px] font-semibold text-gray-700">
        <Link href="/systemlogs" className="cursor-pointer hover:text-gray-900">
          System Logs
        </Link>

        <ApiReferencesDropdown />

        <a
          href="https://github.com/Jtonna/agent-cron-scheduler"
          target="_blank"
          rel="noopener noreferrer"
          className="cursor-pointer hover:text-gray-900 flex items-center gap-1"
        >
          GitHub
          <ExternalLink size={11} className="text-gray-400" />
        </a>

        <a
          href="#"
          target="_blank"
          rel="noopener noreferrer"
          className="cursor-pointer hover:text-gray-900 flex items-center gap-1"
        >
          Community
          <ExternalLink size={11} className="text-gray-400" />
        </a>
      </div>

      <div className="ml-auto flex items-center gap-5">
        <Link href="/create">
          <NavButton icon={<Sparkles size={14} />}>Build a Job</NavButton>
        </Link>

        <Link href="/chat" className="text-gray-500 hover:text-gray-700 transition-colors cursor-pointer">
          <MessageCircle size={20} />
        </Link>

        <Link href="/settings" className="text-gray-500 hover:text-gray-700 transition-colors cursor-pointer">
          <Settings size={20} />
        </Link>

        <InstanceDropdown />
      </div>
    </nav>
  );
}
