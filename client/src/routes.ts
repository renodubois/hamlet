import { lazy } from "solid-js";
import type { RouteDefinition } from "@solidjs/router";

export const routes: RouteDefinition[] = [
  {
    path: "/",
    component: () => null,
  },
  {
    path: "/channel/:id",
    component: lazy(() => import("./pages/channel")),
  },
  {
    path: "**",
    component: lazy(() => import("./errors/404")),
  },
];
