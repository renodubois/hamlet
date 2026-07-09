import { createElement } from "react";
import type { RouteObject } from "react-router-dom";
import ChannelView from "./pages/channel";
import LoginScreen from "./pages/login";
import ThreadsPage from "./pages/threads";
import NotFound from "./errors/404";

export const routes: RouteObject[] = [
  { path: "/login", element: createElement(LoginScreen) },
  { path: "/channel/:id", element: createElement(ChannelView) },
  { path: "/threads", element: createElement(ThreadsPage) },
  { path: "/", element: null },
  { path: "*", element: createElement(NotFound) },
];
