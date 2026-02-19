"use client";

import React, { useState, useEffect, useRef, useMemo, useCallback } from "react";
import {
  MagnifyingGlassIcon,
  XMarkIcon,
  ArrowDownIcon,
  ArrowsPointingOutIcon,
  ChevronRightIcon,
} from "@heroicons/react/24/outline";
import { ArrowPathIcon } from "@heroicons/react/24/outline";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

interface LogViewerProps {
  content: string;
  loading?: boolean;
  error?: string | null;
  live?: boolean;
  className?: string;
  maxHeight?: string;
}

interface LogLine {
  number: number;
  text: string;
  parsedJson?: Record<string, unknown> | unknown[];
}

function highlightMatches(text: string, query: string): React.ReactNode {
  if (!query) return text;
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const regex = new RegExp(`(${escaped})`, "gi");
  const parts = text.split(regex);
  if (parts.length === 1) return text;
  return parts.map((part, i) =>
    regex.test(part) ? (
      <mark
        key={i}
        className="bg-yellow-200 dark:bg-yellow-900/50 text-inherit rounded-sm px-0.5"
      >
        {part}
      </mark>
    ) : (
      part
    )
  );
}

/**
 * Tries to parse a string as JSON. Returns the parsed value only if it's
 * an object or array (not a primitive).
 */
function tryParseJson(text: string): Record<string, unknown> | unknown[] | undefined {
  const trimmed = text.trim();
  // Quick guard: only attempt parse if the line starts with { or [
  if (trimmed.length === 0 || (trimmed[0] !== "{" && trimmed[0] !== "[")) {
    return undefined;
  }
  try {
    const parsed = JSON.parse(trimmed);
    if (parsed !== null && typeof parsed === "object") {
      return parsed as Record<string, unknown> | unknown[];
    }
  } catch {
    // not JSON
  }
  return undefined;
}

/**
 * Truncates raw JSON text for the collapsed preview.
 */
function jsonPreviewText(raw: string, maxLen = 100): string {
  const trimmed = raw.trim();
  if (trimmed.length <= maxLen) return trimmed;
  const suffix = trimmed[0] === "[" ? " ...]" : " ...}";
  return trimmed.slice(0, maxLen - suffix.length) + suffix;
}

/**
 * Lightweight recursive JSON syntax highlighter. Not exported.
 */
function JsonSyntaxHighlight({ value, indent = 0 }: { value: unknown; indent?: number }) {
  const pad = "  ".repeat(indent);
  const padInner = "  ".repeat(indent + 1);

  if (value === null) {
    return <span className="text-muted-foreground italic">null</span>;
  }
  if (typeof value === "boolean") {
    return <span className="text-amber-600 dark:text-amber-400">{String(value)}</span>;
  }
  if (typeof value === "number") {
    return <span className="text-blue-600 dark:text-blue-400">{String(value)}</span>;
  }
  if (typeof value === "string") {
    return (
      <span className="text-emerald-600 dark:text-emerald-400">
        &quot;{value}&quot;
      </span>
    );
  }
  if (Array.isArray(value)) {
    if (value.length === 0) {
      return <span className="text-muted-foreground">{"[]"}</span>;
    }
    return (
      <>
        <span className="text-muted-foreground">{"["}</span>
        {"\n"}
        {value.map((item, i) => (
          <React.Fragment key={i}>
            {padInner}
            <JsonSyntaxHighlight value={item} indent={indent + 1} />
            {i < value.length - 1 && (
              <span className="text-muted-foreground">,</span>
            )}
            {"\n"}
          </React.Fragment>
        ))}
        {pad}
        <span className="text-muted-foreground">{"]"}</span>
      </>
    );
  }
  if (typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>);
    if (entries.length === 0) {
      return <span className="text-muted-foreground">{"{}"}</span>;
    }
    return (
      <>
        <span className="text-muted-foreground">{"{"}</span>
        {"\n"}
        {entries.map(([key, val], i) => (
          <React.Fragment key={key}>
            {padInner}
            <span className="text-primary">&quot;{key}&quot;</span>
            <span className="text-muted-foreground">: </span>
            <JsonSyntaxHighlight value={val} indent={indent + 1} />
            {i < entries.length - 1 && (
              <span className="text-muted-foreground">,</span>
            )}
            {"\n"}
          </React.Fragment>
        ))}
        {pad}
        <span className="text-muted-foreground">{"}"}</span>
      </>
    );
  }
  // Fallback for any other type
  return <span>{String(value)}</span>;
}

