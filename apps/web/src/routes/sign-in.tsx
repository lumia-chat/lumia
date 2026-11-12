// Sign in: the instance (with its trust card, architecture 6.1), then a
// password with a second factor when the account asks for one, or a new
// account. Built from the design canvas's "Sign in" board; the instance
// field and trust card are the additions this product needs before the
// card the board shows.
import { Show, createResource, createSignal, onMount } from "solid-js";
import { useNavigate, useSearch } from "@tanstack/solid-router";
import { Button, Card, Divider, TextField } from "../design/components";
import { AuthPage, TrustCard } from "./auth-page";
import { ClientError, type InstanceSummary } from "../client/bridge";
import { SERVED_BY_INSTANCE, client, openInstance, savedInstance, takeEndedNotice } from "../client/state";
import { assertForPasswordSignIn, passkeysAvailable, signInWithPasskey } from "../client/passkeys";
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
  // Sent back here by the instance (a revoked session), the card opens saying so.
  const [error, setError] = createSignal(takeEndedNotice());
  const [busy, setBusy] = createSignal(false);
  // Magic link (architecture 6.3): asked for with the address in the handle
  // field; completed by the link's `magic` query on this page, or by its
  // code pasted here.
  const [magicSent, setMagicSent] = createSignal(false);
  const [magicCode, setMagicCode] = createSignal("");
  const land = async () => {
    // Signing in for a native app: the session made here is handed to it
    // as a one-time token on its return URL, and this page keeps nothing.
    const back = search().return;
    if (back) {
      const token = await client().handoffToken();
      await client().signOut();
      location.assign(`${back}${back.includes("?") ? "&" : "?"}magic=${encodeURIComponent(token)}`);
      return;
    }
    const code = invite();
    if (code) {
      const serverId = await client().acceptInvite(code);
      navigate({ to: "/app", search: { server: serverId } });
    } else if (search().next === "devices") {
      navigate({ to: "/app", search: { place: "home", view: "devices" } });
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
  const fixed = SERVED_BY_INSTANCE;
  onMount(() => {
    if (fixed) setOpened(location.origin);
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
    if (fixed) return;
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
      // With an invite in hand, the landing joins its server (a new
      // account joined at creation; its code resolves to the same server).
      await land();
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      if (ce?.code === "FAILED_PRECONDITION" && ce.details.mfa === "passkey") {
        // The passkey as the second factor: the browser's ceremony, then
        // the same sign-in carrying its answer.
        try {
          const assertion = await assertForPasswordSignIn(handle());
          await client().signInWithPasskey(handle(), password(), assertion.challengeId, assertion.credentialJson);
          await land();
        } catch (e2) {
          const c2 = e2 instanceof ClientError ? e2 : null;
          setError(c2?.code === "UNAUTHENTICATED" ? "That passkey was not accepted." : e2 instanceof Error && e2.name === "NotAllowedError" ? "This account asks for its passkey; none was offered." : e2 instanceof Error ? e2.message : String(e2));
        }
      } else if (ce?.code === "FAILED_PRECONDITION" && !needsTotp()) {
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

  // The ways in other than a password, each an outlined button under the "or" rule.
  const otherWays = (i: InstanceSummary) => mode() === "signIn" && (!!i.oidcName || passkeysAvailable() || i.magicLinkEnabled || i.identityProofSignInEnabled);

  return (
    <AuthPage
      under={
        <Show when={info()}>
          {(i) => (
            <Show when={mode() === "signIn"} fallback={<>Already have an account? <button type="button" class="lu-link auth__switch" onClick={() => setMode("signIn")}>Sign in</button></>}>
              New to Lumia?{" "}
              <Show when={i().openRegistration || invite()} fallback={<span>This instance is not open for new accounts.</span>}>
                <button type="button" class="lu-link auth__switch" onClick={() => setMode("create")}>
                  Create an account
                </button>
              </Show>
            </Show>
          )}
        </Show>
      }
      // What the instance says of itself: its privacy notice and its terms, where it names them.
      footer={
        <Show when={info()?.privacyUrl || info()?.termsUrl}>
          <Show when={info()?.privacyUrl}>{(url) => <a href={url()} target="_blank" rel="noreferrer">Privacy</a>}</Show>
          <Show when={info()?.termsUrl}>{(url) => <a href={url()} target="_blank" rel="noreferrer">Terms</a>}</Show>
        </Show>
      }
    >
          <Show
            when={info()}
            fallback={
              <Card title="Welcome" subtitle={fixed ? "Reaching this instance…" : "Which instance do you want to sign in to?"}>
                <form class="auth__form" onSubmit={connect}>
                  <Show when={!fixed}>
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
                  <Show when={trust.loading && !fixed}>
                    <p class="auth__note">Reaching the instance…</p>
                  </Show>
                  <Show when={trust.error}>
                    <p class="auth__error" role="alert">
                      Could not reach that instance: {String((trust.error as Error)?.message ?? trust.error)}
                    </p>
                  </Show>
                  <Show when={!fixed}>
                    <Button variant="primary" type="submit" block disabled={trust.loading}>
                      Continue
                    </Button>
                  </Show>
                  <Show when={fixed && trust.error}>
                    <Button variant="primary" type="button" block disabled={trust.loading} onClick={() => void refetch()}>
                      Try again
                    </Button>
                  </Show>
                </form>
              </Card>
            }
          >
            {(i) => (
                <Card title={mode() === "create" ? "Create an account" : "Welcome back"} subtitle={search().return?.startsWith("lumia://") ? "Sign in here for the Lumia app; it takes over once you are in." : invite() ? (mode() === "create" ? `A new account on ${i().host}, in the server you were invited to.` : "Sign in to join the server you were invited to.") : mode() === "create" ? `A new account on ${i().host}.` : "Sign in to pick up your conversations."}>
                  <TrustCard host={i().host} operatorName={i().operatorName} softwareVersion={i().softwareVersion} onChange={fixed ? undefined : () => { setInfo(null); setOpened(null); }} />
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
                    <Show when={otherWays(i())}>
                      <Divider variant="or" />
                      <div class="auth__ways">
                        <Show when={i().oidcName}>
                          <Button icon="globe" block type="button" disabled={busy()} onClick={() => {
                            // Off to the provider; the instance brings the browser back here
                            // with a sign-in token, the invite and return in hand still.
                            const instanceUrl = opened() ?? location.origin;
                            const q = new URLSearchParams({ instance: instanceUrl });
                            if (invite()) q.set("invite", invite() ?? "");
                            if (search().return) q.set("return", search().return ?? "");
                            location.assign(`${instanceUrl}/api/v1/auth/oidc/start?return=${encodeURIComponent(`${location.origin}/sign-in?${q}`)}`);
                          }}>
                            {i().oidcName}
                          </Button>
                        </Show>
                        <Show when={passkeysAvailable()}>
                          <Button icon="key" block type="button" disabled={busy()} onClick={() => void passkey()}>
                            Sign in with a passkey
                          </Button>
                        </Show>
                        <Show when={i().magicLinkEnabled && !magicSent()}>
                          <Button icon="mail" block type="button" disabled={busy() || !handle().trim()} onClick={() => void requestMagicLink()}>
                            Email me a sign-in link
                          </Button>
                        </Show>
                        <Show when={i().identityProofSignInEnabled}>
                          <Button icon="key" block type="button" onClick={() => navigate({ to: "/link" })}>
                            Link this browser to your identity
                          </Button>
                        </Show>
                      </div>
                      <Show when={i().magicLinkEnabled && magicSent()}>
                        <p class="auth__note">A link is on its way to {handle().trim()} if it has an account here. Open it in this browser, or paste its code here.</p>
                        <TextField label="Code from the email" value={magicCode()} onInput={(e) => setMagicCode(e.currentTarget.value)} autocomplete="one-time-code" placeholder="Paste the code" />
                        <Button type="button" block disabled={busy() || !magicCode().trim()} onClick={() => void completeMagicLink(magicCode())}>
                          Sign in with the code
                        </Button>
                      </Show>
                    </Show>
                  </form>
                </Card>
            )}
          </Show>
    </AuthPage>
  );
}
