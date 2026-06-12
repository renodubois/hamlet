export const MESSAGE_PHOTO_LIMITS = {
  maxPhotos: 4,
  maxBytes: 10 * 1024 * 1024,
  maxPixels: 25_000_000,
  fullMaxEdge: 2048,
  thumbnailMaxEdge: 512,
  acceptedMimeTypes: ["image/jpeg", "image/jpg", "image/png", "image/webp"] as const,
  acceptAttribute: "image/jpeg,image/png,image/webp",
} as const;

export type MessagePhotoValidationErrorKind =
  | "too_many_attachments"
  | "payload_too_large"
  | "unsupported_photo"
  | "photo_dimensions_too_large";

export interface MessagePhotoValidationIssue {
  kind: MessagePhotoValidationErrorKind;
  message: string;
  fileName?: string;
}

interface Dimensions {
  width: number;
  height: number;
}

interface WebpInfo {
  dimensions: Dimensions | null;
  animated: boolean;
}

const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] as const;
const JPEG_SOF_MARKERS = new Set([
  0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf,
]);

export class MessagePhotoValidationError extends Error {
  readonly kind: MessagePhotoValidationErrorKind;
  readonly issues: readonly MessagePhotoValidationIssue[];

  constructor(issues: readonly MessagePhotoValidationIssue[]) {
    super(issues.map((issue) => issue.message).join(" "));
    this.name = "MessagePhotoValidationError";
    this.kind = issues[0]?.kind ?? "unsupported_photo";
    this.issues = issues;
  }
}

export async function validateMessagePhotos(
  photos: readonly File[],
): Promise<MessagePhotoValidationIssue[]> {
  if (photos.length > MESSAGE_PHOTO_LIMITS.maxPhotos) {
    return [
      {
        kind: "too_many_attachments",
        message: `Choose up to ${MESSAGE_PHOTO_LIMITS.maxPhotos} photos.`,
      },
    ];
  }

  const issues: MessagePhotoValidationIssue[] = [];
  for (const photo of photos) {
    const issue = await validateMessagePhoto(photo);
    if (issue) issues.push(issue);
  }
  return issues;
}

export async function assertValidMessagePhotos(photos: readonly File[]): Promise<void> {
  const issues = await validateMessagePhotos(photos);
  if (issues.length > 0) throw new MessagePhotoValidationError(issues);
}

async function validateMessagePhoto(file: File): Promise<MessagePhotoValidationIssue | null> {
  const fileName = displayFileName(file);
  if (file.size === 0) return unsupportedPhotoIssue(fileName);
  if (file.size > MESSAGE_PHOTO_LIMITS.maxBytes) {
    return {
      kind: "payload_too_large",
      fileName,
      message: `${fileName} is too large. Choose a photo smaller than ${formatBytes(
        MESSAGE_PHOTO_LIMITS.maxBytes,
      )}.`,
    };
  }

  const mimeType = file.type.trim().toLowerCase();
  if (!isAcceptedMimeType(mimeType)) return unsupportedPhotoIssue(fileName);

  const bytes = new Uint8Array(await file.arrayBuffer());
  const dimensions = readDimensions(mimeType, bytes);
  if (dimensions === "animated-webp") {
    return unsupportedPhotoIssue(fileName, "Animated WebP is not supported.");
  }
  if (!dimensions || dimensions.width <= 0 || dimensions.height <= 0) {
    return unsupportedPhotoIssue(fileName);
  }
  if (dimensions.width > MESSAGE_PHOTO_LIMITS.maxPixels / dimensions.height) {
    return {
      kind: "photo_dimensions_too_large",
      fileName,
      message: `${fileName} is too large in pixel dimensions. Choose a smaller photo.`,
    };
  }

  return null;
}

function isAcceptedMimeType(
  mimeType: string,
): mimeType is (typeof MESSAGE_PHOTO_LIMITS.acceptedMimeTypes)[number] {
  return (MESSAGE_PHOTO_LIMITS.acceptedMimeTypes as readonly string[]).includes(mimeType);
}

function unsupportedPhotoIssue(
  fileName: string,
  prefix = `${fileName} is not a supported photo.`,
): MessagePhotoValidationIssue {
  return {
    kind: "unsupported_photo",
    fileName,
    message: `${prefix} Choose a JPEG, PNG, or static WebP photo.`,
  };
}

function displayFileName(file: File): string {
  return file.name.trim() || "Selected photo";
}

function formatBytes(bytes: number): string {
  const megabytes = bytes / (1024 * 1024);
  return `${Number.isInteger(megabytes) ? megabytes : megabytes.toFixed(1)} MB`;
}

function readDimensions(
  mimeType: (typeof MESSAGE_PHOTO_LIMITS.acceptedMimeTypes)[number],
  bytes: Uint8Array,
): Dimensions | "animated-webp" | null {
  switch (mimeType) {
    case "image/jpeg":
    case "image/jpg":
      return readJpegDimensions(bytes);
    case "image/png":
      return readPngDimensions(bytes);
    case "image/webp": {
      const info = readWebpInfo(bytes);
      if (info.animated) return "animated-webp";
      return info.dimensions;
    }
  }
  return null;
}

