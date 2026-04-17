import { createSignal, Show } from "solid-js";
import Modal from "./modal";
import { CHANNEL_NAME_MAX_LEN, createChannel } from "../api";

export default function AddChannelModal(props: { open: boolean; onClose: () => void }) {
  const [name, setName] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);

  const close = () => {
    setName("");
    setError(null);
    props.onClose();
  };

  const handleSubmit = async (e: SubmitEvent) => {
    e.preventDefault();
    const trimmed = name().trim();
    if (trimmed.length === 0) {
      setError("Channel name cannot be empty.");
      return;
    }
    // eslint-disable-next-line typescript-eslint/no-misused-spread -- match server's code-point count (Rust .chars().count())
    if ([...trimmed].length > CHANNEL_NAME_MAX_LEN) {
      setError(`Channel name must be ${CHANNEL_NAME_MAX_LEN} characters or fewer.`);
      return;
    }

    setError(null);
    setSubmitting(true);
    try {
      const res = await createChannel(trimmed);
      if (res.ok) {
        close();
        return;
      }
      if (res.status === 400) {
        setError("Channel name is invalid (1–128 characters required).");
      } else if (res.status === 401) {
        setError("You must be signed in.");
      } else {
        setError("Server error, please try again.");
      }
    } catch {
      setError("Could not reach server.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal open={props.open} onClose={close} title="Add Channel">
      <form onSubmit={handleSubmit} class="flex flex-col gap-3">
        <input
          class="bg-gray-700 text-gray-100 rounded-md p-3 placeholder-gray-400"
          type="text"
          placeholder="Channel name"
          autofocus
          maxLength={CHANNEL_NAME_MAX_LEN}
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
        />
        <Show when={error()}>
          <p class="text-red-400 text-sm">{error()}</p>
        </Show>
        <div class="flex gap-2 justify-end">
          <button
            type="button"
            class="text-gray-300 hover:text-gray-100 text-sm px-3 py-2"
            onClick={close}
          >
            Cancel
          </button>
          <button
            type="submit"
            class="bg-blue-600 hover:bg-blue-700 text-white rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 transition-colors"
            disabled={submitting()}
          >
            {submitting() ? "Creating..." : "Create"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
