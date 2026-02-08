"use client";

import React, {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  useCallback,
} from "react";

interface SSEEvent {
  type: string;
  data: string;
}

type SSECallback = (event: SSEEvent) => void;

interface SSEContextValue {
  subscribe: (callback: SSECallback) => () => void;
  connected: boolean;
}

const SSEContext = createContext<SSEContextValue | null>(null);

const BASE_URL = process.env.NEXT_PUBLIC_API_URL ?? "";

export function SSEProvider({ children }: { children: React.ReactNode }) {
  const subscribersRef = useRef<Set<SSECallback>>(new Set());
  const [connected, setConnected] = useState(false);
  const eventSourceRef = useRef<EventSource | null>(null);

  useEffect(() => {
    let reconnectTimer: ReturnType<typeof setTimeout>;

    function connect() {
      const es = new EventSource(`${BASE_URL}/api/events`);
      eventSourceRef.current = es;

      es.onopen = () => {
        setConnected(true);
      };

      es.onmessage = (event) => {
        const sseEvent: SSEEvent = {
          type: event.type || "message",
          data: event.data,
        };
        subscribersRef.current.forEach((cb) => {
          try {
            cb(sseEvent);
          } catch {
            // ignore subscriber errors
          }
        });
      };

      // Listen for specific event types
      const eventTypes = [
        "job_changed",
        "started",
        "completed",
        "failed",
        "killed",
        "output",
      ];
      eventTypes.forEach((eventType) => {
        es.addEventListener(eventType, (event) => {
          const sseEvent: SSEEvent = {
            type: eventType,
            data: (event as MessageEvent).data,
          };
          subscribersRef.current.forEach((cb) => {
            try {
              cb(sseEvent);
            } catch {
              // ignore subscriber errors
            }
          });
        });
      });

      es.onerror = () => {
        setConnected(false);
        es.close();
        reconnectTimer = setTimeout(connect, 3000);
      };
    }

    connect();

    return () => {
      clearTimeout(reconnectTimer);
      eventSourceRef.current?.close();
    };
  }, []);

  const subscribe = useCallback((callback: SSECallback) => {
    subscribersRef.current.add(callback);
    return () => {
      subscribersRef.current.delete(callback);
    };
  }, []);

  return React.createElement(
    SSEContext.Provider,
    { value: { subscribe, connected } },
    children
  );
}

export function useSSEEvents(callback: SSECallback) {
  const ctx = useContext(SSEContext);
  const callbackRef = useRef(callback);
  callbackRef.current = callback;

  useEffect(() => {
    if (!ctx) return;

    const wrappedCallback: SSECallback = (event) => {
      callbackRef.current(event);
    };

    return ctx.subscribe(wrappedCallback);
  }, [ctx]);
}

export function useSSEConnected(): boolean {
  const ctx = useContext(SSEContext);
  return ctx?.connected ?? false;
}
