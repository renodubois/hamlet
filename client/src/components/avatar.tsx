import { toSvg } from "jdenticon";
import { Show } from "solid-js";
import { getServerUrl } from "../api";

function resolveUrl(url: string): string {
  if (url.startsWith("http://") || url.startsWith("https://")) return url;
  return `${getServerUrl()}${url}`;
}

export default function Avatar(props: {
  url: string | null | undefined;
  username: string;
  size: number;
}) {
  const label = () => `${props.username}'s avatar`;
  return (
    <span
      class="inline-block overflow-hidden rounded-full bg-gray-700 flex-shrink-0"
      style={{ width: `${props.size}px`, height: `${props.size}px` }}
      aria-label={label()}
      role="img"
    >
      <Show
        when={props.url}
        fallback={
          <span
            aria-hidden="true"
            style={{ width: `${props.size}px`, height: `${props.size}px` }}
            innerHTML={toSvg(props.username, props.size)}
          />
        }
      >
        {(u) => (
          <img
            src={resolveUrl(u())}
            alt=""
            width={props.size}
            height={props.size}
            class="block w-full h-full object-cover"
          />
        )}
      </Show>
    </span>
  );
}
