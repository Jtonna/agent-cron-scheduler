"use client";

import { useState } from "react";
import { ChevronDown, SlidersHorizontal, X } from "lucide-react";

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

function FilterPanel({ filters, onChange, onClose }: { filters: FilterOptions; onChange: (f: FilterOptions) => void; onClose: () => void }) {
  return (
    <div className="border-b border-gray-100 bg-gray-50">
      {/* Header */}
      <div className="px-16 pt-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-gray-400 uppercase tracking-wider">Filters</span>
        <div className="flex items-center gap-2">
          <button
            onClick={() => onChange({})}
            className="text-xs text-gray-400 hover:text-gray-600 transition-colors"
          >
            Clear all
          </button>
          <button
            onClick={onClose}
            className="w-7 h-7 flex items-center justify-center rounded-lg text-gray-400 hover:text-gray-600 hover:bg-gray-200 transition-colors"
          >
            <X size={14} />
          </button>
        </div>
      </div>

      {/* Inputs */}
      <div className="px-16 py-4 flex items-end gap-6">
        {/* Cost */}
        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Cost ($)</span>
          <div className="flex items-center gap-2">
            <input
              type="text"
              placeholder="Min"
              value={filters.costMin ?? ""}
              onChange={(e) => onChange({ ...filters, costMin: e.target.value || undefined })}
              className="w-20 h-8 px-2.5 text-sm border border-gray-200 rounded-lg bg-white outline-none focus:border-pink-300 transition-colors"
            />
            <span className="text-gray-300 text-xs">–</span>
            <input
              type="text"
              placeholder="Max"
              value={filters.costMax ?? ""}
              onChange={(e) => onChange({ ...filters, costMax: e.target.value || undefined })}
              className="w-20 h-8 px-2.5 text-sm border border-gray-200 rounded-lg bg-white outline-none focus:border-pink-300 transition-colors"
            />
          </div>
        </div>

        {/* Duration */}
        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Duration (sec)</span>
          <div className="flex items-center gap-2">
            <input
              type="text"
              placeholder="Min"
              value={filters.durationMin ?? ""}
              onChange={(e) => onChange({ ...filters, durationMin: e.target.value || undefined })}
              className="w-20 h-8 px-2.5 text-sm border border-gray-200 rounded-lg bg-white outline-none focus:border-pink-300 transition-colors"
            />
            <span className="text-gray-300 text-xs">–</span>
            <input
              type="text"
              placeholder="Max"
              value={filters.durationMax ?? ""}
              onChange={(e) => onChange({ ...filters, durationMax: e.target.value || undefined })}
              className="w-20 h-8 px-2.5 text-sm border border-gray-200 rounded-lg bg-white outline-none focus:border-pink-300 transition-colors"
            />
          </div>
        </div>

        {/* Date range */}
        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Date range</span>
          <div className="flex items-center gap-2">
            <input
              type="date"
              value={filters.dateFrom ?? ""}
              onChange={(e) => onChange({ ...filters, dateFrom: e.target.value || undefined })}
              className="h-8 px-2.5 text-sm border border-gray-200 rounded-lg bg-white outline-none focus:border-pink-300 transition-colors"
            />
            <span className="text-gray-300 text-xs">–</span>
            <input
              type="date"
              value={filters.dateTo ?? ""}
              onChange={(e) => onChange({ ...filters, dateTo: e.target.value || undefined })}
              className="h-8 px-2.5 text-sm border border-gray-200 rounded-lg bg-white outline-none focus:border-pink-300 transition-colors"
            />
          </div>
        </div>

      </div>
    </div>
  );
}

export function TabBar({ label, tabs, activeTab, onTabClick, showFilter = true, filters = {}, onFiltersChange }: TabBarProps) {
  const [filterOpen, setFilterOpen] = useState(false);
  const hasActiveFilters = Object.values(filters).some(Boolean);

  return (
    <>
      <div className="border-t border-b border-gray-100">
        <div className="px-16 h-12 flex items-center gap-6 text-sm">
          {label && (
            <>
              <span className="text-gray-900 font-semibold cursor-pointer flex items-center gap-1">
                {label}
                <ChevronDown size={12} />
              </span>
              <div className="w-px h-5 bg-gray-200" />
            </>
          )}
          {tabs.map((tab) => (
            <span
              key={tab}
              onClick={() => onTabClick?.(tab)}
              className={`cursor-pointer transition-colors ${
                tab === activeTab ? "text-gray-900 font-semibold" : "text-gray-500 hover:text-gray-900"
              }`}
            >
              {tab}
            </span>
          ))}
          {showFilter && (
            <div className="ml-auto">
              <button
                onClick={() => setFilterOpen(!filterOpen)}
                className={`w-8 h-8 flex items-center justify-center rounded-lg border transition-colors ${
                  filterOpen || hasActiveFilters
                    ? "border-pink-300 text-pink-500 bg-pink-50"
                    : "border-gray-200 text-gray-400 hover:text-gray-600 hover:border-gray-300"
                }`}
              >
                <SlidersHorizontal size={14} />
              </button>
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
