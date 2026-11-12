import { render } from "solid-js/web";
import { RouterProvider, createRouter } from "@tanstack/solid-router";
import { QueryClientProvider } from "@tanstack/solid-query";
import { routeTree } from "./routes";
import { queryClient } from "./client/state";
// Applies the stored appearance before anything is drawn.
import "./client/preferences";
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
if (mount)
  render(
    () => (
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    ),
    mount,
  );
