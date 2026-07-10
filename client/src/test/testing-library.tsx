import React from "react";
import { useStaticSignalRerender } from "../hooks/react-state";
import {
  render as rtlRender,
  fireEvent,
  screen,
  waitFor,
  within,
  cleanup,
  act,
  type RenderOptions,
} from "@testing-library/react";

export { fireEvent, screen, waitFor, within, cleanup, act };

export function render(
  ui: React.ReactElement | (() => React.ReactElement),
  options?: RenderOptions,
) {
  if (typeof ui === "function") {
    const TestComponent = ui;
    function StaticSignalAwareTestComponent() {
      useStaticSignalRerender();
      return <TestComponent />;
    }
    return rtlRender(<StaticSignalAwareTestComponent />, options);
  }
  const element = ui;
  function StaticSignalAwareElement() {
    useStaticSignalRerender();
    return element;
  }
  return rtlRender(<StaticSignalAwareElement />, options);
}
