// Private messages in this browser (issue #20; identity spec 4.1, 4.3,
// 4.7 and 7). A browser never sets up silently: it shows the recovery
// phrase and checks three of its words before the identity is made, at
// sign-up (its steps 2 and 3), at a sign-in to an account with none, or
// later from Devices and wherever a private message would start. An
// account whose identity is on other devices is offered link, restore
// (on the flagship's origin only) and, last, start over. Built from the
// "WebSignUpPhrase" board and, wider, the phone's "Check your phrase",
// "Private messages need this phone", "Restore" and "Start over".
import { For, Index, Show, createResource, createSignal, onCleanup, onMount } from "solid-js";
import { useNavigate, useSearch } from "@tanstack/solid-router";
import { Button, Card, DeviceChoices, Divider, Notice, PhraseSaveWarning, PhraseWordField, PhraseWords, TextField } from "../design/components";
import { AuthPage } from "./auth-page";
import { ClientError, accountHasIdentity, reauthNeeded, type DeviceSummary } from "../client/bridge";
import { addedLine, afterPhrase, deviceName, keepWords } from "../client/restore";
import { FLAGSHIP_ORIGIN, client, identityChanged, privateState } from "../client/state";

/** The label this browser's device carries in the roster, as linking gives it. */
const LABEL = "browser";

export interface PrivateSearch {
  /** How the page was reached: sign-up (its steps 2 and 3) or a sign-in to an account with no identity. */
  from?: "signup" | "signin";
  /** Starting over rather than setting up (identity spec 4.7). */
  step?: "start-over";
  /** Where to go once done or put off: a server, Home's conversations or Devices. */
  server?: string;
  next?: "home" | "devices";
  /** The reauth token a fresh OIDC login landed with (identity spec 10). */
  reauth?: string;
}

export const privateSearch = (s: Record<string, unknown>): PrivateSearch => ({
  from: s.from === "signup" || s.from === "signin" ? s.from : undefined,
  step: s.step === "start-over" ? "start-over" : undefined,
  server: typeof s.server === "string" && s.server ? s.server : undefined,
  next: s.next === "home" || s.next === "devices" ? s.next : undefined,
  reauth: typeof s.reauth === "string" && s.reauth ? s.reauth : undefined,
});

type Phase = "loading" | "needs" | "startOver" | "reauth" | "phrase" | "check" | "finishing" | "failed";

