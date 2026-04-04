"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { TIMEFRAME_OPTIONS, CUSTOM_TIMEFRAME } from "./utils";

interface TimeframeSelectorProps {
  value: string;
  onTimeframeChange: (timeframe: string, startDate?: string, endDate?: string) => void;
}

export function TimeframeSelector({ value, onTimeframeChange }: TimeframeSelectorProps) {
  const [customStart, setCustomStart] = useState("");
  const [customEnd, setCustomEnd] = useState("");
  const isCustom = value === CUSTOM_TIMEFRAME;

  function handleApply() {
    if (!customStart || !customEnd) return;
    if (customStart > customEnd) return;
    onTimeframeChange(CUSTOM_TIMEFRAME, customStart, customEnd);
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap gap-1">
        {TIMEFRAME_OPTIONS.map((option) => (
          <Button
            key={option.value}
            variant={value === option.value ? "default" : "outline"}
            size="sm"
            onClick={() => onTimeframeChange(option.value)}
          >
            {option.label}
          </Button>
        ))}
        <Button
          variant={isCustom ? "default" : "outline"}
          size="sm"
          onClick={() => onTimeframeChange(CUSTOM_TIMEFRAME)}
        >
          Custom
        </Button>
      </div>

      {isCustom && (
        <div className="flex flex-wrap items-center gap-2">
          <label className="flex items-center gap-1 text-sm">
            <span>From:</span>
            <Input
              type="date"
              value={customStart}
              onChange={(e) => setCustomStart(e.target.value)}
              className="h-8 w-auto"
            />
          </label>
          <label className="flex items-center gap-1 text-sm">
            <span>To:</span>
            <Input
              type="date"
              value={customEnd}
              onChange={(e) => setCustomEnd(e.target.value)}
              className="h-8 w-auto"
            />
          </label>
          <Button
            size="sm"
            onClick={handleApply}
            disabled={!customStart || !customEnd || customStart > customEnd}
          >
            Apply
          </Button>
        </div>
      )}
    </div>
  );
}
