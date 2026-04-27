import { createEffect, createSignal, For, Match, onCleanup, Show, Switch } from "solid-js";
import { deleteAvatar, updateDisplayName, uploadAvatar, type User } from "../api";
import { DISPLAY_NAME_MAX_LEN } from "../constants";
import Avatar from "./avatar";
import CropperDialog from "./cropper-dialog";
import { LogOutIcon } from "./icons";
import Modal from "./modal";
import VoiceSettings from "./voice-settings";

type SectionId = "profile" | "voice";

interface Section {
  id: SectionId;
  label: string;
  tabId: string;
  panelId: string;
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
];

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
  const active = () => SECTIONS.find((s) => s.id === section()) ?? SECTIONS[0];

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
                    </div>
                  )}
                </Show>
              </Match>
              <Match when={section() === "voice"}>
                <VoiceSettings />
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
