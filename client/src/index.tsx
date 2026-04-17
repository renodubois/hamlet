/* @refresh reload */
import "solid-devtools";
import "./index.css";

import { render } from "solid-js/web";

import App from "./App";
import { Router } from "@solidjs/router";
import { routes } from "./routes";
import { AuthProvider } from "./auth_context";

const root = document.getElementById("root");

if (!(root instanceof HTMLElement)) {
  throw new Error(
    "Root element not found. Did you forget to add it to your index.html? Or maybe the id attribute got misspelled?",
  );
}

render(
  () => (
    <AuthProvider>
      <Router root={(props) => <App>{props.children}</App>}>{routes}</Router>
    </AuthProvider>
  ),
  root,
);
