import type { FormEvent } from "react";

import { useSignalState } from "../hooks/react-state";
import Modal from "./modal";
import { createChannel, type ChannelType } from "../api";
import { CHANNEL_NAME_MAX_LEN } from "../constants";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

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

  const handleSubmit = async (e: FormEvent<HTMLFormElement>) => {
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
        <Input
          type="text"
          placeholder="Channel name"
          autoFocus
          maxLength={CHANNEL_NAME_MAX_LEN}
          value={name()}
          onChange={(e) => setName(e.currentTarget.value)}
        />
        <fieldset className="flex gap-2">
          <legend className="text-muted-foreground text-sm mb-1">Channel type</legend>
          <label className="flex-1 flex items-center gap-2 bg-muted hover:bg-accent transition-colors rounded-md px-3 py-2 cursor-pointer text-sm">
            <input
              type="radio"
              name="channel-type"
              value="text"
              checked={type() === "text"}
              onChange={() => setType("text")}
            />
            Text
          </label>
          <label className="flex-1 flex items-center gap-2 bg-muted hover:bg-accent transition-colors rounded-md px-3 py-2 cursor-pointer text-sm">
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
        {error() ? <p className="text-destructive text-sm">{error()}</p> : null}
        <div className="flex gap-2 justify-end">
          <Button type="button" variant="ghost" onClick={close}>
            Cancel
          </Button>
          <Button type="submit" disabled={submitting()}>
            {submitting() ? "Creating..." : "Create"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
