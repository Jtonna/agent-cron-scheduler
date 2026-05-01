"use client";

import { useState } from "react";
import { Button, TextField, Input } from "react-aria-components";
import { ChevronDown, SlidersHorizontal, X } from "lucide-react";

/**
 * TabBar
 *
 * Horizontal tab strip with an optional left-side label dropdown affordance
 * and a right-side filter toggle. Used at the top of the dashboard's
 * recent-runs section. The filter panel slides in below when the filter
 * button is toggled.
 */

export interface FilterOptions {
  costMin?: string;
  costMax?: string;
  durationMin?: string;
  durationMax?: string;
  dateFrom?: string;
  dateTo?: string;
}

interface TabBarProps {
  label?: string;
  tabs: string[];
  activeTab?: string;
  onTabClick?: (tab: string) => void;
  showFilter?: boolean;
  filters?: FilterOptions;
  onFiltersChange?: (filters: FilterOptions) => void;
}

function FilterInput({
  label,
  placeholder,
  value,
  onChange,
}: {
  label: string;
  placeholder: string;
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <TextField aria-label={label} value={value} onChange={onChange}>
      <Input
        placeholder={placeholder}
        className="w-20 h-[var(--height-input)] px-2.5 text-sm border border-border rounded-input bg-surface outline-none focus:border-brand-ring transition-colors"
      />
    </TextField>
  );
}

function FilterPanel({
  filters,
  onChange,
  onClose,
}: {
  filters: FilterOptions;
  onChange: (f: FilterOptions) => void;
  onClose: () => void;
}) {
  return (
    <div className="border-b border-border-subtle bg-surface-secondary">
      {/* Header */}
      <div className="px-16 pt-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-fg-subtle uppercase tracking-wider">
          Filters
        </span>
        <div className="flex items-center gap-2">
          <Button
            onPress={() => onChange({})}
            className="text-xs text-fg-subtle hover:text-fg-tertiary transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring rounded"
          >
            Clear all
          </Button>
          <Button
            onPress={onClose}
            className="w-7 h-7 flex items-center justify-center rounded-input text-fg-subtle hover:text-fg-tertiary hover:bg-surface-tertiary transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring"
            aria-label="Close filters"
          >
            <X size={14} />
          </Button>
        </div>
      </div>

      {/* Inputs */}
      <div className="px-16 py-4 flex items-end gap-6">
        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-semibold text-fg-muted uppercase tracking-wider">
            Cost ($)
          </span>
          <div className="flex items-center gap-2">
            <FilterInput
              label="Minimum cost"
              placeholder="Min"
              value={filters.costMin ?? ""}
              onChange={(v) => onChange({ ...filters, costMin: v || undefined })}
            />
            <span className="text-fg-faint text-xs">–</span>
            <FilterInput
              label="Maximum cost"
              placeholder="Max"
              value={filters.costMax ?? ""}
              onChange={(v) => onChange({ ...filters, costMax: v || undefined })}
            />
          </div>
        </div>

        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-semibold text-fg-muted uppercase tracking-wider">
            Duration (sec)
          </span>
          <div className="flex items-center gap-2">
            <FilterInput
              label="Minimum duration"
              placeholder="Min"
              value={filters.durationMin ?? ""}
              onChange={(v) => onChange({ ...filters, durationMin: v || undefined })}
            />
            <span className="text-fg-faint text-xs">–</span>
            <FilterInput
              label="Maximum duration"
              placeholder="Max"
              value={filters.durationMax ?? ""}
              onChange={(v) => onChange({ ...filters, durationMax: v || undefined })}
            />
          </div>
        </div>

        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-semibold text-fg-muted uppercase tracking-wider">
            Date range
          </span>
          <div className="flex items-center gap-2">
            <TextField
              aria-label="Date from"
              value={filters.dateFrom ?? ""}
              onChange={(v) => onChange({ ...filters, dateFrom: v || undefined })}
            >
              <Input
                type="date"
                className="h-[var(--height-input)] px-2.5 text-sm border border-border rounded-input bg-surface outline-none focus:border-brand-ring transition-colors"
              />
            </TextField>
            <span className="text-fg-faint text-xs">–</span>
            <TextField
              aria-label="Date to"
              value={filters.dateTo ?? ""}
              onChange={(v) => onChange({ ...filters, dateTo: v || undefined })}
            >
              <Input
                type="date"
                className="h-[var(--height-input)] px-2.5 text-sm border border-border rounded-input bg-surface outline-none focus:border-brand-ring transition-colors"
              />
            </TextField>
          </div>
        </div>
      </div>
    </div>
  );
}

export function TabBar({
  label,
  tabs,
  activeTab,
  onTabClick,
  showFilter = true,
  filters = {},
  onFiltersChange,
}: TabBarProps) {
  const [filterOpen, setFilterOpen] = useState(false);
  const hasActiveFilters = Object.values(filters).some(Boolean);

  return (
    <>
      <div className="border-t border-b border-border-subtle">
        <div className="px-16 h-[var(--height-tab-bar)] flex items-center gap-6 text-sm">
          {label && (
            <>
              <span className="text-fg font-semibold cursor-pointer flex items-center gap-1">
                {label}
                <ChevronDown size={12} />
              </span>
              <div className="w-px h-5 bg-border" />
            </>
          )}
          {tabs.map((tab) => (
            <Button
              key={tab}
              onPress={() => onTabClick?.(tab)}
              className={`cursor-pointer transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring rounded px-1 ${
                tab === activeTab ? "text-fg font-semibold" : "text-fg-muted hover:text-fg"
              }`}
            >
              {tab}
            </Button>
          ))}
          {showFilter && (
            <div className="ml-auto">
              <Button
                onPress={() => setFilterOpen(!filterOpen)}
                className={`w-8 h-8 flex items-center justify-center rounded-input border transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring ${
                  filterOpen || hasActiveFilters
                    ? "border-brand-ring text-brand bg-brand-muted"
                    : "border-border text-fg-subtle hover:text-fg-tertiary hover:border-border-strong"
                }`}
                aria-label="Toggle filters"
              >
                <SlidersHorizontal size={14} />
              </Button>
            </div>
          )}
        </div>
      </div>
      {filterOpen && (
        <FilterPanel
          filters={filters}
          onChange={(f) => onFiltersChange?.(f)}
          onClose={() => setFilterOpen(false)}
        />
      )}
    </>
  );
}
