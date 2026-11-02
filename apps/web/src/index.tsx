import { render } from "solid-js/web";
import { RouterProvider, createRouter } from "@tanstack/solid-router";
import { routeTree } from "./routes";
import "./design/tokens.css";
import "./design/components.css";
import "./app.css";

const router = createRouter({ routeTree });

declare module "@tanstack/solid-router" {
  interface Register {
    router: typeof router;
  }
}

const mount = document.getElementById("app");
if (mount) render(() => <RouterProvider router={router} />, mount);
