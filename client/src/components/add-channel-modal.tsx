import { useSignalState, If } from "../hooks/react-state";
import Modal from "./modal";
import { createChannel, type ChannelType } from "../api";
import { CHANNEL_NAME_MAX_LEN } from "../constants";

export default function AddChannelModal(props: { open: boolean; onClose: () => void }) {
  const [name, setName] = useSignalState("");
  const [type, setType] = useSignalState<ChannelType>("text");
  const [error, setError] = useSignalState<string | null>(null);
  const [submitting, setSubmitting] = useSignalState(false);

  const close = () => {
    setName("");
    setType("text");
    setError(null);
    props.onClose();
  };

  const handleSubmit = async (e: any) => {
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
      const res = await createChannel(trimmed, type());
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
      <form onSubmit={handleSubmit} className="flex flex-col gap-3">
        <input
          className="bg-gray-700 text-gray-100 rounded-md p-3 placeholder-gray-400"
          type="text"
          placeholder="Channel name"
          autoFocus
          maxLength={CHANNEL_NAME_MAX_LEN}
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
        />
        <fieldset className="flex gap-2">
          <legend className="text-gray-300 text-sm mb-1">Channel type</legend>
          <label className="flex-1 flex items-center gap-2 bg-gray-700 rounded-md px-3 py-2 cursor-pointer text-gray-100 text-sm">
            <input
              type="radio"
              name="channel-type"
              value="text"
              checked={type() === "text"}
              onChange={() => setType("text")}
            />
            Text
          </label>
          <label className="flex-1 flex items-center gap-2 bg-gray-700 rounded-md px-3 py-2 cursor-pointer text-gray-100 text-sm">
            <input
              type="radio"
              name="channel-type"
              value="voice"
              checked={type() === "voice"}
              onChange={() => setType("voice")}
            />
            Voice
          </label>
        </fieldset>
        <If when={error()}>
          <p className="text-red-400 text-sm">{error()}</p>
        </If>
        <div className="flex gap-2 justify-end">
          <button
            type="button"
            className="text-gray-300 hover:text-gray-100 text-sm px-3 py-2"
            onClick={close}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="bg-blue-600 hover:bg-blue-700 text-white rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 transition-colors"
            disabled={submitting()}
          >
            {submitting() ? "Creating..." : "Create"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
