// The route tree, code-based (TanStack Solid Router). Screens are added
// here as they land; `/design` is the gallery of the design system's
// components with the sample data from the design canvas, which is how
// the port is checked against the boards.
import { Outlet, createRootRoute, createRoute, redirect } from "@tanstack/solid-router";
import { DesignGallery } from "./design";

const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/design" });
  },
});

const designRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/design",
  component: DesignGallery,
});

export const routeTree = rootRoute.addChildren([indexRoute, designRoute]);
