"use client";

import { ReactNode, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { RouterProvider } from "react-aria-components";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { SSEProvider } from "./sse";
import { SSEQueryBridge } from "./sseInvalidator";

declare module "react-aria-components" {
  interface RouterConfig {
    routerOptions: NonNullable<Parameters<ReturnType<typeof useRouter>["push"]>[1]>;
  }
}

export function Providers({ children }: { children: ReactNode }) {
  const router = useRouter();
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            staleTime: 30 * 1000, // 30s
            gcTime: 5 * 60 * 1000, // 5 min
            refetchOnWindowFocus: true,
            retry: 1,
          },
        },
      }),
  );

  // bfcache restores DOM + React state but not in-flight fetches; invalidate so
  // stuck spinners resolve when the user navigates back to a cached page.
  useEffect(() => {
    const handler = (event: PageTransitionEvent) => {
      if (event.persisted) {
        queryClient.invalidateQueries();
      }
    };
    window.addEventListener("pageshow", handler);
    return () => window.removeEventListener("pageshow", handler);
  }, [queryClient]);

  return (
    <RouterProvider navigate={router.push}>
      <QueryClientProvider client={queryClient}>
        <SSEProvider>
          <SSEQueryBridge />
          {children}
        </SSEProvider>
      </QueryClientProvider>
    </RouterProvider>
  );
}
