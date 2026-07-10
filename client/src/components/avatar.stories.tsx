import type { Meta, StoryObj } from "@storybook/react-vite";

import Avatar from "./avatar";

const meta = {
  title: "Components/Avatar",
  component: Avatar,
  args: {
    url: null,
    username: "baipas",
    size: 48,
    isSpeaking: false,
  },
  argTypes: {
    size: {
      control: { type: "range", min: 16, max: 128, step: 4 },
    },
  },
} satisfies Meta<typeof Avatar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Identicon: Story = {};

export const WithImage: Story = {
  args: {
    url: "https://i.pravatar.cc/128?img=5",
    username: "teo",
  },
};

export const Speaking: Story = {
  args: {
    isSpeaking: true,
    username: "alice",
  },
};

export const Sizes: Story = {
  render: () => (
    <div className="flex items-center gap-4">
      <Avatar url={null} username="tiny" size={24} />
      <Avatar url={null} username="default" size={40} />
      <Avatar url={null} username="large" size={64} />
      <Avatar url={null} username="speaking" size={80} isSpeaking />
    </div>
  ),
};
