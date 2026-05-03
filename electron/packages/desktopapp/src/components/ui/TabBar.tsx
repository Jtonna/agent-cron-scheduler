"use client";

import { useState } from "react";
import {
  Button,
  TextField,
  Input,
  MenuTrigger,
  Menu,
  MenuItem,
  Popover,
} from "react-aria-components";
import { ChevronDown, SlidersHorizontal, X, Search } from "lucide-react";

/**
 * TabBar
 *
 * Shared horizontal toolbar used at the top of list views. Slots are
 * positioned consistently across the app so users learn one mental model:
 *
 *  • Left — `label`: a static string OR a `SortMenu` (the "simple
 *    filter"). When given a `SortMenu`, the left dropdown is the page's
 *    primary sort surface.
 *  • Middle — optional clickable `tabs` (e.g. status filters on `/`).
 *  • Right — `search` (icon-prefixed text field) and/or `filter` (the
 *    advanced slide-down panel with cost/duration/date inputs). Both
 *    can be present at once; pages pick whichever combination they need.
 *
 * The component is layout-agnostic — it does NOT add page padding.
 * Consumers should wrap it in a padded container if needed.
 */

export type SortKey =
  | "recent"
  | "oldest"
  | "longest"
  | "shortest"
  | "cost-high"
  | "cost-low";

export interface FilterOptions {
  costMin?: string;
  costMax?: string;
  durationMin?: string;
  durationMax?: string;
  dateFrom?: string;
  dateTo?: string;
  sortBy?: SortKey;
}

export type SortMenu = {
  value: string;
  options: Record<string, string>;
  onChange: (key: string) => void;
};

export type SearchConfig = {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
};

export type FilterConfig = {
  filters: FilterOptions;
  onFiltersChange: (filters: FilterOptions) => void;
  /**
   * Hide the "Sort by" row inside the slide-down panel. Set this when
   * the page already exposes sort via the left `label` SortMenu so the
   * surface isn't duplicated.
   */
  hideSortBy?: boolean;
};

interface TabBarProps {
  /** Left side: static text, or a `SortMenu` for the simple sort dropdown. */
  label?: string | SortMenu;
  tabs?: string[];
  activeTab?: string;
  onTabClick?: (tab: string) => void;
  /** Right side: search input. Coexists with `filter` if both are passed. */
  search?: SearchConfig;
  /** Right side: advanced filter slide-down panel. Coexists with `search`. */
  filter?: FilterConfig;
}

