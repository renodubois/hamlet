import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";

import Modal from "./modal";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Textarea } from "./ui/textarea";

function DefaultBody() {
  return (
    <div className="space-y-3 text-sm text-foreground">
      <p>Use this layer for confirmations, settings, and other focused tasks.</p>
      <div className="flex justify-end gap-2 pt-2">
        <Button type="button" variant="secondary">
          Secondary
        </Button>
        <Button type="button">Primary action</Button>
      </div>
    </div>
  );
}

function ChannelForm() {
  return (
    <form className="space-y-4 text-sm text-foreground">
      <label className="block space-y-1">
        <span className="font-medium">Channel name</span>
        <Input autoFocus defaultValue="design-review" />
      </label>
      <label className="block space-y-1">
        <span className="font-medium">Description</span>
        <Textarea className="min-h-24" defaultValue="A focused place to iterate on UI details." />
      </label>
      <div className="flex justify-end gap-2">
        <Button type="button" variant="secondary">
          Cancel
        </Button>
        <Button type="submit">Create</Button>
      </div>
    </form>
  );
}

const meta = {
  title: "Components/Modal",
  component: Modal,
  args: {
    open: true,
    onClose: fn(),
    title: "Welcome to Hamlet",
    size: "sm",
    children: <DefaultBody />,
  },
  argTypes: {
    size: {
      control: "inline-radio",
      options: ["sm", "lg"],
    },
    children: {
      control: false,
    },
  },
} satisfies Meta<typeof Modal>;

export default meta;
type Story = StoryObj<typeof meta>;

export const SmallDialog: Story = {};

export const LargeDialog: Story = {
  args: {
    size: "lg",
    title: "Review channel guidelines",
  },
};

export const FormContent: Story = {
  args: {
    title: "Create channel",
    children: <ChannelForm />,
  },
};

export const Closed: Story = {
  args: {
    open: false,
  },
};
