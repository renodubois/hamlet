import { useRef, type ChangeEvent, type FormEvent } from "react";

import { useAfterRenderEffect, useSignalState, registerCleanup } from "../hooks/react-state";
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
import { Badge } from "./ui/badge";
import { Button, buttonVariants } from "./ui/button";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import { cn } from "../lib/utils";
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
  const [draft, setDraft] = useSignalState(props.emoji.name);
  const [saving, setSaving] = useSignalState(false);
  const [error, setError] = useSignalState<string | null>(null);
  const [success, setSuccess] = useSignalState<string | null>(null);
  const [actionBusy, setActionBusy] = useSignalState(false);
  const [actionError, setActionError] = useSignalState<string | null>(null);
  const trimmedDraft = () => draft().trim();
  const draftValid = () => /^[A-Za-z0-9_]{2,32}$/.test(trimmedDraft());
  const changed = () => trimmedDraft() !== props.emoji.name;
  const canSave = () => changed() && draftValid() && !saving();

  const previousEmojiNameRef = useRef(props.emoji.name);
  useAfterRenderEffect(() => {
    if (previousEmojiNameRef.current === props.emoji.name) return;
    previousEmojiNameRef.current = props.emoji.name;
    setDraft(props.emoji.name);
  });

  const save = async (ev: FormEvent<HTMLFormElement>) => {
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
      className="flex items-center gap-3 px-3 py-2"
    >
      <img
        src={resolveImageUrl(props.emoji.image_url)}
        alt={`:${props.emoji.name}:`}
        className="h-8 w-8 rounded-md object-contain bg-muted"
      />
      <div className="min-w-0 flex-1">
        <p className="font-medium text-foreground truncate">:{props.emoji.name}:</p>
        <p className="text-xs text-muted-foreground">ID {props.emoji.id}</p>
        <form className="mt-2 flex flex-wrap items-center gap-2" onSubmit={save}>
          <label htmlFor={`custom-emoji-rename-${props.emoji.id}`} className="sr-only">
            Rename :{props.emoji.name}:
          </label>
          <Input
            id={`custom-emoji-rename-${props.emoji.id}`}
            type="text"
            className="h-7 w-40 px-2 text-xs md:text-xs"
            value={draft()}
            onChange={(e) => {
              setError(null);
              setSuccess(null);
              setDraft(e.currentTarget.value);
            }}
            aria-label={`Rename :${props.emoji.name}:`}
            disabled={saving()}
          />
          <Button type="submit" size="xs" disabled={!canSave()}>
            {saving() ? "Renaming..." : "Save rename"}
          </Button>
        </form>
        {trimmedDraft().length > 0 && !draftValid() ? (
          <p className="mt-1 text-xs text-destructive">
            Use 2–32 letters, numbers, or underscores.
          </p>
        ) : null}
        {error() ? (
          <p role="alert" className="mt-1 text-xs text-destructive">
            {error()}
          </p>
        ) : null}
        {actionError() ? (
          <p role="alert" className="mt-1 text-xs text-destructive">
            {actionError()}
          </p>
        ) : null}
        {(success() ?? props.status) ? (
          <p role="status" className="mt-1 text-xs text-green-600">
            {success() ?? props.status}
          </p>
        ) : null}
      </div>
      <div className="ml-auto flex flex-col items-end gap-2">
        <div className="flex gap-2">
          <Badge variant="secondary">{props.emoji.animated ? "animated" : "static"}</Badge>
          {props.emoji.deleted_at ? <Badge variant="destructive">deleted</Badge> : null}
        </div>
        {props.emoji.deleted_at !== null ? (
          <Button type="button" size="xs" onClick={() => void restore()} disabled={actionBusy()}>
            {actionBusy() ? "Restoring..." : "Restore"}
          </Button>
        ) : (
          <Button
            type="button"
            variant="destructive"
            size="xs"
            onClick={() => void requestDelete()}
            disabled={actionBusy()}
          >
            {actionBusy() ? "Deleting..." : "Delete"}
          </Button>
        )}
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
  const [name, setName] = useSignalState("");
  const [file, setFile] = useSignalState<File | null>(null);
  const [previewUrl, setPreviewUrl] = useSignalState<string | null>(null);
  const [uploading, setUploading] = useSignalState(false);
  const [uploadError, setUploadError] = useSignalState<string | null>(null);
  const [rowStatuses, setRowStatuses] = useSignalState<Record<number, string>>({});
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

  const previewRef = useRef<{ file: File; url: string } | null>(null);
  useAfterRenderEffect(() => {
    const selected = file();
    if (previewRef.current?.file === selected) return;
    if (previewRef.current) {
      URL.revokeObjectURL?.(previewRef.current.url);
      previewRef.current = null;
    }

    if (selected && typeof URL.createObjectURL === "function") {
      const url = URL.createObjectURL(selected);
      previewRef.current = { file: selected, url };
      setPreviewUrl(url);
    } else {
      setPreviewUrl(null);
    }
  });
  registerCleanup(() => {
    if (previewRef.current) URL.revokeObjectURL?.(previewRef.current.url);
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

  const handleFilePicked = (ev: ChangeEvent<HTMLInputElement>) => {
    setUploadError(null);
    setFile(ev.currentTarget.files?.[0] ?? null);
  };

  const submit = async (ev: FormEvent<HTMLFormElement>) => {
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
    <div className="flex flex-col gap-4" aria-live="polite">
      <div>
        <h3 className="text-base font-semibold text-foreground">Custom Emojis</h3>
        <p className="text-xs text-muted-foreground">
          Upload PNG, JPEG, static WebP, animated GIF, or animated WebP files. Static uploads are
          normalized to 256×256 WebP; animated uploads keep their original animation.
        </p>
      </div>

      <form
        className="rounded-md border border-border bg-muted p-3 flex flex-col gap-3"
        onSubmit={submit}
      >
        <div>
          <Label htmlFor="custom-emoji-name">Emoji name</Label>
          <Input
            id="custom-emoji-name"
            type="text"
            className="mt-1"
            value={name()}
            onChange={(e) => {
              setUploadError(null);
              setName(e.currentTarget.value);
            }}
            aria-describedby={nameHelpId}
            disabled={uploading()}
          />
          <p id={nameHelpId} className="mt-1 text-xs text-muted-foreground">
            2–32 letters, numbers, or underscores.
          </p>
          {trimmedName().length > 0 && !nameLooksValid() ? (
            <p className="mt-1 text-xs text-destructive">
              Use 2–32 letters, numbers, or underscores.
            </p>
          ) : null}
        </div>

        <div>
          <Label htmlFor="custom-emoji-file">Image file</Label>
          <Input
            id="custom-emoji-file"
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif"
            className="mt-1"
            aria-describedby={fileHelpId}
            onChange={handleFilePicked}
            disabled={uploading()}
          />
          <p id={fileHelpId} className="mt-1 text-xs text-muted-foreground">
            PNG, JPEG, static WebP, animated GIF, or animated WebP. Maximum upload size is 2 MiB.
          </p>
          {file() && !fileLooksValid() ? (
            <p className="mt-1 text-xs text-destructive">
              Choose a PNG, JPEG, static WebP, animated GIF, or animated WebP image.
            </p>
          ) : null}
          {file() && previewUrl() ? (
            <div className="mt-2 flex items-center gap-2 rounded-md border border-border bg-background p-2">
              <img
                src={previewUrl() ?? undefined}
                alt="Selected custom emoji preview"
                className="h-10 w-10 rounded-md object-contain bg-muted"
              />
              <p className="text-xs text-muted-foreground">
                Preview uses the original selected file.
              </p>
            </div>
          ) : null}
        </div>

        {uploadError() ? (
          <p role="alert" className="text-sm text-destructive">
            {uploadError()}
          </p>
        ) : null}

        <Button type="submit" className="self-start" disabled={!canSubmit()}>
          {uploading() ? "Uploading..." : "Upload emoji"}
        </Button>
      </form>

      {registry.allEmojis.loading ? (
        <p className="text-muted-foreground">Loading custom emojis...</p>
      ) : null}

      {registry.error() ? (
        <div
          role="alert"
          className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-destructive"
        >
          <p className="font-medium">Could not load custom emojis.</p>
          <button
            type="button"
            className="mt-2 text-sm text-destructive underline hover:text-destructive/80 transition-colors"
            onClick={registry.refresh}
          >
            Try again
          </button>
        </div>
      ) : null}

      {!registry.allEmojis.loading && !registry.error() ? (
        all().length > 0 ? (
          <div className="flex flex-col gap-4">
            <section className="rounded-md border border-border divide-y divide-border">
              <div className="px-3 py-2 text-xs text-muted-foreground">
                Active emojis: {active().length} / {all().length} total
                {deletedCount() > 0 ? ` (${deletedCount()} deleted)` : null}
              </div>
              {active().length > 0 ? (
                active().map((emoji) => (
                  <CustomEmojiRow
                    key={emoji.id}
                    emoji={emoji}
                    onRename={renameEmoji}
                    onDelete={deleteEmoji}
                    onRestore={restoreEmoji}
                    status={rowStatuses()[emoji.id] ?? null}
                  />
                ))
              ) : (
                <p className="px-3 py-3 text-sm text-muted-foreground">No active custom emojis.</p>
              )}
            </section>

            <section className="rounded-md border border-border divide-y divide-border">
              <div className="px-3 py-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                Deleted emojis
              </div>
              {deleted().length > 0 ? (
                deleted().map((emoji) => (
                  <CustomEmojiRow
                    key={emoji.id}
                    emoji={emoji}
                    onRename={renameEmoji}
                    onDelete={deleteEmoji}
                    onRestore={restoreEmoji}
                    status={rowStatuses()[emoji.id] ?? null}
                  />
                ))
              ) : (
                <p className="px-3 py-3 text-sm text-muted-foreground">No deleted custom emojis.</p>
              )}
            </section>
          </div>
        ) : (
          <div className="rounded-md border border-dashed border-border bg-muted p-4">
            <p className="font-medium text-foreground">No custom emojis yet</p>
            <p className="mt-1 text-muted-foreground">
              Uploaded emojis will be listed here for picker use and message rendering.
            </p>
          </div>
        )
      ) : null}
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
  const [section, setSection] = useSignalState<SectionId>("profile");
  const [confirmLogout, setConfirmLogout] = useSignalState(false);
  const [loggingOut, setLoggingOut] = useSignalState(false);
  const [pendingFile, setPendingFile] = useSignalState<File | null>(null);
  const [avatarError, setAvatarError] = useSignalState<string | null>(null);
  const [removing, setRemoving] = useSignalState(false);
  const [displayNameDraft, setDisplayNameDraft] = useSignalState(props.user?.display_name ?? "");
  const [displayNameSaving, setDisplayNameSaving] = useSignalState(false);
  const [displayNameError, setDisplayNameError] = useSignalState<string | null>(null);
  const [currentPassword, setCurrentPassword] = useSignalState("");
  const [newPassword, setNewPassword] = useSignalState("");
  const [confirmNewPassword, setConfirmNewPassword] = useSignalState("");
  const [passwordSaving, setPasswordSaving] = useSignalState(false);
  const [passwordError, setPasswordError] = useSignalState<string | null>(null);
  const [passwordSuccess, setPasswordSuccess] = useSignalState<string | null>(null);
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
  const previousDisplayNameRef = useRef(props.user?.display_name ?? "");
  useAfterRenderEffect(() => {
    const displayName = props.user?.display_name ?? "";
    if (previousDisplayNameRef.current === displayName) return;
    previousDisplayNameRef.current = displayName;
    setDisplayNameDraft(displayName);
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

  const handleFilePicked = (ev: ChangeEvent<HTMLInputElement>) => {
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

  registerCleanup(() => setPendingFile(null));

  return (
    <>
      <Modal open={props.open} onClose={props.onClose} title="Settings" size="lg">
        <div className="flex gap-4 min-h-64">
          <div className="flex flex-col w-40 border-r border-border pr-2">
            <div
              role="tablist"
              aria-orientation="vertical"
              aria-label="Settings sections"
              className="flex flex-col"
            >
              {SECTIONS.map((s) => {
                const selected = section() === s.id;
                return (
                  <button
                    key={s.id}
                    type="button"
                    role="tab"
                    id={s.tabId}
                    aria-selected={selected}
                    aria-controls={s.panelId}
                    tabIndex={selected ? 0 : -1}
                    className={`text-left px-3 py-2 rounded-md text-sm mb-1 transition-colors ${
                      selected
                        ? "bg-primary/10 text-primary font-medium"
                        : "text-muted-foreground hover:bg-accent hover:text-foreground"
                    }`}
                    onClick={() => setSection(s.id)}
                  >
                    {s.label}
                  </button>
                );
              })}
            </div>
            <div className="mt-auto pt-2 border-t border-border">
              <button
                type="button"
                className="w-full flex items-center gap-2 text-left px-3 py-2 rounded-md text-sm text-destructive hover:bg-destructive/10 transition-colors"
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
            className="flex-1 text-sm text-foreground"
          >
            {section() === "profile" ? (
              props.user ? (
                <div className="flex flex-col items-start gap-4 w-full">
                  <div className="flex items-center gap-4">
                    <Avatar
                      url={props.user.avatar_url}
                      username={props.user.display_name ?? props.user.username}
                      size={96}
                    />
                    <div>
                      <p className="font-semibold text-base">
                        {props.user.display_name ?? props.user.username}
                      </p>
                      {props.user.display_name ? (
                        <p className="text-xs text-muted-foreground">@{props.user.username}</p>
                      ) : null}
                      {props.user.email ? (
                        <p className="text-xs text-muted-foreground">{props.user.email}</p>
                      ) : null}
                    </div>
                  </div>
                  <input
                    type="file"
                    accept="image/jpeg,image/png,image/webp"
                    className="sr-only"
                    id="avatar-file-input"
                    aria-label="Choose profile picture"
                    onChange={handleFilePicked}
                  />
                  <div className="flex gap-2">
                    <label
                      htmlFor="avatar-file-input"
                      className={cn(buttonVariants(), "cursor-pointer")}
                    >
                      Upload picture
                    </label>
                    {props.user.avatar_url ? (
                      <Button
                        type="button"
                        variant="destructive"
                        onClick={handleRemoveAvatar}
                        disabled={removing()}
                      >
                        {removing() ? "Removing..." : "Remove picture"}
                      </Button>
                    ) : null}
                  </div>
                  {avatarError() ? (
                    <p className="text-destructive text-sm">{avatarError()}</p>
                  ) : null}

                  <form
                    className="flex flex-col gap-2 w-full max-w-md pt-4 border-t border-border"
                    onSubmit={(e) => {
                      e.preventDefault();
                      void saveDisplayName();
                    }}
                  >
                    <Label htmlFor="display-name-input">Display name</Label>
                    <p className="text-xs text-muted-foreground">
                      Ifn next to your messages. Leave blank to use your username (@
                      {props.user.username}).
                    </p>
                    <Input
                      id="display-name-input"
                      type="text"
                      placeholder={props.user.username}
                      maxLength={DISPLAY_NAME_MAX_LEN}
                      value={displayNameDraft()}
                      onChange={(e) => setDisplayNameDraft(e.currentTarget.value)}
                      disabled={displayNameSaving()}
                    />
                    <div className="flex gap-2 items-center">
                      <Button
                        type="submit"
                        disabled={
                          displayNameSaving() ||
                          displayNameDraft().trim() === (props.user.display_name ?? "")
                        }
                      >
                        {displayNameSaving() ? "Saving..." : "Save"}
                      </Button>
                      {props.user.display_name ? (
                        <Button
                          type="button"
                          variant="ghost"
                          onClick={() => void clearDisplayName()}
                          disabled={displayNameSaving()}
                        >
                          Reset to username
                        </Button>
                      ) : null}
                    </div>
                    {displayNameError() ? (
                      <p className="text-destructive text-sm">{displayNameError()}</p>
                    ) : null}
                  </form>

                  <form
                    className="flex flex-col gap-3 w-full max-w-md pt-4 border-t border-border"
                    aria-describedby="password-change-help"
                    onSubmit={(e) => {
                      e.preventDefault();
                      void savePassword();
                    }}
                  >
                    <div>
                      <h3 className="text-sm font-semibold text-foreground">Change password</h3>
                      <p id="password-change-help" className="mt-1 text-xs text-muted-foreground">
                        Enter your current password before choosing a new one.
                      </p>
                    </div>

                    <div>
                      <Label htmlFor="current-password-input">Current password</Label>
                      <Input
                        id="current-password-input"
                        type="password"
                        autoComplete="current-password"
                        className="mt-1"
                        value={currentPassword()}
                        onChange={(e) =>
                          handlePasswordInput(setCurrentPassword, e.currentTarget.value)
                        }
                        disabled={passwordSaving()}
                      />
                    </div>

                    <div>
                      <Label htmlFor="new-password-input">New password</Label>
                      <Input
                        id="new-password-input"
                        type="password"
                        autoComplete="new-password"
                        className="mt-1"
                        value={newPassword()}
                        onChange={(e) => handlePasswordInput(setNewPassword, e.currentTarget.value)}
                        disabled={passwordSaving()}
                      />
                    </div>

                    <div>
                      <Label htmlFor="confirm-new-password-input">Confirm new password</Label>
                      <Input
                        id="confirm-new-password-input"
                        type="password"
                        autoComplete="new-password"
                        className="mt-1"
                        value={confirmNewPassword()}
                        onChange={(e) =>
                          handlePasswordInput(setConfirmNewPassword, e.currentTarget.value)
                        }
                        disabled={passwordSaving()}
                      />
                      {passwordMismatch() ? (
                        <p role="alert" className="mt-1 text-xs text-destructive">
                          New passwords do not match.
                        </p>
                      ) : null}
                    </div>

                    <Button type="submit" className="self-start" disabled={!canChangePassword()}>
                      {passwordSaving() ? "Changing..." : "Change password"}
                    </Button>

                    {passwordError() ? (
                      <p role="alert" className="text-sm text-destructive">
                        {passwordError()}
                      </p>
                    ) : null}
                    {passwordSuccess() ? (
                      <p role="status" className="text-sm text-green-600">
                        {passwordSuccess()}
                      </p>
                    ) : null}
                  </form>
                </div>
              ) : (
                <p className="text-muted-foreground">Loading profile...</p>
              )
            ) : section() === "voice" ? (
              <VoiceSettings />
            ) : (
              <CustomEmojiSettings />
            )}
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
        <p className="text-sm text-foreground mb-4">Are you sure you want to log out?</p>
        {loggingOut() ? <p className="text-sm text-muted-foreground mb-2">Logging out...</p> : null}
        <div className="flex gap-2 justify-end">
          <Button
            type="button"
            variant="ghost"
            onClick={() => setConfirmLogout(false)}
            disabled={loggingOut()}
          >
            Cancel
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={handleConfirmLogout}
            disabled={loggingOut()}
          >
            Log out
          </Button>
        </div>
      </Modal>
    </>
  );
}