export const LogViewer = React.memo(function LogViewer({
  content,
  loading = false,
  error = null,
  live = false,
  className = "",
  maxHeight = "calc(100vh - 300px)",
}: LogViewerProps) {
  const [search, setSearch] = useState("");
  const [showAllLines, setShowAllLines] = useState(false);
  const [wordWrap, setWordWrap] = useState(true);
  const [userScrolled, setUserScrolled] = useState(false);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  // Track which JSON lines are expanded
  const [expandedJsonLines, setExpandedJsonLines] = useState<Set<number>>(
    () => new Set()
  );

  const toggleJsonLine = useCallback((lineNumber: number) => {
    setExpandedJsonLines((prev) => {
      const next = new Set(prev);
      if (next.has(lineNumber)) {
        next.delete(lineNumber);
      } else {
        next.add(lineNumber);
      }
      return next;
    });
  }, []);

  // Parse lines, detecting JSON
  const allLines: LogLine[] = useMemo(() => {
    if (!content) return [];
    const raw = content.split("\n");
    // Remove trailing empty line from split
    if (raw.length > 0 && raw[raw.length - 1] === "") {
      raw.pop();
    }
    return raw.map((text, i) => {
      const parsedJson = tryParseJson(text);
      return { number: i + 1, text, parsedJson };
    });
  }, [content]);

  // Filter lines by search
  const query = search.trim().toLowerCase();

  const matchingLineNumbers = useMemo(() => {
    if (!query) return new Set<number>();
    const set = new Set<number>();
    for (const line of allLines) {
      if (line.text.toLowerCase().includes(query)) {
        set.add(line.number);
      }
    }
    return set;
  }, [allLines, query]);

  const displayLines = useMemo(() => {
    if (!query || showAllLines) return allLines;
    return allLines.filter((line) => matchingLineNumbers.has(line.number));
  }, [allLines, query, showAllLines, matchingLineNumbers]);

  const matchCount = matchingLineNumbers.size;
  const totalLines = allLines.length;
  const gutterWidth = Math.max(String(totalLines).length, 3);

  // Scroll tracking
  const handleScroll = useCallback(() => {
    const el = scrollContainerRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
    setUserScrolled(!atBottom);
  }, []);

  // Auto-scroll to bottom for live logs
  useEffect(() => {
    if (live && !userScrolled) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [content, live, userScrolled]);

  const jumpToBottom = useCallback(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    setUserScrolled(false);
  }, []);

  // Loading state
  if (loading) {
    return (
      <div className={`flex items-center justify-center py-8 ${className}`}>
        <ArrowPathIcon className="h-6 w-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div className={`rounded-lg border border-destructive/50 bg-destructive/10 p-4 ${className}`}>
        <p className="text-sm text-destructive">Error loading log: {error}</p>
      </div>
    );
  }

  // Empty content
  if (!content) {
    return (
      <div className={`rounded-lg border bg-muted/50 p-8 text-center ${className}`}>
        <p className="text-sm text-muted-foreground">No output.</p>
      </div>
    );
  }

  return (
    <div className={`flex flex-col rounded-lg border bg-background ${className}`}>
      {/* Toolbar */}
      <div className="flex items-center gap-2 px-3 py-2 border-b bg-muted/30">
        {/* Search */}
        <div className="relative flex-1 max-w-sm">
          <MagnifyingGlassIcon className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
          <Input
            placeholder="Search logs..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="h-7 pl-8 pr-8 text-xs"
          />
          {search.length > 0 && (
            <Button
              variant="ghost"
              size="icon"
              className="absolute right-0.5 top-1/2 -translate-y-1/2 h-6 w-6"
              onClick={() => setSearch("")}
              aria-label="Clear search"
            >
              <XMarkIcon className="h-3 w-3" />
            </Button>
          )}
        </div>

        {/* Stats */}
        <span className="text-xs text-muted-foreground whitespace-nowrap">
          {query
            ? `${matchCount} / ${totalLines} matches`
            : `${totalLines} lines`}
        </span>

        {/* Show all / matching toggle (when searching) */}
        {query && (
          <Button
            variant={showAllLines ? "secondary" : "outline"}
            size="sm"
            className="h-7 text-xs px-2"
            onClick={() => setShowAllLines((v) => !v)}
          >
            {showAllLines ? "All" : "Matches"}
          </Button>
        )}

        {/* Wrap toggle */}
        <TooltipProvider delayDuration={200}>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant={wordWrap ? "secondary" : "outline"}
                size="icon"
                className="h-7 w-7"
                onClick={() => setWordWrap((v) => !v)}
                aria-label="Toggle word wrap"
              >
                <ArrowsPointingOutIcon className="h-3.5 w-3.5" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              {wordWrap ? "Disable word wrap" : "Enable word wrap"}
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>

        {/* Live badge */}
        {live && (
          <Badge variant="running" className="animate-pulse text-xs h-5">
            Live
          </Badge>
        )}
      </div>

      {/* Log content */}
      <div
        ref={scrollContainerRef}
        onScroll={handleScroll}
        className="overflow-auto"
        style={{ maxHeight }}
      >
        <div className="font-mono text-[13px] leading-relaxed bg-muted/40 min-w-0">
          {displayLines.map((line) => {
            const isMatch = query && matchingLineNumbers.has(line.number);
            const isDimmed = query && showAllLines && !isMatch;
            const isJson = !!line.parsedJson;
            const isExpanded = isJson && expandedJsonLines.has(line.number);

            return (
              <div
                key={line.number}
                className={`flex hover:bg-accent/50 border-b border-border/30 group ${
                  isDimmed ? "opacity-40" : ""
                } ${isMatch ? "bg-yellow-50/50 dark:bg-yellow-900/10" : ""}`}
              >
                {/* Line number gutter */}
                <span
                  className="select-none text-muted-foreground/60 text-right px-3 py-0.5 border-r border-border/40 shrink-0 tabular-nums"
                  style={{ minWidth: `${gutterWidth + 2}ch` }}
                >
                  {line.number}
                </span>
                {/* Line content */}
                {isJson ? (
                  <div className="px-3 py-0.5 min-w-0 flex-1">
                    {/* Collapsed JSON preview / trigger */}
                    <button
                      type="button"
                      className="flex items-center gap-1.5 w-full text-left cursor-pointer hover:bg-accent/30 rounded -mx-1 px-1"
                      onClick={() => toggleJsonLine(line.number)}
                      aria-expanded={isExpanded}
                    >
                      <ChevronRightIcon
                        className={`h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform duration-150 ${
                          isExpanded ? "rotate-90" : ""
                        }`}
                      />
                      <span
                        className={`truncate ${
                          wordWrap
                            ? "whitespace-pre-wrap break-words"
                            : "whitespace-pre"
                        }`}
                      >
                        {query && isMatch
                          ? highlightMatches(
                              jsonPreviewText(line.text),
                              search.trim()
                            )
                          : jsonPreviewText(line.text)}
                      </span>
                      <Badge
                        variant="outline"
                        className="ml-1.5 shrink-0 text-[10px] px-1.5 py-0 h-4 font-mono opacity-60"
                      >
                        JSON
                      </Badge>
                    </button>
                    {/* Expanded JSON view */}
                    {isExpanded && (
                      <div className="mt-1 ml-5 border-l-2 border-border/60 pl-3 pb-1">
                        <pre className="text-[12px] leading-relaxed whitespace-pre-wrap break-words">
                          <JsonSyntaxHighlight value={line.parsedJson} />
                        </pre>
                      </div>
                    )}
                  </div>
                ) : (
                  <span
                    className={`px-3 py-0.5 min-w-0 ${
                      wordWrap
                        ? "whitespace-pre-wrap break-words"
                        : "whitespace-pre"
                    }`}
                  >
                    {query && isMatch
                      ? highlightMatches(line.text, search.trim())
                      : line.text || "\u00A0"}
                  </span>
                )}
              </div>
            );
          })}
          <div ref={bottomRef} />
        </div>
      </div>

      {/* Jump to bottom */}
      {userScrolled && (
        <div className="flex justify-center py-1 border-t bg-muted/30">
          <Button
            variant="ghost"
            size="sm"
            className="h-6 text-xs gap-1"
            onClick={jumpToBottom}
          >
            <ArrowDownIcon className="h-3 w-3" />
            Jump to bottom
          </Button>
        </div>
      )}
    </div>
  );
});
