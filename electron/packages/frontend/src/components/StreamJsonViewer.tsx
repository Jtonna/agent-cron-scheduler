"use client";

import React, { useState, useMemo } from "react";
import { ChevronRightIcon } from "@heroicons/react/24/outline";
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

// Event types based on Claude CLI stream-json output
interface BaseEvent {
  type: string;
  [key: string]: unknown;
}

interface SystemEvent extends BaseEvent {
  type: "system";
  session_id?: string;
  tools?: unknown[];
  model?: string;
}

interface TextContent {
  type: "text";
  text: string;
}

interface ToolUseContent {
  type: "tool_use";
  id: string;
  name: string;
  input: Record<string, unknown>;
}

interface ToolResultContent {
  type: "tool_result";
  tool_use_id: string;
  content?: string | unknown[];
  is_error?: boolean;
}

interface AssistantEvent extends BaseEvent {
  type: "assistant";
  content: Array<TextContent | ToolUseContent>;
}

interface UserEvent extends BaseEvent {
  type: "user";
  content: Array<ToolResultContent>;
}

interface ResultEvent extends BaseEvent {
  type: "result";
  success?: boolean;
  error?: string;
  cost?: number;
  usage?: Record<string, unknown>;
}

type ParsedEvent = SystemEvent | AssistantEvent | UserEvent | ResultEvent | BaseEvent;

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
 * Lightweight recursive JSON syntax highlighter (same as LogViewer)
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

  // Extract text messages from assistant events
  const textMessages = useMemo(() => {
    return parsedEvents
      .filter((e): e is AssistantEvent => e.type === "assistant")
      .flatMap((e) =>
        (e.content || []).filter((c): c is TextContent => c.type === "text")
      )
      .map((c) => c.text);
  }, [parsedEvents]);

  // Extract tool calls and their results
  const toolCalls = useMemo(() => {
    const calls: Array<{
      id: string;
      name: string;
      input: Record<string, unknown>;
      result?: ToolResultContent;
    }> = [];

    for (const event of parsedEvents) {
      if (event.type === "assistant") {
        const assistantEvent = event as AssistantEvent;
        for (const c of assistantEvent.content || []) {
          if (c.type === "tool_use") {
            const toolUse = c as ToolUseContent;
            calls.push({
              id: toolUse.id,
              name: toolUse.name,
              input: toolUse.input,
            });
          }
        }
      } else if (event.type === "user") {
        const userEvent = event as UserEvent;
        for (const c of userEvent.content || []) {
          if (c.type === "tool_result") {
            const toolResult = c as ToolResultContent;
            const call = calls.find((tc) => tc.id === toolResult.tool_use_id);
            if (call) {
              call.result = toolResult;
            }
          }
        }
      }
    }

    return calls;
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
        <TabsTrigger value="messages">Messages</TabsTrigger>
        <TabsTrigger value="tools">Tool Calls</TabsTrigger>
        <TabsTrigger value="all">All Parsed</TabsTrigger>
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
                const isExpanded = expandedItems.has(1000 + i); // offset to avoid collision
                const variant = getEventTypeBadgeVariant(event.type);
                const summary = (() => {
                  if (event.type === "system") {
                    const sys = event as SystemEvent;
                    return `Session: ${sys.session_id?.slice(0, 8) || "unknown"}`;
                  }
                  if (event.type === "assistant") {
                    const asst = event as AssistantEvent;
                    const textCount = (asst.content || []).filter(
                      (c) => c.type === "text"
                    ).length;
                    const toolCount = (asst.content || []).filter(
                      (c) => c.type === "tool_use"
                    ).length;
                    return `${textCount} text, ${toolCount} tool(s)`;
                  }
                  if (event.type === "user") {
                    const usr = event as UserEvent;
                    return `${(usr.content || []).length} tool result(s)`;
                  }
                  if (event.type === "result") {
                    const res = event as ResultEvent;
                    return res.success ? "Success" : `Error: ${res.error}`;
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
    </Tabs>
  );
});
