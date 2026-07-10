import { describe, expect, test } from "vitest";
import {
  MESSAGE_PHOTO_LIMITS,
  MessagePhotoValidationError,
  assertValidMessagePhotos,
  validateMessagePhotos,
} from "./photo-validation";
import {
  TINY_PNG_BYTES,
  imageFile,
  tinyJpegFile,
  tinyPngFile,
  tinyWebpFile,
} from "./test/image-fixtures";

function pngHeader(width: number, height: number): Uint8Array {
  const bytes = new Uint8Array(24);
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  bytes.set([0x00, 0x00, 0x00, 0x0d], 8);
  bytes.set([0x49, 0x48, 0x44, 0x52], 12);
  const view = new DataView(bytes.buffer);
  view.setUint32(16, width);
  view.setUint32(20, height);
  return bytes;
}

function animatedWebpHeader(): Uint8Array {
  const bytes = new Uint8Array(30);
  bytes.set([0x52, 0x49, 0x46, 0x46]);
  new DataView(bytes.buffer).setUint32(4, 22, true);
  bytes.set([0x57, 0x45, 0x42, 0x50], 8);
  bytes.set([0x56, 0x50, 0x38, 0x58], 12);
  new DataView(bytes.buffer).setUint32(16, 10, true);
  bytes[20] = 0b0000_0010;
  return bytes;
}

describe("message photo validation", () => {
  test("accepts tiny deterministic JPEG, PNG, and static WebP fixtures", async () => {
    await expect(
      validateMessagePhotos([tinyJpegFile(), tinyPngFile(), tinyWebpFile()]),
    ).resolves.toEqual([]);
  });

  test("rejects too many photos before inspecting individual files", async () => {
    const issues = await validateMessagePhotos([
      tinyPngFile("1.png"),
      tinyPngFile("2.png"),
      tinyPngFile("3.png"),
      tinyPngFile("4.png"),
      tinyPngFile("5.png"),
    ]);

    expect(issues).toMatchObject([
      {
        kind: "too_many_attachments",
        message: `Choose up to ${MESSAGE_PHOTO_LIMITS.maxPhotos} photos.`,
      },
    ]);
  });

  test("rejects oversized files with a smaller-file instruction", async () => {
    const huge = new File([new Uint8Array(MESSAGE_PHOTO_LIMITS.maxBytes + 1)], "huge.png", {
      type: "image/png",
    });

    const issues = await validateMessagePhotos([huge]);

    expect(issues[0]).toMatchObject({ kind: "payload_too_large", fileName: "huge.png" });
    expect(issues[0]?.message).toMatch(/choose a photo smaller than 10 MB/i);
  });

  test("rejects unsupported formats, MIME/signature mismatches, and animated WebP", async () => {
    const issues = await validateMessagePhotos([
      imageFile("wrong.jpg", "image/jpeg", TINY_PNG_BYTES),
      new File(["gif"], "dance.gif", { type: "image/gif" }),
      imageFile("animated.webp", "image/webp", animatedWebpHeader()),
    ]);

    expect(issues.map((issue) => issue.kind)).toEqual([
      "unsupported_photo",
      "unsupported_photo",
      "unsupported_photo",
    ]);
    expect(issues[0]?.message).toMatch(/choose a JPEG, PNG, or static WebP photo/i);
    expect(issues[2]?.message).toMatch(/Animated WebP is not supported/i);
  });

  test("rejects zero and excessive dimensions with stable server-like kinds", async () => {
    const issues = await validateMessagePhotos([
      imageFile("zero.png", "image/png", pngHeader(0, 1)),
      imageFile("giant.png", "image/png", pngHeader(6_000, 5_000)),
    ]);

    expect(issues[0]).toMatchObject({ kind: "unsupported_photo", fileName: "zero.png" });
    expect(issues[1]).toMatchObject({
      kind: "photo_dimensions_too_large",
      fileName: "giant.png",
    });
  });

  test("assertValidMessagePhotos throws an aggregate plain-language validation error", async () => {
    await expect(
      assertValidMessagePhotos([new File(["bad"], "bad.heic", { type: "image/heic" })]),
    ).rejects.toMatchObject({
      name: "MessagePhotoValidationError",
      kind: "unsupported_photo",
      issues: [{ fileName: "bad.heic" }],
    });

    try {
      await assertValidMessagePhotos([new File(["bad"], "bad.heic", { type: "image/heic" })]);
    } catch (error) {
      expect(error).toBeInstanceOf(MessagePhotoValidationError);
      expect((error as Error).message).toMatch(/choose a JPEG, PNG, or static WebP photo/i);
    }
  });
});
