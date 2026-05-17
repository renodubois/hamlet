type NativeThemeLike = {
  themeSource: "system" | "light" | "dark";
};

/**
 * Keep Chromium/Electron from inheriting the operating system dark appearance
 * until the renderer has a complete dark theme.
 */
export function forceLightAppearance(nativeTheme: NativeThemeLike): void {
  nativeTheme.themeSource = "light";
}
