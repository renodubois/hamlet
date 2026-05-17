import { lazy } from "solid-js";
import type { RouteDefinition } from "@solidjs/router";

export const routes: RouteDefinition[] = [
  {
    path: "/login",
    component: lazy(() => import("./pages/login")),
  },
  {
    path: "/channel/:id",
    component: lazy(() => import("./pages/channel")),
  },
  {
    path: "/",
    component: () => null,
  },
  {
    path: "**",
    component: lazy(() => import("./errors/404")),
  },
];
