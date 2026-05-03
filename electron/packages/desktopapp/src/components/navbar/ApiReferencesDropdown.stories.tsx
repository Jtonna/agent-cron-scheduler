import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { ApiReferencesDropdown } from "./ApiReferencesDropdown";

const meta: Meta<typeof ApiReferencesDropdown> = {
  title: "Components/Navbar/ApiReferencesDropdown",
  component: ApiReferencesDropdown,
  parameters: { layout: "padded" },
};
export default meta;

type Story = StoryObj<typeof ApiReferencesDropdown>;

/**
 * Default render — the trigger button. Click it to open the popover and
 * see the menu items (All docs / CLI / REST / MCP).
 */
export const Default: Story = {};

/**
 * The dropdown rendered in the same horizontal nav row it lives in
 * inside the real Navbar, so designers can sanity-check spacing,
 * weight, and color against neighboring links.
 */
export const InNavbarContext: Story = {
  render: () => (
    <div className="bg-surface flex items-center gap-7 text-[14px] font-semibold text-fg-secondary px-8 py-4 border-b border-border-subtle">
      <span className="cursor-pointer hover:text-fg">System Logs</span>
      <ApiReferencesDropdown />
      <span className="cursor-pointer hover:text-fg">GitHub</span>
      <span className="cursor-pointer hover:text-fg">Community</span>
    </div>
  ),
  parameters: { layout: "fullscreen" },
};
