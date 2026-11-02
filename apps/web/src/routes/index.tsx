// The route tree, code-based (TanStack Solid Router). The root elects the
// owning tab (one tab owns the OPFS store; the others are told so), then
// `/` sends a signed-in browser to `/app` and anyone else to `/sign-in`.
// `/design` is the gallery of the design system's components.
import { Outlet, createRootRoute, createRoute, redirect } from "@tanstack/solid-router";
import { Show, createResource } from "solid-js";
import { Button, Card } from "../design/components";
import { ensureOwner, openInstance, opened, savedInstance } from "../client/state";
import { App, type AppSearch } from "./app";
import { DesignGallery } from "./design";
import { Link } from "./link";
import { SignIn } from "./sign-in";

function Root() {
  const [owner] = createResource(ensureOwner);
  return (
    <Show when={owner.loading || owner()} fallback={<OtherTab />}>
      <Outlet />
    </Show>
  );
}

function OtherTab() {
  return (
    <div class="auth">
      <Card title="Lumia is open in another tab" subtitle="Only one tab can hold your conversations at a time.">
        <div class="auth__form">
          <Button variant="primary" block onClick={() => location.reload()}>
            Try again
          </Button>
        </div>
      </Card>
    </div>
  );
}

const rootRoute = createRootRoute({ component: Root });

// A signed-in session, reopened from the saved instance after a reload,
// or the sign-in screen.
async function requireSession(): Promise<void> {
  if (opened()?.signedIn) return;
  const url = savedInstance();
  if (url) {
    try {
      if ((await openInstance(url)).signedIn) return;
    } catch {
      /* the instance could not be opened; sign in again */
    }
  }
  throw redirect({ to: "/sign-in" });
}

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: async () => {
    await requireSession();
    throw redirect({ to: "/app" });
  },
});

const signInRoute = createRoute({ getParentRoute: () => rootRoute, path: "/sign-in", component: SignIn });
const linkRoute = createRoute({ getParentRoute: () => rootRoute, path: "/link", component: Link });
const appRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/app",
  component: App,
  beforeLoad: requireSession,
  validateSearch: (s: Record<string, unknown>): AppSearch => ({
    place: s.place === "home" ? "home" : undefined,
    view: s.view === "requests" || s.view === "devices" || s.view === "new" || s.view === "dm" ? s.view : undefined,
    dm: typeof s.dm === "string" ? s.dm : undefined,
    server: typeof s.server === "string" ? s.server : undefined,
  }),
});
const designRoute = createRoute({ getParentRoute: () => rootRoute, path: "/design", component: DesignGallery });

export const routeTree = rootRoute.addChildren([indexRoute, signInRoute, linkRoute, appRoute, designRoute]);
