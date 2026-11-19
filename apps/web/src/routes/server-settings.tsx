// A server's settings, in sections (architecture 6.7 and 7.5; the "Server
// settings" board): Overview, its picture and name, what the tag beside a
// name shows, the channels newcomers join (architecture 6.8), how long it
// keeps its messages, and its files (specs/media.md 4.5); Roles; and
// Members (`roles.tsx`). Offered from the menu under the server's name to
// whoever may use a section (`settingsSections`, from the account's own
// membership); the instance decides all the same. Shown in the server's
// main pane until a channel is chosen, the sections beside the page, or
// as a screen of their own where the frame is folded.
import { Match, Show, Switch, createEffect, createMemo, createResource, createSignal, on, onCleanup } from "solid-js";
import { Button, Card, ChannelHeader, Checkbox, PictureChoices, SectionList, SegmentedTabs, TagSketch, TextField, type IconName } from "../design/components";
import { ClientError, type ServerSummary, type SnapshotSummary } from "../client/bridge";
import { usageWords } from "../client/media";
import { TAG_CHOICES, TAG_HEADING, TAG_NOTE, TAG_SUBTITLE, rolesCount, settingsSections, type SettingsSection } from "../client/roles";
import { avatarSrc, client, membersKept, membersQuery, pictureRefusal, queryClient } from "../client/state";
import { initialsOf } from "./app";
import { DefaultChannelsCard } from "./channels";
import { MembersSection, RolesSection } from "./roles";

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

const SECTIONS: Record<SettingsSection, { label: string; icon: IconName }> = {
  overview: { label: "Overview", icon: "settings" },
  roles: { label: "Roles", icon: "shield" },
  members: { label: "Members", icon: "members" },
};

/** True while the window matches `query`. */
function useMedia(query: string): () => boolean {
  const media = matchMedia(query);
  const [matches, setMatches] = createSignal(media.matches);
  const changed = () => setMatches(media.matches);
  media.addEventListener("change", changed);
  onCleanup(() => media.removeEventListener("change", changed));
  return matches;
}

export function ServerSettings(props: { server: ServerSummary; snapshot: SnapshotSummary | undefined; back: () => void; close: () => void }) {
  const sections = createMemo(() => settingsSections(props.server));
  // Folded, the sections are a screen of their own and none is shown until one is chosen.
  const folded = useMedia("(max-width: 719px)");
  const touch = useMedia("(pointer: coarse)");
  const [chosen, setChosen] = createSignal<SettingsSection | undefined>();
  const serverId = createMemo(() => props.server.serverId);
  createEffect(on(serverId, () => setChosen(undefined), { defer: true }));
  // A section this account no longer has (a role taken away while the page is open) is not shown.
  const section = createMemo(() => {
    const wanted = chosen();
    if (wanted && sections().includes(wanted)) return wanted;
    return folded() ? undefined : sections()[0];
  });
  // With no section left to use, the page goes.
  createEffect(() => {
    if (sections().length === 0) props.close();
  });
  // The members, for Members and for how many hold each role; read only by those sections.
  const members = membersQuery(() => (section() === "roles" || section() === "members" ? props.server.serverId : undefined));
  const me = () => members.data?.find((m) => m.accountId === props.snapshot?.accountId);
  const note = (id: SettingsSection) => (id === "roles" ? rolesCount(props.server.roles) : undefined);
  const list = () => sections().map((id) => ({ id, label: SECTIONS[id].label, icon: SECTIONS[id].icon, note: note(id) }));
  return (
    <>
      <Show
        when={folded() && section()}
        fallback={<ChannelHeader kind="page" name="Server settings" topic={props.server.name} actions={[]} onBack={props.back} backLabel={`Back to ${props.server.name}`} />}
      >
        {(shown) => <ChannelHeader kind="page" name={SECTIONS[shown()].label} topic={props.server.name} actions={[]} onBack={() => setChosen(undefined)} backLabel="Back to Server settings" />}
      </Show>
      <div class="lu-settings">
        <Show when={!folded() || !section()}>
          <SectionList label="Server settings" sections={list()} value={folded() ? undefined : section()} onChange={(id) => setChosen(id as SettingsSection)} />
        </Show>
        <Show when={section()}>
          <div class="lu-page">
            <Switch>
              <Match when={section() === "overview"}>
                <OverviewSection server={props.server} snapshot={props.snapshot} />
              </Match>
              <Match when={section() === "roles"}>
                <RolesSection server={props.server} members={members.data} me={me()} menuOnly={folded() || touch()} />
              </Match>
              <Match when={section() === "members"}>
                <MembersSection server={props.server} members={members.data} myId={props.snapshot?.accountId ?? ""} error={members.error ? settingRefusal(members.error) : undefined} />
              </Match>
            </Switch>
          </div>
        </Show>
      </div>
    </>
  );
}

