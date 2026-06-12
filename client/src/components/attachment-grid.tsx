import { For, Show, createMemo, createSignal } from "solid-js";
import { resolveServerUrl, type MessageAttachment } from "../api";

function isPhotoAttachment(attachment: MessageAttachment): boolean {
  return (
    attachment.content_type.toLowerCase().startsWith("image/") ||
    attachment.thumbnail_content_type.toLowerCase().startsWith("image/")
  );
}

function positiveDimension(value: number): number {
  return Number.isFinite(value) && value > 0 ? value : 1;
}

function aspectRatio(attachment: MessageAttachment): string {
  const width = positiveDimension(attachment.thumbnail_width || attachment.width);
  const height = positiveDimension(attachment.thumbnail_height || attachment.height);
  return `${width} / ${height}`;
}

function gridClass(count: number): string {
  if (count === 1) return "max-w-md grid-cols-1";
  if (count === 2) return "max-w-xl grid-cols-2";
  return "max-w-2xl grid-cols-2 sm:grid-cols-3";
}

function attachmentAlt(
  authorName: string | null | undefined,
  index: number,
  total: number,
): string {
  const base = total === 1 ? "Photo attachment" : `Photo ${index + 1} of ${total}`;
  return authorName ? `${base} from ${authorName}` : base;
}

function attachmentLinkLabel(
  authorName: string | null | undefined,
  index: number,
  total: number,
): string {
  const base = total === 1 ? "photo attachment" : `photo ${index + 1} of ${total}`;
  return authorName ? `Open ${base} from ${authorName}` : `Open ${base}`;
}

function attachmentListLabel(count: number): string {
  return count === 1 ? "1 photo attachment" : `${count} photo attachments`;
}

export default function AttachmentGrid(props: {
  attachments: readonly MessageAttachment[];
  authorName?: string | null;
}) {
  const photoAttachments = createMemo(() => props.attachments.filter(isPhotoAttachment));
  const [brokenIds, setBrokenIds] = createSignal<ReadonlySet<number>>(new Set());
  const markBroken = (id: number) => setBrokenIds((current) => new Set(current).add(id));
  const isBroken = (id: number) => brokenIds().has(id);

  return (
    <Show when={photoAttachments().length > 0}>
      <div
        role="list"
        aria-label={attachmentListLabel(photoAttachments().length)}
        class={`mt-2 grid gap-2 ${gridClass(photoAttachments().length)}`}
      >
        <For each={photoAttachments()}>
          {(attachment, index) => {
            const total = () => photoAttachments().length;
            const alt = () => attachmentAlt(props.authorName, index(), total());
            const fullUrl = () => resolveServerUrl(attachment.url);
            const thumbnailUrl = () => resolveServerUrl(attachment.thumbnail_url);

            return (
              <div role="listitem" class="min-w-0">
                <a
                  href={fullUrl()}
                  target="_blank"
                  rel="noopener noreferrer"
                  aria-label={attachmentLinkLabel(props.authorName, index(), total())}
                  class="group block w-full overflow-hidden rounded-lg border border-gray-200 bg-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-400"
                  style={{ "aspect-ratio": aspectRatio(attachment) }}
                >
                  <Show
                    when={!isBroken(attachment.id)}
                    fallback={
                      <div
                        role="img"
                        aria-label={`${alt()} unavailable`}
                        class="flex h-full min-h-24 w-full items-center justify-center p-4 text-sm font-medium text-gray-500"
                      >
                        <span aria-hidden="true">Photo unavailable</span>
                      </div>
                    }
                  >
                    <img
                      src={thumbnailUrl()}
                      alt={alt()}
                      loading="lazy"
                      decoding="async"
                      width={positiveDimension(attachment.thumbnail_width)}
                      height={positiveDimension(attachment.thumbnail_height)}
                      class="h-full max-h-96 w-full object-contain transition-transform group-hover:scale-[1.01]"
                      onError={() => markBroken(attachment.id)}
                    />
                  </Show>
                </a>
              </div>
            );
          }}
        </For>
      </div>
    </Show>
  );
}
