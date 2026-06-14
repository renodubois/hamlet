import { For, Show, createMemo, createSignal } from "solid-js";
import { resolveServerUrl, type MessageAttachment } from "../api";
import Modal from "./modal";

const MAX_ATTACHMENT_PREVIEW_WIDTH = 448;
const MAX_ATTACHMENT_PREVIEW_HEIGHT = 384;

interface AttachmentDimensions {
  width: number;
  height: number;
}

interface SelectedAttachmentPreview {
  attachment: MessageAttachment;
  alt: string;
}

function isPhotoAttachment(attachment: MessageAttachment): boolean {
  return (
    attachment.content_type.toLowerCase().startsWith("image/") ||
    attachment.thumbnail_content_type.toLowerCase().startsWith("image/")
  );
}

function positiveDimension(value: number): number {
  return Number.isFinite(value) && value > 0 ? value : 1;
}

function attachmentDimensions(attachment: MessageAttachment): AttachmentDimensions {
  return {
    width: positiveDimension(attachment.thumbnail_width || attachment.width),
    height: positiveDimension(attachment.thumbnail_height || attachment.height),
  };
}

function aspectRatio(attachment: MessageAttachment): string {
  const { width, height } = attachmentDimensions(attachment);
  return `${width} / ${height}`;
}

function previewStyle(attachment: MessageAttachment): Record<string, string> {
  const { width, height } = attachmentDimensions(attachment);
  const widthConstrainedByHeight = (width / height) * MAX_ATTACHMENT_PREVIEW_HEIGHT;
  const previewWidth = Math.max(
    1,
    Math.round(Math.min(MAX_ATTACHMENT_PREVIEW_WIDTH, widthConstrainedByHeight)),
  );

  return {
    "aspect-ratio": aspectRatio(attachment),
    width: `${previewWidth}px`,
    "max-width": "100%",
  };
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

function attachmentOpenLabel(
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
  const [selectedAttachment, setSelectedAttachment] =
    createSignal<SelectedAttachmentPreview | null>(null);
  const [fullImageBroken, setFullImageBroken] = createSignal(false);
  const markBroken = (id: number) => setBrokenIds((current) => new Set(current).add(id));
  const isBroken = (id: number) => brokenIds().has(id);
  const closeAttachment = () => setSelectedAttachment(null);
  const openAttachment = (attachment: MessageAttachment, index: number, total: number) => {
    setFullImageBroken(false);
    setSelectedAttachment({
      attachment,
      alt: attachmentAlt(props.authorName, index, total),
    });
  };

  return (
    <>
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
              const thumbnailUrl = () => resolveServerUrl(attachment.thumbnail_url);

              return (
                <div role="listitem" class="min-w-0">
                  <button
                    type="button"
                    aria-label={attachmentOpenLabel(props.authorName, index(), total())}
                    class="group block max-w-full overflow-hidden rounded-lg border border-gray-200 bg-gray-100 p-0 text-left focus:outline-none focus:ring-2 focus:ring-blue-400"
                    style={previewStyle(attachment)}
                    onClick={() => openAttachment(attachment, index(), total())}
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
                        class="block h-full w-full object-contain transition-transform group-hover:scale-[1.01]"
                        onError={() => markBroken(attachment.id)}
                      />
                    </Show>
                  </button>
                </div>
              );
            }}
          </For>
        </div>
      </Show>
      <Show when={selectedAttachment()}>
        {(selected) => {
          const fullUrl = () => resolveServerUrl(selected().attachment.url);

          return (
            <Modal open={true} onClose={closeAttachment} title={selected().alt} size="lg">
              <Show
                when={!fullImageBroken()}
                fallback={
                  <div
                    role="img"
                    aria-label={`${selected().alt} unavailable`}
                    class="flex min-h-64 items-center justify-center rounded-lg bg-gray-900 p-6 text-sm font-medium text-gray-300"
                  >
                    Photo unavailable
                  </div>
                }
              >
                <img
                  src={fullUrl()}
                  alt={selected().alt}
                  class="mx-auto block max-h-[75vh] max-w-full rounded object-contain"
                  onError={() => setFullImageBroken(true)}
                />
              </Show>
            </Modal>
          );
        }}
      </Show>
    </>
  );
}