const SORT_BY_LABELS: Record<SortKey, string> = {
  recent: "Recent",
  oldest: "Oldest",
  longest: "Longest",
  shortest: "Shortest",
  "cost-high": "Cost (high to low)",
  "cost-low": "Cost (low to high)",
};

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
  hideSortBy,
}: {
  filters: FilterOptions;
  onChange: (f: FilterOptions) => void;
  hideSortBy?: boolean;
}) {
  return (
    <div className="border-b border-border-subtle bg-surface-secondary px-4">
      {/* Header */}
      <div className="pt-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-fg-subtle uppercase tracking-wider">
          Filters
        </span>
        <Button
          onPress={() => onChange({})}
          className="h-[var(--height-input)] px-3 text-sm border border-border rounded-input bg-surface text-fg-secondary hover:text-fg hover:border-border-strong transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring cursor-pointer"
        >
          Clear all
        </Button>
      </div>

      {/* Inputs */}
      <div className="py-4 flex items-end gap-6 flex-wrap">
        {!hideSortBy && (
          <div className="flex flex-col gap-1.5">
            <span className="text-xs font-semibold text-fg-muted uppercase tracking-wider">
              Sort by
            </span>
            <select
              aria-label="Sort by"
              value={filters.sortBy ?? "recent"}
              onChange={(e) =>
                onChange({ ...filters, sortBy: e.target.value as SortKey })
              }
              className="h-[var(--height-input)] px-2.5 pr-8 text-sm border border-border rounded-input bg-surface outline-none focus:border-brand-ring transition-colors cursor-pointer"
            >
              {(Object.keys(SORT_BY_LABELS) as SortKey[]).map((key) => (
                <option key={key} value={key}>
                  {SORT_BY_LABELS[key]}
                </option>
              ))}
            </select>
          </div>
        )}

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

function SearchSlot({ value, onChange, placeholder }: SearchConfig) {
  return (
    <TextField aria-label={placeholder ?? "Search"} value={value} onChange={onChange}>
      <div className="relative">
        <Search
          size={14}
          className="absolute left-3 top-1/2 -translate-y-1/2 text-fg-subtle pointer-events-none"
        />
        <Input
          placeholder={placeholder ?? "Search..."}
          className="w-64 h-[var(--height-input)] pl-9 pr-3 text-sm border border-border rounded-input bg-surface outline-none focus:border-brand-ring transition-colors"
        />
      </div>
    </TextField>
  );
}

function FilterToggleButton({
  open,
  hasActiveFilters,
  onPress,
}: {
  open: boolean;
  hasActiveFilters: boolean;
  onPress: () => void;
}) {
  return (
    <Button
      onPress={onPress}
      className={`w-8 h-8 flex items-center justify-center rounded-input border transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring ${
        open || hasActiveFilters
          ? "border-brand-ring text-brand bg-brand-muted"
          : "border-border text-fg-subtle hover:text-fg-tertiary hover:border-border-strong"
      }`}
      aria-label="Toggle filters"
    >
      {open ? <X size={14} /> : <SlidersHorizontal size={14} />}
    </Button>
  );
}

function LeftLabel({ label }: { label: string | SortMenu }) {
  if (typeof label === "string") {
    return (
      <span className="text-fg font-semibold cursor-default flex items-center gap-1">
        {label}
        <ChevronDown size={12} />
      </span>
    );
  }

  const visible = label.options[label.value] ?? label.value;
  return (
    <MenuTrigger>
      <Button className="text-fg font-semibold cursor-pointer flex items-center gap-1 outline-none focus-visible:ring-2 focus-visible:ring-brand-ring rounded px-1">
        {visible}
        <ChevronDown size={12} />
      </Button>
      <Popover
        placement="bottom start"
        className="w-52 bg-surface border border-border rounded-menu shadow-menu py-1 z-50 outline-none entering:animate-in entering:fade-in entering:zoom-in-95 exiting:animate-out exiting:fade-out exiting:zoom-out-95"
      >
        <Menu className="outline-none">
          {Object.entries(label.options).map(([key, optLabel]) => (
            <MenuItem
              key={key}
              onAction={() => label.onChange(key)}
              className={`px-3 py-2 text-sm hover:bg-surface-secondary outline-none cursor-pointer ${
                key === label.value ? "text-fg font-semibold" : "text-fg-secondary"
              }`}
            >
              {optLabel}
            </MenuItem>
          ))}
        </Menu>
      </Popover>
    </MenuTrigger>
  );
}

export function TabBar({
  label,
  tabs,
  activeTab,
  onTabClick,
  search,
  filter,
}: TabBarProps) {
  const [filterOpen, setFilterOpen] = useState(false);

  const hasActiveFilters = filter
    ? Object.values(filter.filters).some(Boolean)
    : false;

  const hasRight = Boolean(search || filter);

  return (
    <>
      <div className="border-t border-b border-border-subtle">
        <div className="h-[var(--height-tab-bar)] flex items-center gap-6 text-sm">
          {label && (
            <>
              <LeftLabel label={label} />
              <div className="w-px h-5 bg-border" />
            </>
          )}
          {tabs?.map((tab) => (
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

          {hasRight && (
            <div className="ml-auto flex items-center gap-2">
              {search && <SearchSlot {...search} />}
              {filter && (
                <FilterToggleButton
                  open={filterOpen}
                  hasActiveFilters={hasActiveFilters}
                  onPress={() => setFilterOpen(!filterOpen)}
                />
              )}
            </div>
          )}
        </div>
      </div>
      {filter && filterOpen && (
        <FilterPanel
          filters={filter.filters}
          onChange={filter.onFiltersChange}
          hideSortBy={filter.hideSortBy}
        />
      )}
    </>
  );
}
