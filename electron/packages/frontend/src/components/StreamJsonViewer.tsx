"use client";

import React, { useState, useMemo } from "react";
import { ChevronRightIcon, CurrencyDollarIcon } from "@heroicons/react/24/outline";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { LogViewer } from "@/components/LogViewer";

interface StreamJsonViewerProps {
  content: string;
  loading?: boolean;
  error?: string | null;
  live?: boolean;
  className?: string;
  maxHeight?: string;
}

// ── Claude CLI stream-json event types (real format) ─────────────

interface TextContent {
  type: "text";
  text: string;
}

interface ToolUseContent {
  type: "tool_use";
  id: string;
  name: string;
  input: Record<string, unknown>;
  caller?: unknown;
}

interface ToolResultContent {
  type: "tool_result";
  tool_use_id: string;
  content?: string | unknown[];
  is_error?: boolean;
}

type ContentItem = TextContent | ToolUseContent | ToolResultContent;

interface MessageWrapper {
  role?: string;
  content?: ContentItem[];
  [key: string]: unknown;
}

interface SystemEvent {
  type: "system";
  subtype?: string;
  session_id?: string;
  tools?: string[];
  model?: string;
  [key: string]: unknown;
}

interface AssistantEvent {
  type: "assistant";
  message?: MessageWrapper;
  session_id?: string;
  [key: string]: unknown;
}

interface UserEvent {
  type: "user";
  message?: MessageWrapper;
  session_id?: string;
  tool_use_result?: unknown;
  [key: string]: unknown;
}

interface ResultEvent {
  type: "result";
  subtype?: string;
  is_error?: boolean;
  result?: string;
  total_cost_usd?: number;
  duration_ms?: number;
  num_turns?: number;
  usage?: Record<string, unknown>;
  [key: string]: unknown;
}

type ParsedEvent = SystemEvent | AssistantEvent | UserEvent | ResultEvent;

// ── Cost data interface ───────────────────────────────────────────

interface CostData {
  total_cost_usd?: number;
  duration_ms?: number;
  num_turns?: number;
  usage?: Record<string, unknown>;
  model?: string;
}

// ── Helper formatting functions ───────────────────────────────────

function formatDuration(ms: number): string {
  const totalSeconds = Math.round(ms / 1000);
  if (totalSeconds < 60) {
    return `${totalSeconds}s`;
  }
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return seconds > 0 ? `${hours}h ${minutes}m ${seconds}s` : `${hours}h ${minutes}m`;
  }
  return seconds > 0 ? `${minutes}m ${seconds}s` : `${minutes}m`;
}

function formatCost(usd: number): string {
  if (usd === 0) return "$0.00";
  if (usd < 0.0001) return `$${usd.toExponential(2)}`;
  if (usd < 0.01) return `$${usd.toFixed(4)}`;
  return `$${usd.toFixed(4)}`;
}

function formatTokens(count: number): string {
  return count.toLocaleString("en-US");
}

/**
 * Extract the content array from an event, handling the message wrapper
 */
function getContent(event: ParsedEvent): ContentItem[] {
  if (event.type === "assistant" || event.type === "user") {
    const msg = (event as AssistantEvent | UserEvent).message;
    return msg?.content || [];
  }
  return [];
}

/**
 * Tries to parse a single NDJSON line
 */
function parseJsonLine(line: string): ParsedEvent | null {
  const trimmed = line.trim();
  if (!trimmed) return null;
  try {
    const parsed = JSON.parse(trimmed);
    if (parsed && typeof parsed === "object" && "type" in parsed) {
      return parsed as ParsedEvent;
    }
  } catch {
    // ignore
  }
  return null;
}

/**
 * Lightweight recursive JSON syntax highlighter
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
  return <span>{String(value)}</span>;
}

/**
 * Get badge variant based on event type
 */
function getEventTypeBadgeVariant(
  type: string
): "secondary" | "default" | "outline" | "success" {
  switch (type) {
    case "system":
      return "secondary";
    case "assistant":
      return "default";
    case "user":
      return "outline";
    case "result":
      return "success";
    default:
      return "secondary";
  }
}

