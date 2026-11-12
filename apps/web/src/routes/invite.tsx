// An invite link: `/invite/{code}` on the instance. The trust card and
// what the link leads to come first (architecture 6.1), and nothing is
// sent until the person chooses: join with the account they are signed
// in with here, sign in, or create an account that joins at creation.
import { Show, createResource, createSignal, onMount } from "solid-js";
import { useNavigate, useParams, useSearch } from "@tanstack/solid-router";
import { Button, Card } from "../design/components";
import { AuthPage, TrustCard } from "./auth-page";
import { ClientError, type InvitePreviewSummary } from "../client/bridge";
import { SERVED_BY_INSTANCE, client, openInstance, opened, queryClient, refreshSignedIn, savedInstance } from "../client/state";
import type { SignInSearch } from "./index";

export function Invite() {
  const navigate = useNavigate();
  const params = useParams({ from: "/invite/$code" }) as () => { code: string };
  const search = useSearch({ from: "/invite/$code" }) as () => SignInSearch;
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [url, setUrl] = createSignal<string | null>(null);

  // The instance the link is on: this page's origin when the instance
  // serves the UI, else the one the link named, else the saved one.
  onMount(() => {
    setUrl(SERVED_BY_INSTANCE ? location.origin : (search().instance ?? savedInstance()));
  });

  const [preview] = createResource(url, async (u): Promise<{ info: InvitePreviewSummary; signedIn: boolean }> => {
    const b = client();
    if (!opened()) await openInstance(u);
    const info = await b.invitePreview(params().code);
    const signedIn = await refreshSignedIn();
    return { info, signedIn };
  });

  const join = async () => {
    setBusy(true);
    setError("");
    try {
      const serverId = await client().acceptInvite(params().code);
      await queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      navigate({ to: "/app", search: { server: serverId } });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  const toSignIn = () => navigate({ to: "/sign-in", search: { invite: params().code, instance: url() ?? undefined } });
  const notFound = () => preview.error instanceof ClientError && preview.error.code === "NOT_FOUND";

  return (
    <AuthPage
      under={
        <Show when={preview() && !preview()?.signedIn}>
          New to Lumia?{" "}
          <button type="button" class="lu-link auth__switch" onClick={toSignIn}>
            Create an account
          </button>
        </Show>
      }
    >
      <Show
        when={preview()}
        fallback={
          <Card title={notFound() ? "This invite is gone" : preview.error ? "Could not reach the instance" : "You are invited"} subtitle={notFound() ? "It is unknown, expired or used up. Ask for a new one." : preview.error ? String((preview.error as Error)?.message ?? preview.error) : "Looking up the invite…"}>
            <Show when={!url() && !preview.loading}>
              <p class="auth__note">Open this link on the instance it came from, or paste it into Home's "Join a server".</p>
            </Show>
          </Card>
        }
      >
        {(p) => (
          <Card title={p().info.serverName} subtitle={`${p().info.memberCount} ${p().info.memberCount === 1 ? "member" : "members"}${p().info.asGuest ? " · you would join as a guest" : ""}`}>
            <TrustCard host={p().info.host} operatorName={p().info.operatorName} />
            <div class="auth__form">
              <Show when={error()}>
                <p class="auth__error" role="alert">
                  {error()}
                </p>
              </Show>
              <Show
                when={p().signedIn}
                fallback={
                  <Button variant="primary" type="button" block onClick={toSignIn}>
                    Sign in to join
                  </Button>
                }
              >
                <Button variant="primary" type="button" block disabled={busy()} onClick={() => void join()}>
                  Join {p().info.serverName}
                </Button>
              </Show>
            </div>
          </Card>
        )}
      </Show>
    </AuthPage>
  );
}
