import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";

import Modal from "./modal";

function DefaultBody() {
  return (
    <div className="space-y-3 text-sm text-gray-200">
      <p>Use this layer for confirmations, settings, and other focused tasks.</p>
      <div className="flex justify-end gap-2 pt-2">
        <button type="button" className="rounded-md bg-gray-700 px-3 py-2 hover:bg-gray-600">
          Secondary
        </button>
        <button type="button" className="rounded-md bg-blue-600 px-3 py-2 hover:bg-blue-500">
          Primary action
        </button>
      </div>
    </div>
  );
}

function ChannelForm() {
  return (
    <form className="space-y-4 text-sm text-gray-200">
      <label className="block space-y-1">
        <span className="font-medium">Channel name</span>
        <input
          autoFocus
          className="w-full rounded-md border border-gray-600 bg-gray-900 px-3 py-2 text-gray-100"
          defaultValue="design-review"
        />
      </label>
      <label className="block space-y-1">
        <span className="font-medium">Description</span>
        <textarea
          className="min-h-24 w-full rounded-md border border-gray-600 bg-gray-900 px-3 py-2 text-gray-100"
          defaultValue="A focused place to iterate on UI details."
        />
      </label>
      <div className="flex justify-end gap-2">
        <button type="button" className="rounded-md bg-gray-700 px-3 py-2 hover:bg-gray-600">
          Cancel
        </button>
        <button type="submit" className="rounded-md bg-blue-600 px-3 py-2 hover:bg-blue-500">
          Create
        </button>
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
