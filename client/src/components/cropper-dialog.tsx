import { useEffect, useRef, useState } from "react";
import Cropper from "cropperjs";
import Modal from "./modal";
import { Button } from "./ui/button";

export default function CropperDialog(props: {
  open: boolean;
  file: File | null;
  onCancel: () => void;
  onSave: (blob: Blob) => Promise<void>;
}) {
  const imgRef = useRef<HTMLImageElement | null>(null);
  const cropperRef = useRef<Cropper | null>(null);
  const currentFileRef = useRef(props.open ? props.file : null);
  currentFileRef.current = props.open ? props.file : null;
  const [objectUrl, setObjectUrl] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setSaving(false);
    setError(null);

    if (!props.open || !props.file) {
      setObjectUrl(null);
      return;
    }

    const url = URL.createObjectURL(props.file);
    setObjectUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [props.open, props.file]);

  useEffect(() => {
    const image = imgRef.current;
    if (!props.open || !objectUrl || !image) return;

    const cropper = new Cropper(image, {});
    cropperRef.current = cropper;
    const selection = cropper.getCropperSelection();
    if (selection) {
      selection.aspectRatio = 1;
      selection.initialCoverage = 0.8;
    }

    return () => {
      if (cropperRef.current === cropper) cropperRef.current = null;
      cropper.destroy();
    };
  }, [props.open, objectUrl]);

  const handleSave = async () => {
    setError(null);
    const cropper = cropperRef.current;
    const file = currentFileRef.current;
    const selection = cropper?.getCropperSelection();
    if (!selection || !file) {
      setError("Cropper not ready");
      return;
    }

    setSaving(true);
    const isCurrent = () => currentFileRef.current === file && cropperRef.current === cropper;
    try {
      const canvas = await selection.$toCanvas({ width: 256, height: 256 });
      if (!isCurrent()) return;
      const blob: Blob = await new Promise((resolve, reject) => {
        canvas.toBlob(
          (result) => (result ? resolve(result) : reject(new Error("Failed to encode image"))),
          "image/webp",
          0.9,
        );
      });
      if (!isCurrent()) return;
      await props.onSave(blob);
    } catch (cause) {
      if (isCurrent()) {
        setError(cause instanceof Error ? cause.message : "Failed to save");
      }
    } finally {
      if (isCurrent()) setSaving(false);
    }
  };

  return (
    <Modal open={props.open} onClose={props.onCancel} title="Crop your picture" size="lg">
      {objectUrl ? (
        <div className="max-h-[60vh] overflow-hidden flex justify-center items-center bg-muted rounded-md">
          <img ref={imgRef} src={objectUrl} alt="" className="max-w-full max-h-[50vh]" />
        </div>
      ) : null}
      {error ? <p className="text-destructive text-sm mt-3">{error}</p> : null}
      <div className="flex gap-2 justify-end mt-4">
        <Button type="button" variant="ghost" onClick={props.onCancel} disabled={saving}>
          Cancel
        </Button>
        <Button type="button" onClick={handleSave} disabled={saving}>
          {saving ? "Saving..." : "Save"}
        </Button>
      </div>
    </Modal>
  );
}
