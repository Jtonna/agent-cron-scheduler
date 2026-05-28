import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { useState } from "react";
import { LayoutDashboard, List, DatabaseBackup, ScrollText } from "lucide-react";
import { Pill } from "./Pill";

const meta: Meta<typeof Pill> = {
  title: "Components/UI/Pill",
  component: Pill,
};
export default meta;

type Story = StoryObj<typeof Pill>;

export const Default: Story = {
  args: {
    label: "backup-db",
    href: "/workflows/backup-db",
    size: "sm",
  },
};

export const WithIcon: Story = {
  args: {
    label: "Dashboard",
    icon: <LayoutDashboard size={14} strokeWidth={2.5} />,
    href: "/",
    size: "md",
    bordered: true,
  },
};

export const Active: Story = {
  args: {
    label: "Dashboard",
    icon: <LayoutDashboard size={14} strokeWidth={2.5} />,
    href: "/",
    size: "md",
    bordered: true,
    active: true,
  },
};

export const Bordered: Story = {
  args: {
    label: "All Workflows",
    icon: <List size={14} />,
    href: "/workflows",
    size: "md",
    bordered: true,
  },
};

export const ToggleForm: StoryObj = {
  render: () => {
    function ToggleDemo() {
      const [selected, setSelected] = useState(false);
      return (
        <Pill
          label={selected ? "Selected" : "Click to select"}
          active={selected}
          onChange={setSelected}
        />
      );
    }
    return <ToggleDemo />;
  },
};

export const Group: StoryObj = {
  render: () => (
    <div className="flex items-center gap-1.5">
      <Pill href="/workflows/backup-db" label="backup-db" />
      <Pill href="/workflows/sync-users" label="sync-users" />
      <Pill href="/workflows/health-check" label="health-check" />
      <Pill href="/workflows/deploy-staging" label="deploy-staging" />
      <Pill href="/workflows/cleanup-logs" label="cleanup-logs" />
    </div>
  ),
};

export const FilterTabsGroup: StoryObj = {
  render: () => (
    <div className="flex items-center gap-1.5">
      <Pill
        href="/"
        label="Dashboard"
        icon={<LayoutDashboard size={14} strokeWidth={2.5} />}
        size="md"
        bordered
        active
      />
      <Pill href="/workflows" label="All Workflows" icon={<List size={14} />} size="md" bordered />
      <Pill
        href="/backups"
        label="Backups"
        icon={<DatabaseBackup size={14} />}
        size="md"
        bordered
      />
      <Pill
        href="/systemlogs"
        label="System Logs"
        icon={<ScrollText size={14} />}
        size="md"
        bordered
      />
    </div>
  ),
};
