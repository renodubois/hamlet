import { Component, Match, Show, Switch } from "solid-js";
import type { Embed } from "../api";
import { CloseIcon } from "./icons";

/// A single OpenGraph/oEmbed preview card for a URL in a message.
///
/// Three render modes, chosen by the server-side `embed_type` + whether
/// `iframe_url` was extracted from an oEmbed response:
///
///   - **iframe** (`video` / `rich` with an `iframe_url`): provider-native
///     player rendered in a sandboxed, lazy-loaded iframe sized by
///     `iframe_width`/`iframe_height` (aspect ratio; falls back to 16:9).
///     Title/site_name appear above as context; description and thumbnail
///     are dropped — redundant with the player.
///   - **photo** (`photo` type): the resolved image shown large with a
///     title caption, linking to the source.
///   - **link** (default): the Discord-style card — site_name, title link,
///     description (3-line clamp), thumbnail image.
///
/// `onRemove`, when provided, renders the X in the corner — the parent is
/// expected to wire it to the suppress-embeds endpoint. Pass `undefined` to
/// hide it (e.g. viewer isn't the message author).
const MessageEmbed: Component<{
  embed: Embed;
  onRemove?: () => void;
}> = (props) => {
  const hostname = () => {
    try {
      return new URL(props.embed.url).hostname;
    } catch {
      return props.embed.url;
    }
  };

  const aspectRatio = () => {
    const w = props.embed.iframe_width;
    const h = props.embed.iframe_height;
    if (w && h && w > 0 && h > 0) return `${w} / ${h}`;
    return "16 / 9";
  };

  const hasIframe = () =>
    (props.embed.embed_type === "video" || props.embed.embed_type === "rich") &&
    props.embed.iframe_url !== null;

  return (
    <div class="relative mt-1 max-w-xl rounded-md border-l-4 border-gray-300 bg-gray-50 p-3">
      <Show when={props.onRemove}>
        <button
          type="button"
          aria-label="Remove embed"
          title="Remove embed"
          class="absolute top-1 right-1 z-10 p-1 rounded-md bg-white/80 text-gray-500 hover:bg-gray-200 hover:text-gray-800"
          onClick={() => props.onRemove?.()}
        >
          <CloseIcon size={14} />
        </button>
      </Show>

      <div class="text-xs text-gray-500">{props.embed.site_name ?? hostname()}</div>
      <Show when={props.embed.title}>
        <a
          href={props.embed.url}
          target="_blank"
          rel="noopener noreferrer"
          class="block font-semibold text-blue-700 hover:underline"
        >
          {props.embed.title}
        </a>
      </Show>

      <Switch>
        {/* iframe branch */}
        <Match when={hasIframe() && props.embed.iframe_url}>
          {(iframeUrl) => (
            <div class="mt-2 w-full" style={{ "aspect-ratio": aspectRatio() }}>
              <iframe
                class="w-full h-full rounded-md border-0"
                src={iframeUrl()}
                title={props.embed.title ?? "Embedded content"}
                allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
                referrerpolicy="strict-origin-when-cross-origin"
                sandbox="allow-scripts allow-same-origin allow-popups allow-popups-to-escape-sandbox"
                loading="lazy"
                allowfullscreen
              />
            </div>
          )}
        </Match>

        {/* photo branch */}
        <Match when={props.embed.embed_type === "photo" && props.embed.image_url}>
          {(imageUrl) => (
            <a href={props.embed.url} target="_blank" rel="noopener noreferrer">
              <img
                src={imageUrl()}
                alt={props.embed.title ?? ""}
                loading="lazy"
                class="mt-2 max-h-96 w-full rounded-md object-contain"
              />
            </a>
          )}
        </Match>

        {/* link/card branch (default) */}
        <Match when={true}>
          <Show when={props.embed.description}>
            <p class="mt-1 text-sm text-gray-700 line-clamp-3">{props.embed.description}</p>
          </Show>
          <Show when={props.embed.image_url}>
            {(imageUrl) => (
              <img
                src={imageUrl()}
                alt=""
                loading="lazy"
                class="mt-2 max-h-64 rounded-md object-cover"
              />
            )}
          </Show>
        </Match>
      </Switch>
    </div>
  );
};

export default MessageEmbed;
