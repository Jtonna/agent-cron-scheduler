import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { useState } from "react";
import { TabBar, type FilterOptions } from "./TabBar";

const meta: Meta<typeof TabBar> = {
  title: "Components/UI/TabBar",
  component: TabBar,
  parameters: {
    layout: "fullscreen",
  },
  decorators: [
    (Story) => (
      <div className="px-8">
        <Story />
      </div>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof TabBar>;

export const Default: Story = {
  args: {
    label: "Recent",
    tabs: ["All runs", "Running", "Succeeded", "Failed"],
    activeTab: "All runs",
  },
};

export const WithActiveTab: Story = {
  args: {
    label: "Recent",
    tabs: ["All runs", "Running", "Succeeded", "Failed"],
    activeTab: "Running",
  },
};

export const NoLabel: Story = {
  args: {
    tabs: ["All runs", "Running", "Succeeded", "Failed"],
    activeTab: "All runs",
  },
};

export const NoFilter: Story = {
  args: {
    tabs: ["Overview", "Settings", "Logs"],
    activeTab: "Overview",
  },
};

/**
 * `/jobs`-style toolbar: an interactive sort menu on the left and a
 * search input pinned right.
 */
export const WithSortMenuAndSearch: Story = {
  render: () => {
    const [search, setSearch] = useState("");
    const [sortKey, setSortKey] = useState("name");
    return (
      <TabBar
        label={{
          value: sortKey,
          options: { name: "Name (A–Z)", recent: "Recent" },
          onChange: setSortKey,
        }}
        search={{
          value: search,
          onChange: setSearch,
          placeholder: "Search jobs...",
        }}
      />
    );
  },
};

/**
 * `/jobs/[id]`-style toolbar: sort menu on the left, advanced
 * slide-down filter panel on the right. The panel hides its own
 * Sort-by row because sort already lives on the left.
 */
export const WithSortMenuAndFilter: Story = {
  render: () => {
    const [filters, setFilters] = useState<FilterOptions>({});
    return (
      <TabBar
        label={{
          value: filters.sortBy ?? "recent",
          options: {
            recent: "Recent",
            oldest: "Oldest",
            longest: "Longest",
            shortest: "Shortest",
            "cost-high": "Cost (high to low)",
            "cost-low": "Cost (low to high)",
          },
          onChange: (key) =>
            setFilters({ ...filters, sortBy: key as FilterOptions["sortBy"] }),
        }}
        filter={{ filters, onFiltersChange: setFilters, hideSortBy: true }}
      />
    );
  },
};

/**
 * Both right-side slots present at once — search input next to the
 * advanced filter button.
 */
export const WithSearchAndFilter: Story = {
  render: () => {
    const [search, setSearch] = useState("");
    const [filters, setFilters] = useState<FilterOptions>({});
    return (
      <TabBar
        label={{
          value: filters.sortBy ?? "recent",
          options: {
            recent: "Recent",
            oldest: "Oldest",
            longest: "Longest",
          },
          onChange: (key) =>
            setFilters({ ...filters, sortBy: key as FilterOptions["sortBy"] }),
        }}
        search={{
          value: search,
          onChange: setSearch,
          placeholder: "Search...",
        }}
        filter={{ filters, onFiltersChange: setFilters, hideSortBy: true }}
      />
    );
  },
};
