// Sign in: the instance (with its trust card, architecture 6.1), then a
// password with a second factor when the account asks for one, or a new
// account. Built from the design canvas's "Sign in" board; the instance
// field and trust card are the additions this product needs before the
// card the board shows.
import { Show, createResource, createSignal, onMount } from "solid-js";
import { useNavigate } from "@tanstack/solid-router";
import { Button, Card, Checkbox, Divider, TextField } from "../design/components";
import { ClientError, type InstanceSummary } from "../client/bridge";
import { SERVED_BY_INSTANCE, client, inShell, openInstance, savedInstance } from "../client/state";

type Mode = "signIn" | "create";

export function SignIn() {
  const navigate = useNavigate();
  const [instance, setInstance] = createSignal(savedInstance() ?? "");
  const [opened, setOpened] = createSignal<string | null>(null);
  const [info, setInfo] = createSignal<InstanceSummary | null>(null);
  const [mode, setMode] = createSignal<Mode>("signIn");
  const [handle, setHandle] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [totp, setTotp] = createSignal("");
  const [needsTotp, setNeedsTotp] = createSignal(false);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  // Served by an instance, the instance is this page's origin: no address to ask for.
  const fixed = () => SERVED_BY_INSTANCE && !inShell();
  onMount(() => {
    if (fixed()) setOpened(location.origin);
  });

  const [trust] = createResource(opened, async (url) => {
    const b = client();
    await openInstance(url);
    const i = await b.instanceInfo();
    setInfo(i);
    return i;
  });

  const connect = (e: Event) => {
    e.preventDefault();
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
      if (mode() === "create") await b.createAccount(handle(), password());
      else await b.signIn(handle(), password(), needsTotp() ? totp() : undefined);
      navigate({ to: "/app" });
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
              <Show when={!fixed() || trust.error}>
                <Button variant="primary" type="submit" block disabled={trust.loading} onClick={fixed() ? () => setOpened(null) || setOpened(location.origin) : undefined}>
                  {fixed() ? "Try again" : "Continue"}
                </Button>
              </Show>
            </form>
          </Card>
        }
      >
        {(i) => (
          <Card title={mode() === "create" ? "Create an account" : "Welcome back"} subtitle={mode() === "create" ? `A new account on ${i().host}.` : "Sign in to pick up your conversations."}>
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
              <Show when={mode() === "signIn"}>
                <Checkbox label="Keep me signed in on this device" checked />
              </Show>
              <Show when={error()}>
                <p class="auth__error" role="alert">
                  {error()}
                </p>
              </Show>
              <Button variant="primary" type="submit" block disabled={busy()}>
                {mode() === "create" ? "Create account" : "Sign in"}
              </Button>
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
                <Show when={i().openRegistration} fallback={<span>This instance is not open for new accounts.</span>}>
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
