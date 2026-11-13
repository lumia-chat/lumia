// A server's settings (architecture 6.7): its picture and name, the
// channels newcomers join (architecture 6.8), and how long it keeps its
// messages. Offered from the menu under the server's
// name to whoever may change them (`canManage`, from the account's own
// membership); the instance decides all the same. Shown in the server's
// main pane until a channel is chosen. No board draws it: it is the
// Profile page's cards, worded as the desktop shell's and the phones'.
import { Show, createEffect, createMemo, createSignal, on } from "solid-js";
import { Button, Card, ChannelHeader, SegmentedTabs, TextField } from "../design/components";
import { ClientError, type ServerSummary, type SnapshotSummary } from "../client/bridge";
import { avatarSrc, client, pictureRefusal, queryClient } from "../client/state";
import { initialsOf } from "./app";
import { DefaultChannelsCard } from "./channels";

/** The choices offered, in days; 0 keeps messages for ever. */
export const RETENTION_CHOICES = [0, 365, 90, 30];

/** What a retention reads as. */
export function retentionWords(days: number): string {
  return days === 0 ? "Forever" : days === 365 ? "1 year" : days === 1 ? "1 day" : `${days} days`;
}

/** Whether keeping messages `chosen` days instead of `current` deletes some: for ever is the longest. */
export function shortens(current: number, chosen: number): boolean {
  return chosen !== 0 && (current === 0 || chosen < current);
}

/** Why a setting was not changed. */
function settingRefusal(err: unknown): string {
  const ce = err instanceof ClientError ? err : null;
  if (ce?.code === "PERMISSION_DENIED") return "You cannot change this server's settings.";
  if (ce?.code === "INVALID_ARGUMENT" && ce.details.field === "name") return "A server name is 1 to 100 characters.";
  return err instanceof Error ? err.message : String(err);
}

