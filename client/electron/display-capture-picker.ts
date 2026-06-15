import { BrowserWindow, type BrowserWindowConstructorOptions } from "electron";
import {
  createDisplayCapturePickerHtml,
  parseDisplayCapturePickerAction,
  type DisplayCapturePicker,
  type DisplayCaptureSourceChoice,
} from "./display-capture";

const PICKER_WINDOW_OPTIONS = {
  title: "Share screen",
  width: 760,
  height: 560,
  minWidth: 560,
  minHeight: 420,
  show: false,
  backgroundColor: "#111827",
  resizable: true,
  minimizable: false,
  maximizable: false,
  fullscreenable: false,
  webPreferences: {
    nodeIntegration: false,
    contextIsolation: true,
    sandbox: true,
    webSecurity: true,
    allowRunningInsecureContent: false,
    webviewTag: false,
    devTools: false,
  },
} satisfies BrowserWindowConstructorOptions;

export function createFallbackDisplayCapturePicker(
  getParentWindow: () => BrowserWindow | null,
): DisplayCapturePicker {
  return (choices) => showFallbackDisplayCapturePicker(choices, getParentWindow());
}

export function showFallbackDisplayCapturePicker(
  choices: readonly DisplayCaptureSourceChoice[],
  parentWindow: BrowserWindow | null,
): Promise<DisplayCaptureSourceChoice | null> {
  if (choices.length === 0) return Promise.resolve(null);

  return new Promise<DisplayCaptureSourceChoice | null>((resolve) => {
    let settled = false;
    const parent = parentWindow !== null && !parentWindow.isDestroyed() ? parentWindow : undefined;
    const pickerWindow = new BrowserWindow({
      ...PICKER_WINDOW_OPTIONS,
      ...(parent === undefined ? {} : { parent, modal: true }),
    });
    const pickerUrl = `data:text/html;charset=utf-8,${encodeURIComponent(
      createDisplayCapturePickerHtml(choices),
    )}`;

    function finish(choice: DisplayCaptureSourceChoice | null): void {
      if (settled) return;
      settled = true;
      resolve(choice);
      if (!pickerWindow.isDestroyed()) pickerWindow.close();
    }

    function handleActionUrl(url: string): boolean {
      const action = parseDisplayCapturePickerAction(url);
      if (action === null) return false;
      if (action.action === "cancel") {
        finish(null);
        return true;
      }

      const choice = choices.find((candidate) => candidate.index === action.index) ?? null;
      finish(choice);
      return true;
    }

    pickerWindow.setMenuBarVisibility(false);
    pickerWindow.webContents.setWindowOpenHandler(({ url }) => {
      handleActionUrl(url);
      return { action: "deny" };
    });
    pickerWindow.webContents.on("will-navigate", (event, url) => {
      if (handleActionUrl(url)) {
        event.preventDefault();
        return;
      }
      if (url !== pickerUrl) event.preventDefault();
    });
    pickerWindow.webContents.on("will-redirect", (event, url) => {
      event.preventDefault();
      handleActionUrl(url);
    });
    pickerWindow.once("ready-to-show", () => {
      if (!pickerWindow.isDestroyed()) pickerWindow.show();
    });
    pickerWindow.on("closed", () => {
      finish(null);
    });

    void pickerWindow.loadURL(pickerUrl).catch(() => {
      finish(null);
    });
  });
}
