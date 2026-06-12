import { For, Show, createSignal, onCleanup, type Accessor, type JSX } from "solid-js";
import {
  MESSAGE_PHOTO_ACCEPT,
  MESSAGE_PHOTO_MAX_BYTES,
  MESSAGE_PHOTO_MAX_COUNT,
  MESSAGE_PHOTO_SUPPORTED_TYPES,
} from "../constants";
import { ImagePlusIcon } from "./icons";

export interface SelectedComposerPhoto {
  id: string;
  file: File;
  previewUrl: string;
}

interface ComposerPhotoSelectionController {
  photos: Accessor<SelectedComposerPhoto[]>;
  error: Accessor<string | null>;
  addFiles: (files: FileList | readonly File[] | null | undefined) => void;
  removePhoto: (id: string) => void;
  clearPhotos: () => void;
}

let nextPhotoDraftId = 0;

const DEFAULT_ATTACH_BUTTON_CLASS =
  "inline-flex cursor-pointer items-center gap-2 rounded-md bg-gray-100 px-3 py-4 text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-blue-400 disabled:cursor-not-allowed disabled:opacity-50";

function photoLimitMessage(): string {
  return `You can attach up to ${MESSAGE_PHOTO_MAX_COUNT} photos.`;
}

function maxPhotoMegabytes(): number {
  return MESSAGE_PHOTO_MAX_BYTES / (1024 * 1024);
}

function unsupportedPhotoMessage(file: File): string {
  return `${file.name || "Selected file"} must be a JPEG, PNG, or WebP image.`;
}

function oversizedPhotoMessage(file: File): string {
  return `${file.name || "Selected file"} is larger than ${maxPhotoMegabytes()} MB.`;
}

function createPreviewUrl(file: File): string {
  return typeof URL.createObjectURL === "function" ? URL.createObjectURL(file) : "";
}

function revokePreviewUrl(url: string) {
  if (url.length > 0) URL.revokeObjectURL?.(url);
}

function selectedPhotoFromFile(file: File): SelectedComposerPhoto {
  const id = `photo-${Date.now()}-${nextPhotoDraftId++}`;
  return {
    id,
    file,
    previewUrl: createPreviewUrl(file),
  };
}

function validatePhotoBatch(currentCount: number, files: readonly File[]): string | null {
  if (currentCount + files.length > MESSAGE_PHOTO_MAX_COUNT) return photoLimitMessage();

  const unsupported = files.find(
    (file) => !MESSAGE_PHOTO_SUPPORTED_TYPES.has(file.type.toLowerCase()),
  );
  if (unsupported) return unsupportedPhotoMessage(unsupported);

  const oversized = files.find((file) => file.size > MESSAGE_PHOTO_MAX_BYTES);
  if (oversized) return oversizedPhotoMessage(oversized);

  return null;
}

export function createComposerPhotoSelection(): ComposerPhotoSelectionController {
  const [photos, setPhotos] = createSignal<SelectedComposerPhoto[]>([]);
  const [error, setError] = createSignal<string | null>(null);

  const revokePhotos = (selectedPhotos: readonly SelectedComposerPhoto[]) => {
    for (const photo of selectedPhotos) revokePreviewUrl(photo.previewUrl);
  };

  const addFiles = (fileSource: FileList | readonly File[] | null | undefined) => {
    const files = Array.from(fileSource ?? []);
    if (files.length === 0) return;

    const validationError = validatePhotoBatch(photos().length, files);
    if (validationError) {
      setError(validationError);
      return;
    }

    setPhotos((current) => [...current, ...files.map(selectedPhotoFromFile)]);
    setError(null);
  };

  const removePhoto = (id: string) => {
    setPhotos((current) => {
      const removed = current.find((photo) => photo.id === id);
      if (removed) revokePreviewUrl(removed.previewUrl);
      return current.filter((photo) => photo.id !== id);
    });
    setError(null);
  };

  const clearPhotos = () => {
    setPhotos((current) => {
      revokePhotos(current);
      return [];
    });
    setError(null);
  };

  onCleanup(() => revokePhotos(photos()));

  return { photos, error, addFiles, removePhoto, clearPhotos };
}

export function PhotoAttachControl(props: {
  onFilesSelected: (files: FileList) => void;
  disabled?: boolean;
  describedBy?: string;
  class?: string;
}) {
  let inputRef: HTMLInputElement | undefined;

  const openFilePicker = () => {
    if (!props.disabled) inputRef?.click();
  };

  const handleFileChange: JSX.EventHandler<HTMLInputElement, Event> = (event) => {
    const files = event.currentTarget.files;
    if (files) props.onFilesSelected(files);
    event.currentTarget.value = "";
  };

  return (
    <>
      <button
        type="button"
        class={props.class ?? DEFAULT_ATTACH_BUTTON_CLASS}
        aria-label="Attach photos"
        aria-describedby={props.describedBy}
        title="Attach photos"
        disabled={props.disabled}
        onClick={openFilePicker}
      >
        <ImagePlusIcon size={20} aria-hidden="true" />
        <span class="text-sm font-medium">Photo</span>
      </button>
      <input
        ref={(el) => {
          inputRef = el;
        }}
        class="sr-only"
        type="file"
        accept={MESSAGE_PHOTO_ACCEPT}
        multiple
        tabIndex={-1}
        disabled={props.disabled}
        aria-label="Photo files"
        onChange={handleFileChange}
      />
    </>
  );
}

export function SelectedPhotoPreviewList(props: {
  photos: readonly SelectedComposerPhoto[];
  error: string | null;
  errorId: string;
  disabled?: boolean;
  onRemove: (id: string) => void;
}) {
  const selectedPhotoLabel = () =>
    props.photos.length === 1 ? "1 selected photo" : `${props.photos.length} selected photos`;

  return (
    <Show when={props.photos.length > 0 || props.error !== null}>
      <div class="mb-2 flex flex-col gap-2">
        <Show when={props.photos.length > 0}>
          <div
            role="list"
            aria-label={selectedPhotoLabel()}
            class="flex max-w-full flex-wrap gap-2"
          >
            <For each={props.photos}>
              {(photo, index) => (
                <div
                  role="listitem"
                  class="relative h-24 w-24 overflow-hidden rounded-lg border border-gray-200 bg-gray-100"
                >
                  <Show
                    when={photo.previewUrl.length > 0}
                    fallback={
                      <div
                        role="img"
                        aria-label={`Selected photo ${index() + 1}: ${photo.file.name}`}
                        class="flex h-full w-full items-center justify-center p-2 text-center text-xs text-gray-600"
                      >
                        Photo selected
                      </div>
                    }
                  >
                    <img
                      src={photo.previewUrl}
                      alt={`Selected photo ${index() + 1}: ${photo.file.name}`}
                      class="h-full w-full object-cover"
                    />
                  </Show>
                  <button
                    type="button"
                    class="absolute right-1 top-1 rounded bg-white/90 px-2 py-1 text-xs font-medium text-gray-900 shadow focus:outline-none focus:ring-2 focus:ring-blue-400 disabled:opacity-50"
                    aria-label={`Remove selected photo ${index() + 1}: ${photo.file.name}`}
                    disabled={props.disabled}
                    onClick={() => props.onRemove(photo.id)}
                  >
                    Remove
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>
        <Show when={props.error}>
          {(error) => (
            <p id={props.errorId} role="alert" class="text-sm font-medium text-red-700">
              {error()}
            </p>
          )}
        </Show>
      </div>
    </Show>
  );
}
