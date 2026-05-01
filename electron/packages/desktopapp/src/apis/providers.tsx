"use client";

import { ReactNode, useState } from "react";
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
