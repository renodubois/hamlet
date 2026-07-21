import { useEffect, useRef, useState, type ChangeEvent, type FormEvent } from "react";
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
  const [draft, setDraft] = useState(props.emoji.name);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const rowIdentityRef = useRef(props.emoji.id);
  const operationRef = useRef(0);
  const operationActiveRef = useRef(false);
  const initialNameRef = useRef(props.emoji.name);
  initialNameRef.current = props.emoji.name;
  const trimmedDraft = draft.trim();
  const draftValid = /^[A-Za-z0-9_]{2,32}$/.test(trimmedDraft);
  const changed = trimmedDraft !== props.emoji.name;
  const busy = saving || actionBusy;
  const canSave = changed && draftValid && !busy;

  useEffect(() => {
    rowIdentityRef.current = props.emoji.id;
    operationRef.current += 1;
    operationActiveRef.current = false;
    setDraft(initialNameRef.current);
    setSaving(false);
    setError(null);
    setSuccess(null);
    setActionBusy(false);
    setActionError(null);
    return () => {
      rowIdentityRef.current = -1;
      operationRef.current += 1;
      operationActiveRef.current = false;
    };
  }, [props.emoji.id]);

  const save = async (ev: FormEvent<HTMLFormElement>) => {
    ev.preventDefault();
    if (!canSave || operationActiveRef.current) return;
    const rowId = props.emoji.id;
    const operation = ++operationRef.current;
    operationActiveRef.current = true;
    setSaving(true);
    setError(null);
    setSuccess(null);
    setActionError(null);
    try {
      const updated = await props.onRename(rowId, trimmedDraft);
      if (rowIdentityRef.current === rowId && operationRef.current === operation) {
        setSuccess(`Renamed to :${updated.name}:`);
      }
    } catch (err) {
      if (rowIdentityRef.current === rowId && operationRef.current === operation) {
        setError(err instanceof Error ? err.message : "Emoji rename failed");
      }
    } finally {
      if (rowIdentityRef.current === rowId && operationRef.current === operation) {
        operationActiveRef.current = false;
        setSaving(false);
      }
    }
  };

  const requestDelete = async () => {
    if (operationActiveRef.current) return;
    const rowId = props.emoji.id;
    const rowName = props.emoji.name;
    const ok = window.confirm(`Delete :${rowName}:? Old messages will still render it.`);
    if (!ok || operationActiveRef.current) return;
    const operation = ++operationRef.current;
    operationActiveRef.current = true;
    setActionBusy(true);
    setActionError(null);
    setSuccess(null);
    try {
      await props.onDelete(rowId);
      if (rowIdentityRef.current === rowId && operationRef.current === operation) {
        setSuccess(`Deleted :${rowName}:`);
      }
    } catch (err) {
      if (rowIdentityRef.current === rowId && operationRef.current === operation) {
        setActionError(err instanceof Error ? err.message : "Emoji delete failed");
      }
    } finally {
      if (rowIdentityRef.current === rowId && operationRef.current === operation) {
        operationActiveRef.current = false;
        setActionBusy(false);
      }
    }
  };

  const restore = async () => {
    if (operationActiveRef.current) return;
    const rowId = props.emoji.id;
    const operation = ++operationRef.current;
    operationActiveRef.current = true;
    setActionBusy(true);
    setActionError(null);
    setSuccess(null);
    try {
      const restored = await props.onRestore(rowId);
      if (rowIdentityRef.current === rowId && operationRef.current === operation) {
        setSuccess(`Restored :${restored.name}:`);
      }
    } catch (err) {
      if (rowIdentityRef.current === rowId && operationRef.current === operation) {
        setActionError(err instanceof Error ? err.message : "Emoji restore failed");
      }
    } finally {
      if (rowIdentityRef.current === rowId && operationRef.current === operation) {
        operationActiveRef.current = false;
        setActionBusy(false);
      }
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
            value={draft}
            onChange={(e) => {
              setError(null);
              setSuccess(null);
              setDraft(e.currentTarget.value);
            }}
            aria-label={`Rename :${props.emoji.name}:`}
            disabled={busy}
          />
          <Button type="submit" size="xs" disabled={!canSave}>
            {saving ? "Renaming..." : "Save rename"}
          </Button>
        </form>
        {trimmedDraft.length > 0 && !draftValid ? (
          <p className="mt-1 text-xs text-destructive">
            Use 2–32 letters, numbers, or underscores.
          </p>
        ) : null}
        {error ? (
          <p role="alert" className="mt-1 text-xs text-destructive">
            {error}
          </p>
        ) : null}
        {actionError ? (
          <p role="alert" className="mt-1 text-xs text-destructive">
            {actionError}
          </p>
        ) : null}
        {(success ?? props.status) ? (
          <p role="status" className="mt-1 text-xs text-green-600">
            {success ?? props.status}
          </p>
        ) : null}
      </div>
      <div className="ml-auto flex flex-col items-end gap-2">
        <div className="flex gap-2">
          <Badge variant="secondary">{props.emoji.animated ? "animated" : "static"}</Badge>
          {props.emoji.deleted_at ? <Badge variant="destructive">deleted</Badge> : null}
        </div>
        {props.emoji.deleted_at !== null ? (
          <Button type="button" size="xs" onClick={() => void restore()} disabled={busy}>
            {actionBusy ? "Restoring..." : "Restore"}
          </Button>
        ) : (
          <Button
            type="button"
            variant="destructive"
            size="xs"
            onClick={() => void requestDelete()}
            disabled={busy}
          >
            {actionBusy ? "Deleting..." : "Delete"}
          </Button>
        )}
      </div>
    </div>
  );
}