export function ServerSettings(props: { server: ServerSummary; snapshot: SnapshotSummary | undefined; back: () => void }) {
  // The snapshot each call answers with is the account as it now is: the
  // rail and the sidebar follow at once.
  const took = (snap: SnapshotSummary) => queryClient.setQueryData(["snapshot"], snap);

  const [name, setName] = createSignal(props.server.name);
  const [nameBusy, setNameBusy] = createSignal(false);
  const [nameError, setNameError] = createSignal("");
  const [nameSaved, setNameSaved] = createSignal(false);
  const saveName = async (e: Event) => {
    e.preventDefault();
    const trimmed = name().trim();
    if (!trimmed || trimmed === props.server.name || nameBusy()) return;
    setNameBusy(true);
    setNameError("");
    setNameSaved(false);
    try {
      took(await client().updateServer(props.server.serverId, trimmed));
      setName(trimmed);
      setNameSaved(true);
    } catch (err) {
      setNameError(settingRefusal(err));
    } finally {
      setNameBusy(false);
    }
  };

  const [retention, setRetention] = createSignal(props.server.retentionDays);
  const [retentionBusy, setRetentionBusy] = createSignal(false);
  const [retentionError, setRetentionError] = createSignal("");
  const [retentionSaved, setRetentionSaved] = createSignal(false);
  const choices = () => (RETENTION_CHOICES.includes(props.server.retentionDays) ? RETENTION_CHOICES : [...RETENTION_CHOICES, props.server.retentionDays]);
  const saveRetention = async () => {
    if (retentionBusy() || retention() === props.server.retentionDays) return;
    setRetentionBusy(true);
    setRetentionError("");
    setRetentionSaved(false);
    try {
      took(await client().updateServer(props.server.serverId, undefined, retention()));
      setRetentionSaved(true);
    } catch (err) {
      setRetentionError(settingRefusal(err));
    } finally {
      setRetentionBusy(false);
    }
  };

  // Another server on screen is another page. The id, not the server: a
  // snapshot that changes this one (its own save among them) is the same page.
  const serverId = createMemo(() => props.server.serverId);
  createEffect(
    on(
      serverId,
      () => {
        setName(props.server.name);
        setRetention(props.server.retentionDays);
        setNameError("");
        setNameSaved(false);
        setRetentionError("");
        setRetentionSaved(false);
      },
      { defer: true },
    ),
  );

  // The picture: chosen as a file, which the instance makes its own square of.
  let pictureInput: HTMLInputElement | undefined;
  const [pictureBusy, setPictureBusy] = createSignal(false);
  const [pictureError, setPictureError] = createSignal("");
  const [pictureSaved, setPictureSaved] = createSignal(false);
  const picture = () => avatarSrc(props.server.iconUrl);
  const pictureCall = async (call: () => Promise<SnapshotSummary>) => {
    setPictureError("");
    setPictureSaved(false);
    setPictureBusy(true);
    try {
      took(await call());
      setPictureSaved(true);
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setPictureError(ce?.code === "PERMISSION_DENIED" ? settingRefusal(err) : pictureRefusal(err));
    } finally {
      setPictureBusy(false);
    }
  };
  const choosePicture = async (file: File | undefined) => {
    if (!file) return;
    // What the instance would refuse for its size is not sent at all.
    if (file.size > 8 * 1024 * 1024) {
      setPictureSaved(false);
      setPictureError("That picture is too large. Choose one under 8 MB.");
      return;
    }
    const bytes = new Uint8Array(await file.arrayBuffer());
    await pictureCall(() => client().setServerIcon(props.server.serverId, bytes, file.type));
  };

  return (
    <>
      <ChannelHeader kind="page" name="Server settings" topic={props.server.name} actions={[]} onBack={props.back} backLabel={`Back to ${props.server.name}`} />
      <div class="lu-page">
        <Card title="Overview" subtitle="How the server shows on the rail and in invites.">
          <div class="app__photo">
            <span class="app__server-picture" aria-hidden="true">
              <Show when={picture()} fallback={initialsOf(props.server.name)}>
                <img class="lu-avatar__photo" src={picture()} alt="" decoding="async" draggable={false} />
              </Show>
            </span>
            <div class="app__photo-words">
              <div class="app__row">
                <input
                  ref={pictureInput}
                  type="file"
                  accept="image/png,image/jpeg,image/webp,image/gif"
                  class="lu-sr"
                  tabindex={-1}
                  aria-label="Server picture"
                  onChange={(e) => {
                    const file = e.currentTarget.files?.[0];
                    // The same file chosen again is a change too.
                    e.currentTarget.value = "";
                    void choosePicture(file);
                  }}
                />
                <Button size="sm" disabled={pictureBusy()} onClick={() => pictureInput?.click()}>
                  {pictureBusy() ? "Sending…" : picture() ? "Change picture…" : "Choose a picture…"}
                </Button>
                <Show when={picture()}>
                  <Button size="sm" disabled={pictureBusy()} onClick={() => void pictureCall(() => client().removeServerIcon(props.server.serverId))}>
                    Remove
                  </Button>
                </Show>
              </div>
              <Show when={pictureError()} fallback={<p class="auth__note" role="status">{pictureSaved() ? "Saved." : "A PNG, JPEG, WebP or GIF up to 8 MB. Everyone sees its middle, as a circle."}</p>}>
                <p class="auth__error" role="alert">
                  {pictureError()}
                </p>
              </Show>
            </div>
          </div>
          <form class="auth__form" onSubmit={(e) => void saveName(e)}>
            <TextField label="Name" value={name()} onInput={(e) => setName(e.currentTarget.value)} placeholder="Acme" autocomplete="off" />
            <Show when={nameError()}>
              <p class="auth__error" role="alert">
                {nameError()}
              </p>
            </Show>
            <Show when={nameSaved()}>
              <p class="auth__note" role="status">
                Saved.
              </p>
            </Show>
            <Button variant="primary" size="md" type="submit" disabled={nameBusy() || !name().trim() || name().trim() === props.server.name}>
              Save
            </Button>
          </form>
        </Card>
        <DefaultChannelsCard server={props.server} snapshot={props.snapshot} />
        <Card title="Message history" subtitle="How long this server keeps its messages.">
          <div class="auth__form">
            <SegmentedTabs
              label="Keep messages"
              tabs={choices().map((days) => ({ id: String(days), label: retentionWords(days) }))}
              value={String(retention())}
              onChange={(id) => {
                setRetention(Number(id));
                setRetentionSaved(false);
              }}
            />
            <Show when={shortens(props.server.retentionDays, retention())}>
              <p class="auth__error">Messages older than {retentionWords(retention())} will be deleted for everyone. This cannot be undone.</p>
            </Show>
            <Show when={retentionError()}>
              <p class="auth__error" role="alert">
                {retentionError()}
              </p>
            </Show>
            <Show when={retentionSaved()}>
              <p class="auth__note" role="status">
                Saved.
              </p>
            </Show>
            <Button variant="primary" size="md" disabled={retentionBusy() || retention() === props.server.retentionDays} onClick={() => void saveRetention()}>
              Save
            </Button>
          </div>
        </Card>
      </div>
    </>
  );
}
