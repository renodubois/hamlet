import { describe, expect, test, vi } from "vitest";
import { screen } from "@testing-library/react";
import { renderNative } from "../test/render";
import type { Embed } from "../api";
import { expectNoA11yViolations } from "../test/a11y";
import MessageEmbed from "./message-embed";

function makeEmbed(partial: Partial<Embed>): Embed {
  return {
    id: 1,
    message_id: 1,
    url: "https://example.com",
    title: null,
    description: null,
    image_url: null,
    site_name: null,
    embed_type: "link",
    iframe_url: null,
    iframe_width: null,
    iframe_height: null,
    ...partial,
  };
}

describe("<MessageEmbed>", () => {
  test("renders the Discord-style card for a plain link embed", async () => {
    const embed = makeEmbed({
      title: "Example domain",
      description: "A description.",
      site_name: "Example",
    });
    const { container } = renderNative(<MessageEmbed embed={embed} />);
    expect(screen.getByRole("link", { name: /example domain/i })).toHaveAttribute(
      "href",
      "https://example.com",
    );
    expect(screen.getByText("A description.")).toBeInTheDocument();
    expect(screen.getByText("Example")).toBeInTheDocument();
    expect(container.querySelector("iframe")).toBeNull();
    await expectNoA11yViolations(container, "link card");
  });

  test("renders an iframe for a video embed with iframe_url", async () => {
    const embed = makeEmbed({
      title: "A video",
      site_name: "YouTube",
      embed_type: "video",
      iframe_url: "https://www.youtube.com/embed/abc",
      iframe_width: 560,
      iframe_height: 315,
    });
    const { container } = renderNative(<MessageEmbed embed={embed} />);
    const iframe = container.querySelector("iframe");
    expect(iframe).not.toBeNull();
    expect(iframe?.getAttribute("src")).toBe("https://www.youtube.com/embed/abc");
    expect(iframe?.getAttribute("sandbox")).toContain("allow-scripts");
    expect(iframe?.getAttribute("loading")).toBe("lazy");
    expect(iframe?.getAttribute("referrerPolicy")).toBe("strict-origin-when-cross-origin");
    // Iframe exposes a title for screen readers (axe requires this).
    expect(iframe?.getAttribute("title")).toBeTruthy();
    // Header title is still present for context.
    expect(screen.getByRole("link", { name: /a video/i })).toBeInTheDocument();
    // `expectNoA11yViolations` not run here: axe tries to postMessage into
    // iframes for in-iframe rule checks and happy-dom can't satisfy that.
    // The outer-DOM a11y contract we care about (iframe has a `title`,
    // sandbox/referrerPolicy set) is asserted directly above.
    void container;
  });

  test("renders an iframe for a rich embed (Bluesky-style) too", () => {
    const embed = makeEmbed({
      title: "A post",
      site_name: "Bluesky",
      embed_type: "rich",
      iframe_url: "https://embed.bsky.app/embed/alice/app.bsky.feed.post/abc",
      iframe_width: 600,
      iframe_height: 400,
    });
    const { container } = renderNative(<MessageEmbed embed={embed} />);
    const iframe = container.querySelector("iframe");
    expect(iframe?.getAttribute("src")).toBe(
      "https://embed.bsky.app/embed/alice/app.bsky.feed.post/abc",
    );
  });

  test("prefers iframe media when a rich embed also has a photo", () => {
    const embed = makeEmbed({
      title: "Mixed media",
      embed_type: "rich",
      iframe_url: "https://player.example.com/embed/abc",
      image_url: "https://example.com/fallback.jpg",
    });
    const { container } = renderNative(<MessageEmbed embed={embed} />);

    expect(container.querySelector("iframe")).toHaveAttribute(
      "src",
      "https://player.example.com/embed/abc",
    );
    expect(container.querySelector("img")).toBeNull();
  });

  test("video embed with null iframe_url falls back to the link card", () => {
    // Server should degrade embed_type to 'link' when it can't extract an
    // iframe, but guard against the defensive case anyway.
    const embed = makeEmbed({
      title: "Broken",
      description: "No iframe available",
      embed_type: "video",
      iframe_url: null,
    });
    const { container } = renderNative(<MessageEmbed embed={embed} />);
    expect(container.querySelector("iframe")).toBeNull();
    expect(screen.getByText("No iframe available")).toBeInTheDocument();
  });

  test("renders a large image for a photo embed", () => {
    const embed = makeEmbed({
      title: "A photo",
      embed_type: "photo",
      image_url: "https://example.com/img.jpg",
    });
    const { container } = renderNative(<MessageEmbed embed={embed} />);
    const img = container.querySelector("img");
    expect(img?.getAttribute("src")).toBe("https://example.com/img.jpg");
    // Photo mode doesn't render a description block.
    expect(container.querySelector("p")).toBeNull();
  });

  test("shows the remove button only when onRemove is provided", () => {
    const embed = makeEmbed({ title: "T" });
    const { unmount } = renderNative(<MessageEmbed embed={embed} />);
    expect(screen.queryByRole("button", { name: /remove embed/i })).toBeNull();
    unmount();

    const onRemove = vi.fn();
    renderNative(<MessageEmbed embed={embed} onRemove={onRemove} />);
    const btn = screen.getByRole("button", { name: /remove embed/i });
    expect(btn).toBeInTheDocument();
    btn.click();
    expect(onRemove).toHaveBeenCalledTimes(1);
  });
});
