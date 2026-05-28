import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { FavoriteToggle } from "./FavoriteToggle";

const meta: Meta<typeof FavoriteToggle> = {
  title: "Components/Jobs/FavoriteToggle",
  component: FavoriteToggle,
};
export default meta;

type Story = StoryObj<typeof FavoriteToggle>;

export const NotFavorited: Story = {
  args: {
    jobId: "00000000-0000-0000-0000-000000000001",
    favorited: false,
  },
};

export const Favorited: Story = {
  args: {
    jobId: "00000000-0000-0000-0000-000000000002",
    favorited: true,
  },
};

export const Large: Story = {
  args: {
    jobId: "00000000-0000-0000-0000-000000000003",
    favorited: true,
    size: 24,
  },
};
