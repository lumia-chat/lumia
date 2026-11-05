// Sign in: the instance (with its trust card, architecture 6.1), then a
// password with a second factor when the account asks for one, or a new
// account. Built from the design canvas's "Sign in" board; the instance
// field and trust card are the additions this product needs before the
// card the board shows.
import { Show, createResource, createSignal, onMount } from "solid-js";
import { useNavigate, useSearch } from "@tanstack/solid-router";
import { Button, Card, Divider, TextField } from "../design/components";
import { ClientError, type InstanceSummary } from "../client/bridge";
import { SERVED_BY_INSTANCE, client, inShell, openInstance, savedInstance } from "../client/state";
import { passkeysAvailable, signInWithPasskey } from "../client/passkeys";
import type { SignInSearch } from "./index";

type Mode = "signIn" | "create";

export function SignIn() {
  const navigate = useNavigate();
  // From an invite link: the code to join with once signed in (or to
  // create the account with), and the instance the link named.
  const search = useSearch({ from: "/sign-in" }) as () => SignInSearch;
  const invite = () => search().invite;
  const [instance, setInstance] = createSignal(search().instance ?? savedInstance() ?? "");
  const [opened, setOpened] = createSignal<string | null>(null);
  const [info, setInfo] = createSignal<InstanceSummary | null>(null);
  const [mode, setMode] = createSignal<Mode>("signIn");
  const [handle, setHandle] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [totp, setTotp] = createSignal("");
  const [needsTotp, setNeedsTotp] = createSignal(false);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  // Magic link (architecture 6.3): asked for with the address in the handle
  // field; completed by the link's `magic` query on this page, or by its
  // code pasted here.
  const [magicSent, setMagicSent] = createSignal(false);
  const [magicCode, setMagicCode] = createSignal("");
  const land = async () => {
    const code = invite();
    if (code) {
      const serverId = await client().acceptInvite(code);
      navigate({ to: "/app", search: { server: serverId } });
    } else {
      navigate({ to: "/app" });
    }
  };
  const passkey = async () => {
    setError("");
    setBusy(true);
    try {
      await signInWithPasskey(handle());
      await land();
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(ce?.code === "UNAUTHENTICATED" ? "That passkey was not accepted." : err instanceof Error && err.name === "NotAllowedError" ? "No passkey was offered." : err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };
  const requestMagicLink = async () => {
    setError("");
    setBusy(true);
    try {
      await client().requestMagicLink(handle());
      setMagicSent(true);
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(ce?.code === "RATE_LIMITED" ? "Too many links in the last hour; try later." : err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };
  const completeMagicLink = async (token: string) => {
    setError("");
    setBusy(true);
    try {
      await client().signInMagicLink(token);
      await land();
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(ce?.code === "UNAUTHENTICATED" ? "That link is unknown, used or expired. Ask for a new one." : err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  // Served by an instance, the instance is this page's origin: no address to ask for.
  const fixed = () => SERVED_BY_INSTANCE && !inShell();
  onMount(() => {
    if (fixed()) setOpened(location.origin);
    else if (search().instance) setOpened(search().instance ?? null);
  });

  const [trust, { refetch }] = createResource(opened, async (url) => {
    const b = client();
    await openInstance(url);
    const i = await b.instanceInfo();
    setInfo(i);
    const magic = search().magic;
    if (magic) void completeMagicLink(magic);
    return i;
  });

  const connect = (e: Event) => {
    e.preventDefault();
    if (fixed()) return;
    setError("");
    const url = instance().trim().replace(/\/+$/, "");
    if (!/^https?:\/\//.test(url)) {
      setError("Enter the instance address, starting with http:// or https://");
      return;
    }
    setOpened(url);
  };

  const submit = async (e: Event) => {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      const b = client();
      const code = invite();
      if (mode() === "create") await b.createAccount(handle(), password(), code);
      else await b.signIn(handle(), password(), needsTotp() ? totp() : undefined);
      // Signed in with an invite in hand: join its server and land there.
      // A new account joined at creation; its code resolves to the same server.
      if (code) {
        const serverId = await b.acceptInvite(code);
        navigate({ to: "/app", search: { server: serverId } });
      } else {
        navigate({ to: "/app" });
      }
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      if (ce?.code === "FAILED_PRECONDITION" && !needsTotp()) {
        setNeedsTotp(true);
        setError("");
      } else if (ce?.code === "UNAUTHENTICATED") {
        setError(needsTotp() ? "That code was not accepted." : "Wrong handle or password.");
      } else if (ce?.code === "ALREADY_EXISTS") {
        setError("That handle is taken.");
      } else {
        setError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="auth">
      <div class="auth__logo" aria-hidden="true">
        <img src="/brand/lumia-original-light.svg" alt="" class="lu-only-light" />
        <img src="/brand/lumia-original-dark.svg" alt="" class="lu-only-dark" />
      </div>
      <Show
        when={info()}
        fallback={
          <Card title="Welcome" subtitle={fixed() ? "Reaching this instance…" : "Which instance do you want to sign in to?"}>
            <form class="auth__form" onSubmit={connect}>
              <Show when={!fixed()}>
                <TextField
                  label="Instance"
                  type="url"
                  placeholder="https://chat.example.com"
                  value={instance()}
                  onInput={(e) => setInstance(e.currentTarget.value)}
                  autocomplete="url"
                />
              </Show>
              <Show when={error()}>
                <p class="auth__error" role="alert">
                  {error()}
                </p>
              </Show>
              <Show when={trust.loading && !fixed()}>
                <p class="auth__note">Reaching the instance…</p>
              </Show>
              <Show when={trust.error}>
                <p class="auth__error" role="alert">
                  Could not reach that instance: {String((trust.error as Error)?.message ?? trust.error)}
                </p>
              </Show>
              <Show when={!fixed()}>
                <Button variant="primary" type="submit" block disabled={trust.loading}>
                  Continue
                </Button>
              </Show>
              <Show when={fixed() && trust.error}>
                <Button variant="primary" type="button" block disabled={trust.loading} onClick={() => void refetch()}>
                  Try again
                </Button>
              </Show>
            </form>
          </Card>
        }
      >
        {(i) => (
          <Card title={mode() === "create" ? "Create an account" : "Welcome back"} subtitle={invite() ? (mode() === "create" ? `A new account on ${i().host}, in the server you were invited to.` : "Sign in to join the server you were invited to.") : mode() === "create" ? `A new account on ${i().host}.` : "Sign in to pick up your conversations."}>
            <div class="auth__trust">
              <span class="auth__trust-host">{i().host}</span>
              <span class="auth__trust-op">
                run by {i().operatorName || "an unnamed operator"}
                {i().softwareVersion ? ` · Lumia ${i().softwareVersion}` : ""}
              </span>
              <Show when={!fixed()}>
                <button type="button" class="lu-link auth__change" onClick={() => { setInfo(null); setOpened(null); }}>
                  Change instance
                </button>
              </Show>
            </div>
            <form class="auth__form" onSubmit={(e) => void submit(e)}>
              <TextField label={mode() === "create" ? "Handle" : "Email or handle"} value={handle()} onInput={(e) => setHandle(e.currentTarget.value)} autocomplete="username" placeholder={mode() === "create" ? "yourname" : "you@example.com"} />
              <TextField label="Password" reveal value={password()} onInput={(e) => setPassword(e.currentTarget.value)} autocomplete={mode() === "create" ? "new-password" : "current-password"} placeholder={mode() === "create" ? "At least 12 characters" : "Your password"} />
              <Show when={needsTotp()}>
                <TextField label="Verification code" value={totp()} onInput={(e) => setTotp(e.currentTarget.value)} inputmode="numeric" autocomplete="one-time-code" placeholder="6 digits from your authenticator" />
              </Show>
              <Show when={error()}>
                <p class="auth__error" role="alert">
                  {error()}
                </p>
              </Show>
              <Button variant="primary" type="submit" block disabled={busy()}>
                {mode() === "create" ? "Create account" : "Sign in"}
              </Button>
              <Show when={mode() === "signIn" && passkeysAvailable()}>
                <Button icon="key" block type="button" disabled={busy()} onClick={() => void passkey()}>
                  Sign in with a passkey
                </Button>
              </Show>
              <Show when={mode() === "signIn" && i().magicLinkEnabled}>
                <Show
                  when={magicSent()}
                  fallback={
                    <button type="button" class="lu-link auth__switch" disabled={busy() || !handle().trim()} onClick={() => void requestMagicLink()}>
                      Email me a sign-in link
                    </button>
                  }
                >
                  <p class="auth__note">A link is on its way to {handle().trim()} if it has an account here. Open it in this browser, or paste its code here.</p>
                  <TextField label="Code from the email" value={magicCode()} onInput={(e) => setMagicCode(e.currentTarget.value)} autocomplete="one-time-code" placeholder="Paste the code" />
                  <Button variant="primary" type="button" block disabled={busy() || !magicCode().trim()} onClick={() => void completeMagicLink(magicCode())}>
                    Sign in with the code
                  </Button>
                </Show>
              </Show>
              <Show when={mode() === "signIn" && i().identityProofSignInEnabled}>
                <Divider variant="or" />
                <Button icon="key" block onClick={() => navigate({ to: "/link" })}>
                  Link this browser to your identity
                </Button>
              </Show>
            </form>
            <p class="auth__foot">
              <Show when={mode() === "signIn"} fallback={<>Already have an account? <button type="button" class="lu-link auth__switch" onClick={() => setMode("signIn")}>Sign in</button></>}>
                New to Lumia?{" "}
                <Show when={i().openRegistration || invite()} fallback={<span>This instance is not open for new accounts.</span>}>
                  <button type="button" class="lu-link auth__switch" onClick={() => setMode("create")}>
                    Create an account
                  </button>
                </Show>
              </Show>
            </p>
          </Card>
        )}
      </Show>
    </div>
  );
}
