import { describe, expect, it } from "vitest";
import { forceLightAppearance } from "./appearance";

describe("Electron appearance defaults", () => {
  it("opts out of automatic system dark mode", () => {
    const theme: { themeSource: "system" | "light" | "dark" } = { themeSource: "system" };

    forceLightAppearance(theme);

    expect(theme.themeSource).toBe("light");
  });
});
