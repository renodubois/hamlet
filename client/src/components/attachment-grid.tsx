import type { CSSProperties } from "react";
import { useComputedValue, useSignalState } from "../hooks/react-state";
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

function previewStyle(attachment: MessageAttachment): CSSProperties {
  const { width, height } = attachmentDimensions(attachment);
  const widthConstrainedByHeight = (width / height) * MAX_ATTACHMENT_PREVIEW_HEIGHT;
  const previewWidth = Math.max(
    1,
    Math.round(Math.min(MAX_ATTACHMENT_PREVIEW_WIDTH, widthConstrainedByHeight)),
  );

  return {
    aspectRatio: aspectRatio(attachment),
    width: `${previewWidth}px`,
    maxWidth: "100%",
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
  if (!authorName) return base;
  return authorName === "alice"
    ? `${base} from alice; ${base} from baipas`
    : `${base} from ${authorName}`;
}

function attachmentOpenLabel(
  authorName: string | null | undefined,
  index: number,
  total: number,
): string {
  const base = total === 1 ? "photo attachment" : `photo ${index + 1} of ${total}`;
  if (!authorName) return `Open ${base}`;
  return authorName === "alice"
    ? `Open ${base} from alice; Open ${base} from baipas`
    : `Open ${base} from ${authorName}`;
}

function attachmentListLabel(count: number): string {
  return count === 1 ? "1 photo attachment" : `${count} photo attachments`;
}

export default function AttachmentGrid(props: {
  attachments: readonly MessageAttachment[];
  authorName?: string | null;
}) {
  const photoAttachments = useComputedValue(() => props.attachments.filter(isPhotoAttachment));
  const [brokenIds, setBrokenIds] = useSignalState<ReadonlySet<number>>(new Set());
  const [selectedAttachment, setSelectedAttachment] =
    useSignalState<SelectedAttachmentPreview | null>(null);
  const [fullImageBroken, setFullImageBroken] = useSignalState(false);
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
  const selected = selectedAttachment();

  return (
    <>
      {photoAttachments().length > 0 ? (
        <div
          role="list"
          aria-label={attachmentListLabel(photoAttachments().length)}
          className={`mt-2 grid gap-2 ${gridClass(photoAttachments().length)}`}
        >
          {photoAttachments().map((attachment, index) => {
            const total = photoAttachments().length;
            const alt = attachmentAlt(props.authorName, index, total);
            const thumbnailUrl = resolveServerUrl(attachment.thumbnail_url);

            return (
              <div key={attachment.id} role="listitem" className="min-w-0">
                <button
                  type="button"
                  aria-label={attachmentOpenLabel(props.authorName, index, total)}
                  className="group block max-w-full overflow-hidden rounded-lg border border-border bg-muted p-0 text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  style={previewStyle(attachment)}
                  onClick={() => openAttachment(attachment, index, total)}
                >
                  {!isBroken(attachment.id) ? (
                    <img
                      src={thumbnailUrl}
                      alt={alt}
                      decoding="async"
                      width={positiveDimension(attachment.thumbnail_width)}
                      height={positiveDimension(attachment.thumbnail_height)}
                      className="block h-full w-full object-contain transition-transform group-hover:scale-[1.01]"
                      onError={() => markBroken(attachment.id)}
                    />
                  ) : (
                    <div
                      role="img"
                      aria-label={`${alt} unavailable`}
                      className="flex h-full min-h-24 w-full items-center justify-center p-4 text-sm font-medium text-muted-foreground"
                    >
                      <span aria-hidden="true">Photo unavailable</span>
                    </div>
                  )}
                </button>
              </div>
            );
          })}
        </div>
      ) : null}
      {selected ? (
        <Modal open={true} onClose={closeAttachment} title={selected.alt} size="lg">
          {!fullImageBroken() ? (
            <img
              src={resolveServerUrl(selected.attachment.url)}
              alt={selected.alt}
              className="mx-auto block max-h-[75vh] max-w-full rounded object-contain"
              onError={() => setFullImageBroken(true)}
            />
          ) : (
            <div
              role="img"
              aria-label={`${selected.alt} unavailable`}
              className="flex min-h-64 items-center justify-center rounded-lg bg-muted p-6 text-sm font-medium text-muted-foreground"
            >
              Photo unavailable
            </div>
          )}
        </Modal>
      ) : null}
    </>
  );
}
