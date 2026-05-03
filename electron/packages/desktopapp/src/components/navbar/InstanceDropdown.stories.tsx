import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { InstanceDropdown } from "./InstanceDropdown";

const meta: Meta<typeof InstanceDropdown> = {
  title: "Components/Navbar/InstanceDropdown",
  component: InstanceDropdown,
  parameters: { layout: "padded" },
};
export default meta;

type Story = StoryObj<typeof InstanceDropdown>;

/**
 * Default render — the avatar trigger. Click to open the popover and
 * see the "Connect to instance" + "Recent instances" sections.
 */
export const Default: Story = {};

/**
 * The dropdown rendered pinned to the right of a faux navbar row, so
 * designers can sanity-check it sitting alongside other nav controls.
 */
export const InNavbarContext: Story = {
  render: () => (
    <div className="bg-surface flex items-center h-[var(--height-navbar)] px-8 border-b border-border-subtle">
      <span className="text-[22px] font-extrabold tracking-tight text-fg">ACS</span>
      <div className="ml-auto flex items-center gap-5">
        <span className="text-fg-muted text-sm">Build a Job</span>
        <InstanceDropdown />
      </div>
    </div>
  ),
  parameters: { layout: "fullscreen" },
};