const text = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function PrivateMessages() {
  const navigate = useNavigate();
  const search = useSearch({ from: "/private-messages" }) as () => PrivateSearch;
  const [phase, setPhase] = createSignal<Phase>("loading");
  const [startingOver, setStartingOver] = createSignal(search().step === "start-over");
  const [words, setWords] = createSignal<string[]>([]);
  // The check answered wrong: the phrase is shown again, saying so.
  const [wrong, setWrong] = createSignal(false);
  // A reauth asked for after the identity was made here: the one in hand ran out.
  const [again, setAgain] = createSignal(false);
  const [error, setError] = createSignal("");
  const [info] = createResource(() => client().instanceInfo());

  const done = () => {
    const s = search();
    if (s.server) void navigate({ to: "/app", search: { server: s.server } });
    else if (s.next) void navigate({ to: "/app", search: { place: "home", view: s.next === "devices" ? "devices" : undefined } });
    else void navigate({ to: "/app" });
  };

  const toPhrase = async () => {
    setError("");
    try {
      // One phrase for the whole flow: shown again after a wrong check, never made twice.
      if (words().length === 0) setWords((await client().identityNewPhrase()).split(/\s+/));
      setPhase("phrase");
    } catch (e) {
      setError(text(e));
      setPhase("failed");
    }
  };

  // The identity made here from the confirmed phrase and the account bound
  // to it, or moved to it when starting over. Made first, so a reauth that
  // ran out (or an OIDC login, which leaves the page) costs no phrase.
  const finish = async () => {
    setPhase("finishing");
    setError("");
    try {
      if (startingOver()) await client().identityStartOver(LABEL);
      else await client().identitySetup(LABEL);
      identityChanged();
      done();
    } catch (e) {
      if (reauthNeeded(e)) {
        setAgain(true);
        setPhase(startingOver() ? "startOver" : "reauth");
      } else if (accountHasIdentity(e)) {
        setPhase("needs");
      } else {
        const ce = e instanceof ClientError ? e : null;
        setError(ce?.code === "ALREADY_EXISTS" ? "The identity made in this browser is another account’s on this instance." : text(e));
        setPhase("failed");
      }
    }
  };

  // Confirmed it is you: on to the phrase, or, where the identity was
  // already made here (a reauth ran out after it), straight to binding it.
  const confirmed = async () => ((await client().hasIdentity()) ? finish() : toPhrase());

  // Where this browser stands decides the first screen. Asked again by Try
  // again when nothing was shown yet: a browser that holds an identity
  // binds that one, so a new phrase there would restore nothing.
  const begin = async () => {
    setPhase("loading");
    setError("");
    try {
      if (startingOver()) return void setPhase("startOver");
      const state = await privateState();
      if (state === "ready") return done();
      if (state === "elsewhere") return void setPhase("needs");
      if (state === "unbound") return void (await finish());
      // A start-over that made this browser's identity and did not bind it:
      // its phrase was shown and checked then, so finishing only binds it.
      if (state === "mismatched") {
        setStartingOver(true);
        setAgain(true);
        return void setPhase("startOver");
      }
      // Nothing anywhere: a credentialed sign-in carries a reauth, which goes
      // with the phrase; without one, it is asked for first.
      if (await client().hasReauth()) await toPhrase();
      else setPhase("reauth");
    } catch (e) {
      setError(text(e));
      setPhase("failed");
    }
  };

  onMount(async () => {
    const token = search().reauth;
    if (!token) return void (await begin());
    try {
      await client().reauthTake(token);
      // The token is spent by the next call; it does not stay in the address.
      void navigate({ to: "/private-messages", search: { ...search(), reauth: undefined }, replace: true });
      await confirmed();
    } catch (e) {
      setError(text(e));
      setPhase("failed");
    }
  });

  // Where an OIDC reauth comes back to: this page, in the same flow.
  const returnTo = () => {
    const q = new URLSearchParams();
    const s = search();
    if (s.from) q.set("from", s.from);
    if (startingOver()) q.set("step", "start-over");
    if (s.server) q.set("server", s.server);
    if (s.next) q.set("next", s.next);
    return `${location.origin}/private-messages${q.size ? `?${q}` : ""}`;
  };

  const signup = () => search().from === "signup";
  const eyebrow = (step: 1 | 2) => (signup() ? `Create your account · Step ${step + 1} of 3` : startingOver() ? `Start over · Step ${step} of 2` : `Set up private messages · Step ${step} of 2`);
  const later = () => (
    <button type="button" class="lu-link auth__switch" onClick={done}>
      {startingOver() ? "Not now" : "Set up later"}
    </button>
  );

  return (
    <Show
      when={phase() === "phrase" || phase() === "check"}
      fallback={
        <AuthPage under={phase() === "loading" || phase() === "finishing" || phase() === "startOver" ? undefined : later()}>
          <Show when={phase() === "loading" || phase() === "finishing"}>
            <Card title={phase() === "finishing" ? "Setting up private messages…" : "Private messages"} subtitle={phase() === "finishing" ? "Making this browser’s keys and telling the instance." : "Looking…"} />
          </Show>
          <Show when={phase() === "needs"}>
            <NeedsBrowser
              onLink={() => void navigate({ to: "/link" })}
              onRestore={() => void navigate({ to: "/restore" })}
              onStartOver={() => {
                setStartingOver(true);
                setPhase("startOver");
              }}
            />
          </Show>
          <Show when={phase() === "startOver"}>
            <StartOver
              again={again()}
              oidcName={info()?.oidcName ?? ""}
              returnTo={returnTo}
              onConfirmed={() => void confirmed()}
              onKeepLooking={() => {
                setStartingOver(false);
                setAgain(false);
                setPhase("needs");
              }}
            />
          </Show>
          <Show when={phase() === "reauth"}>
            <Card eyebrow={signup() ? undefined : "Set up private messages"} title="Confirm it’s you" subtitle={again() ? "Confirm it’s you once more to finish. A confirmation lasts five minutes; this browser keeps the phrase you checked." : "Private messages are tied to your account, so setting them up starts by confirming it’s you."}>
              <ConfirmItsYou oidcName={info()?.oidcName ?? ""} returnTo={returnTo} submit="Continue" onConfirmed={() => void confirmed()} />
            </Card>
          </Show>
          <Show when={phase() === "failed"}>
            <Card title="Private messages aren’t set up" subtitle="Nothing was sent to the people you talk to.">
              <div class="auth__form">
                <p class="auth__error" role="alert">
                  {error()}
                </p>
                <Button variant="primary" block onClick={() => void (words().length ? finish() : begin())}>
                  Try again
                </Button>
              </div>
            </Card>
          </Show>
        </AuthPage>
      }
    >
      <AuthPage logo={false} wide under={later()}>
        <Show
          when={phase() === "phrase"}
          fallback={
            <PhraseCheck
              eyebrow={eyebrow(2)}
              onConfirmed={() => void finish()}
              onWrong={() => {
                setWrong(true);
                setPhase("phrase");
              }}
              onShowAgain={() => {
                setWrong(false);
                setPhase("phrase");
              }}
            />
          }
        >
          <PhraseStep eyebrow={eyebrow(1)} words={words()} wrong={wrong()} onNext={() => setPhase("check")} />
        </Show>
      </AuthPage>
    </Show>
  );
}

