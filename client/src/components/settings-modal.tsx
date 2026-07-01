import { createEffect, createSignal, For, Match, onCleanup, Show, Switch } from "solid-js";
import {
  changePassword,
  deleteAvatar,
  getServerUrl,
  updateDisplayName,
  uploadAvatar,
  type CustomEmoji,
  type User,
} from "../api";
import { useCustomEmojis } from "../contexts/custom-emojis";
import { DISPLAY_NAME_MAX_LEN } from "../constants";
import Avatar from "./avatar";
import CropperDialog from "./cropper-dialog";
import { LogOutIcon } from "./icons";
import Modal from "./modal";
import VoiceSettings from "./voice-settings";

type SectionId = "profile" | "voice" | "emojis";

interface Section {
  id: SectionId;
  label: string;
  tabId: string;
  panelId: string;
}

function resolveImageUrl(url: string): string {
  if (url.startsWith("http://") || url.startsWith("https://")) return url;
  return `${getServerUrl()}${url}`;
}

const SECTIONS: Section[] = [
  {
    id: "profile",
    label: "User Profile",
    tabId: "settings-tab-profile",
    panelId: "settings-panel-profile",
  },
  {
    id: "voice",
    label: "Voice & Video",
    tabId: "settings-tab-voice",
    panelId: "settings-panel-voice",
  },
  {
    id: "emojis",
    label: "Custom Emojis",
    tabId: "settings-tab-emojis",
    panelId: "settings-panel-emojis",
  },
];

