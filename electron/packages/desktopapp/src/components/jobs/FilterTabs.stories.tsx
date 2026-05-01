import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { FilterTabs } from "./FilterTabs";

const meta: Meta<typeof FilterTabs> = {
  title: "Components/Jobs/FilterTabs",
  component: FilterTabs,
};
export default meta;

type Story = StoryObj<typeof FilterTabs>;

export const Default: Story = {};
