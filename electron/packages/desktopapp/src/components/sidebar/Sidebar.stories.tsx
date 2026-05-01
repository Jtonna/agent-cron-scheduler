import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { Plus } from "lucide-react";
import { Sidebar } from "./Sidebar";
import { SidebarSection } from "./SidebarSection";
import { Button } from "@/components/ui/Button";

const meta: Meta<typeof Sidebar> = {
  title: "Components/Sidebar/Sidebar",
  component: Sidebar,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <div
        style={{
          width: 280,
          borderRight: "1px solid var(--color-border-subtle)",
          height: "100vh",
        }}
      >
        <Story />
      </div>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof Sidebar>;

export const Default: Story = {
  render: () => (
    <Sidebar>
      <SidebarSection title="Quick actions">
        <Button href="/create" icon={<Plus size={14} />} fullWidth size="sm">
          New job
        </Button>
      </SidebarSection>
      <SidebarSection title="Notes">
        <div className="px-2 py-2 text-sm text-fg-secondary">Compose any rail content here.</div>
      </SidebarSection>
    </Sidebar>
  ),
};

export const WithSections: Story = {
  render: () => (
    <Sidebar>
      <SidebarSection title="Quick actions">
        <Button href="/create" icon={<Plus size={14} />} fullWidth size="sm">
          New job
        </Button>
      </SidebarSection>
      <SidebarSection title="Favorited" emptyText="No favorites yet" />
      <SidebarSection title="Recent">
        <div className="px-2 py-1.5 text-sm text-fg-secondary">backup-db</div>
        <div className="px-2 py-1.5 text-sm text-fg-secondary">sync-users</div>
        <div className="px-2 py-1.5 text-sm text-fg-secondary">health-check</div>
      </SidebarSection>
    </Sidebar>
  ),
};
