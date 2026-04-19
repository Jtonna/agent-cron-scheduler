"use client";

import type { GlobalCostSummaryResponse, Job } from "@/lib/types";
import { ChatInput } from "./ChatInput";
import { QuickActions } from "./QuickActions";
import { WidgetCarousel } from "./WidgetCarousel";

interface HeroSectionProps {
  summary: GlobalCostSummaryResponse | null;
  jobs: Job[];
  scrollToLogs?: () => void;
}

export function HeroSection({ summary, jobs, scrollToLogs }: HeroSectionProps) {
  return (
    <div data-testid="hero-section" className="grid grid-cols-1 lg:grid-cols-[55fr_45fr] gap-6">
      {/* Left column */}
      <div className="space-y-4">
        <div>
          <h1 className="text-xl font-semibold tracking-tight text-foreground">Dashboard</h1>
          <p className="text-sm text-muted-foreground mt-1">Monitor your jobs and system health</p>
        </div>
        <ChatInput />
        <QuickActions scrollToLogs={scrollToLogs} />
      </div>

      {/* Right column */}
      <div>
        <WidgetCarousel summary={summary} jobs={jobs} />
      </div>
    </div>
  );
}
