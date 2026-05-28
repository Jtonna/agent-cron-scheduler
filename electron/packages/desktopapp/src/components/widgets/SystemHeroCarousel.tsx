"use client";

/**
 * SystemHeroCarousel
 *
 * Auto-rotating observability widget carousel that replaces the static
 * hero illustration on the dashboard `/` route. Cycles every 5 seconds
 * through system-level slides (total spend, yearly spend, daily cost
 * trend, token flow, and workflow inventory).
 *
 * Behavior:
 *   - Auto-advances every 5s.
 *   - Pauses on mouse hover (mouseenter/mouseleave) and on focus-within
 *     so keyboard users are not yanked mid-read.
 *   - Honors `prefers-reduced-motion`: disables both auto-advance and
 *     the slide transition.
 *   - Indicator dots are clickable for direct navigation. Arrow keys
 *     ←/→ also navigate when the carousel has focus.
 *   - Slides crossfade-translate at ~300ms ease-out (CSS transform).
 *   - Fixed `min-h-[420px]` to match the placeholder it replaces, so
 *     there is no layout shift when slides change.
 *
 * Pulls data from `useGlobalCostSummary` (system-scoped cost summary)
 * and `useJobs` (workflow inventory). Each slide is its own widget;
 * this component only orchestrates rotation and chrome.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { Pause } from "lucide-react";
import { useGlobalCostSummary } from "@/apis/useGlobalCostSummary";
import { useJobs } from "@/apis/useJobs";
import { HeroTotalCostSlide } from "@/components/widgets/HeroTotalCostSlide";
import { HeroYearCostSlide } from "@/components/widgets/HeroYearCostSlide";
import { HeroDailyCostSlide } from "@/components/widgets/HeroDailyCostSlide";
import { HeroTokensSlide } from "@/components/widgets/HeroTokensSlide";
import { HeroWorkflowInventorySlide } from "@/components/widgets/HeroWorkflowInventorySlide";

const ROTATE_MS = 5000;

interface SystemHeroCarouselProps {
  /** Optional fixed height. Defaults to 420px to match the previous placeholder. */
  minHeight?: number;
}

function usePrefersReducedMotion(): boolean {
  const [prefers, setPrefers] = useState(false);
  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mql = window.matchMedia("(prefers-reduced-motion: reduce)");
    setPrefers(mql.matches);
    const handler = (e: MediaQueryListEvent) => setPrefers(e.matches);
    mql.addEventListener("change", handler);
    return () => mql.removeEventListener("change", handler);
  }, []);
  return prefers;
}

export function SystemHeroCarousel({ minHeight = 420 }: SystemHeroCarouselProps = {}) {
  const { summary, loading: costLoading } = useGlobalCostSummary();
  const { jobs, loading: jobsLoading } = useJobs();
  const reducedMotion = usePrefersReducedMotion();

  const systemSummary = summary?.system_cost_summary ?? null;
  const dailyBuckets = summary?.system_cost_summary?.daily_buckets ?? [];

  // Slide list — order is the rotation order.
  const slides = [
    {
      key: "total-cost-30d",
      label: "Total spend (30d)",
      node: <HeroTotalCostSlide summary={systemSummary} loading={costLoading} />,
    },
    {
      key: "daily-cost-30d",
      label: "Daily cost trend",
      node: <HeroDailyCostSlide data={dailyBuckets} loading={costLoading} />,
    },
    {
      key: "tokens-30d",
      label: "Token flow (30d)",
      node: <HeroTokensSlide summary={systemSummary} loading={costLoading} />,
    },
    {
      key: "year-cost",
      label: "Total spend (1y)",
      node: <HeroYearCostSlide summary={systemSummary} loading={costLoading} />,
    },
    {
      key: "workflows",
      label: "Workflow inventory",
      node: <HeroWorkflowInventorySlide jobs={jobs} loading={jobsLoading} />,
    },
  ];
  const slideCount = slides.length;

  const [index, setIndex] = useState(0);
  const [paused, setPaused] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const goTo = useCallback(
    (next: number) => {
      setIndex(((next % slideCount) + slideCount) % slideCount);
    },
    [slideCount],
  );
  const goNext = useCallback(() => goTo(index + 1), [goTo, index]);
  const goPrev = useCallback(() => goTo(index - 1), [goTo, index]);

  // Auto-advance.
  useEffect(() => {
    if (paused || reducedMotion) return;
    const id = window.setInterval(() => {
      setIndex((i) => (i + 1) % slideCount);
    }, ROTATE_MS);
    return () => window.clearInterval(id);
  }, [paused, reducedMotion, slideCount]);

  // Keyboard navigation when the carousel has focus.
  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "ArrowRight") {
      e.preventDefault();
      goNext();
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      goPrev();
    }
  };

  return (
    <div
      ref={containerRef}
      role="region"
      aria-roledescription="carousel"
      aria-label="System observability"
      tabIndex={0}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={(e) => {
        // Only unpause if focus is leaving the carousel entirely.
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) {
          setPaused(false);
        }
      }}
      onKeyDown={onKeyDown}
      className="relative rounded-card overflow-hidden outline-none focus-visible:ring-2 focus-visible:ring-brand-ring"
      style={{ minHeight }}
    >
      {/* Slide track */}
      <div
        className="flex h-full w-full"
        style={{
          width: `${slideCount * 100}%`,
          transform: `translateX(-${(index * 100) / slideCount}%)`,
          transition: reducedMotion ? "none" : "transform 300ms ease-out",
          minHeight,
        }}
      >
        {slides.map((s) => (
          <div
            key={s.key}
            className="shrink-0"
            style={{ width: `${100 / slideCount}%`, minHeight }}
            aria-roledescription="slide"
            aria-label={s.label}
            aria-hidden={slides[index].key !== s.key}
          >
            {s.node}
          </div>
        ))}
      </div>

      {/* Paused indicator — subtle pill, top-right. */}
      {paused && !reducedMotion && (
        <div className="absolute top-3 right-3 flex items-center gap-1 px-2 py-0.5 rounded-pill bg-surface/80 border border-border text-fg-muted text-[10px] uppercase tracking-wider pointer-events-none">
          <Pause size={10} />
          <span>Paused</span>
        </div>
      )}

      {/* Indicator dots */}
      <div className="absolute bottom-4 left-1/2 -translate-x-1/2 flex items-center gap-2">
        {slides.map((s, i) => {
          const active = i === index;
          return (
            <button
              key={s.key}
              type="button"
              onClick={() => goTo(i)}
              aria-label={`Go to slide ${i + 1}: ${s.label}`}
              aria-current={active}
              className={`h-2 rounded-pill transition-all duration-200 ${
                active
                  ? "w-6 bg-fg-secondary"
                  : "w-2 bg-fg-faint hover:bg-fg-subtle"
              }`}
            />
          );
        })}
      </div>
    </div>
  );
}
