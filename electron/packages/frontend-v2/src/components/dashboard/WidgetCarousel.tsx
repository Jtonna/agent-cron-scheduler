"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import type { GlobalCostSummaryResponse, Job } from "@/lib/types";
import { DonutChart } from "./charts/DonutChart";
import { BarChart } from "./charts/BarChart";
import { NextRunTimer } from "./charts/NextRunTimer";

interface WidgetCarouselProps {
  summary: GlobalCostSummaryResponse | null;
  jobs: Job[];
}

const SLIDE_COUNT = 3;
const AUTO_ADVANCE_MS = 5000;

export function WidgetCarousel({ summary, jobs }: WidgetCarouselProps) {
  const [activeIndex, setActiveIndex] = useState(0);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const isPausedRef = useRef(false);

  const startTimer = useCallback(() => {
    // Clear any existing timer first
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
    }
    intervalRef.current = setInterval(() => {
      if (!isPausedRef.current) {
        setActiveIndex((prev) => (prev + 1) % SLIDE_COUNT);
      }
    }, AUTO_ADVANCE_MS);
  }, []);

  useEffect(() => {
    startTimer();
    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
    };
  }, [startTimer]);

  const handleMouseEnter = () => {
    isPausedRef.current = true;
  };

  const handleMouseLeave = () => {
    isPausedRef.current = false;
  };

  const handleDotClick = (index: number) => {
    setActiveIndex(index);
    // Reset timer on manual navigation
    startTimer();
  };

  // Loading state
  if (!summary) {
    return (
      <div
        className="rounded-xl bg-card shadow-[var(--shadow-card)] p-5 min-h-[280px] flex items-center justify-center"
        data-testid="widget-carousel-loading"
      >
        <div className="flex flex-col items-center gap-2 text-muted-foreground">
          <div className="h-6 w-6 animate-spin rounded-full border-2 border-current border-t-transparent" />
          <span className="text-sm">Loading widgets...</span>
        </div>
      </div>
    );
  }

  const slides = [
    <DonutChart key="donut" topJobs={summary.top_jobs} />,
    <BarChart key="bar" dailyTrend={summary.daily_trend} />,
    <NextRunTimer key="timer" jobs={jobs} />,
  ];

  return (
    <div
      className="rounded-xl bg-card shadow-[var(--shadow-card)] p-5 min-h-[280px] flex flex-col"
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      data-testid="widget-carousel"
    >
      {/* Slide content */}
      <div className="flex-1 flex items-center justify-center" data-testid="carousel-slide">
        {slides[activeIndex]}
      </div>

      {/* Dot indicators */}
      <div className="flex items-center justify-center gap-2 pt-3" data-testid="carousel-dots">
        {Array.from({ length: SLIDE_COUNT }).map((_, i) => (
          <button
            key={i}
            type="button"
            onClick={() => handleDotClick(i)}
            aria-label={`Go to slide ${i + 1}`}
            className={`h-2 rounded-full transition-all ${
              i === activeIndex
                ? "w-5 bg-primary"
                : "w-2 bg-muted-foreground/30 hover:bg-muted-foreground/50"
            }`}
          />
        ))}
      </div>
    </div>
  );
}
