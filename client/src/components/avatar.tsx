import { toSvg } from "jdenticon";
import { If } from "../hooks/react-state";
import { getServerUrl } from "../api";

function resolveUrl(url: string): string {
  if (url.startsWith("http://") || url.startsWith("https://")) return url;
  return `${getServerUrl()}${url}`;
}

export default function Avatar(props: {
  url: string | null | undefined;
  username: string;
  size: number;
  isSpeaking?: boolean;
}) {
  const label = () => `${props.username}'s avatar`;
  return (
    <span
      className={`inline-block overflow-hidden rounded-full bg-gray-700 flex-shrink-0 ${
        props.isSpeaking ? "ring-2 ring-green-500" : ""
      }`}
      style={{ width: `${props.size}px`, height: `${props.size}px` }}
      aria-label={label()}
      role="img"
    >
      <If
        when={props.url}
        fallback={
          <span
            aria-hidden="true"
            style={{ width: `${props.size}px`, height: `${props.size}px` }}
            dangerouslySetInnerHTML={{ __html: toSvg(props.username, props.size) }}
          />
        }
      >
        {(u) => (
          <img
            src={resolveUrl(u())}
            alt=""
            width={props.size}
            height={props.size}
            className="block w-full h-full object-cover"
          />
        )}
      </If>
    </span>
  );
}