// Overview: what the server's settings were before they had sections, and
// what the tag beside a name shows.
function OverviewSection(props: { server: ServerSummary; snapshot: SnapshotSummary | undefined }) {
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

  // "Show deleted messages": saved as it is turned, apart from the retention's Save (architecture 6.7).
  const [deletedBusy, setDeletedBusy] = createSignal(false);
  const [deletedError, setDeletedError] = createSignal("");
  const toggleDeleted = async (show: boolean) => {
    if (deletedBusy()) return;
    setDeletedBusy(true);
    setDeletedError("");
    try {
      took(await client().setShowDeleted(props.server.serverId, show));
      // What the core reads of every conversation here has changed.
      void queryClient.invalidateQueries({ queryKey: ["messages"] });
    } catch (err) {
      setDeletedError(settingRefusal(err));
    } finally {
      setDeletedBusy(false);
    }
  };

  // "Tag beside names": roles or titles, saved as it is chosen (architecture 7.5).
  const [tagBusy, setTagBusy] = createSignal(false);
  const [tagError, setTagError] = createSignal("");
  const chooseTag = async (shows: "roles" | "titles") => {
    if (tagBusy() || (shows === "titles") === props.server.tagTitles) return;
    setTagBusy(true);
    setTagError("");
    try {
      took(await client().setTagTitles(props.server.serverId, shows === "titles"));
      // The tags beside the names are worked out again from what the core now keeps.
      void membersKept(props.server.serverId);
    } catch (err) {
      setTagError(settingRefusal(err));
    } finally {
      setTagBusy(false);
    }
  };

  // Another server on screen is another page. The id, not the server: a
  // snapshot that changes this one (its own save among them) is the same page.
  const serverId = createMemo(() => props.server.serverId);

  // Its files (specs/media.md 4.5): what they take, how long attachments are
  // kept and what a full server does, the last two saved together.
  const [storage] = createResource(serverId, (id) => client().serverStorage(id));
  const [keepFiles, setKeepFiles] = createSignal(props.server.attachmentRetentionDays);
  const [deleteOldest, setDeleteOldest] = createSignal(props.server.deleteOldestWhenFull);
  const [filesBusy, setFilesBusy] = createSignal(false);
  const [filesError, setFilesError] = createSignal("");
  const [filesSaved, setFilesSaved] = createSignal(false);
  const fileChoices = () => (RETENTION_CHOICES.includes(props.server.attachmentRetentionDays) ? RETENTION_CHOICES : [...RETENTION_CHOICES, props.server.attachmentRetentionDays]);
  const filesChanged = () => keepFiles() !== props.server.attachmentRetentionDays || deleteOldest() !== props.server.deleteOldestWhenFull;
  const saveFiles = async () => {
    if (filesBusy() || !filesChanged()) return;
    setFilesBusy(true);
    setFilesError("");
    setFilesSaved(false);
    try {
      took(await client().setServerFiles(props.server.serverId, keepFiles(), deleteOldest()));
      setFilesSaved(true);
    } catch (err) {
      setFilesError(settingRefusal(err));
    } finally {
      setFilesBusy(false);
    }
  };
  createEffect(
    on(
      serverId,
      () => {
        setName(props.server.name);
        setRetention(props.server.retentionDays);
        setKeepFiles(props.server.attachmentRetentionDays);
        setDeleteOldest(props.server.deleteOldestWhenFull);
        setFilesError("");
        setFilesSaved(false);
        setTagError("");
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
      <Card title={TAG_HEADING} subtitle={TAG_SUBTITLE}>
        <div class="auth__form">
          <PictureChoices
            label={TAG_HEADING}
            class="lu-pictiles--two"
            value={props.server.tagTitles ? "titles" : "roles"}
            onChange={(shows) => void chooseTag(shows)}
            options={[
              { value: "roles", label: TAG_CHOICES.roles.label, caption: TAG_CHOICES.roles.caption, picture: <TagSketch shows="roles" /> },
              { value: "titles", label: TAG_CHOICES.titles.label, caption: TAG_CHOICES.titles.caption, picture: <TagSketch shows="titles" /> },
            ]}
          />
          <Show when={tagError()} fallback={<p class="auth__note">{TAG_NOTE}</p>}>
            <p class="auth__error" role="alert">
              {tagError()}
            </p>
          </Show>
        </div>
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
          <div class="app__switch">
            <Checkbox label="Show deleted messages" checked={props.server.showDeleted} disabled={deletedBusy()} onChange={(e) => void toggleDeleted(e.currentTarget.checked)} />
            <p class="auth__note">Off, a deleted message disappears for everyone. On, it stays in its place as “(deleted)”.</p>
          </div>
          <Show when={deletedError()}>
            <p class="auth__error" role="alert">
              {deletedError()}
            </p>
          </Show>
        </div>
      </Card>
      <Card title="Files" subtitle="The attachments sent in this server's channels.">
        <div class="auth__form">
          <Show when={!storage.error} fallback={<p class="auth__error">{settingRefusal(storage.error)}</p>}>
            <p class="auth__note" role="status">
              {storage() ? usageWords(storage()!) : "Counting…"}
            </p>
          </Show>
          <SegmentedTabs
            label="Keep attachments"
            tabs={fileChoices().map((days) => ({ id: String(days), label: retentionWords(days) }))}
            value={String(keepFiles())}
            onChange={(id) => {
              setKeepFiles(Number(id));
              setFilesSaved(false);
            }}
          />
          <Show when={shortens(props.server.attachmentRetentionDays, keepFiles())}>
            <p class="auth__error">Attachments older than {retentionWords(keepFiles())} will be deleted for everyone. Their messages stay. This cannot be undone.</p>
          </Show>
          <SegmentedTabs
            label="When the server is full"
            tabs={[
              { id: "refuse", label: "Refuse new uploads" },
              { id: "delete", label: "Delete the oldest attachments" },
            ]}
            value={deleteOldest() ? "delete" : "refuse"}
            onChange={(id) => {
              setDeleteOldest(id === "delete");
              setFilesSaved(false);
            }}
          />
          <p class="auth__note">
            {deleteOldest() ? "When an upload would not fit, the oldest messages’ attachments are deleted for everyone to make room." : "When an upload would not fit, it is refused until files are deleted."}
          </p>
          <Show when={filesError()}>
            <p class="auth__error" role="alert">
              {filesError()}
            </p>
          </Show>
          <Show when={filesSaved()}>
            <p class="auth__note" role="status">
              Saved.
            </p>
          </Show>
          <Button variant="primary" size="md" disabled={filesBusy() || !filesChanged()} onClick={() => void saveFiles()}>
            Save
          </Button>
        </div>
      </Card>
    </>
  );
}
