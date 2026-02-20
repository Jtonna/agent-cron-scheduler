"use client";

import React from "react";
import { Card, CardHeader, CardDescription, CardContent } from "@/components/ui/card";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

interface HealthCardProps {
  label: string;
  value: React.ReactNode;
  icon?: React.ReactNode;
  tooltip?: string;
}

export function HealthCard({ label, value, icon, tooltip }: HealthCardProps) {
  const renderedValue = (
    <span className="text-lg font-semibold truncate">{value}</span>
  );

  return (
    <Card className="transition-shadow hover:shadow-md">
      <CardHeader className="pb-2">
        <CardDescription className="text-xs font-medium uppercase tracking-wider">
          {label}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex items-center gap-3">
          {icon && (
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
              {icon}
            </div>
          )}
          <div className="min-w-0 flex-1">
            {tooltip ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  {renderedValue}
                </TooltipTrigger>
                <TooltipContent>
                  <p>{tooltip}</p>
                </TooltipContent>
              </Tooltip>
            ) : (
              renderedValue
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