function CustomEmojiSettings() {
  const registry = useCustomEmojis();
  const all = registry.allEmojis;
  const active = registry.activeEmojis;
  const deleted = all.filter((emoji) => emoji.deleted_at !== null);
  const deletedCount = deleted.length;
  const [name, setName] = useState("");
  const [file, setFile] = useState<File | null>(null);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const [rowStatuses, setRowStatuses] = useState<Record<number, string>>({});
  const fileRef = useRef<File | null>(null);
  const uploadOperationRef = useRef(0);
  const uploadOwnerRef = useRef(false);
  const emojiIdsRef = useRef<ReadonlySet<number>>(new Set());
  emojiIdsRef.current = new Set(all.map((emoji) => emoji.id));
  const nameHelpId = "custom-emoji-name-help";
  const fileHelpId = "custom-emoji-file-help";
  const allowedTypes = ["image/png", "image/jpeg", "image/webp", "image/gif"];
  const trimmedName = name.trim();
  const nameLooksValid = /^[A-Za-z0-9_]{2,32}$/.test(trimmedName);
  const fileLooksValid = !!file && allowedTypes.includes(file.type);
  const canSubmit = nameLooksValid && fileLooksValid && !uploading;

  useEffect(() => {
    uploadOwnerRef.current = true;
    return () => {
      uploadOwnerRef.current = false;
      uploadOperationRef.current += 1;
      fileRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (!file || typeof URL.createObjectURL !== "function") {
      setPreviewUrl(null);
      return;
    }

    const ownedUrl = URL.createObjectURL(file);
    setPreviewUrl(ownedUrl);
    return () => {
      URL.revokeObjectURL?.(ownedUrl);
    };
  }, [file]);

  const setRowStatus = (id: number, message: string) => {
    setRowStatuses((current) => ({ ...current, [id]: message }));
  };

  const renameEmoji = async (id: number, nextName: string) => {
    const emoji = await registry.rename(id, nextName);
    if (uploadOwnerRef.current && emojiIdsRef.current.has(id)) {
      setRowStatus(id, `Renamed to :${emoji.name}:`);
    }
    return emoji;
  };

  const deleteEmoji = async (id: number) => {
    const emoji = await registry.remove(id);
    if (uploadOwnerRef.current && emojiIdsRef.current.has(id)) {
      setRowStatus(id, `Deleted :${emoji.name}:`);
    }
    return emoji;
  };

  const restoreEmoji = async (id: number) => {
    const emoji = await registry.restore(id);
    if (uploadOwnerRef.current && emojiIdsRef.current.has(id)) {
      setRowStatus(id, `Restored :${emoji.name}:`);
    }
    return emoji;
  };

  const handleFilePicked = (ev: ChangeEvent<HTMLInputElement>) => {
    const selected = ev.currentTarget.files?.[0] ?? null;
    uploadOperationRef.current += 1;
    fileRef.current = selected;
    setUploadError(null);
    setFile(selected);
  };

  const submit = async (ev: FormEvent<HTMLFormElement>) => {
    ev.preventDefault();
    if (!canSubmit || !file) return;
    const selected = file;
    const submittedName = trimmedName;
    const operation = ++uploadOperationRef.current;

    setUploading(true);
    setUploadError(null);
    try {
      await registry.create(submittedName, selected);
      if (
        uploadOwnerRef.current &&
        fileRef.current === selected &&
        uploadOperationRef.current === operation
      ) {
        setUploading(false);
        fileRef.current = null;
        setName("");
        setFile(null);
        const input = document.getElementById("custom-emoji-file") as HTMLInputElement | null;
        if (input) input.value = "";
      }
    } catch (err) {
      if (
        uploadOwnerRef.current &&
        fileRef.current === selected &&
        uploadOperationRef.current === operation
      ) {
        setUploadError(err instanceof Error ? err.message : "Emoji upload failed");
      }
    } finally {
      if (
        uploadOwnerRef.current &&
        fileRef.current === selected &&
        uploadOperationRef.current === operation
      ) {
        setUploading(false);
      }
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
            value={name}
            onChange={(e) => {
              setUploadError(null);
              setName(e.currentTarget.value);
            }}
            aria-describedby={nameHelpId}
            disabled={uploading}
          />
          <p id={nameHelpId} className="mt-1 text-xs text-muted-foreground">
            2–32 letters, numbers, or underscores.
          </p>
          {trimmedName.length > 0 && !nameLooksValid ? (
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
            disabled={uploading}
          />
          <p id={fileHelpId} className="mt-1 text-xs text-muted-foreground">
            PNG, JPEG, static WebP, animated GIF, or animated WebP. Maximum upload size is 2 MiB.
          </p>
          {file && !fileLooksValid ? (
            <p className="mt-1 text-xs text-destructive">
              Choose a PNG, JPEG, static WebP, animated GIF, or animated WebP image.
            </p>
          ) : null}
          {file && previewUrl ? (
            <div className="mt-2 flex items-center gap-2 rounded-md border border-border bg-background p-2">
              <img
                src={previewUrl ?? undefined}
                alt="Selected custom emoji preview"
                className="h-10 w-10 rounded-md object-contain bg-muted"
              />
              <p className="text-xs text-muted-foreground">
                Preview uses the original selected file.
              </p>
            </div>
          ) : null}
        </div>

        {uploadError ? (
          <p role="alert" className="text-sm text-destructive">
            {uploadError}
          </p>
        ) : null}

        <Button type="submit" className="self-start" disabled={!canSubmit}>
          {uploading ? "Uploading..." : "Upload emoji"}
        </Button>
      </form>

      {registry.status === "idle" || registry.status === "loading" ? (
        <p className="text-muted-foreground">Loading custom emojis...</p>
      ) : null}

      {registry.error ? (
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

      {registry.status === "ready" && !registry.error ? (
        all.length > 0 ? (
          <div className="flex flex-col gap-4">
            <section className="rounded-md border border-border divide-y divide-border">
              <div className="px-3 py-2 text-xs text-muted-foreground">
                Active emojis: {active.length} / {all.length} total
                {deletedCount > 0 ? ` (${deletedCount} deleted)` : null}
              </div>
              {active.length > 0 ? (
                active.map((emoji) => (
                  <CustomEmojiRow
                    key={emoji.id}
                    emoji={emoji}
                    onRename={renameEmoji}
                    onDelete={deleteEmoji}
                    onRestore={restoreEmoji}
                    status={rowStatuses[emoji.id] ?? null}
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
              {deleted.length > 0 ? (
                deleted.map((emoji) => (
                  <CustomEmojiRow
                    key={emoji.id}
                    emoji={emoji}
                    onRename={renameEmoji}
                    onDelete={deleteEmoji}
                    onRestore={restoreEmoji}
                    status={rowStatuses[emoji.id] ?? null}
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
  const [section, setSection] = useState<SectionId>("profile");
  const [confirmLogout, setConfirmLogout] = useState(false);
  const [loggingOut, setLoggingOut] = useState(false);
  const [pendingFile, setPendingFile] = useState<File | null>(null);
  const [avatarError, setAvatarError] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);
  const [displayNameDraft, setDisplayNameDraft] = useState(props.user?.display_name ?? "");
  const [displayNameSaving, setDisplayNameSaving] = useState(false);
  const [displayNameError, setDisplayNameError] = useState<string | null>(null);
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmNewPassword, setConfirmNewPassword] = useState("");
  const [passwordSaving, setPasswordSaving] = useState(false);
  const [passwordError, setPasswordError] = useState<string | null>(null);
  const [passwordSuccess, setPasswordSuccess] = useState<string | null>(null);
  const active = SECTIONS.find((s) => s.id === section) ?? SECTIONS[0];
  const passwordMismatch = confirmNewPassword.length > 0 && newPassword !== confirmNewPassword;
  const canChangePassword =
    currentPassword.length > 0 &&
    newPassword.length > 0 &&
    confirmNewPassword.length > 0 &&
    !passwordMismatch &&
    !passwordSaving;
  const userIdentityRef = useRef(props.user?.id ?? null);
  const nextUserDraftRef = useRef(props.user?.display_name ?? "");
  const pendingFileRef = useRef<File | null>(null);
  const logoutOperationRef = useRef(0);
  const avatarOperationRef = useRef(0);
  const displayNameOperationRef = useRef(0);
  const passwordOperationRef = useRef(0);
  nextUserDraftRef.current = props.user?.display_name ?? "";

  useEffect(() => {
    userIdentityRef.current = props.user?.id ?? null;
    logoutOperationRef.current += 1;
    avatarOperationRef.current += 1;
    displayNameOperationRef.current += 1;
    passwordOperationRef.current += 1;
    pendingFileRef.current = null;
    setPendingFile(null);
    setAvatarError(null);
    setRemoving(false);
    setDisplayNameDraft(nextUserDraftRef.current);
    setDisplayNameSaving(false);
    setDisplayNameError(null);
    setCurrentPassword("");
    setNewPassword("");
    setConfirmNewPassword("");
    setPasswordSaving(false);
    setPasswordError(null);
    setPasswordSuccess(null);
    return () => {
      userIdentityRef.current = null;
      pendingFileRef.current = null;
      logoutOperationRef.current += 1;
      avatarOperationRef.current += 1;
      displayNameOperationRef.current += 1;
      passwordOperationRef.current += 1;
    };
  }, [props.user?.id]);

  const handleConfirmLogout = async () => {
    const userId = props.user?.id ?? null;
    const operation = ++logoutOperationRef.current;
    setLoggingOut(true);
    try {
      await props.onLogout();
    } finally {
      if (userIdentityRef.current === userId && logoutOperationRef.current === operation) {
        setLoggingOut(false);
        setConfirmLogout(false);
      }
    }
  };

  const handleFilePicked = (ev: ChangeEvent<HTMLInputElement>) => {
    setAvatarError(null);
    const file = ev.currentTarget.files?.[0];
    if (!file) return;
    pendingFileRef.current = file;
    setPendingFile(file);
    // Reset so picking the same file again re-opens the cropper.
    ev.currentTarget.value = "";
  };

  const handleCropSave = async (blob: Blob) => {
    const userId = props.user?.id ?? null;
    const selectedFile = pendingFile;
    const operation = ++avatarOperationRef.current;
    try {
      await uploadAvatar(blob);
      if (
        userIdentityRef.current === userId &&
        pendingFileRef.current === selectedFile &&
        avatarOperationRef.current === operation
      ) {
        props.onAvatarChange?.();
        pendingFileRef.current = null;
        setPendingFile(null);
      }
    } catch (e) {
      if (userIdentityRef.current === userId && avatarOperationRef.current === operation) {
        setAvatarError(e instanceof Error ? e.message : "Upload failed");
      }
      throw e;
    }
  };

  const saveDisplayName = async () => {
    const trimmed = displayNameDraft.trim();
    if (trimmed.length > DISPLAY_NAME_MAX_LEN) {
      setDisplayNameError(`Display name must be ${DISPLAY_NAME_MAX_LEN} characters or fewer`);
      return;
    }
    const userId = props.user?.id ?? null;
    const operation = ++displayNameOperationRef.current;
    setDisplayNameError(null);
    setDisplayNameSaving(true);
    try {
      await updateDisplayName(trimmed.length === 0 ? null : trimmed);
      if (userIdentityRef.current === userId && displayNameOperationRef.current === operation) {
        props.onAvatarChange?.();
      }
    } catch (e) {
      if (userIdentityRef.current === userId && displayNameOperationRef.current === operation) {
        setDisplayNameError(e instanceof Error ? e.message : "Save failed");
      }
    } finally {
      if (userIdentityRef.current === userId && displayNameOperationRef.current === operation) {
        setDisplayNameSaving(false);
      }
    }
  };

  const clearDisplayName = async () => {
    const userId = props.user?.id ?? null;
    const operation = ++displayNameOperationRef.current;
    setDisplayNameError(null);
    setDisplayNameSaving(true);
    try {
      await updateDisplayName(null);
      if (userIdentityRef.current === userId && displayNameOperationRef.current === operation) {
        setDisplayNameDraft("");
        props.onAvatarChange?.();
      }
    } catch (e) {
      if (userIdentityRef.current === userId && displayNameOperationRef.current === operation) {
        setDisplayNameError(e instanceof Error ? e.message : "Reset failed");
      }
    } finally {
      if (userIdentityRef.current === userId && displayNameOperationRef.current === operation) {
        setDisplayNameSaving(false);
      }
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
      currentPassword.length === 0 ||
      newPassword.length === 0 ||
      confirmNewPassword.length === 0
    ) {
      setPasswordError("Fill out all password fields.");
      return;
    }
    if (newPassword !== confirmNewPassword) {
      setPasswordError("New passwords do not match.");
      return;
    }

    const userId = props.user?.id ?? null;
    const submittedCurrentPassword = currentPassword;
    const submittedNewPassword = newPassword;
    const operation = ++passwordOperationRef.current;
    setPasswordSaving(true);
    try {
      await changePassword(submittedCurrentPassword, submittedNewPassword);
      if (userIdentityRef.current === userId && passwordOperationRef.current === operation) {
        setCurrentPassword("");
        setNewPassword("");
        setConfirmNewPassword("");
        setPasswordSuccess("Password changed.");
      }
    } catch (e) {
      if (userIdentityRef.current === userId && passwordOperationRef.current === operation) {
        const message = e instanceof Error ? e.message : "Password change failed";
        setPasswordError(
          /invalid credentials/i.test(message) ? "Current password is incorrect." : message,
        );
      }
    } finally {
      if (userIdentityRef.current === userId && passwordOperationRef.current === operation) {
        setPasswordSaving(false);
      }
    }
  };

  const handleRemoveAvatar = async () => {
    const userId = props.user?.id ?? null;
    const operation = ++avatarOperationRef.current;
    setAvatarError(null);
    setRemoving(true);
    try {
      await deleteAvatar();
      if (userIdentityRef.current === userId && avatarOperationRef.current === operation) {
        props.onAvatarChange?.();
      }
    } catch (e) {
      if (userIdentityRef.current === userId && avatarOperationRef.current === operation) {
        setAvatarError(e instanceof Error ? e.message : "Remove failed");
      }
    } finally {
      if (userIdentityRef.current === userId && avatarOperationRef.current === operation) {
        setRemoving(false);
      }
    }
  };

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
                const selected = section === s.id;
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
            id={active.panelId}
            aria-labelledby={active.tabId}
            className="flex-1 text-sm text-foreground"
          >
            {section === "profile" ? (
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
                        disabled={removing}
                      >
                        {removing ? "Removing..." : "Remove picture"}
                      </Button>
                    ) : null}
                  </div>
                  {avatarError ? <p className="text-destructive text-sm">{avatarError}</p> : null}

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
                      value={displayNameDraft}
                      onChange={(e) => setDisplayNameDraft(e.currentTarget.value)}
                      disabled={displayNameSaving}
                    />
                    <div className="flex gap-2 items-center">
                      <Button
                        type="submit"
                        disabled={
                          displayNameSaving ||
                          displayNameDraft.trim() === (props.user.display_name ?? "")
                        }
                      >
                        {displayNameSaving ? "Saving..." : "Save"}
                      </Button>
                      {props.user.display_name ? (
                        <Button
                          type="button"
                          variant="ghost"
                          onClick={() => void clearDisplayName()}
                          disabled={displayNameSaving}
                        >
                          Reset to username
                        </Button>
                      ) : null}
                    </div>
                    {displayNameError ? (
                      <p className="text-destructive text-sm">{displayNameError}</p>
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
                        value={currentPassword}
                        onChange={(e) =>
                          handlePasswordInput(setCurrentPassword, e.currentTarget.value)
                        }
                        disabled={passwordSaving}
                      />
                    </div>

                    <div>
                      <Label htmlFor="new-password-input">New password</Label>
                      <Input
                        id="new-password-input"
                        type="password"
                        autoComplete="new-password"
                        className="mt-1"
                        value={newPassword}
                        onChange={(e) => handlePasswordInput(setNewPassword, e.currentTarget.value)}
                        disabled={passwordSaving}
                      />
                    </div>

                    <div>
                      <Label htmlFor="confirm-new-password-input">Confirm new password</Label>
                      <Input
                        id="confirm-new-password-input"
                        type="password"
                        autoComplete="new-password"
                        className="mt-1"
                        value={confirmNewPassword}
                        onChange={(e) =>
                          handlePasswordInput(setConfirmNewPassword, e.currentTarget.value)
                        }
                        disabled={passwordSaving}
                      />
                      {passwordMismatch ? (
                        <p role="alert" className="mt-1 text-xs text-destructive">
                          New passwords do not match.
                        </p>
                      ) : null}
                    </div>

                    <Button type="submit" className="self-start" disabled={!canChangePassword}>
                      {passwordSaving ? "Changing..." : "Change password"}
                    </Button>

                    {passwordError ? (
                      <p role="alert" className="text-sm text-destructive">
                        {passwordError}
                      </p>
                    ) : null}
                    {passwordSuccess ? (
                      <p role="status" className="text-sm text-green-600">
                        {passwordSuccess}
                      </p>
                    ) : null}
                  </form>
                </div>
              ) : (
                <p className="text-muted-foreground">Loading profile...</p>
              )
            ) : section === "voice" ? (
              <VoiceSettings />
            ) : (
              <CustomEmojiSettings />
            )}
          </div>
        </div>
      </Modal>

      <CropperDialog
        open={pendingFile !== null}
        file={pendingFile}
        onCancel={() => {
          avatarOperationRef.current += 1;
          pendingFileRef.current = null;
          setPendingFile(null);
        }}
        onSave={handleCropSave}
      />

      <Modal
        open={confirmLogout}
        onClose={() => !loggingOut && setConfirmLogout(false)}
        title="Log out?"
      >
        <p className="text-sm text-foreground mb-4">Are you sure you want to log out?</p>
        {loggingOut ? <p className="text-sm text-muted-foreground mb-2">Logging out...</p> : null}
        <div className="flex gap-2 justify-end">
          <Button
            type="button"
            variant="ghost"
            onClick={() => setConfirmLogout(false)}
            disabled={loggingOut}
          >
            Cancel
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={handleConfirmLogout}
            disabled={loggingOut}
          >
            Log out
          </Button>
        </div>
      </Modal>
    </>
  );
}
