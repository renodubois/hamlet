import Cropper from "cropperjs";
import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import Modal from "./modal";

export default function CropperDialog(props: {
  open: boolean;
  file: File | null;
  onCancel: () => void;
  onSave: (blob: Blob) => Promise<void>;
}) {
  let imgRef: HTMLImageElement | undefined;
  const setImgRef = (el: HTMLImageElement) => {
    imgRef = el;
  };
  let cropper: Cropper | null = null;
  const [objectUrl, setObjectUrl] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  createEffect(() => {
    if (!props.open || !props.file) {
      setObjectUrl(null);
      return;
    }
    const url = URL.createObjectURL(props.file);
    setObjectUrl(url);
    onCleanup(() => URL.revokeObjectURL(url));
  });

  createEffect(() => {
    const url = objectUrl();
    const img = imgRef;
    if (!url || !img) return;

    cropper = new Cropper(img, {});
    const selection = cropper.getCropperSelection();
    if (selection) {
      selection.aspectRatio = 1;
      selection.initialCoverage = 0.8;
    }

    onCleanup(() => {
      cropper?.destroy();
      cropper = null;
    });
  });

  const handleSave = async () => {
    setError(null);
    const selection = cropper?.getCropperSelection();
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
      <Show when={objectUrl()}>
        {(u) => (
          <div class="max-h-[60vh] overflow-hidden flex justify-center items-center bg-gray-900 rounded">
            <img ref={setImgRef} src={u()} alt="" class="max-w-full max-h-[50vh]" />
          </div>
        )}
      </Show>
      <Show when={error()}>{(msg) => <p class="text-red-400 text-sm mt-3">{msg()}</p>}</Show>
      <div class="flex gap-2 justify-end mt-4">
        <button
          type="button"
          class="text-gray-300 hover:text-gray-100 text-sm px-3 py-2 disabled:opacity-50"
          onClick={props.onCancel}
          disabled={saving()}
        >
          Cancel
        </button>
        <button
          type="button"
          class="bg-blue-600 hover:bg-blue-700 text-white rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 transition-colors"
          onClick={handleSave}
          disabled={saving()}
        >
          {saving() ? "Saving..." : "Save"}
        </button>
      </div>
    </Modal>
  );
}
