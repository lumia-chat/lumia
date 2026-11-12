// Link this browser as a web device of an identity (identity spec 4.2,
// architecture 11.4): show the request for a native device to approve
// (the CLI's `identity approve <request>` for now), then claim the code
// it prints and sign in by identity proof.
import { Show, createSignal } from "solid-js";
import { useNavigate } from "@tanstack/solid-router";
import { Button, Card, TextField } from "../design/components";
import { client } from "../client/state";

export function Link() {
  const navigate = useNavigate();
  const [request, setRequest] = createSignal("");
  const [code, setCode] = createSignal("");
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  const start = async () => {
    setError("");
    try {
      setRequest(await client().identityLinkRequest("browser"));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const claim = async (e: Event) => {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      await client().identityClaim(code().trim());
      navigate({ to: "/app" });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="auth">
      <Card title="Link this browser" subtitle="A device you already use approves this one; the recovery phrase never enters the browser.">
        <Show
          when={request()}
          fallback={
            <div class="auth__form">
              <p class="auth__note">{`This browser gets its own device key, scoped to ${typeof location !== "undefined" ? location.origin : "this origin"}.`}</p>
              <Button variant="primary" block onClick={() => void start()}>
                Start linking
              </Button>
            </div>
          }
        >
          <form class="auth__form" onSubmit={(e) => void claim(e)}>
            <p class="auth__note">On a device that holds your identity, approve this request and enter the code it gives you.</p>
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
        <p class="auth__foot">
          <button type="button" class="lu-link auth__switch" onClick={() => navigate({ to: "/sign-in" })}>
            Back to sign in
          </button>
        </p>
      </Card>
    </div>
  );
}
