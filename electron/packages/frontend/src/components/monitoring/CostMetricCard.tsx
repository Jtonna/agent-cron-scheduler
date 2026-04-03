"use client";

import React from "react";
import { HealthCard } from "@/components/dashboard/HealthCard";
import { formatCost, formatTokenCount } from "@/lib/format";

interface CostMetricCardProps {
  label: string;
  amountUsd: number | null;
  tokens?: { input: number; output: number };
  icon: React.ComponentType<{ className?: string }>;
}

export function CostMetricCard({
  label,
  amountUsd,
  tokens,
  icon: Icon,
}: CostMetricCardProps) {
  const formattedCost = formatCost(amountUsd);

  // Render token breakdown as subtitle if provided
  const subtitle =
    tokens && amountUsd !== null
      ? `${formatTokenCount(tokens.input)} in / ${formatTokenCount(tokens.output)} out`
      : undefined;

  // Create the value content: formatted cost with optional tokens
  const value = subtitle ? (
    <div className="flex flex-col">
      <span>{formattedCost}</span>
      <span className="text-xs text-muted-foreground font-normal">
        {subtitle}
      </span>
    </div>
  ) : (
    formattedCost
  );

  return (
    <HealthCard
      label={label}
      value={value}
      icon={<Icon className="h-4 w-4" />}
    />
  );
}
