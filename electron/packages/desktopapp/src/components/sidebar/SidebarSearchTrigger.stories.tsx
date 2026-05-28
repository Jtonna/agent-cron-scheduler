import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { SidebarSearchTrigger } from "./SidebarSearchTrigger";
import { CommandPaletteContext } from "@/components/command-palette/CommandPaletteContext";

const fakeCtx = {
  isOpen: false,
  open: () => {},
  close: () => {},
  toggle: () => {},
  registerCommands: () => "reg-0",
  unregisterCommands: () => {},
};

const meta: Meta<typeof SidebarSearchTrigger> = {
  title: "Components/Sidebar/SidebarSearchTrigger",
  component: SidebarSearchTrigger,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <CommandPaletteContext.Provider value={fakeCtx}>
        <div style={{ width: 260 }}>
          <Story />
        </div>
      </CommandPaletteContext.Provider>
    ),
  ],
};
export default meta;
type Story = StoryObj<typeof SidebarSearchTrigger>;

export const Default: Story = {};

export const CustomPlaceholder: Story = {
  args: { placeholder: "Find a workflow…" },
};
