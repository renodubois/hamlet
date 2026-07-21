import { useCallback, useEffect, useRef, useState, type ChangeEvent } from "react";

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

export interface ComposerPhotoSelection {
  photos: readonly SelectedComposerPhoto[];
  error: string | null;
  addFiles: (files: FileList | readonly File[] | null | undefined) => void;
  removePhoto: (id: string) => void;
  clearPhotos: () => void;
}

let nextPhotoDraftId = 0;

const DEFAULT_ATTACH_BUTTON_CLASS =
  "inline-flex cursor-pointer items-center gap-2 rounded-md bg-muted px-3 py-4 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50";

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

function revokePhotos(selectedPhotos: readonly SelectedComposerPhoto[]) {
  for (const photo of selectedPhotos) revokePreviewUrl(photo.previewUrl);
}

export function useComposerPhotoSelection(): ComposerPhotoSelection {
  const [photos, setPhotos] = useState<SelectedComposerPhoto[]>([]);
  const [error, setError] = useState<string | null>(null);
  const ownedPhotosRef = useRef<SelectedComposerPhoto[]>([]);

  const addFiles = useCallback((fileSource: FileList | readonly File[] | null | undefined) => {
    const files = Array.from(fileSource ?? []);
    if (files.length === 0) return;

    const validationError = validatePhotoBatch(ownedPhotosRef.current.length, files);
    if (validationError) {
      setError(validationError);
      return;
    }

    const next = [...ownedPhotosRef.current, ...files.map(selectedPhotoFromFile)];
    ownedPhotosRef.current = next;
    setPhotos(next);
    setError(null);
  }, []);

  const removePhoto = useCallback((id: string) => {
    const removed = ownedPhotosRef.current.find((photo) => photo.id === id);
    if (!removed) return;

    const next = ownedPhotosRef.current.filter((photo) => photo.id !== id);
    ownedPhotosRef.current = next;
    setPhotos(next);
    revokePreviewUrl(removed.previewUrl);
    setError(null);
  }, []);

  const clearPhotos = useCallback(() => {
    const owned = ownedPhotosRef.current;
    ownedPhotosRef.current = [];
    setPhotos([]);
    setError(null);
    revokePhotos(owned);
  }, []);

  useEffect(
    () => () => {
      const owned = ownedPhotosRef.current;
      ownedPhotosRef.current = [];
      revokePhotos(owned);
    },
    [],
  );

  return { photos, error, addFiles, removePhoto, clearPhotos };
}

export function PhotoAttachControl(props: {
  onFilesSelected: (files: FileList) => void;
  disabled?: boolean;
  describedBy?: string;
  className?: string;
}) {
  const inputRef = useRef<HTMLInputElement>(null);

  const openFilePicker = () => {
    if (!props.disabled) inputRef.current?.click();
  };

  const handleFileChange = (event: ChangeEvent<HTMLInputElement>) => {
    const files = event.currentTarget.files;
    if (files) props.onFilesSelected(files);
    event.currentTarget.value = "";
  };

  return (
    <>
      <button
        type="button"
        className={props.className ?? DEFAULT_ATTACH_BUTTON_CLASS}
        aria-label="Attach photos"
        aria-describedby={props.describedBy}
        title="Attach photos"
        disabled={props.disabled}
        onClick={openFilePicker}
      >
        <ImagePlusIcon size={20} aria-hidden="true" />
        <span className="text-sm font-medium">Photo</span>
      </button>
      <input
        ref={inputRef}
        className="sr-only"
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
  const selectedPhotoLabel =
    props.photos.length === 1 ? "1 selected photo" : `${props.photos.length} selected photos`;

  if (props.photos.length === 0 && props.error === null) return null;

  return (
    <div className="mb-2 flex flex-col gap-2">
      {props.photos.length > 0 ? (
        <div
          role="list"
          aria-label={selectedPhotoLabel}
          className="flex max-w-full flex-wrap gap-2"
        >
          {props.photos.map((photo, index) => (
            <div
              key={photo.id}
              role="listitem"
              className="relative h-24 w-24 overflow-hidden rounded-lg border border-border bg-muted"
            >
              {photo.previewUrl.length > 0 ? (
                <img
                  src={photo.previewUrl}
                  alt={`Selected photo ${index + 1}: ${photo.file.name}`}
                  className="h-full w-full object-cover"
                />
              ) : (
                <div
                  role="img"
                  aria-label={`Selected photo ${index + 1}: ${photo.file.name}`}
                  className="flex h-full w-full items-center justify-center p-2 text-center text-xs text-muted-foreground"
                >
                  Photo selected
                </div>
              )}
              <button
                type="button"
                className="absolute right-1 top-1 rounded-md bg-background/90 px-2 py-1 text-xs font-medium text-foreground shadow focus:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
                aria-label={`Remove selected photo ${index + 1}: ${photo.file.name}`}
                disabled={props.disabled}
                onClick={() => props.onRemove(photo.id)}
              >
                Remove
              </button>
            </div>
          ))}
        </div>
      ) : null}
      {props.error ? (
        <p id={props.errorId} role="alert" className="text-sm font-medium text-destructive">
          {props.error}
        </p>
      ) : null}
    </div>
  );
}
