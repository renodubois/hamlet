import { fireEvent, screen } from "@testing-library/react";
import { Link, Route, Routes, BrowserRouter } from "react-router-dom";
import { afterEach, describe, expect, test } from "vitest";

import { renderNative } from "./test/render";
import { normalizeRouterBasename } from "./router-basename";

const originalPath = window.location.pathname;

afterEach(() => {
  window.history.replaceState({}, "", originalPath);
});

describe("static-host router basename", () => {
  test.each([
    ["/", "/"],
    ["", "/"],
    ["/hamlet/", "/hamlet"],
    ["hamlet", "/hamlet"],
    ["///hamlet///", "/hamlet"],
  ])("normalizes %j to %j", (baseUrl, expected) => {
    expect(normalizeRouterBasename(baseUrl)).toBe(expected);
  });

  test("matches routes and keeps navigation within a GitHub Pages project base", () => {
    window.history.replaceState({}, "", "/hamlet/login");

    renderNative(
      <BrowserRouter basename={normalizeRouterBasename("/hamlet/")}>
        <Routes>
          <Route path="login" element={<Link to="/channel/123">Open channel</Link>} />
          <Route path="channel/:id" element={<h1>Project channel</h1>} />
        </Routes>
      </BrowserRouter>,
    );

    expect(screen.getByRole("link", { name: "Open channel" })).toHaveAttribute(
      "href",
      "/hamlet/channel/123",
    );
    fireEvent.click(screen.getByRole("link", { name: "Open channel" }));

    expect(screen.getByRole("heading", { name: "Project channel" })).toBeInTheDocument();
    expect(window.location.pathname).toBe("/hamlet/channel/123");
  });
});
