import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";

import type { Embed } from "../api";
import MessageEmbed from "./message-embed";

const sampleImage = `data:image/svg+xml,${encodeURIComponent(`
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 360">
  <defs>
    <linearGradient id="g" x1="0" x2="1" y1="0" y2="1">
      <stop offset="0%" stop-color="#60a5fa" />
      <stop offset="100%" stop-color="#a78bfa" />
    </linearGradient>
  </defs>
  <rect width="640" height="360" rx="24" fill="url(#g)" />
  <circle cx="502" cy="96" r="54" fill="#ffffff" fill-opacity="0.24" />
  <path d="M96 260 L222 138 L326 224 L394 168 L544 260 Z" fill="#ffffff" fill-opacity="0.72" />
</svg>
`)}`;

function makeEmbed(overrides: Partial<Embed> = {}): Embed {
  return {
    id: 1,
    message_id: 10,
    url: "https://example.com/hamlet-design-notes",
    title: "Hamlet design notes",
    description:
      "A compact link card showing site metadata, titles, descriptions, and optional thumbnails.",
    image_url: null,
    site_name: "Example Gazette",
    embed_type: "link",
    iframe_url: null,
    iframe_width: null,
    iframe_height: null,
    ...overrides,
  };
}

const meta = {
  title: "Components/MessageEmbed",
  component: MessageEmbed,
  args: {
    embed: makeEmbed({ image_url: sampleImage }),
  },
} satisfies Meta<typeof MessageEmbed>;

export default meta;
type Story = StoryObj<typeof meta>;

export const LinkCard: Story = {};

export const PhotoEmbed: Story = {
  args: {
    embed: makeEmbed({
      embed_type: "photo",
      title: "Gradient landscape",
      description: "Photo embeds prioritize the image and skip the description block.",
      image_url: sampleImage,
    }),
  },
};

export const VideoEmbed: Story = {
  args: {
    embed: makeEmbed({
      url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
      title: "A video embed",
      description: null,
      image_url: null,
      site_name: "YouTube",
      embed_type: "video",
      iframe_url: "https://www.youtube.com/embed/dQw4w9WgXcQ",
      iframe_width: 560,
      iframe_height: 315,
    }),
  },
};

export const Removable: Story = {
  args: {
    onRemove: fn(),
  },
};

export const MalformedUrlFallback: Story = {
  args: {
    embed: makeEmbed({
      url: "not a valid url",
      title: null,
      description: "When URL parsing fails, the raw URL is used as the source label.",
      site_name: null,
      image_url: null,
    }),
  },
};