function readPngDimensions(bytes: Uint8Array): Dimensions | null {
  if (!startsWith(bytes, PNG_SIGNATURE)) return null;
  if (bytes.length < 24) return null;
  if (ascii(bytes, 12, 16) !== "IHDR") return null;
  return {
    width: readUint32BE(bytes, 16),
    height: readUint32BE(bytes, 20),
  };
}

function readJpegDimensions(bytes: Uint8Array): Dimensions | null {
  if (bytes.length < 4 || bytes[0] !== 0xff || bytes[1] !== 0xd8) return null;

  let offset = 2;
  while (offset < bytes.length) {
    if (bytes[offset] !== 0xff) return null;
    while (offset < bytes.length && bytes[offset] === 0xff) offset += 1;
    if (offset >= bytes.length) return null;

    const marker = bytes[offset];
    offset += 1;
    if (marker === 0xd9 || marker === 0xda) return null;
    if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7)) continue;
    if (offset + 2 > bytes.length) return null;

    const segmentLength = readUint16BE(bytes, offset);
    if (segmentLength < 2 || offset + segmentLength > bytes.length) return null;
    const segmentStart = offset + 2;
    if (JPEG_SOF_MARKERS.has(marker)) {
      if (segmentLength < 7) return null;
      return {
        height: readUint16BE(bytes, segmentStart + 1),
        width: readUint16BE(bytes, segmentStart + 3),
      };
    }
    offset += segmentLength;
  }

  return null;
}

function readWebpInfo(bytes: Uint8Array): WebpInfo {
  if (bytes.length < 12 || ascii(bytes, 0, 4) !== "RIFF" || ascii(bytes, 8, 12) !== "WEBP") {
    return { dimensions: null, animated: false };
  }

  let offset = 12;
  while (offset + 8 <= bytes.length) {
    const chunk = ascii(bytes, offset, offset + 4);
    const size = readUint32LE(bytes, offset + 4);
    const dataOffset = offset + 8;
    const nextOffset = dataOffset + size + (size % 2);
    if (dataOffset + size > bytes.length) return { dimensions: null, animated: false };

    if (chunk === "ANIM" || chunk === "ANMF") return { dimensions: null, animated: true };
    if (chunk === "VP8X") return readVp8xInfo(bytes, dataOffset, size);
    if (chunk === "VP8 ") {
      return { dimensions: readVp8Dimensions(bytes, dataOffset, size), animated: false };
    }
    if (chunk === "VP8L") {
      return { dimensions: readVp8LosslessDimensions(bytes, dataOffset, size), animated: false };
    }

    offset = nextOffset;
  }

  return { dimensions: null, animated: false };
}

function readVp8xInfo(bytes: Uint8Array, offset: number, size: number): WebpInfo {
  if (size < 10 || offset + 10 > bytes.length) return { dimensions: null, animated: false };
  const flags = bytes[offset];
  const animated = (flags & 0b0000_0010) !== 0;
  return {
    animated,
    dimensions: {
      width: readUint24LE(bytes, offset + 4) + 1,
      height: readUint24LE(bytes, offset + 7) + 1,
    },
  };
}

function readVp8Dimensions(bytes: Uint8Array, offset: number, size: number): Dimensions | null {
  if (size < 10 || offset + 10 > bytes.length) return null;
  if (bytes[offset + 3] !== 0x9d || bytes[offset + 4] !== 0x01 || bytes[offset + 5] !== 0x2a) {
    return null;
  }
  return {
    width: readUint16LE(bytes, offset + 6) & 0x3fff,
    height: readUint16LE(bytes, offset + 8) & 0x3fff,
  };
}

function readVp8LosslessDimensions(
  bytes: Uint8Array,
  offset: number,
  size: number,
): Dimensions | null {
  if (size < 5 || offset + 5 > bytes.length || bytes[offset] !== 0x2f) return null;
  const bits =
    bytes[offset + 1] |
    (bytes[offset + 2] << 8) |
    (bytes[offset + 3] << 16) |
    (bytes[offset + 4] << 24);
  return {
    width: (bits & 0x3fff) + 1,
    height: ((bits >>> 14) & 0x3fff) + 1,
  };
}

function startsWith(bytes: Uint8Array, signature: readonly number[]): boolean {
  return signature.every((byte, index) => bytes[index] === byte);
}

function ascii(bytes: Uint8Array, start: number, end: number): string {
  return String.fromCharCode(...bytes.slice(start, end));
}

function readUint16BE(bytes: Uint8Array, offset: number): number {
  return (bytes[offset] << 8) | bytes[offset + 1];
}

function readUint16LE(bytes: Uint8Array, offset: number): number {
  return bytes[offset] | (bytes[offset + 1] << 8);
}

function readUint24LE(bytes: Uint8Array, offset: number): number {
  return bytes[offset] | (bytes[offset + 1] << 8) | (bytes[offset + 2] << 16);
}

function readUint32BE(bytes: Uint8Array, offset: number): number {
  return (
    bytes[offset] * 0x1000000 +
    bytes[offset + 1] * 0x10000 +
    bytes[offset + 2] * 0x100 +
    bytes[offset + 3]
  );
}

function readUint32LE(bytes: Uint8Array, offset: number): number {
  return (
    (bytes[offset] |
      (bytes[offset + 1] << 8) |
      (bytes[offset + 2] << 16) |
      (bytes[offset + 3] << 24)) >>>
    0
  );
}
