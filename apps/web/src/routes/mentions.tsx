// The Mentions page (architecture 6.6): what notifies this account, newest
// first, a page at a time as the reader scrolls: in one server from the
// Mentions row atop its sidebar, or across every server from Home. Each
// item is where it was said over the message as the timeline draws it;
// pressing it opens the channel at that message.
import { For, Show, createMemo } from "solid-js";
import { ChannelHeader, EmptyState, Mention, MentionEntry, Message, ReplyRef } from "../design/components";
import type { MessageSummary, ServerSummary, SnapshotSummary } from "../client/bridge";
import { mentionsQuery } from "../client/state";
import { useNames } from "../client/names";
import { preferences } from "../client/preferences";
import { dayLabel, initialsOf, timeOf, toneFor } from "./app";
import { bodySegments, mentionWords } from "./timeline";

export interface MentionsPageProps {
  /** The server whose mentions these are; every server's at Home. */
  server?: ServerSummary;
  snapshot: SnapshotSummary | undefined;
  back: () => void;
  /** Opens the channel at the message. */
  open: (m: MessageSummary) => void;
}

export function MentionsPage(props: MentionsPageProps) {
  const { nameOf, photoOf, profile } = useNames(() => props.snapshot);
  const pages = mentionsQuery(() => props.server?.serverId ?? null);
  const messages = createMemo(() => pages.data?.pages.flatMap((p) => p.messages) ?? []);
  const where = (m: MessageSummary) => {
    const ch = props.snapshot?.channels.find((c) => c.channelId === m.channelId);
    const channel = ch ? (ch.kind === "SERVER_DM" ? nameOf(ch.participantAccountIds.find((id) => id !== props.snapshot?.accountId) ?? "") : `#${ch.name}`) : "A channel you left";
    if (props.server) return channel;
    const server = props.snapshot?.servers.find((s) => s.serverId === ch?.serverId);
    return server ? `${server.name} · ${channel}` : channel;
  };
  const when = (micros: number) => {
    const day = dayLabel(micros);
    const t = timeOf(micros).time;
    return day === "Today" ? t : `${day}, ${t}`;
  };
  const quoted = (id: string) => {
    if (id && id === props.snapshot?.accountId) return nameOf(id);
    const p = profile(id);
    return p ? p.displayName || p.nickname || p.handle || undefined : undefined;
  };
  // A page more as the end comes into view.
  const onScroll = (e: Event & { currentTarget: HTMLDivElement }) => {
    const el = e.currentTarget;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 200 && pages.hasNextPage && !pages.isFetchingNextPage) void pages.fetchNextPage();
  };
  return (
    <>
      <ChannelHeader kind="page" name="Mentions" topic={props.server ? undefined : "Across your servers"} actions={[]} onBack={props.back} backLabel={props.server ? `Back to ${props.server.name}` : "Back to Home"} />
      <div class="lu-page" onScroll={onScroll}>
        <Show when={pages.isError}>
          <p class="auth__error" role="alert">
            {pages.error instanceof Error ? pages.error.message : String(pages.error)}
          </p>
        </Show>
        <Show when={!pages.isPending && messages().length === 0 && !pages.isError}>
          <EmptyState icon="mention" title="No mentions yet">
            When someone mentions you or replies to you, it shows up here.
          </EmptyState>
        </Show>
        <Show when={messages().length > 0}>
          <ul class="lu-mentionentries" aria-label="Mentions">
            <For each={messages()}>
              {(m) => (
                <MentionEntry where={where(m)} onOpen={() => props.open(m)}>
                  <Message
                    author={nameOf(m.authorAccountId)}
                    initials={initialsOf(nameOf(m.authorAccountId))}
                    src={photoOf(m.authorAccountId)}
                    tone={toneFor(m.authorAccountId)}
                    time={when(m.createdAtMicros)}
                    actions={false}
                    density={preferences().density}
                    reference={
                      m.reply ? (
                        <ReplyRef
                          name={m.reply.authorAccountId ? quoted(m.reply.authorAccountId) : undefined}
                          initials={initialsOf(quoted(m.reply.authorAccountId) ?? "")}
                          src={m.reply.authorAccountId ? photoOf(m.reply.authorAccountId) : undefined}
                          tone={toneFor(m.reply.authorAccountId)}
                          self={m.reply.authorAccountId === props.snapshot?.accountId}
                          text={m.reply.text}
                          deleted={m.reply.originalDeleted}
                        />
                      ) : undefined
                    }
                  >
                    {m.deleted ? (
                      "(deleted)"
                    ) : (
                      <p>
                        <For each={bodySegments(m.body, m.mentions)}>{(s) => <Show when={s.mention} fallback={s.text}>{(mention) => <Mention>{mentionWords(s.text, mention(), quoted)}</Mention>}</Show>}</For>
                        <Show when={m.edited}>
                          {" "}
                          <span class="lu-msg__edited">
                            <span aria-hidden="true">(edited)</span>
                            <span class="lu-sr">edited</span>
                          </span>
                        </Show>
                      </p>
                    )}
                  </Message>
                </MentionEntry>
              )}
            </For>
          </ul>
        </Show>
      </div>
    </>
  );
}
