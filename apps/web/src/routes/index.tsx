// The route tree, code-based (TanStack Solid Router). The root elects the
// owning tab (one tab owns the OPFS store; the others are told so), then
// `/` sends a signed-in browser to `/app` and anyone else to `/sign-in`.
// `/design` is the gallery of the design system's components.
import { Outlet, createRootRoute, createRoute, redirect, useNavigate, type ErrorComponentProps } from "@tanstack/solid-router";
import { Show, createEffect, createResource } from "solid-js";
import { Button, Card } from "../design/components";
import { ensureOwner, forgetRemovedDevice, openInstance, opened, refreshSignedIn, removed, savedInstance } from "../client/state";
import { forgetOpenFolders } from "./rail";
import { App, type AppSearch } from "./app";
import { AuthPage } from "./auth-page";
import { DesignGallery } from "./design";
import { Invite } from "./invite";
import { Link } from "./link";
import { PrivateMessages, Restore, privateSearch } from "./private";
import { SignIn } from "./sign-in";

function Root() {
  const [owner] = createResource(ensureOwner);
  // Removed by another device (identity spec 4.4), on whatever screen: the
  // core has erased what it kept; what the page holds goes too, and the
  // sign-in card says why.
  const navigate = useNavigate();
  createEffect(() => {
    if (!removed()) return;
    forgetOpenFolders();
    void forgetRemovedDevice().finally(() => void navigate({ to: "/sign-in" }));
  });
  return (
    <Show when={owner.loading || owner()} fallback={<OtherTab />}>
      <Outlet />
    </Show>
  );
}

function OtherTab() {
  return (
    <AuthPage>
      <Card title="Lumia is open in another tab" subtitle="Only one tab can hold your conversations at a time.">
        <div class="auth__form">
          <Button variant="primary" block onClick={() => location.reload()}>
            Try again
          </Button>
        </div>
      </Card>
    </AuthPage>
  );
}

// What a screen threw while it was drawn, in place of the screen: said,
// with a reload (what is held is in the store and the instance, so a
// reload loses nothing sent), and logged with its stack for whoever
// looks at the console.
function Failed(props: ErrorComponentProps) {
  console.error(props.error);
  return (
    <AuthPage>
      <Card title="Something went wrong" subtitle="This screen ran into a problem it couldn’t recover from. Reloading usually fixes it.">
        <div class="auth__form">
          <p class="auth__error" role="alert">
            {props.error instanceof Error ? props.error.message : String(props.error)}
          </p>
          <Button variant="primary" block onClick={() => location.reload()}>
            Reload
          </Button>
        </div>
      </Card>
    </AuthPage>
  );
}

const rootRoute = createRootRoute({ component: Root, errorComponent: Failed });

// A signed-in session, reopened from the saved instance after a reload,
// or the sign-in screen. Synchronous once signed in: this runs on every
// navigation within `/app` too. An instance open but not yet signed in
// at open time is asked rather than opened again (the sign-in screens
// sign in after the open); the answer is kept, so it is asked once.
async function requireSession(): Promise<void> {
  const o = opened();
  if (o?.signedIn) return;
  if (o) {
    if (await refreshSignedIn()) return;
    throw redirect({ to: "/sign-in" });
  }
  const url = savedInstance();
  if (url) {
    try {
      if ((await openInstance(url)).signedIn) return;
    } catch (e) {
      // The instance could not be opened; sign in again.
      console.error("open:", e);
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

// An invite carries the sign-in page the code to join with, and the
// instance the link named when the page is not served by one.
export interface SignInSearch {
  invite?: string;
  instance?: string;
  /** The token from a magic-link mail's link. */
  magic?: string;
  /**
   * Where a sign-in made here is handed to: a native app's own `lumia:`
   * URL (its browser sheet comes back to it with a one-time token), or a
   * page of this origin. Nothing else: the token signs a device in.
   */
  return?: string;
  /** Where to land once signed in: the Devices page, for a passkey made there, or the link page, to link this browser by a prompt. */
  next?: "devices" | "link";
}
const returnOf = (value: unknown): string | undefined =>
  typeof value === "string" && (value.startsWith("lumia://") || value.startsWith(location.origin + "/")) ? value : undefined;
const signInSearch = (s: Record<string, unknown>): SignInSearch => ({
  invite: typeof s.invite === "string" && s.invite ? s.invite : undefined,
  instance: typeof s.instance === "string" && s.instance ? s.instance : undefined,
  magic: typeof s.magic === "string" && s.magic ? s.magic : undefined,
  return: returnOf(s.return),
  next: s.next === "devices" || s.next === "link" ? s.next : undefined,
});
const signInRoute = createRoute({ getParentRoute: () => rootRoute, path: "/sign-in", component: SignIn, validateSearch: signInSearch });
// The link an invite is: `/invite/{code}` on the instance (architecture
// 6.1: the trust card and what the link leads to, before anything is sent).
const inviteRoute = createRoute({ getParentRoute: () => rootRoute, path: "/invite/$code", component: Invite, validateSearch: signInSearch });
// For a page that works signed in or not: the instance open, as sign-in
// leaves it or as the saved one reopens.
async function openSaved() {
  if (opened()) return;
  const url = savedInstance();
  if (!url) throw redirect({ to: "/sign-in" });
  await openInstance(url);
}
// Link this browser: by a prompt signed in, another way signed out.
const linkRoute = createRoute({ getParentRoute: () => rootRoute, path: "/link", component: Link, beforeLoad: openSaved });
// Private messages in this browser (issue #20): the phrase step and its
// check, or link, restore and start over; signed in.
const privateRoute = createRoute({ getParentRoute: () => rootRoute, path: "/private-messages", component: PrivateMessages, beforeLoad: requireSession, validateSearch: privateSearch });
// Restore with the phrase, signed in or not (it signs in by the identity).
const restoreRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/restore",
  component: Restore,
  beforeLoad: openSaved,
});
const appRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/app",
  component: App,
  beforeLoad: requireSession,
  validateSearch: (s: Record<string, unknown>): AppSearch => ({
    place: s.place === "home" ? "home" : undefined,
    view: s.view === "requests" || s.view === "devices" || s.view === "profile" || s.view === "preferences" || s.view === "new" || s.view === "add" || s.view === "dm" ? s.view : undefined,
    dm: typeof s.dm === "string" ? s.dm : undefined,
    server: typeof s.server === "string" ? s.server : undefined,
  }),
});
const designRoute = createRoute({ getParentRoute: () => rootRoute, path: "/design", component: DesignGallery });

export const routeTree = rootRoute.addChildren([indexRoute, signInRoute, inviteRoute, linkRoute, privateRoute, restoreRoute, appRoute, designRoute]);
