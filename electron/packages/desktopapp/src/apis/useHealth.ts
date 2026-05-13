"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/apis/client";

export function useHealth() {
  const query = useQuery({
    queryKey: ["health"],
    queryFn: () => api.health(),
  });

  return {
    health: query.data ?? null,
    loading: query.isPending,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
  };
}
