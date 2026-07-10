import { Component, Case, If, Choose } from "../hooks/react-state";
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
    <div className="relative mt-1 max-w-xl rounded-md border-l-4 border-border bg-muted p-3">
      <If when={props.onRemove}>
        <button
          type="button"
          aria-label="Remove embed"
          title="Remove embed"
          className="absolute top-1 right-1 z-10 p-1 rounded-md bg-background/80 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onClick={() => props.onRemove?.()}
        >
          <CloseIcon size={14} />
        </button>
      </If>

      <div className="text-xs text-muted-foreground">{props.embed.site_name ?? hostname()}</div>
      <If when={props.embed.title}>
        <a
          href={props.embed.url}
          target="_blank"
          rel="noopener noreferrer"
          className="block font-semibold text-primary hover:underline"
        >
          {props.embed.title}
        </a>
      </If>

      <Choose>
        {/* iframe branch */}
        <Case when={hasIframe() && props.embed.iframe_url}>
          {(iframeUrl) => (
            <div className="mt-2 w-full" style={{ aspectRatio: aspectRatio() }}>
              <iframe
                className="w-full h-full rounded-md border-0"
                src={iframeUrl()}
                title={props.embed.title ?? "Embedded content"}
                allow="accelerometer; autoPlay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
                referrerPolicy="strict-origin-when-cross-origin"
                sandbox="allow-scripts allow-same-origin allow-popups allow-popups-to-escape-sandbox"
                loading="lazy"
                allowFullScreen
              />
            </div>
          )}
        </Case>

        {/* photo branch */}
        <Case when={props.embed.embed_type === "photo" && props.embed.image_url}>
          {(imageUrl) => (
            <a href={props.embed.url} target="_blank" rel="noopener noreferrer">
              <img
                src={imageUrl()}
                alt={props.embed.title ?? ""}
                loading="lazy"
                className="mt-2 max-h-96 w-full rounded-md object-contain"
              />
            </a>
          )}
        </Case>

        {/* link/card branch (default) */}
        <Case when={true}>
          <If when={props.embed.description}>
            <p className="mt-1 text-sm text-muted-foreground line-clamp-3">
              {props.embed.description}
            </p>
          </If>
          <If when={props.embed.image_url}>
            {(imageUrl) => (
              <img
                src={imageUrl()}
                alt=""
                loading="lazy"
                className="mt-2 max-h-64 rounded-md object-cover"
              />
            )}
          </If>
        </Case>
      </Choose>
    </div>
  );
};

export default MessageEmbed;