export const StreamJsonViewer = React.memo(function StreamJsonViewer({
  content,
  loading = false,
  error = null,
  live = false,
  className = "",
  maxHeight = "calc(100vh - 300px)",
}: StreamJsonViewerProps) {
  const [expandedItems, setExpandedItems] = useState<Set<number>>(new Set());

  // Parse all events from NDJSON
  const parsedEvents = useMemo(() => {
    if (!content) return [];
    const lines = content.split("\n");
    const events: ParsedEvent[] = [];
    for (const line of lines) {
      const parsed = parseJsonLine(line);
      if (parsed) {
        events.push(parsed);
      }
    }
    return events;
  }, [content]);

  // Extract text messages from assistant events (via message.content)
  const textMessages = useMemo(() => {
    return parsedEvents
      .filter((e): e is AssistantEvent => e.type === "assistant")
      .flatMap((e) =>
        getContent(e).filter((c): c is TextContent => c.type === "text")
      )
      .map((c) => c.text)
      .filter((t) => t.trim().length > 0);
  }, [parsedEvents]);

  // Extract tool calls and their results (via message.content)
  const toolCalls = useMemo(() => {
    const calls: Array<{
      id: string;
      name: string;
      input: Record<string, unknown>;
      result?: ToolResultContent;
    }> = [];

    for (const event of parsedEvents) {
      if (event.type === "assistant") {
        for (const c of getContent(event)) {
          if (c.type === "tool_use") {
            calls.push({
              id: c.id,
              name: c.name,
              input: c.input,
            });
          }
        }
      } else if (event.type === "user") {
        for (const c of getContent(event)) {
          if (c.type === "tool_result") {
            const call = calls.find((tc) => tc.id === c.tool_use_id);
            if (call) {
              call.result = c;
            }
          }
        }
      }
    }

    return calls;
  }, [parsedEvents]);

  // Extract cost data from parsed events
  const costData = useMemo((): CostData | null => {
    const resultEvent = [...parsedEvents]
      .reverse()
      .find((e): e is ResultEvent => e.type === "result");
    if (!resultEvent) return null;

    const systemEvent = parsedEvents.find(
      (e): e is SystemEvent => e.type === "system"
    );

    return {
      total_cost_usd: resultEvent.total_cost_usd,
      duration_ms: resultEvent.duration_ms,
      num_turns: resultEvent.num_turns,
      usage: resultEvent.usage,
      model: systemEvent?.model,
    };
  }, [parsedEvents]);

  const toggleExpanded = (index: number) => {
    setExpandedItems((prev) => {
      const next = new Set(prev);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  };

  return (
    <Tabs defaultValue="raw" className={className}>
      <TabsList className="w-full justify-start">
        <TabsTrigger value="raw">Raw</TabsTrigger>
        <TabsTrigger value="messages">
          Messages{textMessages.length > 0 && ` (${textMessages.length})`}
        </TabsTrigger>
        <TabsTrigger value="tools">
          Tool Calls{toolCalls.length > 0 && ` (${toolCalls.length})`}
        </TabsTrigger>
        <TabsTrigger value="all">All Parsed</TabsTrigger>
        <TabsTrigger value="cost" className="flex items-center gap-1">
          <CurrencyDollarIcon className="h-3.5 w-3.5" />
          Cost
        </TabsTrigger>
      </TabsList>

      {/* RAW mode */}
      <TabsContent value="raw" className="mt-0">
        <LogViewer
          content={content}
          loading={loading}
          error={error}
          live={live}
          maxHeight={maxHeight}
        />
      </TabsContent>

      {/* MESSAGES mode */}
      <TabsContent value="messages" className="mt-0">
        <div
          className="overflow-auto rounded-lg border bg-muted/40"
          style={{ maxHeight }}
        >
          {textMessages.length === 0 ? (
            <div className="p-8 text-center">
              <p className="text-sm text-muted-foreground">No messages found.</p>
            </div>
          ) : (
            <div className="prose prose-sm dark:prose-invert max-w-none p-4 space-y-4">
              {textMessages.map((msg, i) => (
                <div
                  key={i}
                  className="border-b border-border/30 pb-4 last:border-b-0 last:pb-0"
                >
                  <p className="whitespace-pre-wrap text-sm leading-relaxed">
                    {msg}
                  </p>
                </div>
              ))}
            </div>
          )}
        </div>
      </TabsContent>

      {/* TOOL CALLS mode */}
      <TabsContent value="tools" className="mt-0">
        <div
          className="overflow-auto rounded-lg border bg-muted/40"
          style={{ maxHeight }}
        >
          {toolCalls.length === 0 ? (
            <div className="p-8 text-center">
              <p className="text-sm text-muted-foreground">No tool calls found.</p>
            </div>
          ) : (
            <div className="p-2 space-y-2">
              {toolCalls.map((tool, i) => {
                const isExpanded = expandedItems.has(i);
                return (
                  <div
                    key={i}
                    className="border rounded-lg bg-background overflow-hidden"
                  >
                    <button
                      type="button"
                      className="flex items-center gap-2 w-full text-left px-3 py-2 hover:bg-accent/30 cursor-pointer"
                      onClick={() => toggleExpanded(i)}
                      aria-expanded={isExpanded}
                    >
                      <ChevronRightIcon
                        className={`h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform duration-150 ${
                          isExpanded ? "rotate-90" : ""
                        }`}
                      />
                      <Badge variant="secondary" className="text-xs">
                        {tool.name}
                      </Badge>
                      <span className="text-xs text-muted-foreground font-mono truncate">
                        {tool.id}
                      </span>
                    </button>
                    {isExpanded && (
                      <div className="border-t px-3 py-2 space-y-2">
                        <div>
                          <p className="text-xs font-semibold text-muted-foreground mb-1">
                            Input:
                          </p>
                          <pre className="font-mono text-[12px] leading-relaxed bg-muted/50 p-2 rounded overflow-x-auto">
                            <JsonSyntaxHighlight value={tool.input} />
                          </pre>
                        </div>
                        {tool.result && (
                          <div>
                            <p className="text-xs font-semibold text-muted-foreground mb-1">
                              Result:
                            </p>
                            <pre className="font-mono text-[12px] leading-relaxed bg-muted/50 p-2 rounded overflow-x-auto">
                              <JsonSyntaxHighlight value={tool.result.content} />
                            </pre>
                            {tool.result.is_error && (
                              <Badge variant="destructive" className="text-xs mt-1">
                                Error
                              </Badge>
                            )}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </TabsContent>

      {/* ALL PARSED mode */}
      <TabsContent value="all" className="mt-0">
        <div
          className="overflow-auto rounded-lg border bg-muted/40"
          style={{ maxHeight }}
        >
          {parsedEvents.length === 0 ? (
            <div className="p-8 text-center">
              <p className="text-sm text-muted-foreground">
                No parseable events found.
              </p>
            </div>
          ) : (
            <div className="p-2 space-y-2">
              {parsedEvents.map((event, i) => {
                const isExpanded = expandedItems.has(1000 + i);
                const variant = getEventTypeBadgeVariant(event.type);
                const summary = (() => {
                  if (event.type === "system") {
                    const sys = event as SystemEvent;
                    return `Session: ${sys.session_id?.slice(0, 8) || "unknown"}`;
                  }
                  if (event.type === "assistant") {
                    const items = getContent(event);
                    const textCount = items.filter(
                      (c) => c.type === "text"
                    ).length;
                    const toolCount = items.filter(
                      (c) => c.type === "tool_use"
                    ).length;
                    const parts: string[] = [];
                    if (textCount > 0) parts.push(`${textCount} text`);
                    if (toolCount > 0) parts.push(`${toolCount} tool(s)`);
                    return parts.length > 0 ? parts.join(", ") : "empty";
                  }
                  if (event.type === "user") {
                    const items = getContent(event);
                    return `${items.length} tool result(s)`;
                  }
                  if (event.type === "result") {
                    const res = event as ResultEvent;
                    if (res.is_error) {
                      return `Error: ${res.result || "unknown"}`;
                    }
                    const cost = res.total_cost_usd;
                    const costStr = cost != null ? ` ($${cost.toFixed(4)})` : "";
                    return `${res.subtype || "done"}${costStr}`;
                  }
                  return JSON.stringify(event).slice(0, 50) + "...";
                })();

                return (
                  <div
                    key={i}
                    className="border rounded-lg bg-background overflow-hidden"
                  >
                    <button
                      type="button"
                      className="flex items-center gap-2 w-full text-left px-3 py-2 hover:bg-accent/30 cursor-pointer"
                      onClick={() => toggleExpanded(1000 + i)}
                      aria-expanded={isExpanded}
                    >
                      <ChevronRightIcon
                        className={`h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform duration-150 ${
                          isExpanded ? "rotate-90" : ""
                        }`}
                      />
                      <Badge variant={variant} className="text-xs">
                        {event.type}
                      </Badge>
                      <span className="text-xs text-muted-foreground truncate">
                        {summary}
                      </span>
                    </button>
                    {isExpanded && (
                      <div className="border-t px-3 py-2">
                        <pre className="font-mono text-[12px] leading-relaxed bg-muted/50 p-2 rounded overflow-x-auto">
                          <JsonSyntaxHighlight value={event} />
                        </pre>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </TabsContent>
      {/* COST mode */}
      <TabsContent value="cost" className="mt-0">
        <div
          className="overflow-auto rounded-lg border bg-muted/40"
          style={{ maxHeight }}
        >
          {!costData ? (
            <div className="p-8 text-center">
              <CurrencyDollarIcon className="h-8 w-8 mx-auto mb-2 text-muted-foreground/40" />
              <p className="text-sm text-muted-foreground">
                No cost data available for this run
              </p>
            </div>
          ) : (
            <div className="p-4 space-y-4">
              {/* Summary cards */}
              <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                <div className="rounded-lg border bg-background p-3">
                  <p className="text-xs text-muted-foreground mb-1">Total Cost</p>
                  <p className="font-mono text-lg font-semibold text-primary">
                    {costData.total_cost_usd != null
                      ? formatCost(costData.total_cost_usd)
                      : "--"}
                  </p>
                </div>
                <div className="rounded-lg border bg-background p-3">
                  <p className="text-xs text-muted-foreground mb-1">Duration</p>
                  <p className="font-mono text-lg font-semibold text-primary">
                    {costData.duration_ms != null
                      ? formatDuration(costData.duration_ms)
                      : "--"}
                  </p>
                </div>
                <div className="rounded-lg border bg-background p-3">
                  <p className="text-xs text-muted-foreground mb-1">Turns</p>
                  <p className="font-mono text-lg font-semibold text-primary">
                    {costData.num_turns != null ? costData.num_turns : "--"}
                  </p>
                </div>
                <div className="rounded-lg border bg-background p-3">
                  <p className="text-xs text-muted-foreground mb-1">Model</p>
                  <p className="text-sm font-medium text-primary truncate">
                    {costData.model ?? "--"}
                  </p>
                </div>
              </div>

              {/* Token usage */}
              {costData.usage && Object.keys(costData.usage).length > 0 && (
                <div className="rounded-lg border bg-background overflow-hidden">
                  <div className="px-3 py-2 border-b bg-muted/30">
                    <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
                      Token Usage
                    </p>
                  </div>
                  <table className="w-full text-sm">
                    <tbody>
                      {(() => {
                        // Flatten usage entries: expand nested objects into sub-rows,
                        // skip null/undefined/empty values, and skip object-valued
                        // keys that are already represented by flat numeric fields.
                        const flatEntries: Array<{ key: string; value: number | string }> = [];
                        const usageObj = costData.usage as Record<string, unknown>;
                        for (const [key, value] of Object.entries(usageObj)) {
                          if (value === null || value === undefined) continue;
                          if (typeof value === "object" && !Array.isArray(value)) {
                            // Expand nested object into flat sub-entries prefixed by parent key
                            for (const [subKey, subVal] of Object.entries(value as Record<string, unknown>)) {
                              if (subVal === null || subVal === undefined) continue;
                              if (typeof subVal === "number" || typeof subVal === "string") {
                                flatEntries.push({ key: `${key}_${subKey}`, value: subVal as number | string });
                              }
                            }
                          } else if (typeof value === "number") {
                            flatEntries.push({ key, value });
                          } else if (typeof value === "string") {
                            if (value.trim() === "") continue;
                            flatEntries.push({ key, value });
                          }
                        }
                        return flatEntries.map(({ key, value }, i) => {
                          const label = key
                            .replace(/_/g, " ")
                            .replace(/\b\w/g, (c) => c.toUpperCase());
                          const displayValue = typeof value === "number"
                            ? formatTokens(value)
                            : value;
                          return (
                            <tr
                              key={key}
                              className={i % 2 === 0 ? "bg-background" : "bg-muted/20"}
                            >
                              <td className="px-3 py-2 text-muted-foreground">
                                {label}
                              </td>
                              <td className="px-3 py-2 text-right font-mono font-medium text-primary">
                                {displayValue}
                              </td>
                            </tr>
                          );
                        });
                      })()}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}
        </div>
      </TabsContent>
    </Tabs>
  );
});
