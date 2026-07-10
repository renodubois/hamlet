import { useRef } from "react";
import Cropper from "cropperjs";
import { useAfterRenderEffect, useSignalState, registerCleanup, If } from "../hooks/react-state";
import Modal from "./modal";
import { Button } from "./ui/button";

export default function CropperDialog(props: {
  open: boolean;
  file: File | null;
  onCancel: () => void;
  onSave: (blob: Blob) => Promise<void>;
}) {
  const imgRef = useRef<HTMLImageElement | null>(null);
  const setImgRef = (el: HTMLImageElement | null) => {
    imgRef.current = el;
  };
  const cropperRef = useRef<Cropper | null>(null);
  const objectUrlRef = useRef<{ file: File; url: string } | null>(null);
  const [objectUrl, setObjectUrl] = useSignalState<string | null>(null);
  const [saving, setSaving] = useSignalState(false);
  const [error, setError] = useSignalState<string | null>(null);

  registerCleanup(() => {
    if (objectUrlRef.current) URL.revokeObjectURL(objectUrlRef.current.url);
    objectUrlRef.current = null;
  });

  useAfterRenderEffect(() => {
    if (!props.open || !props.file) {
      if (objectUrlRef.current) URL.revokeObjectURL(objectUrlRef.current.url);
      objectUrlRef.current = null;
      setObjectUrl(null);
      return;
    }
    if (objectUrlRef.current?.file === props.file) return;
    if (objectUrlRef.current) URL.revokeObjectURL(objectUrlRef.current.url);
    const url = URL.createObjectURL(props.file);
    objectUrlRef.current = { file: props.file, url };
    setObjectUrl(url);
  });

  useAfterRenderEffect(() => {
    const url = objectUrl();
    const img = imgRef.current;
    if (!url || !img || cropperRef.current) return;

    cropperRef.current = new Cropper(img, {});
    const selection = cropperRef.current.getCropperSelection();
    if (selection) {
      selection.aspectRatio = 1;
      selection.initialCoverage = 0.8;
    }

    registerCleanup(() => {
      cropperRef.current?.destroy();
      cropperRef.current = null;
    });
  });

  const handleSave = async () => {
    setError(null);
    const selection = cropperRef.current?.getCropperSelection();
    if (!selection) {
      setError("Cropper not ready");
      return;
    }
    setSaving(true);
    try {
      const canvas = await selection.$toCanvas({ width: 256, height: 256 });
      const blob: Blob = await new Promise((resolve, reject) => {
        canvas.toBlob(
          (b) => (b ? resolve(b) : reject(new Error("Failed to encode image"))),
          "image/webp",
          0.9,
        );
      });
      await props.onSave(blob);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal open={props.open} onClose={props.onCancel} title="Crop your picture" size="lg">
      <If when={objectUrl()}>
        {(u) => (
          <div className="max-h-[60vh] overflow-hidden flex justify-center items-center bg-muted rounded-md">
            <img ref={setImgRef} src={u()} alt="" className="max-w-full max-h-[50vh]" />
          </div>
        )}
      </If>
      <If when={error()}>{(msg) => <p className="text-destructive text-sm mt-3">{msg()}</p>}</If>
      <div className="flex gap-2 justify-end mt-4">
        <Button type="button" variant="ghost" onClick={props.onCancel} disabled={saving()}>
          Cancel
        </Button>
        <Button type="button" onClick={handleSave} disabled={saving()}>
          {saving() ? "Saving..." : "Save"}
        </Button>
      </div>
    </Modal>
  );
}