/** "Private messages need this browser": the account's identity is on other devices. Link first, restore, and starting over last. */
export function NeedsBrowser(props: { onLink: () => void; onRestore: () => void; onStartOver: () => void }) {
  return (
    <Card title="Private messages need this browser" subtitle="Your account has private messages on another device. Bring this browser in to read and send them here.">
      <div class="auth__ways">
        <Button variant="primary" block onClick={() => props.onLink()}>
          Link from a device you use
        </Button>
        <Button block onClick={() => props.onRestore()}>
          Restore with your recovery phrase
        </Button>
      </div>
      <p class="auth__note private__last">
        Lost every device and the phrase?{" "}
        <button type="button" class="lu-link auth__switch" onClick={() => props.onStartOver()}>
          Start over
        </button>
      </p>
    </Card>
  );
}

/** The phrase, shown once (a browser keeps no copy): print it, or save it behind its warning, then check three words. */
export function PhraseStep(props: { eyebrow: string; words: string[]; wrong?: boolean; onNext?: () => void }) {
  const [saving, setSaving] = createSignal(false);
  const save = () => {
    const lines = [
      "Lumia recovery phrase",
      "",
      "Whoever has these words has your identity and your private messages. Lumia can't recover them.",
      "",
      ...props.words.map((w, i) => `${String(i + 1).padStart(2, " ")}  ${w}`),
      "",
    ];
    const url = URL.createObjectURL(new Blob([lines.join("\n")], { type: "text/plain" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = "lumia-recovery-phrase.txt";
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    setSaving(false);
  };
  // Where the identity begins, said on any origin but the flagship's (identity spec 7).
  const host = () => (typeof location === "undefined" ? "" : location.origin === FLAGSHIP_ORIGIN ? "" : location.host);
  return (
    <Card class="private__phrase" eyebrow={props.eyebrow} title="Save your recovery phrase" subtitle="Your private messages are encrypted with keys only your devices hold. These 24 words bring them back if you lose every device.">
      <Show when={props.wrong}>
        <p class="auth__error" role="alert">
          Those words didn’t match. Here is your phrase again: check what you wrote down, then the check asks for three other words.
        </p>
      </Show>
      <Notice tone="caution" icon="key" class="is-printed">
        Whoever has these words has your identity and your private messages. Lumia can’t recover them. This browser won’t keep a copy, so this is the only time they’re shown.
      </Notice>
      <PhraseWords words={props.words} columns={3} class="is-printed" />
      <Show when={host()}>
        <Notice icon="shield">Your identity begins on {host()}, so it trusts the people who run it, as every site you type into does. To use it elsewhere too, install the Lumia app and link it.</Notice>
      </Show>
      <div class="private__keep">
        <Button onClick={() => window.print()}>Print</Button>
        <Button icon="download" aria-expanded={saving()} onClick={() => setSaving(true)}>
          Save as a file
        </Button>
      </div>
      <Show when={saving()}>
        <PhraseSaveWarning onSave={save} onCancel={() => setSaving(false)} />
      </Show>
      <Button variant="primary" block onClick={() => props.onNext?.()}>
        I’ve saved them — check three words
      </Button>
    </Card>
  );
}

/** Three of the 24 words, at positions the core chose, each completed from the wordlist as it is typed. */
export function PhraseCheck(props: { eyebrow: string; onConfirmed: () => void; onWrong: () => void; onShowAgain: () => void }) {
  const [quiz] = createResource(() => client().identityPhraseQuiz());
  const [answers, setAnswers] = createSignal<string[]>(["", "", ""]);
  const [offered, setOffered] = createSignal<string[][]>([[], [], []]);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const answer = (i: number, value: string) => {
    setAnswers((a) => a.map((v, j) => (j === i ? value : v)));
    const prefix = value.trim().toLowerCase();
    const show = (list: string[]) => {
      // A word typed whole, with nothing longer that begins with it, needs no list.
      if (answers()[i]?.trim().toLowerCase() !== prefix) return;
      setOffered((o) => o.map((v, j) => (j === i ? (list.length === 1 && list[0] === prefix ? [] : list) : v)));
    };
    if (!prefix) show([]);
    else void client().phraseCompletions(prefix).then(show, () => show([]));
  };
  const confirm = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      if (await client().identityPhraseConfirm(answers().map((a) => a.trim().toLowerCase()))) props.onConfirmed();
      else props.onWrong();
    } catch (err) {
      setError(text(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card eyebrow={props.eyebrow} title="Check your phrase" subtitle="Type these three words from what you wrote down, to make sure it’s right.">
      <form class="auth__form" onSubmit={(e) => void confirm(e)}>
        <Show when={quiz()} fallback={<p class="auth__note">{quiz.error ? text(quiz.error) : "Choosing three words…"}</p>}>
          {(positions) => (
            <Index each={positions()}>
              {(position, i) => <PhraseWordField position={position()} value={answers()[i] ?? ""} completions={offered()[i] ?? []} onInput={(v) => answer(i, v)} onPick={(w) => answer(i, w)} />}
            </Index>
          )}
        </Show>
        <Show when={error()}>
          <p class="auth__error" role="alert">
            {error()}
          </p>
        </Show>
        <Button variant="primary" type="submit" block disabled={busy() || !quiz() || answers().some((a) => !a.trim())}>
          Confirm
        </Button>
        <Button block onClick={() => props.onShowAgain()}>
          Show the phrase again
        </Button>
      </form>
    </Card>
  );
}

/**
 * A fresh reauth (identity spec 10): the password, a code mailed to the
 * account's address, or a fresh login at the instance's OIDC provider,
 * which leaves the page and comes back to `returnTo` with the token.
 */
export function ConfirmItsYou(props: { oidcName: string; returnTo: () => string; submit: string; danger?: boolean; label?: string; onConfirmed: () => void }) {
  const [password, setPassword] = createSignal("");
  const [mailed, setMailed] = createSignal(false);
  const [code, setCode] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const submit = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      if (mailed()) await client().reauthCode(code().trim(), true);
      else await client().reauthPassword(password());
      props.onConfirmed();
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(ce?.code === "UNAUTHENTICATED" ? (mailed() ? "That code was not accepted. Check it, or ask for a new one." : "That password was not accepted.") : ce?.code === "RATE_LIMITED" ? "Too many tries. Wait a little, then try again." : text(err));
    } finally {
      setBusy(false);
    }
  };
  const mail = async () => {
    setBusy(true);
    setError("");
    try {
      await client().reauthMailCode();
      setMailed(true);
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(
        ce?.code === "FAILED_PRECONDITION"
          ? "This account has no email address. Confirm with your password instead."
          : ce?.code === "POLICY_DENIED"
            ? "This instance doesn’t send codes. Confirm with your password instead."
            : ce?.code === "RATE_LIMITED"
              ? "Too many codes in the last hour. Try again later, or use your password."
              : text(err),
      );
    } finally {
      setBusy(false);
    }
  };
  const oidc = async () => {
    setBusy(true);
    setError("");
    try {
      location.assign(await client().reauthOidcUrl(props.returnTo()));
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(ce?.code === "FAILED_PRECONDITION" ? `This account doesn’t sign in with ${props.oidcName}.` : text(err));
      setBusy(false);
    }
  };
  return (
    <form class="auth__form" onSubmit={(e) => void submit(e)}>
      <Show
        when={mailed()}
        fallback={
          <TextField
            label={props.label ?? "Password"}
            reveal
            value={password()}
            onInput={(e) => setPassword(e.currentTarget.value)}
            autocomplete="current-password"
            placeholder="Your password"
            action={{
              label: "Email me a code instead",
              onClick: (e) => {
                e.preventDefault();
                void mail();
              },
            }}
          />
        }
      >
        <p class="auth__note">A code is on its way to your account’s email address. It works once, for fifteen minutes.</p>
        <TextField
          label="Code from the email"
          value={code()}
          onInput={(e) => setCode(e.currentTarget.value)}
          inputmode="numeric"
          autocomplete="one-time-code"
          placeholder="6 digits"
          action={{
            label: "Use your password instead",
            onClick: (e) => {
              e.preventDefault();
              setMailed(false);
              setCode("");
            },
          }}
        />
      </Show>
      <Show when={error()}>
        <p class="auth__error" role="alert">
          {error()}
        </p>
      </Show>
      <Button variant={props.danger ? "danger" : "primary"} type="submit" block disabled={busy() || (mailed() ? !code().trim() : !password())}>
        {props.submit}
      </Button>
      <Show when={props.oidcName}>
        <Divider variant="or" />
        <Button icon="globe" block type="button" disabled={busy()} onClick={() => void oidc()}>
          Continue with {props.oidcName}
        </Button>
      </Show>
    </form>
  );
}

/**
 * Starting over with a new identity: what it costs, said before the reauth
 * that does it (identity spec 4.7). `again`: the new identity is already
 * made here (a reauth ran out after it, or moving the account to it
 * failed), so finishing moves the account to it and no new phrase comes.
 */
export function StartOver(props: { again?: boolean; oidcName: string; returnTo: () => string; onConfirmed: () => void; onKeepLooking: () => void }) {
  const points = () => ["Your account, servers and channels stay as they are.", "Private messages from before stay unreadable.", "People you talk to privately will see that your key changed.", ...(props.again ? [] : ["You’ll get a new recovery phrase to save."])];
  return (
    <Card title={props.again ? "Finish starting over" : "Start over"} subtitle="With a new identity">
      <p class="auth__note private__lead">
        {props.again
          ? "This browser holds the new identity, made from the phrase you checked, but your account isn’t using it yet. A confirmation lasts five minutes: confirm it’s you once more to finish."
          : "Without a device or your recovery phrase, your old identity can’t come back. You can start over in this browser with a new one."}
      </p>
      <ul class="private__points">
        <For each={points()}>{(point) => <li>{point}</li>}</For>
      </ul>
      <Notice tone="caution" icon="key">
        Only do this if the old phrase is truly gone. If you find it later, your old identity can be restored, but not onto this account.
      </Notice>
      <ConfirmItsYou oidcName={props.oidcName} returnTo={props.returnTo} label="Confirm with your password" submit={props.again ? "Finish starting over" : "Start over with a new identity"} danger onConfirmed={props.onConfirmed} />
      <Button block onClick={() => props.onKeepLooking()}>
        Keep looking for the phrase
      </Button>
    </Card>
  );
}

/**
 * Restore with the phrase (identity spec 4.3), which a browser does only
 * on the flagship's origin (spec 7); anywhere else the page says where to
 * restore and to link this browser after. Reached from sign-in and sign-up
 * ("Already use Lumia on another device?") and from "Private messages need
 * this browser"; restoring signs in by the identity. It asks first whether
 * a device with the private messages is still at hand, since then linking
 * is what is wanted; once the phrase is in, the devices restoring would
 * stop are read, and when there are any the person ticks the ones they
 * still have (the "PhoneRestoreAsk" and "PhoneRestoreKeep" boards). The
 * phrase is held only between those two steps, never kept.
 */
export function Restore() {
  const navigate = useNavigate();
  const here = () => typeof location !== "undefined" && location.origin === FLAGSHIP_ORIGIN;
  const [step, setStep] = createSignal<"ask" | "phrase" | "keep">("ask");
  const [phrase, setPhrase] = createSignal("");
  const [stopping, setStopping] = createSignal<DeviceSummary[]>([]);
  onCleanup(() => setPhrase(""));
  const restored = () => {
    setPhrase("");
    identityChanged();
    void navigate({ to: "/app" });
  };
  const submit = async (words: string) => {
    const devices = await client().identityRecoverDevices(words);
    if (afterPhrase(devices) === "restore") {
      await client().identityRecover(words, LABEL);
      return restored();
    }
    setPhrase(words);
    setStopping(devices.filter((d) => !d.revoked));
    setStep("keep");
  };
  const keep = async (ids: string[]) => {
    await client().identityRecoverKeeping(phrase(), LABEL, ids);
    restored();
  };
  const back = (
    <button
      type="button"
      class="lu-link auth__switch"
      onClick={() => {
        if (step() === "keep") return void setStep("phrase");
        if (step() === "phrase" && here()) {
          setPhrase("");
          return void setStep("ask");
        }
        history.back();
      }}
    >
      Back
    </button>
  );
  return (
    <AuthPage under={back}>
      <Show
        when={here()}
        fallback={
          <Card title="Restore in a Lumia app" subtitle="A browser restores your identity only on my.lumia.chat.">
            <div class="auth__form">
              <p class="auth__note">Restore with your recovery phrase in a Lumia app, or on my.lumia.chat, and then link this browser from there.</p>
              <Button variant="primary" block onClick={() => void navigate({ to: "/link" })}>
                Link this browser
              </Button>
            </div>
          </Card>
        }
      >
        <Show when={step() === "ask"}>
          <RestoreAsk onLink={() => void navigate({ to: "/link" })} onPhrase={() => setStep("phrase")} />
        </Show>
        <Show when={step() === "phrase"}>
          <RestoreForm initial={phrase()} onRestore={submit} />
        </Show>
        <Show when={step() === "keep"}>
          <RestoreKeep devices={stopping()} onRestore={keep} />
        </Show>
      </Show>
    </AuthPage>
  );
}

/** "Do you still have a device with your private messages?": linking from it, or on to the phrase. */
export function RestoreAsk(props: { onLink: () => void; onPhrase: () => void }) {
  return (
    <Card title="Restore" subtitle="With your recovery phrase">
      <h2 class="lu-title-channel private__question">Do you still have a device with your private messages?</h2>
      <p class="auth__note private__lead">If you do, link this browser from it instead. Restoring is for when your devices are gone: it stops the ones you don’t keep.</p>
      <div class="auth__ways">
        <Button variant="primary" icon="phone" block onClick={() => props.onLink()}>
          Yes, link from it
        </Button>
        <Button block onClick={() => props.onPhrase()}>
          No, restore with my phrase
        </Button>
      </div>
    </Card>
  );
}

/** "Keep any devices?": the devices restoring would stop, none ticked at first; restoring keeps the ticked ones and stops the rest. */
export function RestoreKeep(props: { devices: DeviceSummary[]; initial?: string[]; onRestore: (keep: string[]) => Promise<void> }) {
  const [chosen, setChosen] = createSignal<string[]>(props.initial ?? []);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const entries = () => props.devices.map((d) => ({ id: d.deviceId, kind: d.kind, label: deviceName(d), meta: addedLine(d) }));
  const toggle = (id: string) => setChosen((c) => (c.includes(id) ? c.filter((x) => x !== id) : [...c, id]));
  const restore = async () => {
    setBusy(true);
    setError("");
    try {
      await props.onRestore(chosen());
    } catch (err) {
      setError(text(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card title="Keep any devices?" subtitle="Before this browser restores">
      <p class="auth__note private__lead">Tick the devices you still have. The others stop reading your private messages, so a lost one can’t keep reading them.</p>
      <DeviceChoices devices={entries()} chosen={chosen()} onToggle={toggle} />
      <Notice tone="caution" icon="shield">
        Keep only devices you have with you now. A kept device you’ve lost would keep reading your private messages.
      </Notice>
      <Show when={error()}>
        <p class="auth__error" role="alert">
          {error()}
        </p>
      </Show>
      <Button variant="primary" block disabled={busy()} onClick={() => void restore()}>
        {keepWords(chosen().length)}
      </Button>
    </Card>
  );
}

/** The phrase typed into one field, counted, and restored from at 24 words. */
export function RestoreForm(props: { initial?: string; onRestore: (phrase: string) => Promise<void> }) {
  const [phrase, setPhrase] = createSignal(props.initial ?? "");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const count = () => phrase().trim().split(/\s+/).filter(Boolean).length;
  const restore = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      await props.onRestore(phrase().trim().toLowerCase().split(/\s+/).join(" "));
    } catch (err) {
      setError(text(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card title="Restore" subtitle="With your recovery phrase">
      <form class="auth__form" onSubmit={(e) => void restore(e)}>
        <div class="lu-textfield">
          <div class="lu-textfield__top">
            <label for="restore-words" class="lu-textfield__label">
              Your 24 words, in order
            </label>
            <span id="restore-count" class="private__count">
              {count()} of 24
            </span>
          </div>
          <textarea id="restore-words" class="private__words" rows={6} autocomplete="off" autocapitalize="none" spellcheck={false} aria-describedby="restore-count" value={phrase()} onInput={(e) => setPhrase(e.currentTarget.value)} />
        </div>
        <p class="auth__note">Restoring makes this browser your identity’s only device. Every other device of yours stops reading private messages, so a lost one can’t keep reading them.</p>
        <p class="auth__note">Messages from before can’t be read in a browser.</p>
        <Show when={error()}>
          <p class="auth__error" role="alert">
            {error()}
          </p>
        </Show>
        <Button variant="primary" type="submit" block disabled={busy() || count() !== 24}>
          Restore
        </Button>
        <Show when={count() !== 24}>
          <p class="auth__note private__hint">Restore turns on once all 24 words are in.</p>
        </Show>
      </form>
    </Card>
  );
}
