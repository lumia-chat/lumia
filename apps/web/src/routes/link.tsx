// Link this browser as a web device of an identity (architecture 11.4).
// By a prompt (identity spec 4.2.1): signed in to the account, the browser
// opens a link session, every device that holds the identity asks about
// it, the person checks that both show the same six digits and approves
// there, and the core claims the code and signs in by identity proof. A
// browser is only ever the new device. "Link another way" is 4.2's copied
// request and typed code, for when the other device can't reach the
// instance; signed out it is the only way here, since a session is the
// account's.
import { Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { useNavigate } from "@tanstack/solid-router";
import { Button, Card, LinkDigits, TextField, Waiting } from "../design/components";
import { AuthPage } from "./auth-page";
import { ClientError } from "../client/bridge";
import { SERVED_BY_INSTANCE, client, identityChanged, refreshSignedIn, savedInstance } from "../client/state";
import { LINK_POLL_MS, linkEnded, linkFailurePasses, linkOpen, linkPolls, linkWords, type LinkScreen } from "../client/link";

// This device's name in the roster, as setting up names it.
const LABEL = "browser";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function Link() {
  const navigate = useNavigate();
  const [way, setWay] = createSignal<"checking" | "prompt" | "signedOut" | "another">("checking");
  const [signedIn, setSignedIn] = createSignal(false);
  onMount(async () => {
    const yes = await refreshSignedIn().catch(() => false);
    // Linked already (another tab, or back here by the address): nothing to do.
    if (yes && (await client().hasIdentity().catch(() => false))) {
      navigate({ to: "/app" });
      return;
    }
    setSignedIn(yes);
    setWay(yes ? "prompt" : "signedOut");
  });
  const done = () => navigate({ to: "/app" });
  const anotherWay = () => (
    <button type="button" class="lu-link auth__switch" onClick={() => setWay("another")}>
      Link another way
    </button>
  );

  return (
    <Show when={way() !== "checking"}>
      <Show when={way() === "prompt"}>
        <LinkByPrompt onLinked={done} onCancel={done} under={anotherWay()} />
      </Show>
      <Show when={way() === "signedOut"}>
        <AuthPage under={anotherWay()}>
          <Card title="Link this browser" subtitle="A device you already use approves this one, and your recovery phrase stays where it is.">
            <div class="auth__form">
              <p class="auth__note">Sign in to your account here first. Then this browser asks your other devices, and you check that both show the same number.</p>
              <Button variant="primary" block onClick={() => navigate({ to: "/sign-in", search: { next: "link", instance: SERVED_BY_INSTANCE ? undefined : (savedInstance() ?? undefined) } })}>
                Sign in
              </Button>
            </div>
          </Card>
        </AuthPage>
      </Show>
      <Show when={way() === "another"}>
        <LinkAnotherWay
          onLinked={done}
          under={
            <button type="button" class="lu-link auth__switch" onClick={() => setWay(signedIn() ? "prompt" : "signedOut")}>
              Back
            </button>
          }
        />
      </Show>
    </Show>
  );
}

/**
 * The session, opened as the screen opens and followed until it ends:
 * asked where it stands on every account event and every two seconds
 * while it waits, given up when the screen goes while it is still open.
 */
function LinkByPrompt(props: { onLinked: () => void; onCancel: () => void; under: JSX.Element }) {
  const [screen, setScreen] = createSignal<LinkScreen>("starting");
  const [digits, setDigits] = createSignal("");
  const [reason, setReason] = createSignal("");
  const [trouble, setTrouble] = createSignal("");
  // The session the screen follows: an answer about an earlier one is dropped.
  let run = 0;
  let asking = false;

  const take = (n: number, p: { stage: LinkScreen; digits: string }) => {
    if (n !== run) return;
    setTrouble("");
    setDigits(p.digits);
    setScreen(p.stage);
    if (p.stage === "linked") {
      // What the page held of this browser having no identity is asked again.
      identityChanged();
      props.onLinked();
    }
  };
  const fail = (n: number, e: unknown) => {
    if (n !== run) return;
    setReason(message(e));
    setScreen("failed");
  };
  const start = async () => {
    const n = ++run;
    setScreen("starting");
    setDigits("");
    setReason("");
    setTrouble("");
    try {
      take(n, await client().linkStart(LABEL));
    } catch (e) {
      fail(n, e);
    }
  };
  const ask = async () => {
    if (asking || !linkPolls(screen())) return;
    const n = run;
    asking = true;
    try {
      take(n, await client().linkProgress());
    } catch (e) {
      const code = e instanceof ClientError ? e.code : "";
      if (n === run && linkFailurePasses(code, message(e))) setTrouble(`Can’t reach your instance; still trying. (${message(e)})`);
      else fail(n, e);
    } finally {
      asking = false;
    }
  };
  let gone = false;
  const giveUp = () => {
    if (gone) return;
    gone = true;
    run++;
    void client().linkCancel().catch(() => {});
  };

  onMount(() => {
    void start();
    const timer = setInterval(() => void ask(), LINK_POLL_MS);
    // The session moves by the account's log (`LinkSessionUpdated`).
    const off = client().onEvent((e) => {
      if (e.kind === "accountApplied") void ask();
    });
    onCleanup(() => {
      clearInterval(timer);
      off();
      if (linkOpen(screen())) giveUp();
    });
  });

  return (
    <AuthPage under={screen() === "compare" ? undefined : props.under}>
      <LinkPromptCard
        screen={screen()}
        digits={digits()}
        reason={reason()}
        trouble={trouble()}
        onTryAgain={() => void start()}
        onCancel={() => {
          giveUp();
          props.onCancel();
        }}
      />
    </AuthPage>
  );
}

/** The card at each stage of linking by a prompt: waiting, the digits to compare, or why it ended with Try again. */
export function LinkPromptCard(props: { screen: LinkScreen; digits: string; reason?: string; trouble?: string; onTryAgain: () => void; onCancel: () => void }) {
  const words = () => linkWords(props.screen, props.reason);
  return (
    <Card title={words().title}>
      <div class="auth__form">
        <p class={props.screen === "failed" ? "auth__error" : "auth__note"} role={linkEnded(props.screen) ? "alert" : "status"}>
          {words().text}
        </p>
        <Show when={props.screen === "compare"}>
          <LinkDigits digits={props.digits} />
        </Show>
        <Show when={props.screen === "starting" || props.screen === "waiting" || props.screen === "linked"}>
          <Waiting label={props.screen === "linked" ? "Linked. Signing in…" : "Waiting for your other device…"} />
        </Show>
        <Show when={props.trouble}>
          <p class="auth__error">{props.trouble}</p>
        </Show>
        <div class="auth__ways">
          <Show when={linkEnded(props.screen)}>
            <Button variant="primary" block onClick={() => props.onTryAgain()}>
              Try again
            </Button>
          </Show>
          <Button block disabled={props.screen === "linked"} onClick={() => props.onCancel()}>
            Cancel
          </Button>
        </div>
      </div>
    </Card>
  );
}

/** Identity spec 4.2 by hand: the request shown to copy to a device that approves it, then the code it gives typed here. Signed out, it signs in by identity proof. */
function LinkAnotherWay(props: { onLinked: () => void; under: JSX.Element }) {
  const [request, setRequest] = createSignal("");
  const [code, setCode] = createSignal("");
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  const start = async () => {
    setError("");
    try {
      setRequest(await client().identityLinkRequest(LABEL));
    } catch (e) {
      setError(message(e));
    }
  };

  const claim = async (e: Event) => {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      await client().identityClaim(code().trim());
      identityChanged();
      props.onLinked();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthPage under={props.under}>
      <Card title="Link this browser another way" subtitle="For when your other device can’t reach your instance: copy a request to it, and type back the code it gives.">
        <Show
          when={request()}
          fallback={
            <div class="auth__form">
              <p class="auth__note">{`This browser gets its own device key, scoped to ${typeof location !== "undefined" ? location.origin : "this origin"}.`}</p>
              <Show when={error()}>
                <p class="auth__error" role="alert">
                  {error()}
                </p>
              </Show>
              <Button variant="primary" block onClick={() => void start()}>
                Start linking
              </Button>
            </div>
          }
        >
          <form class="auth__form" onSubmit={(e) => void claim(e)}>
            <p class="auth__note">On a device that has your private messages, open Devices, choose Approve a device and paste this request. Then type the code it shows here.</p>
            <textarea class="auth__code" readonly rows={5} value={request()} aria-label="Link request" onFocus={(e) => e.currentTarget.select()} />
            <TextField label="Code from the approving device" value={code()} onInput={(e) => setCode(e.currentTarget.value)} autocomplete="one-time-code" placeholder="8 characters" />
            <Show when={error()}>
              <p class="auth__error" role="alert">
                {error()}
              </p>
            </Show>
            <Button variant="primary" type="submit" block disabled={busy() || !code().trim()}>
              Claim and sign in
            </Button>
          </form>
        </Show>
      </Card>
    </AuthPage>
  );
}