function CustomEmojiRow(props: {
  emoji: CustomEmoji;
  onRename: (id: number, name: string) => Promise<CustomEmoji>;
  onDelete: (id: number) => Promise<CustomEmoji>;
  onRestore: (id: number) => Promise<CustomEmoji>;
  status?: string | null;
}) {
  const [draft, setDraft] = createSignal(props.emoji.name);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal<string | null>(null);
  const [actionBusy, setActionBusy] = createSignal(false);
  const [actionError, setActionError] = createSignal<string | null>(null);
  const trimmedDraft = () => draft().trim();
  const draftValid = () => /^[A-Za-z0-9_]{2,32}$/.test(trimmedDraft());
  const changed = () => trimmedDraft() !== props.emoji.name;
  const canSave = () => changed() && draftValid() && !saving();

  createEffect(() => {
    setDraft(props.emoji.name);
  });

  const save = async (ev: SubmitEvent) => {
    ev.preventDefault();
    if (!canSave()) return;
    setSaving(true);
    setError(null);
    setSuccess(null);
    setActionError(null);
    try {
      const updated = await props.onRename(props.emoji.id, trimmedDraft());
      setSuccess(`Renamed to :${updated.name}:`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Emoji rename failed");
    } finally {
      setSaving(false);
    }
  };

  const requestDelete = async () => {
    const ok = window.confirm(`Delete :${props.emoji.name}:? Old messages will still render it.`);
    if (!ok) return;
    setActionBusy(true);
    setActionError(null);
    setSuccess(null);
    try {
      await props.onDelete(props.emoji.id);
      setSuccess(`Deleted :${props.emoji.name}:`);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Emoji delete failed");
    } finally {
      setActionBusy(false);
    }
  };

  const restore = async () => {
    setActionBusy(true);
    setActionError(null);
    setSuccess(null);
    try {
      const restored = await props.onRestore(props.emoji.id);
      setSuccess(`Restored :${restored.name}:`);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Emoji restore failed");
    } finally {
      setActionBusy(false);
    }
  };

  return (
    <div
      role="group"
      aria-label={`Custom emoji :${props.emoji.name}: ${props.emoji.deleted_at === null ? "active" : "deleted"}`}
      class="flex items-center gap-3 px-3 py-2"
    >
      <img
        src={resolveImageUrl(props.emoji.image_url)}
        alt={`:${props.emoji.name}:`}
        class="h-8 w-8 rounded object-contain bg-gray-900"
      />
      <div class="min-w-0 flex-1">
        <p class="font-medium text-gray-100 truncate">:{props.emoji.name}:</p>
        <p class="text-xs text-gray-500">ID {props.emoji.id}</p>
        <form class="mt-2 flex flex-wrap items-center gap-2" onSubmit={save}>
          <label for={`custom-emoji-rename-${props.emoji.id}`} class="sr-only">
            Rename :{props.emoji.name}:
          </label>
          <input
            id={`custom-emoji-rename-${props.emoji.id}`}
            type="text"
            class="w-40 rounded-md border border-gray-600 bg-gray-700 px-2 py-1 text-xs text-gray-100 focus:border-blue-500 focus:outline-none"
            value={draft()}
            onInput={(e) => {
              setError(null);
              setSuccess(null);
              setDraft(e.currentTarget.value);
            }}
            aria-label={`Rename :${props.emoji.name}:`}
            disabled={saving()}
          />
          <button
            type="submit"
            class="rounded bg-blue-600 px-2 py-1 text-xs font-medium text-white hover:bg-blue-700 disabled:opacity-50"
            disabled={!canSave()}
          >
            {saving() ? "Renaming..." : "Save rename"}
          </button>
        </form>
        <Show when={trimmedDraft().length > 0 && !draftValid()}>
          <p class="mt-1 text-xs text-red-300">Use 2–32 letters, numbers, or underscores.</p>
        </Show>
        <Show when={error()}>
          {(msg) => (
            <p role="alert" class="mt-1 text-xs text-red-300">
              {msg()}
            </p>
          )}
        </Show>
        <Show when={actionError()}>
          {(msg) => (
            <p role="alert" class="mt-1 text-xs text-red-300">
              {msg()}
            </p>
          )}
        </Show>
        <Show when={success() ?? props.status}>
          {(msg) => (
            <p role="status" class="mt-1 text-xs text-green-300">
              {msg()}
            </p>
          )}
        </Show>
      </div>
      <div class="ml-auto flex flex-col items-end gap-2">
        <div class="flex gap-2">
          <span class="rounded bg-gray-700 px-2 py-0.5 text-xs text-gray-300">
            {props.emoji.animated ? "animated" : "static"}
          </span>
          <Show when={props.emoji.deleted_at}>
            <span class="rounded bg-red-950 px-2 py-0.5 text-xs text-red-200">deleted</span>
          </Show>
        </div>
        <Show
          when={props.emoji.deleted_at !== null}
          fallback={
            <button
              type="button"
              class="rounded bg-red-900 px-2 py-1 text-xs font-medium text-red-100 hover:bg-red-800 disabled:opacity-50"
              onClick={() => void requestDelete()}
              disabled={actionBusy()}
            >
              {actionBusy() ? "Deleting..." : "Delete"}
            </button>
          }
        >
          <button
            type="button"
            class="rounded bg-green-700 px-2 py-1 text-xs font-medium text-white hover:bg-green-600 disabled:opacity-50"
            onClick={() => void restore()}
            disabled={actionBusy()}
          >
            {actionBusy() ? "Restoring..." : "Restore"}
          </button>
        </Show>
      </div>
    </div>
  );
}

function CustomEmojiSettings() {
  const registry = useCustomEmojis();
  const all = () => registry.allEmojis() ?? [];
  const active = registry.activeEmojis;
  const deleted = () => all().filter((emoji) => emoji.deleted_at !== null);
  const deletedCount = () => deleted().length;
  const [name, setName] = createSignal("");
  const [file, setFile] = createSignal<File | null>(null);
  const [previewUrl, setPreviewUrl] = createSignal<string | null>(null);
  const [uploading, setUploading] = createSignal(false);
  const [uploadError, setUploadError] = createSignal<string | null>(null);
  const [rowStatuses, setRowStatuses] = createSignal<Record<number, string>>({});
  const nameHelpId = "custom-emoji-name-help";
  const fileHelpId = "custom-emoji-file-help";
  const allowedTypes = ["image/png", "image/jpeg", "image/webp", "image/gif"];
  const trimmedName = () => name().trim();
  const nameLooksValid = () => /^[A-Za-z0-9_]{2,32}$/.test(trimmedName());
  const fileLooksValid = () => {
    const selected = file();
    return !!selected && allowedTypes.includes(selected.type);
  };
  const canSubmit = () => nameLooksValid() && fileLooksValid() && !uploading();

  let currentPreviewUrl: string | null = null;
  createEffect(() => {
    const selected = file();
    if (currentPreviewUrl) {
      URL.revokeObjectURL?.(currentPreviewUrl);
      currentPreviewUrl = null;
    }

    if (selected && typeof URL.createObjectURL === "function") {
      currentPreviewUrl = URL.createObjectURL(selected);
      setPreviewUrl(currentPreviewUrl);
    } else {
      setPreviewUrl(null);
    }
  });
  onCleanup(() => {
    if (currentPreviewUrl) URL.revokeObjectURL?.(currentPreviewUrl);
  });

  const setRowStatus = (id: number, message: string) => {
    setRowStatuses((current) => ({ ...current, [id]: message }));
  };

  const renameEmoji = async (id: number, nextName: string) => {
    const emoji = await registry.rename(id, nextName);
    setRowStatus(id, `Renamed to :${emoji.name}:`);
    return emoji;
  };

  const deleteEmoji = async (id: number) => {
    const emoji = await registry.remove(id);
    setRowStatus(id, `Deleted :${emoji.name}:`);
    return emoji;
  };

  const restoreEmoji = async (id: number) => {
    const emoji = await registry.restore(id);
    setRowStatus(id, `Restored :${emoji.name}:`);
    return emoji;
  };

  const handleFilePicked = (ev: Event & { currentTarget: HTMLInputElement }) => {
    setUploadError(null);
    setFile(ev.currentTarget.files?.[0] ?? null);
  };

  const submit = async (ev: SubmitEvent) => {
    ev.preventDefault();
    if (!canSubmit()) return;
    const selected = file();
    if (!selected) return;

    setUploading(true);
    setUploadError(null);
    try {
      await registry.create(trimmedName(), selected);
      setName("");
      setFile(null);
      const input = document.getElementById("custom-emoji-file") as HTMLInputElement | null;
      if (input) input.value = "";
    } catch (err) {
      setUploadError(err instanceof Error ? err.message : "Emoji upload failed");
    } finally {
      setUploading(false);
    }
  };

  return (
    <div class="flex flex-col gap-4" aria-live="polite">
      <div>
        <h3 class="text-base font-semibold text-gray-100">Custom Emojis</h3>
        <p class="text-xs text-gray-400">
          Upload PNG, JPEG, static WebP, animated GIF, or animated WebP files. Static uploads are
          normalized to 256×256 WebP; animated uploads keep their original animation.
        </p>
      </div>

      <form
        class="rounded-md border border-gray-700 bg-gray-800/40 p-3 flex flex-col gap-3"
        onSubmit={submit}
      >
        <div>
          <label for="custom-emoji-name" class="text-sm font-medium text-gray-200">
            Emoji name
          </label>
          <input
            id="custom-emoji-name"
            type="text"
            class="mt-1 w-full bg-gray-700 text-gray-100 rounded-md px-3 py-2 text-sm border border-gray-600 focus:border-blue-500 focus:outline-none"
            value={name()}
            onInput={(e) => {
              setUploadError(null);
              setName(e.currentTarget.value);
            }}
            aria-describedby={nameHelpId}
            disabled={uploading()}
          />
          <p id={nameHelpId} class="mt-1 text-xs text-gray-400">
            2–32 letters, numbers, or underscores.
          </p>
          <Show when={trimmedName().length > 0 && !nameLooksValid()}>
            <p class="mt-1 text-xs text-red-300">Use 2–32 letters, numbers, or underscores.</p>
          </Show>
        </div>

        <div>
          <label for="custom-emoji-file" class="text-sm font-medium text-gray-200">
            Image file
          </label>
          <input
            id="custom-emoji-file"
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif"
            class="mt-1 block w-full text-sm text-gray-200 file:mr-3 file:rounded-md file:border-0 file:bg-blue-600 file:px-3 file:py-2 file:text-sm file:font-medium file:text-white hover:file:bg-blue-700"
            aria-describedby={fileHelpId}
            onChange={handleFilePicked}
            disabled={uploading()}
          />
          <p id={fileHelpId} class="mt-1 text-xs text-gray-400">
            PNG, JPEG, static WebP, animated GIF, or animated WebP. Maximum upload size is 2 MiB.
          </p>
          <Show when={file() && !fileLooksValid()}>
            <p class="mt-1 text-xs text-red-300">
              Choose a PNG, JPEG, static WebP, animated GIF, or animated WebP image.
            </p>
          </Show>
          <Show when={file() && previewUrl()}>
            {(url) => (
              <div class="mt-2 flex items-center gap-2 rounded border border-gray-700 bg-gray-900/60 p-2">
                <img
                  src={url()}
                  alt="Selected custom emoji preview"
                  class="h-10 w-10 rounded object-contain bg-gray-950"
                />
                <p class="text-xs text-gray-400">Preview uses the original selected file.</p>
              </div>
            )}
          </Show>
        </div>

        <Show when={uploadError()}>
          {(msg) => (
            <p role="alert" class="text-sm text-red-300">
              {msg()}
            </p>
          )}
        </Show>

        <button
          type="submit"
          class="self-start bg-blue-600 hover:bg-blue-700 text-white rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50"
          disabled={!canSubmit()}
        >
          {uploading() ? "Uploading..." : "Upload emoji"}
        </button>
      </form>

      <Show when={registry.allEmojis.loading}>
        <p class="text-gray-400">Loading custom emojis...</p>
      </Show>

      <Show when={registry.error()}>
        <div role="alert" class="rounded-md border border-red-900 bg-red-950/40 p-3 text-red-200">
          <p class="font-medium">Could not load custom emojis.</p>
          <button
            type="button"
            class="mt-2 text-sm text-red-100 underline hover:text-white"
            onClick={registry.refresh}
          >
            Try again
          </button>
        </div>
      </Show>

      <Show when={!registry.allEmojis.loading && !registry.error()}>
        <Show
          when={all().length > 0}
          fallback={
            <div class="rounded-md border border-dashed border-gray-600 bg-gray-800/50 p-4">
              <p class="font-medium text-gray-100">No custom emojis yet</p>
              <p class="mt-1 text-gray-400">
                Uploaded emojis will be listed here for picker use and message rendering.
              </p>
            </div>
          }
        >
          <div class="flex flex-col gap-4">
            <section class="rounded-md border border-gray-700 divide-y divide-gray-700">
              <div class="px-3 py-2 text-xs text-gray-400">
                Active emojis: {active().length} / {all().length} total
                <Show when={deletedCount() > 0}> ({deletedCount()} deleted)</Show>
              </div>
              <Show
                when={active().length > 0}
                fallback={<p class="px-3 py-3 text-sm text-gray-400">No active custom emojis.</p>}
              >
                <For each={active()}>
                  {(emoji) => (
                    <CustomEmojiRow
                      emoji={emoji}
                      onRename={renameEmoji}
                      onDelete={deleteEmoji}
                      onRestore={restoreEmoji}
                      status={rowStatuses()[emoji.id] ?? null}
                    />
                  )}
                </For>
              </Show>
            </section>

            <section class="rounded-md border border-gray-700 divide-y divide-gray-700">
              <div class="px-3 py-2 text-xs font-medium uppercase tracking-wide text-gray-400">
                Deleted emojis
              </div>
              <Show
                when={deleted().length > 0}
                fallback={<p class="px-3 py-3 text-sm text-gray-400">No deleted custom emojis.</p>}
              >
                <For each={deleted()}>
                  {(emoji) => (
                    <CustomEmojiRow
                      emoji={emoji}
                      onRename={renameEmoji}
                      onDelete={deleteEmoji}
                      onRestore={restoreEmoji}
                      status={rowStatuses()[emoji.id] ?? null}
                    />
                  )}
                </For>
              </Show>
            </section>
          </div>
        </Show>
      </Show>
    </div>
  );
}

export default function SettingsModal(props: {
  open: boolean;
  onClose: () => void;
  onLogout: () => Promise<void>;
  user?: User | null;
  onAvatarChange?: () => void;
}) {
  const [section, setSection] = createSignal<SectionId>("profile");
  const [confirmLogout, setConfirmLogout] = createSignal(false);
  const [loggingOut, setLoggingOut] = createSignal(false);
  const [pendingFile, setPendingFile] = createSignal<File | null>(null);
  const [avatarError, setAvatarError] = createSignal<string | null>(null);
  const [removing, setRemoving] = createSignal(false);
  const [displayNameDraft, setDisplayNameDraft] = createSignal("");
  const [displayNameSaving, setDisplayNameSaving] = createSignal(false);
  const [displayNameError, setDisplayNameError] = createSignal<string | null>(null);
  const [currentPassword, setCurrentPassword] = createSignal("");
  const [newPassword, setNewPassword] = createSignal("");
  const [confirmNewPassword, setConfirmNewPassword] = createSignal("");
  const [passwordSaving, setPasswordSaving] = createSignal(false);
  const [passwordError, setPasswordError] = createSignal<string | null>(null);
  const [passwordSuccess, setPasswordSuccess] = createSignal<string | null>(null);
  const active = () => SECTIONS.find((s) => s.id === section()) ?? SECTIONS[0];
  const passwordMismatch = () =>
    confirmNewPassword().length > 0 && newPassword() !== confirmNewPassword();
  const canChangePassword = () =>
    currentPassword().length > 0 &&
    newPassword().length > 0 &&
    confirmNewPassword().length > 0 &&
    !passwordMismatch() &&
    !passwordSaving();

  // Keep the editable field in sync with the latest persisted display name
  // (e.g. when the auth context refetches after a successful save).
  createEffect(() => {
    setDisplayNameDraft(props.user?.display_name ?? "");
  });

  const handleConfirmLogout = async () => {
    setLoggingOut(true);
    try {
      await props.onLogout();
    } finally {
      setLoggingOut(false);
      setConfirmLogout(false);
    }
  };

  const handleFilePicked = (ev: Event & { currentTarget: HTMLInputElement }) => {
    setAvatarError(null);
    const file = ev.currentTarget.files?.[0];
    if (!file) return;
    setPendingFile(file);
    // Reset so picking the same file again re-opens the cropper.
    ev.currentTarget.value = "";
  };

  const handleCropSave = async (blob: Blob) => {
    try {
      await uploadAvatar(blob);
      props.onAvatarChange?.();
      setPendingFile(null);
    } catch (e) {
      setAvatarError(e instanceof Error ? e.message : "Upload failed");
      throw e;
    }
  };

  const saveDisplayName = async () => {
    const trimmed = displayNameDraft().trim();
    if (trimmed.length > DISPLAY_NAME_MAX_LEN) {
      setDisplayNameError(`Display name must be ${DISPLAY_NAME_MAX_LEN} characters or fewer`);
      return;
    }
    setDisplayNameError(null);
    setDisplayNameSaving(true);
    try {
      await updateDisplayName(trimmed.length === 0 ? null : trimmed);
      props.onAvatarChange?.();
    } catch (e) {
      setDisplayNameError(e instanceof Error ? e.message : "Save failed");
    } finally {
      setDisplayNameSaving(false);
    }
  };

  const clearDisplayName = async () => {
    setDisplayNameError(null);
    setDisplayNameSaving(true);
    try {
      await updateDisplayName(null);
      setDisplayNameDraft("");
      props.onAvatarChange?.();
    } catch (e) {
      setDisplayNameError(e instanceof Error ? e.message : "Reset failed");
    } finally {
      setDisplayNameSaving(false);
    }
  };

  const handlePasswordInput = (setter: (value: string) => void, value: string) => {
    setter(value);
    setPasswordError(null);
    setPasswordSuccess(null);
  };

  const savePassword = async () => {
    setPasswordError(null);
    setPasswordSuccess(null);

    if (
      currentPassword().length === 0 ||
      newPassword().length === 0 ||
      confirmNewPassword().length === 0
    ) {
      setPasswordError("Fill out all password fields.");
      return;
    }
    if (newPassword() !== confirmNewPassword()) {
      setPasswordError("New passwords do not match.");
      return;
    }

    setPasswordSaving(true);
    try {
      await changePassword(currentPassword(), newPassword());
      setCurrentPassword("");
      setNewPassword("");
      setConfirmNewPassword("");
      setPasswordSuccess("Password changed.");
    } catch (e) {
      const message = e instanceof Error ? e.message : "Password change failed";
      setPasswordError(
        /invalid credentials/i.test(message) ? "Current password is incorrect." : message,
      );
    } finally {
      setPasswordSaving(false);
    }
  };

  const handleRemoveAvatar = async () => {
    setAvatarError(null);
    setRemoving(true);
    try {
      await deleteAvatar();
      props.onAvatarChange?.();
    } catch (e) {
      setAvatarError(e instanceof Error ? e.message : "Remove failed");
    } finally {
      setRemoving(false);
    }
  };

  onCleanup(() => setPendingFile(null));

  return (
    <>
      <Modal open={props.open} onClose={props.onClose} title="Settings" size="lg">
        <div class="flex gap-4 min-h-64">
          <div class="flex flex-col w-40 border-r border-gray-700 pr-2">
            <div
              role="tablist"
              aria-orientation="vertical"
              aria-label="Settings sections"
              class="flex flex-col"
            >
              <For each={SECTIONS}>
                {(s) => {
                  const selected = () => section() === s.id;
                  return (
                    <button
                      type="button"
                      role="tab"
                      id={s.tabId}
                      aria-selected={selected()}
                      aria-controls={s.panelId}
                      tabIndex={selected() ? 0 : -1}
                      class={`text-left px-3 py-2 rounded text-sm mb-1 ${
                        selected()
                          ? "bg-gray-700 text-white font-medium"
                          : "text-gray-300 hover:bg-gray-700 hover:text-gray-100"
                      }`}
                      onClick={() => setSection(s.id)}
                    >
                      {s.label}
                    </button>
                  );
                }}
              </For>
            </div>
            <div class="mt-auto pt-2 border-t border-gray-700">
              <button
                type="button"
                class="w-full flex items-center gap-2 text-left px-3 py-2 rounded text-sm text-red-400 hover:bg-gray-700 hover:text-red-300"
                onClick={() => setConfirmLogout(true)}
              >
                <LogOutIcon size={16} aria-hidden="true" />
                Log Out
              </button>
            </div>
          </div>
          <div
            role="tabpanel"
            id={active().panelId}
            aria-labelledby={active().tabId}
            class="flex-1 text-sm text-gray-200"
          >
            <Switch>
              <Match when={section() === "profile"}>
                <Show when={props.user} fallback={<p class="text-gray-400">Loading profile...</p>}>
                  {(u) => (
                    <div class="flex flex-col items-start gap-4 w-full">
                      <div class="flex items-center gap-4">
                        <Avatar
                          url={u().avatar_url}
                          username={u().display_name ?? u().username}
                          size={96}
                        />
                        <div>
                          <p class="font-semibold text-base">{u().display_name ?? u().username}</p>
                          <Show when={u().display_name}>
                            <p class="text-xs text-gray-400">@{u().username}</p>
                          </Show>
                          <Show when={u().email}>
                            {(e) => <p class="text-xs text-gray-400">{e()}</p>}
                          </Show>
                        </div>
                      </div>
                      <input
                        type="file"
                        accept="image/jpeg,image/png,image/webp"
                        class="sr-only"
                        id="avatar-file-input"
                        aria-label="Choose profile picture"
                        onChange={handleFilePicked}
                      />
                      <div class="flex gap-2">
                        <label
                          for="avatar-file-input"
                          class="bg-blue-600 hover:bg-blue-700 text-white rounded-md px-4 py-2 text-sm font-medium cursor-pointer"
                        >
                          Upload picture
                        </label>
                        <Show when={u().avatar_url}>
                          <button
                            type="button"
                            class="text-red-400 hover:text-red-300 text-sm px-3 py-2 disabled:opacity-50"
                            onClick={handleRemoveAvatar}
                            disabled={removing()}
                          >
                            {removing() ? "Removing..." : "Remove picture"}
                          </button>
                        </Show>
                      </div>
                      <Show when={avatarError()}>
                        {(msg) => <p class="text-red-400 text-sm">{msg()}</p>}
                      </Show>

                      <form
                        class="flex flex-col gap-2 w-full max-w-md pt-4 border-t border-gray-700"
                        onSubmit={(e) => {
                          e.preventDefault();
                          void saveDisplayName();
                        }}
                      >
                        <label for="display-name-input" class="text-sm font-medium text-gray-200">
                          Display name
                        </label>
                        <p class="text-xs text-gray-400">
                          Shown next to your messages. Leave blank to use your username (@
                          {u().username}).
                        </p>
                        <input
                          id="display-name-input"
                          type="text"
                          class="bg-gray-700 text-gray-100 rounded-md px-3 py-2 text-sm border border-gray-600 focus:border-blue-500 focus:outline-none"
                          placeholder={u().username}
                          maxLength={DISPLAY_NAME_MAX_LEN}
                          value={displayNameDraft()}
                          onInput={(e) => setDisplayNameDraft(e.currentTarget.value)}
                          disabled={displayNameSaving()}
                        />
                        <div class="flex gap-2 items-center">
                          <button
                            type="submit"
                            class="bg-blue-600 hover:bg-blue-700 text-white rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50"
                            disabled={
                              displayNameSaving() ||
                              displayNameDraft().trim() === (u().display_name ?? "")
                            }
                          >
                            {displayNameSaving() ? "Saving..." : "Save"}
                          </button>
                          <Show when={u().display_name}>
                            <button
                              type="button"
                              class="text-gray-300 hover:text-gray-100 text-sm px-3 py-2 disabled:opacity-50"
                              onClick={() => void clearDisplayName()}
                              disabled={displayNameSaving()}
                            >
                              Reset to username
                            </button>
                          </Show>
                        </div>
                        <Show when={displayNameError()}>
                          {(msg) => <p class="text-red-400 text-sm">{msg()}</p>}
                        </Show>
                      </form>

                      <form
                        class="flex flex-col gap-3 w-full max-w-md pt-4 border-t border-gray-700"
                        aria-describedby="password-change-help"
                        onSubmit={(e) => {
                          e.preventDefault();
                          void savePassword();
                        }}
                      >
                        <div>
                          <h3 class="text-sm font-semibold text-gray-100">Change password</h3>
                          <p id="password-change-help" class="mt-1 text-xs text-gray-400">
                            Enter your current password before choosing a new one.
                          </p>
                        </div>

                        <div>
                          <label
                            for="current-password-input"
                            class="text-sm font-medium text-gray-200"
                          >
                            Current password
                          </label>
                          <input
                            id="current-password-input"
                            type="password"
                            autocomplete="current-password"
                            class="mt-1 w-full bg-gray-700 text-gray-100 rounded-md px-3 py-2 text-sm border border-gray-600 focus:border-blue-500 focus:outline-none"
                            value={currentPassword()}
                            onInput={(e) =>
                              handlePasswordInput(setCurrentPassword, e.currentTarget.value)
                            }
                            disabled={passwordSaving()}
                          />
                        </div>

                        <div>
                          <label for="new-password-input" class="text-sm font-medium text-gray-200">
                            New password
                          </label>
                          <input
                            id="new-password-input"
                            type="password"
                            autocomplete="new-password"
                            class="mt-1 w-full bg-gray-700 text-gray-100 rounded-md px-3 py-2 text-sm border border-gray-600 focus:border-blue-500 focus:outline-none"
                            value={newPassword()}
                            onInput={(e) =>
                              handlePasswordInput(setNewPassword, e.currentTarget.value)
                            }
                            disabled={passwordSaving()}
                          />
                        </div>

                        <div>
                          <label
                            for="confirm-new-password-input"
                            class="text-sm font-medium text-gray-200"
                          >
                            Confirm new password
                          </label>
                          <input
                            id="confirm-new-password-input"
                            type="password"
                            autocomplete="new-password"
                            class="mt-1 w-full bg-gray-700 text-gray-100 rounded-md px-3 py-2 text-sm border border-gray-600 focus:border-blue-500 focus:outline-none"
                            value={confirmNewPassword()}
                            onInput={(e) =>
                              handlePasswordInput(setConfirmNewPassword, e.currentTarget.value)
                            }
                            disabled={passwordSaving()}
                          />
                          <Show when={passwordMismatch()}>
                            <p role="alert" class="mt-1 text-xs text-red-300">
                              New passwords do not match.
                            </p>
                          </Show>
                        </div>

                        <button
                          type="submit"
                          class="self-start bg-blue-600 hover:bg-blue-700 text-white rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50"
                          disabled={!canChangePassword()}
                        >
                          {passwordSaving() ? "Changing..." : "Change password"}
                        </button>

                        <Show when={passwordError()}>
                          {(msg) => (
                            <p role="alert" class="text-sm text-red-300">
                              {msg()}
                            </p>
                          )}
                        </Show>
                        <Show when={passwordSuccess()}>
                          {(msg) => (
                            <p role="status" class="text-sm text-green-300">
                              {msg()}
                            </p>
                          )}
                        </Show>
                      </form>
                    </div>
                  )}
                </Show>
              </Match>
              <Match when={section() === "voice"}>
                <VoiceSettings />
              </Match>
              <Match when={section() === "emojis"}>
                <CustomEmojiSettings />
              </Match>
            </Switch>
          </div>
        </div>
      </Modal>

      <CropperDialog
        open={pendingFile() !== null}
        file={pendingFile()}
        onCancel={() => setPendingFile(null)}
        onSave={handleCropSave}
      />

      <Modal
        open={confirmLogout()}
        onClose={() => !loggingOut() && setConfirmLogout(false)}
        title="Log out?"
      >
        <p class="text-sm text-gray-200 mb-4">Are you sure you want to log out?</p>
        <Show when={loggingOut()}>
          <p class="text-sm text-gray-400 mb-2">Logging out...</p>
        </Show>
        <div class="flex gap-2 justify-end">
          <button
            type="button"
            class="text-gray-300 hover:text-gray-100 text-sm px-3 py-2 disabled:opacity-50"
            onClick={() => setConfirmLogout(false)}
            disabled={loggingOut()}
          >
            Cancel
          </button>
          <button
            type="button"
            class="bg-red-600 hover:bg-red-700 text-white rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 transition-colors"
            onClick={handleConfirmLogout}
            disabled={loggingOut()}
          >
            Log out
          </button>
        </div>
      </Modal>
    </>
  );
}
