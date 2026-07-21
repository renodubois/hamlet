import { screen } from "@testing-library/react";
import { renderNative } from "../test/render";
import { describe, expect, test } from "vitest";
import { expectNoA11yViolations } from "../test/a11y";
import Avatar from "./avatar";

describe("<Avatar>", () => {
  test("renders an <img> when url is provided", () => {
    renderNative(<Avatar url="/uploads/avatars/1.webp?v=7" username="alice" size={32} />);
    const img = screen.getByRole("img", { name: /alice's avatar/i });
    // Wrapper has role=img with the accessible label; the <img> inside has empty alt.
    const imgEl = img.querySelector("img");
    expect(imgEl).not.toBeNull();
    expect(imgEl?.getAttribute("src")).toContain("/uploads/avatars/1.webp?v=7");
    expect(imgEl?.getAttribute("alt")).toBe("");
  });

  test("renders an identicon SVG when url is null", () => {
    renderNative(<Avatar url={null} username="bob" size={24} />);
    const wrapper = screen.getByRole("img", { name: /bob's avatar/i });
    const svg = wrapper.querySelector("svg");
    expect(svg).not.toBeNull();
  });

  test("has no axe violations with url", async () => {
    const { container } = renderNative(
      <Avatar url="/uploads/avatars/2.webp?v=1" username="alice" size={32} />,
    );
    await expectNoA11yViolations(container, "Avatar with url");
  });

  test("has no axe violations with identicon fallback", async () => {
    const { container } = renderNative(<Avatar url={null} username="bob" size={32} />);
    await expectNoA11yViolations(container, "Avatar with identicon");
  });

  test("applies the speaking ring when isSpeaking is true", () => {
    renderNative(<Avatar url={null} username="bob" size={32} isSpeaking={true} />);
    const wrapper = screen.getByRole("img", { name: /bob's avatar/i });
    expect(wrapper.className).toMatch(/ring-green-500/);
  });

  test("omits the speaking ring when isSpeaking is false", () => {
    renderNative(<Avatar url={null} username="bob" size={32} isSpeaking={false} />);
    const wrapper = screen.getByRole("img", { name: /bob's avatar/i });
    expect(wrapper.className).not.toMatch(/ring-green-500/);
  });
});
