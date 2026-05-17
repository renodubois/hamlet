import { setupServer } from "msw/node";
import { createHandlers, createState, type HandlerState } from "./handlers";

let state: HandlerState = createState();
export const server = setupServer(...createHandlers(state));

export function resetMswState(overrides: Partial<HandlerState> = {}): HandlerState {
  state = createState(overrides);
  server.resetHandlers(...createHandlers(state));
  return state;
}

export function mswState(): HandlerState {
  return state;
}
