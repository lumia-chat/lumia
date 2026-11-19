// The design gallery: the Quiet Split board and the sign-in card from the
// design canvas, rebuilt from the ported components with the same sample
// data, plus the smaller pieces in isolation. Theme and layout toggles at
// the top. Not a product screen.
import { For, Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { PRESETS, applyPreferences, type Preferences } from "../client/preferences";
import type { BodyBlock, BodyLink, BodyRun, DeviceSummary, EmbedSummary } from "../client/bridge";
import { frameBox } from "../client/embeds";
import { QUICK_REACTIONS } from "../design/emoji";
import {
  AppShell,
  Attachment,
  AudioAttachment,
  ComposerMedia,
  DropZone,
  EmbedFrame,
  LinkCard,
  MediaGrid,
  Viewer,
  type MediaTileProps,
  Avatar,
  AvatarStack,
  Badge,
  BrowseRow,
  Button,
  Card,
  ChannelHeader,
  ChannelList,
  Checkbox,
  Choice,
  Composer,
  ComposerNote,
  DeviceList,
  Divider,
  EmojiPicker,
  EmptyState,
  Icon,
  IconButton,
  InvitationRow,
  JoinBar,
  MemberList,
  Mention,
  MentionEntry,
  MentionList,
  Menu,
  MenuHead,
  MenuItem,
  MenuLabel,
  MenuRadio,
  MenuRule,
  Message,
  MessageActions,
  MessageBody,
  MessageList,
  Notice,
  NoticeLine,
  PickRow,
  Pop,
  ProfileCard,
  QuickTiles,
  ConfirmDelete,
  MessageContextMenu,
  MessageEdit,
  MessageMenu,
  PersonMenu,
  QuoteReplyButton,
  ReactionChip,
  ReplyBar,
  ReplyRef,
  RequestCard,
  SearchField,
  SearchTrigger,
  ServerHeader,
  ServerRail,
  SidePanel,
  SidePanelScroll,
  SidePanelSearch,
  SystemMessage,
  Tag,
  TextField,
  ThreadPanel,
  TypingIndicator,
  FolderSettings,
  LayoutSketch,
  PictureChoices,
  Segmented,
  SettingRow,
  SettingsGroup,
  ThemeSketch,
  Toggle,
  icons,
  type IconName,
  type RailDrop,
  type RailServer,
  type RailTile,
  type Tone,
  PhraseSaveWarning,
  PhraseWordField,
  ConfirmPop,
  MemberEdit,
  RoleChoices,
  RoleDot,
  RoleList,
  Roster,
  SectionList,
  SegmentedTabs,
  TagSketch,
  ToneSwatches,
  type RoleChoice,
  type RoleEntry,
  type RosterEntry,
  type TagTone,
} from "../design/components";
import { NeedsBrowser, PhraseStep, RestoreAsk, RestoreForm, RestoreKeep, StartOver } from "./private";
import { ROLES_HINT } from "../client/roles";
import { endedNotice } from "../client/ended";
import { LinkPromptCard } from "./link";
import { MessagesPreview, PreferencesView } from "./preferences";

type Theme = "light" | "dark";

// The gallery names a theme on the root so a board can be looked at in
// either; the app names none and follows the system (tokens.css). The
// one chosen here is kept for the next visit, and the root is left as it
// was found when the gallery goes away.
function useTheme() {
  const system = (): Theme => (typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  const [theme, setTheme] = createSignal<Theme>(system());
  const apply = (t: Theme, keep = true) => {
    setTheme(t);
    document.documentElement.dataset.theme = t;
    if (!keep) return;
    try {
      localStorage.setItem("lumia:theme", t);
    } catch {
      /* ignore */
    }
  };
  onMount(() => {
    let saved: string | null = null;
    try {
      saved = localStorage.getItem("lumia:theme");
    } catch {
      /* storage may be unavailable */
    }
    apply(saved === "dark" || saved === "light" ? saved : system(), false);
  });
  // Leaving the gallery gives the page back to the appearance Preferences holds.
  onCleanup(applyPreferences);
  return { theme, apply };
}

const PEOPLE = {
  maya: { name: "Maya Chen", initials: "MC", tone: "peach" as const },
  devon: { name: "Devon Park", initials: "DP", tone: "purple" as const },
  sam: { name: "Sam Okafor", initials: "SO", tone: "mint" as const },
  priya: { name: "Priya Natarajan", initials: "PN", tone: "amber" as const },
  leo: { name: "Leo Martins", initials: "LM", tone: "sky" as const },
  self: { name: "Ryan Winchester", initials: "RW", tone: "indigo" as const },
};

function entry(person: { name: string; initials: string; tone: "peach" | "purple" | "mint" | "amber" | "sky" | "indigo" }, time: string, text: string) {
  return { author: person.name, initials: person.initials, tone: person.tone, time, text };
}

function QuietSplit() {
  const [tab, setTab] = createSignal("members");
  return (
    <AppShell
      height={900}
      rail={
        <ServerRail
          logoSrc="/brand/lumia-original-icon.svg"
          homeUnread={3}
          servers={[
            { name: "Acme", initials: "A", current: true },
            { name: "Design Guild", initials: "DG", ink: "indigo", unread: true },
            { name: "Lumia Builders", initials: "LB", ink: "green", mentions: 2 },
            { name: "Friends", initials: "F", ink: "amber" },
          ]}
          self={{ ...PEOPLE.self, status: "online" }}
        />
      }
      sidebar={
        <>
          <ServerHeader name="Acme" onOpenMenu={() => {}} onNewMessage={() => {}} />
          <SearchTrigger label="Search Acme" />
          <QuickTiles
            items={[
              { icon: "thread", label: "Threads", count: 2 },
              { icon: "mention", label: "Mentions", count: 1 },
              { icon: "saved", label: "Saved" },
            ]}
          />
          <ChannelList
            sections={[
              {
                title: "Channels",
                items: [
                  { name: "general", active: true },
                  { name: "design", unread: true, mentions: 1 },
                  { name: "engineering", unread: true },
                  { name: "random" },
                ],
              },
              {
                title: "Private",
                items: [
                  { name: "leadership", kind: "private" },
                  { name: "northstar-launch", kind: "private", unread: true },
                ],
              },
              {
                title: "Direct messages",
                items: [
                  { name: PEOPLE.maya.name, kind: "dm", initials: "MC", tone: "peach", status: "online", unread: true, count: 2 },
                  { name: PEOPLE.devon.name, kind: "dm", initials: "DP", tone: "purple", status: "away" },
                  { name: "Sam, Priya", kind: "group", people: [PEOPLE.sam, PEOPLE.priya] },
                ],
              },
            ]}
          />
        </>
      }
      main={
        <>
          <ChannelHeader name="general" topic="Company-wide announcements and water-cooler chat" />
          <MessageList>
            <Divider variant="day" label="Today" />
            <SystemMessage time="9:02 AM" datetime="2026-11-02T09:02">
              <strong>Leo Martins</strong> joined #general
            </SystemMessage>
            <Message author={PEOPLE.maya.name} initials="MC" tone="peach" tag={{ label: "Design" }} time="9:04 AM" datetime="2026-11-02T09:04">
              {["Morning all. The new onboarding flow is in the design channel if anyone wants a look before standup.", "Two screens changed since Friday, both in the sign-in card."]}
            </Message>
            <Message
              author={PEOPLE.devon.name}
              initials="DP"
              tone="purple"
              time="9:11 AM"
              datetime="2026-11-02T09:11"
              reactions={[
                { icon: "thumbs-up", label: "Thumbs up", count: 4, defaultSelected: true },
                { icon: "sparkle", label: "Sparkle", count: 1 },
              ]}
              onAddReaction={() => {}}
              thread={{ count: 3, lastReply: "9:40 AM", people: [PEOPLE.sam, PEOPLE.priya], open: true }}
              highlighted
            >
              <p>
                Looks great. <Mention>@Sam Okafor</Mention> can you check the copy on the second screen matches the tone guide?
              </p>
            </Message>
            <Divider variant="new" />
            <Message author={PEOPLE.sam.name} initials="SO" tone="mint" time="9:38 AM" datetime="2026-11-02T09:38" attachment={{ name: "Tone guide v3.pdf", size: "1.2 MB" }}>
              On it. Attaching the current guide for reference.
            </Message>
          </MessageList>
          <Composer placeholder="Message #general" typing={PEOPLE.maya.name} />
        </>
      }
      aside={
        <SidePanel
          tabs={[
            { id: "members", label: "Members", count: 128 },
            { id: "thread", label: "Thread", count: 3 },
          ]}
          value={tab()}
          onChange={setTab}
        >
          {(id) =>
            id === "members" ? (
              <>
              <SidePanelSearch>
                <SearchField label="Find a member" />
              </SidePanelSearch>
              <SidePanelScroll>
              <MemberList
                sections={[
                  {
                    title: "Online · 32",
                    members: [
                      { ...PEOPLE.maya, status: "online", tag: "Admin", action: removing(PEOPLE.maya.name) },
                      { ...PEOPLE.devon, status: "away", subtitle: "In a meeting", action: removing(PEOPLE.devon.name) },
                      { ...PEOPLE.sam, status: "online", action: removing(PEOPLE.sam.name) },
                    ],
                  },
                  { title: "Offline · 96", members: [{ ...PEOPLE.priya, action: removing(PEOPLE.priya.name) }, { ...PEOPLE.leo, action: removing(PEOPLE.leo.name) }] },
                ]}
                total={128}
              />
              </SidePanelScroll>
              </>
            ) : (
              <ThreadPanel
                parent={entry(PEOPLE.devon, "9:11 AM", "Looks great. Can you check the copy on the second screen matches the tone guide?")}
                earlier={10}
                replies={[entry(PEOPLE.sam, "9:38 AM", "On it."), entry(PEOPLE.priya, "9:40 AM", "The second screen uses sentence case now.")]}
                channel="general"
              />
            )
          }
        </SidePanel>
      }
    />
  );
}

// Home: personal DMs grouped by the instance that hosts them, requests,
// and this identity's devices. The same shell as a server; the rail's
// Home tile is current and the sidebar lists conversations, not channels.
function HomeSidebar(props: { view: "dm" | "requests" | "devices" }) {
  return (
    <>
      <ServerHeader name="Home" onNewMessage={() => {}} />
      <SearchTrigger label="Search messages" />
      <QuickTiles
        items={[
          { icon: "mention", label: "Requests", count: 2 },
          { icon: "shield", label: "Devices" },
          { icon: "plus", label: "Instance" },
        ]}
      />
      <ChannelList
        sections={[
          {
            title: "chat.acme.dev",
            items: [
              { name: PEOPLE.maya.name, kind: "dm", initials: "MC", tone: "peach", status: "online", active: props.view === "dm", unread: true, count: 2 },
              { name: PEOPLE.devon.name, kind: "dm", initials: "DP", tone: "purple", status: "away" },
              { name: "Sam, Priya", kind: "group", people: [PEOPLE.sam, PEOPLE.priya] },
            ],
          },
          {
            title: "lumia.winchester.dev",
            items: [{ name: PEOPLE.leo.name, kind: "dm", initials: "LM", tone: "sky", status: "online" }],
          },
        ]}
      />
    </>
  );
}

function HomeBoard(props: { view: "dm" | "requests" | "devices" }) {
  return (
    <AppShell
      height={900}
      rail={
        <ServerRail
          logoSrc="/brand/lumia-original-icon.svg"
          homeUnread={3}
          homeCurrent
          servers={[
            { name: "Acme", initials: "A" },
            { name: "Design Guild", initials: "DG", ink: "indigo", unread: true },
            { name: "Lumia Builders", initials: "LB", ink: "green", mentions: 2 },
          ]}
          self={{ ...PEOPLE.self, status: "online" }}
        />
      }
      sidebar={<HomeSidebar view={props.view} />}
      main={props.view === "dm" ? <DmConversation /> : props.view === "requests" ? <Requests /> : <Devices />}
      aside={props.view === "dm" ? <DmAside /> : undefined}
    />
  );
}

// The canvas's "Inline replies" board (960 by 800): a channel where Jonah's
// message is being replied to (its toolbar up, Reply pressed), replies with
// the line of what they quote over them (one to the reader, one whose
// original is gone), and the composer with the reply bar atop its frame.
const REPLIES = {
  jonah: { name: "Jonah Patel", initials: "JP", tone: "blue" as const },
  dana: { name: "Dana Kim", initials: "DK", tone: "amber" as const },
  maya: { name: "Maya Chen", initials: "MC", tone: "peach" as const },
  sam: { name: "Sam Rivera", initials: "SR", tone: "indigo" as const },
  priya: { name: "Priya Raman", initials: "PR", tone: "purple" as const },
  leo: { name: "Leo Okafor", initials: "LO", tone: "mint" as const },
};

function RepliesBoard() {
  const [notify, setNotify] = createSignal(true);
  return (
    <div class="gallery-replies">
      <main class="lu-panel gallery-replies__panel">
        <ChannelHeader name="general" topic="Company-wide news, wins and everyday hellos" actions={[]} />
        <MessageList label="Messages in #general">
          <Message
            {...REPLIES.jonah}
            author={REPLIES.jonah.name}
            tag={{ label: "Leadership" }}
            time="9:31 AM"
            datetime="2026-11-13T09:31"
            actions={false}
            attachment={{ name: "Q4 All-Hands Agenda.pdf", size: "1.2 MB" }}
            replying
            tools={<MessageActions onReply={() => {}} replying />}
          >
            Q4 all-hands is Thursday at 2:00 PM in the Atrium and on the livestream. The agenda is attached.
          </Message>
          <Message
            {...REPLIES.dana}
            author={REPLIES.dana.name}
            tag={{ label: "Workplace" }}
            time="9:36 AM"
            datetime="2026-11-13T09:36"
            actions={false}
            reference={<ReplyRef {...REPLIES.maya} text="Morning, Acme! 2027 benefits enrollment closes next Friday. The plan guide is pinned." onOpen={() => {}} />}
            tools={<MessageActions onReply={() => {}} />}
          >
            Does the new dental plan cover dependants too, or just employees?
          </Message>
          <Message
            {...REPLIES.jonah}
            author={REPLIES.jonah.name}
            tag={{ label: "Leadership" }}
            time="9:44 AM"
            datetime="2026-11-13T09:44"
            actions={false}
            mentioned
            reference={<ReplyRef {...REPLIES.sam} self text="Can we get the recording link once it's up? Half of design is out Thursday." onOpen={() => {}} />}
            tools={<MessageActions onReply={() => {}} />}
          >
            Yes — it goes up in #announcements by Friday morning. I'll ping you when it's live.
          </Message>
          <Message
            {...REPLIES.priya}
            author={REPLIES.priya.name}
            tag={{ label: "Design" }}
            time="10:02 AM"
            datetime="2026-11-13T10:02"
            actions={false}
            reference={<ReplyRef {...REPLIES.jonah} text="Q4 All-Hands Agenda.pdf" onOpen={() => {}} />}
            tools={<MessageActions onReply={() => {}} />}
          >
            Page 2 still says 1:00 PM — worth a quick fix before it goes out wider.
          </Message>
          <Message
            {...REPLIES.leo}
            author={REPLIES.leo.name}
            tag={{ label: "New", tone: "accent" }}
            time="10:11 AM"
            datetime="2026-11-13T10:11"
            actions={false}
            reference={<ReplyRef deleted text="" />}
            tools={<MessageActions onReply={() => {}} />}
          >
            Ha, too late — already saved it to my calendar.
          </Message>
        </MessageList>
        <Composer
          placeholder="Reply to Jonah Patel in #general"
          tools={false}
          value="Thanks! Sharing with the design crew"
          hint="Return to send · Esc to cancel the reply"
          bar={<ReplyBar name={REPLIES.jonah.name} mention={notify()} onMention={setNotify} />}
        />
      </main>
    </div>
  );
}

function DmConversation() {
  return (
    <>
      <ChannelHeader
        kind="dm"
        name={PEOPLE.maya.name}
        topic="on chat.acme.dev"
        actions={[
          { icon: "search", label: "Search in this conversation" },
          { icon: "more", label: "Conversation settings" },
        ]}
      />
      <MessageList>
        <Notice icon="shield" tone="secure">Messages here are end-to-end encrypted. Only your devices and Maya's can read them; chat.acme.dev delivers them without seeing them.</Notice>
        <Divider variant="day" label="Yesterday" />
        <Message author={PEOPLE.maya.name} initials="MC" tone="peach" time="4:12 PM" datetime="2026-11-01T16:12">
          {["Are you around tomorrow? I want to walk you through the onboarding changes before they go to the guild.", "Ten minutes, promise."]}
        </Message>
        <Message author={PEOPLE.self.name} initials="RW" tone="indigo" time="4:30 PM" datetime="2026-11-01T16:30">
          Sure. After standup works.
        </Message>
        <Divider variant="day" label="Today" />
        <Divider variant="new" />
        <Message author={PEOPLE.maya.name} initials="MC" tone="peach" time="9:41 AM" datetime="2026-11-02T09:41" attachment={{ name: "Onboarding v4.pdf", size: "860 KB" }}>
          Here it is. Second screen is the one I am least sure about.
        </Message>
        <SystemMessage time="9:50 AM" datetime="2026-11-02T09:50">
          <strong>Maya</strong> added a device, <strong>iPhone</strong>. Earlier messages stay unreadable on it.
        </SystemMessage>
      </MessageList>
      <Composer placeholder={"Message " + PEOPLE.maya.name} />
    </>
  );
}

/** The dark Quiet Split's Remove at the end of each member row (a channel's, for one who manages channels). */
const removing = (name: string) => ({ label: "Remove from channel", description: `Remove ${name} from channel`, onClick: () => {} });

function DmAside() {
  return (
    <SidePanel tabs={[{ id: "details", label: "Details" }]} value="details" onChange={() => {}}>
      <SidePanelScroll>
      <MemberList
        sections={[
          {
            title: "In this conversation",
            members: [
              { ...PEOPLE.maya, status: "online", subtitle: "@maya · chat.acme.dev" },
              { ...PEOPLE.self, status: "online", subtitle: "@ryan · you", tag: "You" },
            ],
          },
          {
            title: "Devices that can read it",
            members: [
              { name: "Maya's laptop", initials: "M", tone: "peach", subtitle: "Added by Maya, Oct 12" },
              { name: "Maya's iPhone", initials: "M", tone: "peach", subtitle: "Added today, sees new messages only" },
              { name: "This browser", initials: "R", tone: "indigo", subtitle: "localhost:5173", tag: "You" },
              { name: "laptop", initials: "R", tone: "indigo", subtitle: "Added by you, Oct 30" },
            ],
          },
        ]}
      />
      <Notice icon="lock" class="gallery-aside-note">History backup is off for this identity. Messages live only on your devices.</Notice>
      </SidePanelScroll>
    </SidePanel>
  );
}

function Requests() {
  return (
    <>
      <ChannelHeader kind="page" name="Requests" topic="People who want to message you" actions={[]} />
      <div class="lu-page">
        <Notice icon="shield">A request shows who is asking and what you share, nothing else. Accepting opens an encrypted conversation; the sender cannot see that you looked.</Notice>
        <RequestCard name="Jordan Lee" handle="jordan" initials="JL" tone="mint" meta="Shares 2 servers with you · 3 mutual friends · chat.acme.dev" />
        <RequestCard name="Alex Rivera" handle="arivera" initials="AR" tone="amber" meta="Shares Lumia Builders with you · lumia.winchester.dev" />
        <p class="lu-page__title">Earlier</p>
        <EmptyState icon="mention" title="Nothing else waiting">
          Declined requests disappear; blocked people cannot ask again.
        </EmptyState>
      </div>
    </>
  );
}

function Devices() {
  return (
    <>
      <ChannelHeader kind="page" name="Devices" topic="Everything that holds your identity" actions={[]} />
      <div class="lu-page">
        <Notice icon="shield" action={{ label: "Link a device" }}>Your identity lives on these devices, not on any instance. Revoking one takes effect on every instance that holds your roster.</Notice>
        <DeviceList
          devices={[
            { id: "1", kind: "web", label: "This browser", meta: "localhost:5173 · added today by laptop", thisDevice: true },
            { id: "2", kind: "native", label: "laptop", meta: "Root device · added Oct 30" },
            { id: "3", kind: "mobile", label: "iPhone", meta: "Added Nov 1 by laptop" },
          ]}
          onRevoke={() => {}}
        />
        <Card title="Revoke iPhone?" subtitle="It stops reading new messages the moment every instance has the change. Confirm with your password on chat.acme.dev.">
          <div class="gallery-auth__form">
            <TextField label="Password" type="password" reveal placeholder="Your password" autocomplete="current-password" />
            <div class="pieces__row">
              <Button variant="primary" size="md">Revoke iPhone</Button>
              <Button size="md">Keep it</Button>
            </div>
          </div>
        </Card>
        <p class="lu-page__title">Revoked</p>
        <DeviceList devices={[{ id: "4", kind: "native", label: "old laptop", meta: "Revoked Oct 28 by laptop", revoked: true }]} />
      </div>
    </>
  );
}

function SignIn() {
  return (
    <div class="gallery-auth">
      <div class="gallery-auth__logo">
        <img src="/brand/lumia-original-light.svg" alt="Lumia" class="lu-only-light" />
        <img src="/brand/lumia-original-dark.svg" alt="Lumia" class="lu-only-dark" />
      </div>
      <Card title="Welcome back" subtitle="Sign in to pick up your conversations.">
        <form class="gallery-auth__form" onSubmit={(e) => e.preventDefault()}>
          <TextField label="Email or handle" type="text" placeholder="you@example.com" autocomplete="username" />
          <TextField label="Password" reveal placeholder="Your password" autocomplete="current-password" action={{ label: "Forgot?" }} />
          <Checkbox label="Keep me signed in on this device" checked />
          <Button variant="primary" type="submit" block>
            Sign in
          </Button>
          <Divider variant="or" />
          <Button icon="key" block>
            Continue with SSO
          </Button>
          <Button icon="mail" block>
            Email me a sign-in link
          </Button>
        </form>
        <p class="gallery-auth__foot">
          New to Lumia? <a href="#" class="lu-link">Create an account</a>
        </p>
      </Card>
    </div>
  );
}

// iOS (architecture section 13, plan step 7): the Quiet Split folded for a
// phone, as settled on the design canvas. The rail and the sidebar stay one
// screen (Home, a server); the main pane and the aside become floating
// panels pushed over it (a conversation, a channel, a thread, requests,
// devices); there is no tab bar. The status bar and the home indicator are
// the system's, so a frame keeps only their insets. Same components, same
// sample data as the other boards.
function PhoneFrame(props: { label: string; split?: boolean; children: JSX.Element }) {
  return (
    <figure class="phone-figure">
      <div class={cx("phone", props.split && "phone--split")} role="group" aria-label={props.label}>
        {props.children}
      </div>
      <figcaption class="phone__label">{props.label}</figcaption>
    </figure>
  );
}

function cx(...parts: (string | false | undefined)[]) {
  return parts.filter(Boolean).join(" ");
}

const PHONE_SERVERS = [
  { name: "Acme", initials: "A", meta: "chat.acme.dev · 128 members" },
  { name: "Design Guild", initials: "DG", ink: "indigo" as const, unread: true },
  { name: "Lumia Builders", initials: "LB", ink: "green" as const, mentions: 2 },
  { name: "Friends", initials: "F", ink: "amber" as const },
];

function PhoneRail(props: { current: "home" | "acme" }) {
  return (
    <ServerRail
      class="phone__rail"
      logoSrc="/brand/lumia-original-icon.svg"
      homeCurrent={props.current === "home"}
      homeUnread={props.current === "home" ? undefined : 3}
      servers={PHONE_SERVERS.map((s, i) => ({ ...s, current: props.current === "acme" && i === 0 }))}
      self={{ ...PEOPLE.self, status: "online" }}
    />
  );
}

// A floating panel pushed over the rail and sidebar: the main pane's card
// with a back button in its header.
function PhonePanel(props: { back: string; lead?: JSX.Element; title: JSX.Element; subtitle?: string; actions?: { icon: IconName; label: string }[]; children: JSX.Element }) {
  return (
    <>
      <div class="phone__inset" aria-hidden="true"></div>
      <main class="phone__panel lu-panel">
        <header class="phone__panel-head">
          <button type="button" class="phone__back" aria-label={"Back to " + props.back}>
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="m15 5-7 7 7 7" />
            </svg>
          </button>
          {props.lead}
          <div class="phone__panel-title">
            <h1>{props.title}</h1>
            <Show when={props.subtitle}>
              <p>{props.subtitle}</p>
            </Show>
          </div>
          <For each={props.actions ?? []}>{(a) => <IconButton icon={a.icon} label={a.label} size={40} />}</For>
        </header>
        {props.children}
      </main>
    </>
  );
}

function PhoneSignIn() {
  return (
    <PhoneFrame label="Sign in: the instance, password, or link this phone">
      <div class="phone__inset" aria-hidden="true"></div>
      <div class="phone__auth">
        <div class="gallery-auth__logo phone__logo">
          <img src="/brand/lumia-original-light.svg" alt="Lumia" class="lu-only-light" />
          <img src="/brand/lumia-original-dark.svg" alt="Lumia" class="lu-only-dark" />
        </div>
        <Card title="Welcome back" subtitle="Sign in to pick up your conversations.">
          <div class="phone__instance">
            <span class="lu-rail__server lu-tile-ink-default phone__instance-tile" aria-hidden="true">
              A
            </span>
            <span class="phone__instance-text">
              <span class="phone__instance-host">chat.acme.dev</span>
              <span class="phone__instance-meta">Run by Acme · Lumia 0.1.0</span>
            </span>
            <Button variant="text">Change</Button>
          </div>
          <form class="gallery-auth__form" onSubmit={(e) => e.preventDefault()}>
            <TextField label="Email or handle" type="text" placeholder="you@example.com" autocomplete="username" />
            <TextField label="Password" reveal placeholder="Your password" autocomplete="current-password" action={{ label: "Forgot password?" }} />
            <Button variant="primary" type="submit" block>
              Sign in
            </Button>
            <Divider variant="or" />
            <Button icon="phone" block>
              Link this phone from another device
            </Button>
          </form>
        </Card>
        <p class="gallery-auth__foot">
          New to Lumia? <a href="#" class="lu-link">Create an account</a>
        </p>
      </div>
      <footer class="phone__footer">
        <a href="#">Help</a>
        <a href="#">Privacy</a>
        <a href="#">Terms</a>
      </footer>
    </PhoneFrame>
  );
}

function DmRow(props: { person: { name: string; initials: string; tone: Tone }; status?: "online" | "away"; people?: { initials: string; tone: Tone }[]; time: string; datetime: string; preview: string; count?: number }) {
  return (
    <li>
      <a href="#" class={cx("phone__dm", !!props.count && "is-unread")}>
        <Show when={props.people} fallback={<Avatar initials={props.person.initials} tone={props.person.tone} size={40} status={props.status} ring="bg" />}>
          {(people) => <AvatarStack people={people()} size={28} />}
        </Show>
        <span class="phone__dm-text">
          <span class="phone__dm-top">
            <span class="phone__dm-name">{props.person.name}</span>
            <time datetime={props.datetime} class="phone__dm-time">
              {props.time}
            </time>
          </span>
          <span class="phone__dm-preview">
            <span>{props.preview}</span>
            <Show when={props.count}>
              <Badge count={props.count} label={props.count + " unread"} />
            </Show>
          </span>
        </span>
      </a>
    </li>
  );
}

function PhoneHome() {
  return (
    <PhoneFrame label="Home: the rail and the sidebar, conversations with their last line" split>
      <PhoneRail current="home" />
      <div class="phone__side">
        <div class="phone__side-head">
          <h1>Home</h1>
          <IconButton icon="compose" label="New message" size={40} />
        </div>
        <div class="phone__side-body">
          <SearchTrigger label="Search messages" shortcut="" />
          <QuickTiles
            items={[
              { icon: "mention", label: "Requests", count: 2 },
              { icon: "saved", label: "Saved" },
              { icon: "shield", label: "Devices" },
            ]}
          />
          <section>
            <h2 class="lu-chlist__title">Direct messages</h2>
            <ul class="phone__dms">
              <DmRow person={PEOPLE.maya} status="online" time="9:42 AM" datetime="2026-11-03T09:42" preview="No rush, comments in the doc are fine." count={2} />
              <DmRow person={{ name: "Sam, Priya", initials: "", tone: "mint" }} people={[PEOPLE.sam, PEOPLE.priya]} time="9:20 AM" datetime="2026-11-03T09:20" preview="Priya: Coffee at 3 before the all-hands?" count={1} />
              <DmRow person={PEOPLE.devon} status="away" time="Yesterday" datetime="2026-11-02" preview="You: Sent the agenda over." />
              <DmRow person={PEOPLE.leo} status="online" time="Mon" datetime="2026-11-02" preview="The desk by the windows is yours." />
              <DmRow person={PEOPLE.sam} time="Oct 28" datetime="2026-10-28" preview="Tacos are back on Friday." />
              <DmRow person={PEOPLE.priya} time="Oct 26" datetime="2026-10-26" preview="Thanks for the intro!" />
            </ul>
          </section>
        </div>
      </div>
    </PhoneFrame>
  );
}

function PhoneConversation() {
  return (
    <PhoneFrame label="A conversation: the main pane as a floating panel over Home">
      <PhonePanel
        back="Home"
        lead={<Avatar initials="MC" tone="peach" size={34} status="online" ring="surface" />}
        title={PEOPLE.maya.name}
        subtitle="Online · Design"
        actions={[
          { icon: "voice", label: "Start a voice call" },
          { icon: "more", label: "Conversation details" },
        ]}
      >
        <MessageList>
          <Notice icon="shield" tone="secure">Messages with Maya are end-to-end encrypted. Only your devices and Maya's can read them.</Notice>
          <Divider variant="day" label="Yesterday" />
          <Message author={PEOPLE.maya.name} initials="MC" tone="peach" time="4:12 PM" datetime="2026-11-02T16:12">
            Are you around tomorrow? I want to walk you through the onboarding changes.
          </Message>
          <Message author={PEOPLE.self.name} initials="RW" tone="indigo" time="4:30 PM" datetime="2026-11-02T16:30" reactions={[{ icon: "thumbs-up", label: "Thumbs up", count: 1 }]}>
            Sure, after standup works.
          </Message>
          <Divider variant="day" label="Today" />
          <Divider variant="new" />
          <Message author={PEOPLE.maya.name} initials="MC" tone="peach" time="9:41 AM" datetime="2026-11-03T09:41" attachment={{ name: "Onboarding v4.pdf", size: "860 KB" }}>
            {["Here it is. Slides 3 and 4 are the big ones.", "No rush, comments in the doc are fine."]}
          </Message>
          <SystemMessage time="9:50 AM" datetime="2026-11-03T09:50">
            <strong>Maya</strong> added a device, <strong>iPhone</strong>.
          </SystemMessage>
        </MessageList>
        <Composer placeholder={"Message " + PEOPLE.maya.name} typing={PEOPLE.maya.name} hint="" />
      </PhonePanel>
    </PhoneFrame>
  );
}

function PhoneRequests() {
  return (
    <PhoneFrame label="Requests: waiting for you, and the ones you sent">
      <PhonePanel back="Home" title="Requests" subtitle="2 waiting for you">
        <div class="phone__panel-body">
          <Notice icon="shield">A request shows who is asking and the servers you share. Nothing else reaches you until you accept.</Notice>
          <h2 class="phone__section">Waiting for you</h2>
          <RequestCard name="Jordan Lee" handle="jordan" initials="JL" tone="mint" meta="Shares Acme and Design Guild with you" />
          <RequestCard name="Alex Rivera" handle="arivera" initials="AR" tone="amber" meta="Shares Lumia Builders with you · lumia.winchester.dev" />
          <h2 class="phone__section">Sent</h2>
          <div class="phone__sent">
            <Avatar initials="NH" tone="green" size={40} />
            <span class="phone__sent-text">
              <span class="phone__sent-name">
                Noor Haddad <span class="lu-request__handle">@noor</span>
              </span>
              <span class="phone__sent-meta">Waiting for Noor to accept</span>
            </span>
            <Button variant="text">Cancel request</Button>
          </div>
          <p class="phone__foot">Declined requests disappear quietly. Blocked people cannot send you another.</p>
        </div>
      </PhonePanel>
    </PhoneFrame>
  );
}

function PhoneDevices() {
  return (
    <PhoneFrame label="Devices: revoke confirms in a sheet with the password">
      <PhonePanel back="Home" title="Devices" subtitle="3 linked to you" actions={[{ icon: "plus", label: "Link a device" }]}>
        <div class="phone__panel-body">
          <Notice icon="shield">Your identity lives on these devices, not on any instance.</Notice>
          <DeviceList
            devices={[
              { id: "1", kind: "mobile", label: "This iPhone", meta: "Added today by MacBook Pro", thisDevice: true },
              { id: "2", kind: "native", label: "MacBook Pro", meta: "Root device · added Oct 30" },
              { id: "3", kind: "web", label: "Safari", meta: "On chat.acme.dev · added Nov 1" },
            ]}
            onRevoke={() => {}}
          />
        </div>
      </PhonePanel>
      <div class="phone__scrim" aria-hidden="true"></div>
      <section class="phone__sheet" role="dialog" aria-modal="true" aria-labelledby="phone-revoke-title">
        <span class="phone__sheet-grip" aria-hidden="true"></span>
        <div class="lu-card__head">
          <h2 id="phone-revoke-title" class="lu-card__title">
            Revoke Safari?
          </h2>
          <p class="lu-card__subtitle">Safari stops reading new messages as soon as each instance gets the change. Enter your password to confirm.</p>
        </div>
        <TextField label="Password" type="password" reveal placeholder="Your password" autocomplete="current-password" />
        <div class="pieces__row">
          <Button variant="primary" size="md">Revoke Safari</Button>
          <Button size="md">Keep it</Button>
        </div>
      </section>
    </PhoneFrame>
  );
}

function PhoneServer() {
  return (
    <PhoneFrame label="A server: the rail and its sidebar" split>
      <PhoneRail current="acme" />
      <div class="phone__side">
        <div class="phone__side-head">
          <div class="phone__side-title">
            <h1>Acme</h1>
            <span class="phone__side-meta">chat.acme.dev · 128 members</span>
          </div>
          <IconButton icon="compose" label="New message" size={40} />
        </div>
        <div class="phone__side-body">
          <SearchTrigger label="Search Acme" shortcut="" />
          <QuickTiles
            items={[
              { icon: "thread", label: "Threads", count: 2 },
              { icon: "mention", label: "Mentions" },
              { icon: "saved", label: "Saved" },
            ]}
          />
          <ChannelList
            sections={[
              {
                title: "Channels",
                items: [
                  { name: "announcements" },
                  { name: "general", active: true },
                  { name: "design", unread: true },
                  { name: "engineering", unread: true, mentions: 2 },
                  { name: "random" },
                ],
              },
              {
                title: "Private",
                items: [
                  { name: "leadership", kind: "private" },
                  { name: "northstar-launch", kind: "private", unread: true },
                ],
              },
              {
                title: "Direct messages",
                items: [
                  { name: PEOPLE.maya.name, kind: "dm", initials: "MC", tone: "peach", status: "online", unread: true, count: 1 },
                  { name: PEOPLE.devon.name, kind: "dm", initials: "DP", tone: "purple", status: "away" },
                  { name: "Sam, Priya", kind: "group", people: [PEOPLE.sam, PEOPLE.priya] },
                ],
              },
            ]}
          />
        </div>
      </div>
    </PhoneFrame>
  );
}

function PhoneChannel() {
  return (
    <PhoneFrame label="A channel: members and settings behind the header's actions">
      <PhonePanel
        back="Acme"
        title={
          <>
            <Icon name="hash" size={18} strokeWidth={2.2} class="phone__hash" />
            general
          </>
        }
        subtitle="Company-wide announcements and water-cooler chat"
        actions={[
          { icon: "member-joined", label: "Members, 128" },
          { icon: "more", label: "Channel settings" },
        ]}
      >
        <MessageList>
          <Divider variant="day" label="Today" />
          <Message
            author={PEOPLE.maya.name}
            initials="MC"
            tone="peach"
            tag={{ label: "Design" }}
            time="9:04 AM"
            datetime="2026-11-03T09:04"
            reactions={[
              { icon: "thumbs-up", label: "Thumbs up", count: 12, defaultSelected: true },
              { icon: "check-circle", label: "Done", count: 5 },
            ]}
          >
            Morning all. The new onboarding flow is in the design channel if anyone wants a look before standup.
          </Message>
          <Message
            author={PEOPLE.devon.name}
            initials="DP"
            tone="purple"
            time="9:31 AM"
            datetime="2026-11-03T09:31"
            attachment={{ name: "Q4 all-hands agenda.pdf", size: "1.2 MB" }}
            thread={{ count: 14, lastReply: "11:36 AM", people: [PEOPLE.priya, PEOPLE.leo, PEOPLE.maya] }}
          >
            Q4 all-hands is Thursday at 2:00 PM in the Atrium and on the livestream. Drop your AMA questions in the thread.
          </Message>
          <SystemMessage time="10:02 AM" datetime="2026-11-03T10:02">
            <strong>Leo Martins</strong> joined #general
          </SystemMessage>
          <Message author={PEOPLE.priya.name} initials="PN" tone="amber" time="10:05 AM" datetime="2026-11-03T10:05" reactions={[{ icon: "heart", label: "Heart", count: 9, defaultSelected: true }]}>
            <p>
              Please welcome <Mention>@Leo Martins</Mention> to Design! He joins from Lisbon to lead our mobile work.
            </p>
          </Message>
          <Divider variant="new" />
          <Message author={PEOPLE.sam.name} initials="SO" tone="mint" time="11:48 AM" datetime="2026-11-03T11:48">
            Lunch is on us for the launch. Tacos land in the kitchen at 12:30.
          </Message>
        </MessageList>
        <Composer placeholder="Message #general" typing={PEOPLE.maya.name} hint="" />
      </PhonePanel>
    </PhoneFrame>
  );
}

function PhoneThread() {
  return (
    <PhoneFrame label="A thread: the aside as its own panel">
      <PhonePanel back="#general" title="Thread" subtitle="#general · 14 replies" actions={[{ icon: "more", label: "Thread options" }]}>
        <ThreadPanel
          parent={entry(PEOPLE.devon, "9:31 AM", "Q4 all-hands is Thursday at 2:00 PM in the Atrium and on the livestream. Drop your AMA questions in the thread.")}
          earlier={10}
          replies={[
            entry(PEOPLE.priya, "9:40 AM", "Will the livestream be recorded for anyone who cannot make it live?"),
            entry(PEOPLE.devon, "9:44 AM", "Yes. The recording goes up in #announcements by Friday morning."),
            entry(PEOPLE.leo, "10:31 AM", "Is there a seating plan for the Atrium? Want to find the design crew."),
            entry(PEOPLE.maya, "11:36 AM", "Design sits front-left, Leo. We will save you a seat."),
          ]}
          channel="general"
        />
      </PhonePanel>
    </PhoneFrame>
  );
}

function PhoneBoard() {
  return (
    <div class="phones">
      <PhoneSignIn />
      <PhoneHome />
      <PhoneConversation />
      <PhoneRequests />
      <PhoneDevices />
      <PhoneServer />
      <PhoneChannel />
      <PhoneThread />
    </div>
  );
}

// The canvas's "Server folders" boards (1440 by 900): the rail with a
// folder closed and open, a tile dragged between two and onto one, a
// server's and a folder's menu, and Folder settings.
const WORK: RailServer[] = [
  { id: "ns", name: "Northwind Studio", initials: "NS", ink: "indigo", unread: true },
  { id: "os", name: "Open Source Guild", initials: "OS", ink: "green", mentions: 2 },
  { id: "ds", name: "Design Systems Club", initials: "DS" },
  { id: "lb", name: "Lumia Beta", initials: "LB", mentions: 1 },
];

function folderRail(open: boolean): RailTile[] {
  return [
    { kind: "server", id: "acme", name: "Acme", initials: "A", current: true },
    { kind: "folder", id: "work", label: "Work", tone: "mint", open, servers: WORK },
    { kind: "server", id: "tr", name: "Trail Runners", initials: "TR", ink: "amber" },
  ];
}

function FoldersFigure(props: { caption: string; items: RailTile[]; still?: { id: string; drop: RailDrop | null; ghost: { left: number; top: number } } }) {
  return (
    <figure class="gallery-folders__figure">
      <ServerRail logoSrc="/brand/lumia-original-icon.svg" class="gallery-folders__rail" items={props.items} still={props.still} />
      <figcaption>{props.caption}</figcaption>
    </figure>
  );
}

function FoldersBoard() {
  return (
    <div class="gallery-folders">
      <header>
        <h1>Server rail: order and folders</h1>
        <p>Drag a server to move it, onto another to make a folder. Home stays first and + last. The arrangement is this account’s, the same on every device.</p>
      </header>
      <div class="gallery-folders__row">
        <div class="gallery-folders__rails">
          <FoldersFigure caption="A closed folder: its first four servers, its tone, and its servers’ mentions added up." items={folderRail(false)} />
          <FoldersFigure caption="Open: the folder’s glyph, then its servers in its tone. Open or closed is kept on each device." items={folderRail(true)} />
          <FoldersFigure caption="Between: a tile’s top or bottom quarter, shown as a line in the accent. Escape puts it back." items={folderRail(false)} still={{ id: "tr", drop: { kind: "between", before: "work" }, ghost: { left: 34, top: 140 } }} />
          <FoldersFigure caption="Onto: a tile’s middle half. Onto a server makes a folder of the two; onto a folder adds to it." items={folderRail(false)} still={{ id: "tr", drop: { kind: "onto", id: "acme", folder: false }, ghost: { left: 36, top: 70 } }} />
        </div>
        <div class="gallery-folders__side">
          <div class="pieces__row pieces__row--top">
            <div class="lu-menupair">
              <Menu label="Open Source Guild" class="lu-menu--rail">
                <MenuLabel>Open Source Guild</MenuLabel>
                <MenuItem icon="arrow-up">Move up</MenuItem>
                <MenuItem icon="arrow-down">Move down</MenuItem>
                <MenuRule />
                <MenuItem icon="folder-plus">New folder</MenuItem>
                <MenuItem icon="folder-plus" submenu={{ expanded: true }}>
                  Move to folder
                </MenuItem>
                <MenuItem icon="folder-remove">Remove from folder</MenuItem>
              </Menu>
              <Menu label="Move to folder" class="lu-menu--rail">
                <MenuItem icon="folder">Games</MenuItem>
                <MenuItem icon="folder">Acme and Trail Runners</MenuItem>
              </Menu>
            </div>
            <Menu label="Work" class="lu-menu--rail">
              <MenuLabel>Work</MenuLabel>
              <MenuItem icon="arrow-up">Move up</MenuItem>
              <MenuItem icon="arrow-down">Move down</MenuItem>
              <MenuRule />
              <MenuItem icon="preferences">Folder settings…</MenuItem>
              <MenuRule />
              <MenuItem icon="trash" danger>
                Delete folder
              </MenuItem>
            </Menu>
          </div>
          <FolderSettings name="Work" words="Northwind Studio, Open Source Guild and 2 more" tone="mint" />
          <p class="gallery-folders__note">A tile’s menu (right-click, Shift+F10 or the context menu key): the keyboard’s way to do everything a drag does.</p>
        </div>
      </div>
    </div>
  );
}

// Setting up private messages (issue #20): the "WebSignUpPhrase" board's
// step at its 1440 by 900, then the phone boards' check, "need this
// browser", start over and restore, as the web draws them; then linking
// by a prompt (identity spec 4.2.1, no board): waiting, the digits, and
// declined and expired; then restoring ("PhoneRestoreAsk", "PhoneRestoreKeep")
// and the sign-in notice of a browser another device removed.
const SAMPLE_PHRASE = "orbit cactus timber velvet harbor pencil meadow copper rescue glimpse fabric lunar anchor whisper salad ticket marble frozen clever sunny violin garden token mirror".split(" ");

function PrivateBoard() {
  return (
    <div class="gallery-private">
      <div class="gallery-private__step">
        <PhraseStep eyebrow="Create your account · Step 2 of 3" words={SAMPLE_PHRASE} />
      </div>
      <div class="gallery-private__row">
        <div class="gallery-private__card">
          <PhraseSaveWarning />
          <Card eyebrow="Create your account · Step 3 of 3" title="Check your phrase" subtitle="Type these three words from what you wrote down, to make sure it’s right.">
            <form class="auth__form" onSubmit={(e) => e.preventDefault()}>
              <PhraseWordField position={4} value="velvet" completions={[]} />
              <PhraseWordField position={11} value="fa" completions={["fabric", "face", "faculty", "fade"]} />
              <PhraseWordField position={19} value="" completions={[]} />
              <Button variant="primary" type="submit" block disabled>
                Confirm
              </Button>
              <Button block>Show the phrase again</Button>
            </form>
          </Card>
        </div>
        <div class="gallery-private__card">
          <NeedsBrowser onLink={() => {}} onRestore={() => {}} onStartOver={() => {}} />
          <RestoreForm initial={SAMPLE_PHRASE.slice(0, 12).join(" ")} onRestore={async () => {}} />
        </div>
        <div class="gallery-private__card">
          <StartOver oidcName="Acme SSO" returnTo={() => ""} onConfirmed={() => {}} onKeepLooking={() => {}} />
        </div>
      </div>
      <div class="gallery-private__row">
        <div class="gallery-private__card">
          <LinkPromptCard screen="waiting" digits="" onTryAgain={() => {}} onCancel={() => {}} />
          <LinkPromptCard screen="waiting" digits="" trouble="Can’t reach your instance; still trying. (transport: error sending request)" onTryAgain={() => {}} onCancel={() => {}} />
        </div>
        <div class="gallery-private__card">
          <LinkPromptCard screen="compare" digits="482 913" onTryAgain={() => {}} onCancel={() => {}} />
        </div>
        <div class="gallery-private__card">
          <LinkPromptCard screen="declined" digits="" onTryAgain={() => {}} onCancel={() => {}} />
          <LinkPromptCard screen="expired" digits="" onTryAgain={() => {}} onCancel={() => {}} />
        </div>
      </div>
      <div class="gallery-private__row">
        <div class="gallery-private__card">
          <RestoreAsk onLink={() => {}} onPhrase={() => {}} />
        </div>
        <div class="gallery-private__card">
          <RestoreKeep devices={SAMPLE_KEEP} initial={["mac"]} onRestore={async () => {}} />
        </div>
        <div class="gallery-private__card">
          <Notice icon="shield" action={{ label: "Dismiss" }}>
            {endedNotice(true).words}
          </Notice>
          <Card title="Welcome back" subtitle="Sign in to pick up your conversations." />
        </div>
      </div>
    </div>
  );
}

// The "PhoneRestoreKeep" board's three devices.
const SAMPLE_KEEP: DeviceSummary[] = [
  { deviceId: "mac", kind: "native", label: "MacBook Pro", webOrigin: "", addedAtMicros: new Date(2026, 10, 12).getTime() * 1000, thisDevice: false, revoked: false },
  { deviceId: "iphone", kind: "native", label: "iPhone", webOrigin: "", addedAtMicros: new Date(2026, 10, 3).getTime() * 1000, thisDevice: false, revoked: false },
  { deviceId: "safari", kind: "web", label: "", webOrigin: "https://chat.acme.dev", addedAtMicros: new Date(2026, 10, 1).getTime() * 1000, thisDevice: false, revoked: false },
];

// ── Roles and titles on the clients (architecture 7.5, plan step 11) ──
// Seven boards no canvas drew: a server's settings with its sections, its
// roles, a role's editor, its members, the tag beside a name under each of
// the server's two settings, the profile card, and the same on phones.
// Sample data only; nothing here asks the core for anything. Ryan owns
// Acme and holds no role. Devon's highest role is Moderator, and the
// figures of what someone may not manage are drawn as Devon sees them.

const ACME_ROLES: RoleEntry[] = [
  { id: "admin", name: "Admin", tone: "indigo", members: 2 },
  { id: "mod", name: "Moderator", tone: "green", members: 3 },
  { id: "design", name: "Design", tone: "peach", members: 12 },
  { id: "eng", name: "Engineering", tone: "sky", members: 24 },
  { id: "invite", name: "Can invite", tone: null, members: 41, untagged: true },
  { id: "everyone", name: "Everyone", tone: null, members: 128, everyone: true },
];

interface AcmePerson {
  id: string;
  name: string;
  initials: string;
  tone: Tone;
  handle: string;
  title?: string;
  /** The ids of the roles held. */
  roles: string[];
  mark?: "Owner" | "Guest";
  status?: "online" | "away";
}

const ACME: Record<"ryan" | "maya" | "devon" | "sam" | "priya" | "leo" | "jordan", AcmePerson> = {
  ryan: { ...PEOPLE.self, id: "ryan", handle: "ryan", title: "Founder", roles: [], mark: "Owner", status: "online" },
  maya: { ...PEOPLE.maya, id: "maya", handle: "maya", title: "Head of Design", roles: ["admin", "design"], status: "online" },
  devon: { ...PEOPLE.devon, id: "devon", handle: "devon", title: "Staff Engineer", roles: ["mod", "eng"], status: "away" },
  sam: { ...PEOPLE.sam, id: "sam", handle: "sam", title: "Backend Engineer", roles: ["eng", "invite"], status: "online" },
  priya: { ...PEOPLE.priya, id: "priya", handle: "priya", roles: ["design"] },
  leo: { ...PEOPLE.leo, id: "leo", handle: "leo", title: "Mobile Lead", roles: ["invite"] },
  jordan: { id: "jordan", name: "Jordan Lee", initials: "JL", tone: "stone", handle: "jordan", title: "Contractor", roles: [], mark: "Guest" },
};
const ACME_MEMBERS: AcmePerson[] = [ACME.ryan, ACME.maya, ACME.devon, ACME.sam, ACME.priya, ACME.leo, ACME.jordan];

/** The roles a person holds, in the server's order, as a card and a settings row list them. */
function rolesOf(p: AcmePerson): { label: string; tone: Tone | null }[] {
  return ACME_ROLES.filter((r) => p.roles.includes(r.id)).map((r) => ({ label: r.name, tone: r.tone ?? null }));
}

/**
 * The tag beside a name, as the core will work it out (authorization 3.5):
 * where the server shows roles, the highest role held that is shown as a
 * tag, in its tone; where it shows titles, the title on the plain chip;
 * nothing where there is none, and never one in place of the other.
 */
function tagOf(p: AcmePerson, shows: "roles" | "titles"): { label: string; tone: TagTone } | undefined {
  if (shows === "titles") return p.title ? { label: p.title, tone: "neutral" } : undefined;
  const role = ACME_ROLES.find((r) => p.roles.includes(r.id) && !r.untagged && !r.everyone);
  return role ? { label: role.name, tone: role.tone ?? "neutral" } : undefined;
}

function rosterEntry(p: AcmePerson): RosterEntry {
  return { id: p.id, name: p.name, initials: p.initials, tone: p.tone, handle: p.handle, title: p.title, mark: p.mark, roles: rolesOf(p) };
}

/** A person's roles to tick; `above` are the roles at or above the viewer's own highest, shown and not changeable. */
function choicesFor(p: AcmePerson, above: string[] = []): RoleChoice[] {
  return ACME_ROLES.filter((r) => !r.everyone).map((r) => ({ id: r.id, name: r.name, tone: r.tone, checked: p.roles.includes(r.id), locked: above.includes(r.id) }));
}

/** The roles at or above Devon's highest: what Devon may not manage. */
const ABOVE_DEVON = ["admin", "mod"];

function AcmeRail() {
  return (
    <ServerRail
      logoSrc="/brand/lumia-original-icon.svg"
      homeUnread={3}
      servers={[
        { name: "Acme", initials: "A", current: true },
        { name: "Design Guild", initials: "DG", ink: "indigo", unread: true },
        { name: "Lumia Builders", initials: "LB", ink: "green", mentions: 2 },
        { name: "Friends", initials: "F", ink: "amber" },
      ]}
      self={{ ...PEOPLE.self, status: "online" }}
    />
  );
}

function AcmeSidebar(props: { channel?: string }) {
  return (
    <>
      <ServerHeader name="Acme" onOpenMenu={() => {}} onNewMessage={() => {}} />
      <SearchTrigger label="Search Acme" />
      <QuickTiles
        items={[
          { icon: "thread", label: "Threads", count: 2 },
          { icon: "mention", label: "Mentions", count: 1 },
          { icon: "saved", label: "Saved" },
        ]}
      />
      <ChannelList
        sections={[
          {
            title: "Channels",
            items: [{ name: "general", active: props.channel === "general" }, { name: "design", unread: true, mentions: 1 }, { name: "engineering", unread: true }, { name: "random" }],
          },
          {
            title: "Private",
            items: [
              { name: "leadership", kind: "private" },
              { name: "northstar-launch", kind: "private", unread: true },
            ],
          },
          {
            title: "Direct messages",
            items: [
              { name: PEOPLE.maya.name, kind: "dm", initials: "MC", tone: "peach", status: "online", unread: true, count: 2 },
              { name: PEOPLE.devon.name, kind: "dm", initials: "DP", tone: "purple", status: "away" },
            ],
          },
        ]}
      />
    </>
  );
}

const SETTINGS_SECTIONS: { id: string; label: string; icon: IconName; note?: string }[] = [
  { id: "overview", label: "Overview", icon: "settings" },
  { id: "roles", label: "Roles", icon: "shield", note: "5 roles" },
  { id: "members", label: "Members", icon: "members", note: "128" },
];

/** Server settings in the server's main pane: the section list beside the section shown. */
function SettingsFrame(props: { section: "overview" | "roles" | "members"; tall?: boolean; onSection?: (id: string) => void; children: JSX.Element }) {
  return (
    <AppShell
      height={props.tall ? undefined : 900}
      class={props.tall ? "gallery-roles__tall" : undefined}
      rail={<AcmeRail />}
      sidebar={<AcmeSidebar />}
      main={
        <>
          <ChannelHeader kind="page" name="Server settings" topic="Acme" actions={[]} />
          <div class="lu-settings">
            <SectionList label="Server settings" sections={SETTINGS_SECTIONS.map((s) => ({ id: s.id, label: s.label, icon: s.icon }))} value={props.section} onChange={props.onSection} />
            <div class="lu-page">{props.children}</div>
          </div>
        </>
      }
    />
  );
}

/** A state a board cannot show at once: drawn on a pane of its own (or bare, where it is its own surface), a line under it saying what it is. */
function Figure(props: { caption: string; wide?: boolean; bare?: boolean; children: JSX.Element }) {
  return (
    <figure class={cx("gallery-roles__figure", props.wide && "gallery-roles__figure--wide")}>
      <Show when={!props.bare} fallback={props.children}>
        <div class="gallery-roles__pane">{props.children}</div>
      </Show>
      <figcaption>{props.caption}</figcaption>
    </figure>
  );
}

// Overview: today's cards (routes/server-settings.tsx) with sample values,
// and the one control this step adds, "Tag beside names".
function OverviewSection() {
  const [shows, setShows] = createSignal<"roles" | "titles">("roles");
  const [keep, setKeep] = createSignal("0");
  const [keepFiles, setKeepFiles] = createSignal("0");
  const [full, setFull] = createSignal("refuse");
  const days = [
    { id: "0", label: "Forever" },
    { id: "365", label: "1 year" },
    { id: "90", label: "90 days" },
    { id: "30", label: "30 days" },
  ];
  return (
    <>
      <Card title="Overview" subtitle="How the server shows on the rail and in invites.">
        <div class="app__photo">
          <span class="app__server-picture" aria-hidden="true">
            A
          </span>
          <div class="app__photo-words">
            <div class="app__row">
              <Button size="sm">Choose a picture…</Button>
            </div>
            <p class="auth__note">A PNG, JPEG, WebP or GIF up to 8 MB. Everyone sees its middle, as a circle.</p>
          </div>
        </div>
        <form class="auth__form" onSubmit={(e) => e.preventDefault()}>
          <TextField label="Name" value="Acme" placeholder="Acme" autocomplete="off" />
          <Button variant="primary" size="md" type="submit" disabled>
            Save
          </Button>
        </form>
      </Card>
      <Card title="Tag beside names" subtitle="What shows beside a member’s name in messages and in the member list.">
        <div class="auth__form">
          <PictureChoices
            label="Tag beside names"
            class="lu-pictiles--two"
            value={shows()}
            onChange={setShows}
            options={[
              { value: "roles", label: "Roles", caption: "Each member’s highest role that is shown as a tag, in the role’s colour.", picture: <TagSketch shows="roles" /> },
              { value: "titles", label: "Titles", caption: "Each member’s title, in no colour. A member without a title has no tag.", picture: <TagSketch shows="titles" /> },
            ]}
          />
          <p class="auth__note">A profile shows a member’s title and roles either way.</p>
        </div>
      </Card>
      <Card title="Default channels" subtitle="New members join these channels.">
        <div class="auth__form">
          <Checkbox label="#general" checked />
          <Checkbox label="#design" />
          <Checkbox label="#engineering" />
          <Checkbox label="#random" checked />
          <Button variant="primary" size="md">
            Save
          </Button>
        </div>
      </Card>
      <Card title="Message history" subtitle="How long this server keeps its messages.">
        <div class="auth__form">
          <SegmentedTabs label="Keep messages" tabs={days} value={keep()} onChange={setKeep} />
          <Button variant="primary" size="md" disabled={keep() === "0"}>
            Save
          </Button>
          <div class="app__switch">
            <Checkbox label="Show deleted messages" />
            <p class="auth__note">Off, a deleted message disappears for everyone. On, it stays in its place as “(deleted)”.</p>
          </div>
        </div>
      </Card>
      <Card title="Files" subtitle="The attachments sent in this server's channels.">
        <div class="auth__form">
          <p class="auth__note">412 MB of 2 GB used</p>
          <SegmentedTabs label="Keep attachments" tabs={days} value={keepFiles()} onChange={setKeepFiles} />
          <SegmentedTabs
            label="When the server is full"
            tabs={[
              { id: "refuse", label: "Refuse new uploads" },
              { id: "delete", label: "Delete the oldest attachments" },
            ]}
            value={full()}
            onChange={setFull}
          />
          <p class="auth__note">{full() === "delete" ? "When an upload would not fit, the oldest messages’ attachments are deleted for everyone to make room." : "When an upload would not fit, it is refused until files are deleted."}</p>
          <Button variant="primary" size="md" disabled={keepFiles() === "0" && full() === "refuse"}>
            Save
          </Button>
        </div>
      </Card>
    </>
  );
}

function SettingsBoard(props: { onSection: (id: string) => void }) {
  return (
    <div class="gallery-roles">
      <p class="gallery-roles__lead">Server settings with sections: Overview, Roles, Members, each shown to those who may use it (Manage server, Manage roles, Manage members). Overview keeps today’s cards and gains “Tag beside names”, saved as it is chosen. The pane scrolls; here it is drawn at its full length.</p>
      <SettingsFrame section="overview" tall onSection={props.onSection}>
        <OverviewSection />
      </SettingsFrame>
    </div>
  );
}

function RolesSection(props: { roles: RoleEntry[]; reorder?: "drag" | "menu"; still?: { id: string; before: string }; hint: string }) {
  return (
    <>
      <div class="lu-settings__head">
        <div class="lu-settings__headtext">
          <h2 class="lu-settings__title">Roles</h2>
          <p class="lu-settings__subtitle">{props.hint}</p>
        </div>
        <Button size="sm" icon="plus">
          New role
        </Button>
      </div>
      <RoleList roles={props.roles} reorder={props.reorder} still={props.still} onOpen={() => {}} onMenu={() => {}} />
    </>
  );
}


/** A role's menu: what a right-click, Shift+F10 or the context menu key opens on its row, and its More button on a phone. */
function RoleMenu(props: { name: string; first?: boolean; last?: boolean; class?: string }) {
  return (
    <Menu label={props.name} compact class={cx("lu-menu--context", props.class)}>
      <MenuLabel>{props.name}</MenuLabel>
      <MenuItem icon="settings">Edit role</MenuItem>
      <MenuRule />
      <MenuItem icon="arrow-up" disabled={props.first}>
        Move up
      </MenuItem>
      <MenuItem icon="arrow-down" disabled={props.last}>
        Move down
      </MenuItem>
      <MenuRule />
      <MenuItem icon="trash" danger>
        Delete role
      </MenuItem>
    </Menu>
  );
}

function RolesBoard(props: { onSection: (id: string) => void }) {
  const lifted = ACME_ROLES.filter((r) => r.id === "eng");
  return (
    <div class="gallery-roles">
      <SettingsFrame section="roles" onSection={props.onSection}>
        <RolesSection roles={ACME_ROLES} hint={ROLES_HINT} />
      </SettingsFrame>
      <div class="gallery-roles__more">
        <Figure caption="The same list to Devon, who holds Manage roles and whose highest role is Moderator. Admin and Moderator are at or above it: no handle, a lock, and they do not open. The rest are Devon’s to move, change and delete.">
          <RoleList roles={ACME_ROLES.map((r) => ({ ...r, locked: ABOVE_DEVON.includes(r.id) }))} onOpen={() => {}} />
        </Figure>
        <Figure caption="A drag, held: Engineering lifted from its place, which stays dashed, and a line in the accent where it would land, above Design. Escape puts it back. The whole order is sent as one request when it is let go.">
          <div class="gallery-roles__drag">
            <RoleList roles={ACME_ROLES} still={{ id: "eng", before: "design" }} />
            <RoleList class="gallery-roles__ghost" roles={lifted} />
          </div>
        </Figure>
        <Figure bare caption="A role’s menu (right-click, Shift+F10 or the context menu key on its row): the keyboard’s way to do what a drag does. The highest role has no way up, and the last above Everyone no way down.">
          <div class="pieces__row pieces__row--top">
            <RoleMenu name="Design" />
            <RoleMenu name="Admin" first />
          </div>
        </Figure>
      </div>
    </div>
  );
}

// The fourteen permissions a client offers, under their headings: the
// core's list (`lumia_client::roles`, which the screens read through
// `rolePermissions`), written out here as sample data.
const PERMISSIONS: { heading: string; items: { id: string; name: string; caption: string }[] }[] = [
  {
    heading: "Server",
    items: [
      { id: "administrator", name: "Administrator", caption: "Everything below, in every channel. A channel’s own permissions do not limit it." },
      { id: "manage_server", name: "Manage server", caption: "Change the server’s name, picture and settings." },
      { id: "manage_roles", name: "Manage roles", caption: "Create, change and delete roles below their own highest, and give them to members." },
      { id: "manage_channels", name: "Manage channels", caption: "Create channels, and add people to them or remove them." },
      { id: "manage_members", name: "Manage members", caption: "Kick members below them and change those members’ titles." },
      { id: "create_invites", name: "Create invites", caption: "Make invite links and send invitations to join the server." },
    ],
  },
  {
    heading: "Channels",
    items: [
      { id: "view_channel", name: "See channels", caption: "See the server’s public channels and join them." },
      { id: "read_history", name: "Read message history", caption: "Read the messages already in a channel." },
    ],
  },
  {
    heading: "Messages",
    items: [
      { id: "send_messages", name: "Send messages", caption: "Write in the channels they are in." },
      { id: "attach_files", name: "Attach files", caption: "Add pictures, videos and files to a message." },
      { id: "add_reactions", name: "Add reactions", caption: "React to messages with emoji." },
      { id: "mention_everyone", name: "Mention everyone", caption: "Notify a whole channel with @everyone." },
      { id: "manage_messages", name: "Manage messages", caption: "Delete anyone’s messages." },
      { id: "start_dms", name: "Start direct messages", caption: "Start a direct message with another member of the server." },
    ],
  },
];

const EVERYONE_ON = ["create_invites", "view_channel", "read_history", "send_messages", "attach_files", "add_reactions", "start_dms"];
const MODERATOR_ON = ["manage_roles", "manage_channels", "manage_members", "mention_everyone", "manage_messages"];

type EditorPart = "head" | "identity" | "server" | "channels" | "messages" | "holders" | "delete";

/**
 * A role's editor. A new role has no holders and nothing to delete;
 * Everyone has its permissions and nothing else. `parts` draws some of it
 * (a phone's screen, scrolled), `withheld` the permissions the viewer does
 * not hold and so cannot give.
 */
function RoleEditor(props: { role?: RoleEntry; on: string[]; withheld?: string[]; holders?: AcmePerson[]; parts?: EditorPart[]; class?: string }) {
  const everyone = () => !!props.role?.everyone;
  const [name, setName] = createSignal(props.role?.name ?? "");
  const [tone, setTone] = createSignal<Tone | null>(props.role?.tone ?? null);
  const [tagged, setTagged] = createSignal(!props.role?.untagged);
  const [on, setOn] = createSignal(props.on);
  const has = (part: EditorPart) => !props.parts || props.parts.includes(part);
  const permissions = (heading: string) => PERMISSIONS.find((g) => g.heading === heading)?.items ?? [];
  const row = (p: { id: string; name: string; caption: string }) => {
    const withheld = () => !!props.withheld?.includes(p.id);
    return (
      <SettingRow
        name={p.name}
        caption={withheld() ? `${p.caption} You don’t hold it, so it is not yours to give or take.` : p.caption}
        control={(ids) => <Toggle labelledBy={ids.name} describedBy={ids.caption} checked={on().includes(p.id)} disabled={withheld()} onChange={(checked) => setOn(checked ? [...on(), p.id] : on().filter((id) => id !== p.id))} />}
      />
    );
  };
  const group = (heading: string) => (
    <>
      <h4 class="lu-roleedit__group">{heading}</h4>
      <Show when={heading === "Server"}>
        <SettingsGroup class="lu-setgroup--apart">
          <For each={permissions(heading).filter((p) => p.id === "administrator")}>{row}</For>
        </SettingsGroup>
      </Show>
      <SettingsGroup>
        <For each={permissions(heading).filter((p) => p.id !== "administrator")}>{row}</For>
      </SettingsGroup>
    </>
  );
  return (
    <div class={cx("lu-roleedit", props.class)}>
      <Show when={has("head")}>
        <div class="lu-settings__head">
          <IconButton icon="back" label="Back to roles" />
          <div class="lu-settings__headtext">
            <h2 class="lu-settings__title">
              <RoleDot tone={tone()} />
              {props.role ? (everyone() ? "Everyone" : name() || props.role.name) : "New role"}
            </h2>
            <Show when={everyone()}>
              <p class="lu-settings__subtitle">What every member may do before any role adds to it. Guests are not among them.</p>
            </Show>
          </div>
          <Button variant="primary" size="md" disabled={!name().trim()}>
            Save
          </Button>
        </div>
      </Show>
      <Show when={has("identity") && !everyone()}>
        <div class="lu-roleedit__part">
          <div class="lu-roleedit__field">
            <TextField label="Name" value={name()} maxLength={32} placeholder="Moderator" autocomplete="off" onInput={(e) => setName(e.currentTarget.value)} />
            <p class="lu-roleedit__hint">1 to 32 characters. No other role in Acme can have the same name.</p>
          </div>
          <div class="lu-roleedit__field">
            <span class="lu-roleedit__legend">Colour</span>
            <ToneSwatches label="Colour" value={tone()} onChange={setTone} onNone={() => setTone(null)} />
          </div>
          <SettingsGroup>
            <SettingRow
              name="Show as tag"
              caption={tagged() ? "This role can be the tag beside a member’s name." : "This role only carries permissions. It is nobody’s tag."}
              control={(ids) => <Toggle labelledBy={ids.name} describedBy={ids.caption} checked={tagged()} onChange={setTagged} />}
            />
            <div class="lu-roleedit__preview">
              <span class="lu-roleedit__previewlabel">Preview</span>
              <Avatar initials="DP" tone="purple" size={28} />
              <span class="lu-roleedit__previewname">Devon Park</span>
              <Show when={tagged() && name().trim()}>
                <Tag tone={tone() ?? "neutral"}>{name().trim()}</Tag>
              </Show>
              <span class="lu-roleedit__previewtime">9:11 AM</span>
            </div>
          </SettingsGroup>
        </div>
      </Show>
      <Show when={has("server") || has("channels") || has("messages")}>
        <div class="lu-roleedit__part">
          <Show when={has("server")}>
            <h3 class="lu-roleedit__heading">Permissions</h3>
            {group("Server")}
          </Show>
          <Show when={has("channels")}>{group("Channels")}</Show>
          <Show when={has("messages")}>{group("Messages")}</Show>
        </div>
      </Show>
      <Show when={has("holders") && props.holders}>
        {(holders) => (
          <div class="lu-roleedit__part">
            <div class="lu-roleedit__row">
              <h3 class="lu-roleedit__heading">
                Members with this role <span class="lu-settings__count">{holders().length}</span>
              </h3>
              <Button size="sm" icon="plus">
                Add members
              </Button>
            </div>
            <div class="lu-roleedit__holders">
              <MemberList
                sections={[
                  {
                    members: holders().map((p) => ({
                      name: p.name,
                      initials: p.initials,
                      tone: p.tone,
                      subtitle: "@" + p.handle,
                      action: { label: "Remove from role", description: `Remove ${p.name} from ${props.role?.name ?? "this role"}`, onClick: () => {} },
                    })),
                  },
                ]}
              />
            </div>
          </div>
        )}
      </Show>
      <Show when={has("delete") && props.role && !everyone()}>
        <div class="lu-roleedit__part">
          <h3 class="lu-roleedit__heading">Delete role</h3>
          <div class="lu-roleedit__row">
            <p class="lu-roleedit__hint">Its members lose what it allowed. This can’t be undone.</p>
            <Button variant="danger" size="md">
              Delete role
            </Button>
          </div>
        </div>
      </Show>
    </div>
  );
}

const MODERATOR = ACME_ROLES.find((r) => r.id === "mod");
const EVERYONE = ACME_ROLES.find((r) => r.everyone);
const MODERATORS: AcmePerson[] = [ACME.devon, { id: "noor", name: "Noor Haddad", initials: "NH", tone: "green", handle: "noor", roles: ["mod"] }, { id: "alex", name: "Alex Rivera", initials: "AR", tone: "amber", handle: "arivera", roles: ["mod"] }];

/** The two questions the editor asks first. */
function RoleConfirmations() {
  return (
    <>
      <ConfirmPop title="Turn on Administrator?" confirm="Turn on">
        <p>Administrator allows everything in Acme, in every channel, whatever a channel’s own permissions say.</p>
        <p>3 members hold Moderator.</p>
      </ConfirmPop>
      <ConfirmPop title="Delete Moderator?" confirm="Delete role" danger>
        <p>3 members hold this role. They lose what it allowed.</p>
        <p>This can’t be undone.</p>
      </ConfirmPop>
    </>
  );
}

function RoleEditorBoard(props: { onSection: (id: string) => void }) {
  return (
    <div class="gallery-roles">
      <p class="gallery-roles__lead">A role’s editor, opened from its row in Roles: its name, colour and tag, its permissions under three headings, who holds it, and Delete. One Save keeps it all. The pane scrolls; here it is drawn at its full length.</p>
      <SettingsFrame section="roles" tall onSection={props.onSection}>
        <RoleEditor role={MODERATOR} on={MODERATOR_ON} holders={MODERATORS} />
      </SettingsFrame>
      <div class="gallery-roles__more">
        <Figure wide caption="The editor for Everyone: permissions only. No name, colour, tag switch, holders or Delete.">
          <RoleEditor role={EVERYONE} on={EVERYONE_ON} />
        </Figure>
        <div class="gallery-roles__stack">
          <Figure bare caption="Asked before Administrator is turned on, against its switch; and before a role is deleted, against Delete role. Cancel has the keyboard first.">
            <RoleConfirmations />
          </Figure>
          <Figure caption="“New role”: the same editor with nothing in it yet. Save is off until it has a name; it is placed last, just above Everyone. No holders, nothing to delete.">
            <RoleEditor on={[]} parts={["head", "identity"]} />
          </Figure>
          <Figure caption="“Show as tag” off, and no colour: the preview has no tag. In Roles the row carries “Not shown as tag”.">
            <RoleEditor role={ACME_ROLES.find((r) => r.id === "invite")} on={["create_invites"]} parts={["identity"]} />
          </Figure>
          <Figure caption="To Devon, editing Engineering: Devon holds neither Administrator nor Manage server, so cannot give them. Each is off, dimmed, and says why.">
            <RoleEditor role={ACME_ROLES.find((r) => r.id === "eng")} on={[]} withheld={["administrator", "manage_server"]} parts={["server"]} />
          </Figure>
        </div>
      </div>
    </div>
  );
}

function MembersSection(props: { members: AcmePerson[]; open?: string; editor?: (m: RosterEntry) => JSX.Element; more?: boolean }) {
  return (
    <>
      <div class="lu-settings__head">
        <div class="lu-settings__headtext">
          <h2 class="lu-settings__title">
            Members <span class="lu-settings__count">128</span>
          </h2>
        </div>
      </div>
      <SearchField label="Search members" />
      <Roster members={props.members.map(rosterEntry)} open={props.open} editor={props.editor} />
      <Show when={props.more}>
        <Button size="sm" block>
          Show more members
        </Button>
      </Show>
    </>
  );
}

/** Devon's editing state as the owner sees it: every role changeable, the title, Kick. */
const devonEditor = () => <MemberEdit name={ACME.devon.name} roles={choicesFor(ACME.devon)} title={ACME.devon.title ?? ""} onKick={() => {}} />;

function MembersBoard(props: { onSection: (id: string) => void }) {
  return (
    <div class="gallery-roles">
      <SettingsFrame section="members" tall onSection={props.onSection}>
        <MembersSection members={ACME_MEMBERS} open="devon" editor={devonEditor} more />
      </SettingsFrame>
      <div class="gallery-roles__more">
        <Figure wide caption="To Devon (Manage roles and Manage members, highest role Moderator), Sam’s row: Admin and Moderator are shown and not changeable; the roles below Devon’s own are. Sam is below Devon, so the title and Kick are offered.">
          <Roster members={[rosterEntry(ACME.sam)]} open="sam" editor={() => <MemberEdit name={ACME.sam.name} roles={choicesFor(ACME.sam, ABOVE_DEVON)} title={ACME.sam.title ?? ""} onKick={() => {}} />} />
        </Figure>
        <Figure wide caption="To Devon, Maya’s row: Maya is above Devon, so no title and no Kick; the roles below Devon’s own can still be given and taken, whoever the member is.">
          <Roster members={[rosterEntry(ACME.maya)]} open="maya" editor={() => <MemberEdit name={ACME.maya.name} roles={choicesFor(ACME.maya, ABOVE_DEVON)} />} />
        </Figure>
        <Figure wide caption="A guest: a title and Kick. A guest is offered no roles.">
          <Roster members={[rosterEntry(ACME.jordan)]} open="jordan" editor={() => <MemberEdit name={ACME.jordan.name} rolesNote="Guests aren’t given roles." title={ACME.jordan.title ?? ""} onKick={() => {}} />} />
        </Figure>
      </div>
    </div>
  );
}

// The same morning in #general under each of the server's two settings.
const TAG_TALK: { who: AcmePerson; time: string; text: string }[] = [
  { who: ACME.maya, time: "9:04 AM", text: "Morning all. The new onboarding flow is in the design channel if anyone wants a look before standup." },
  { who: ACME.devon, time: "9:11 AM", text: "Looks great. I’ll check the copy on the second screen against the tone guide." },
  { who: ACME.sam, time: "9:20 AM", text: "The importer keeps its old path for one more release, so nothing blocks it." },
  { who: ACME.priya, time: "9:24 AM", text: "Two screens changed since Friday, both in the sign-in card." },
  { who: ACME.leo, time: "9:31 AM", text: "Mobile gets the same composer this week." },
  { who: ACME.ryan, time: "9:38 AM", text: "Thanks, everyone. Thursday still stands." },
  { who: ACME.jordan, time: "9:40 AM", text: "I’ll have the audit notes in by then." },
];

function TagsShell(props: { shows: "roles" | "titles" }) {
  const member = (p: AcmePerson) => {
    const tag = tagOf(p, props.shows);
    return { name: p.name, initials: p.initials, tone: p.tone, status: p.status, tag: tag?.label, tagTone: tag?.tone };
  };
  return (
    <AppShell
      height={720}
      rail={<AcmeRail />}
      sidebar={<AcmeSidebar channel="general" />}
      main={
        <>
          <ChannelHeader name="general" topic="Company-wide announcements and water-cooler chat" />
          <MessageList>
            <Divider variant="day" label="Today" />
            <For each={TAG_TALK}>
              {(m) => (
                <Message author={m.who.name} initials={m.who.initials} tone={m.who.tone} tag={tagOf(m.who, props.shows)} time={m.time} actions={false}>
                  {m.text}
                </Message>
              )}
            </For>
          </MessageList>
          <Composer placeholder="Message #general" />
        </>
      }
      aside={
        <SidePanel tabs={[{ id: "members", label: "Members", count: 128 }]} value="members" onChange={() => {}}>
          <SidePanelSearch>
            <SearchField label="Find a member" />
          </SidePanelSearch>
          <SidePanelScroll>
            <MemberList
              sections={[
                { title: "Online · 32", members: [ACME.ryan, ACME.maya, ACME.devon, ACME.sam].map(member) },
                { title: "Offline · 96", members: [ACME.priya, ACME.leo, ACME.jordan].map(member) },
              ]}
              total={128}
            />
          </SidePanelScroll>
        </SidePanel>
      }
    />
  );
}

const LONGEST_TITLE = "Principal Reliability Engineer I"; // 32 characters

function TagsBoard() {
  const tones: Tone[] = ["peach", "blue", "purple", "mint", "amber", "sky", "stone", "green", "indigo"];
  return (
    <div class="gallery-roles">
      <p class="gallery-roles__lead">Acme shows roles, the default: beside each name, the member’s highest role that is shown as a tag, on the role’s tone. Leo holds only Can invite, which is not shown as a tag; Ryan owns Acme and holds no role; Jordan is a guest. None of the three has a tag.</p>
      <TagsShell shows="roles" />
      <p class="gallery-roles__lead">The same conversation with Acme showing titles: each member’s title on the plain chip, in no colour. Priya has no title, so no tag; her Design role does not stand in for it.</p>
      <TagsShell shows="titles" />
      <div class="gallery-roles__more">
        <Figure wide caption="A role’s tag on each of the nine tones and with no colour, as a message draws it and as the member list does; then a title’s, and the accent’s “New”, which stays as it was.">
          <div class="pieces__row">
            <For each={tones}>{(t) => <Tag tone={t}>{t[0]?.toUpperCase() + t.slice(1)}</Tag>}</For>
            <Tag>No colour</Tag>
          </div>
          <div class="pieces__row gallery-roles__gap">
            <For each={tones}>
              {(t) => (
                <Tag tone={t} size="sm">
                  {t[0]?.toUpperCase() + t.slice(1)}
                </Tag>
              )}
            </For>
            <Tag size="sm">No colour</Tag>
          </div>
          <div class="pieces__row gallery-roles__gap">
            <Tag>Staff Engineer</Tag>
            <Tag size="sm">Staff Engineer</Tag>
            <Tag tone="accent">New</Tag>
            <Tag tone="accent" size="sm">
              You
            </Tag>
          </div>
        </Figure>
        <Figure wide caption="The longest a tag can be, 32 characters: whole in a message where the row has room, and in a member row cut with an ellipsis, the tag giving way before the name does.">
          <MessageList class="pieces__list">
            <Message author={ACME.devon.name} initials="DP" tone="purple" tag={{ label: LONGEST_TITLE }} time="9:11 AM" actions={false}>
              Looks great.
            </Message>
            <Message author={ACME.devon.name} initials="DP" tone="purple" tag={{ label: "Community and Events Coordinator", tone: "mint" }} time="9:12 AM" actions={false}>
              A role’s name can be as long.
            </Message>
          </MessageList>
          <div class="gallery-roles__narrow">
            <MemberList
              sections={[
                {
                  members: [
                    { ...PEOPLE.devon, status: "online", tag: LONGEST_TITLE, tagTone: "neutral" },
                    { ...PEOPLE.priya, status: "online", tag: "Community and Events Coordinator", tagTone: "mint" },
                  ],
                },
              ]}
            />
          </div>
        </Figure>
      </div>
    </div>
  );
}

function ProfileCardBoard() {
  const devon = { name: ACME.devon.name, initials: "DP", tone: "purple" as const, handle: "devon", status: "away" as const, title: ACME.devon.title, roles: rolesOf(ACME.devon) };
  const ways = { action: { label: "Message Devon" }, secondary: { label: "Message privately" } };
  return (
    <div class="gallery-roles gallery-roles--cards">
      <p class="gallery-roles__lead">The profile card in a server: the title under the name and every role the member holds, whichever of the two the server shows as tags. Opened from a name or a member’s row, as today.</p>
      <div class="gallery-roles__more">
        <Figure bare caption="As others see it.">
          <ProfileCard {...devon} {...ways} />
        </Figure>
        <Figure bare caption="As someone who may edit sees it (here the owner): Edit beside the title and beside Roles. Someone with Manage roles alone has the second; Manage members and a member below them, the first.">
          <ProfileCard {...devon} {...ways} edit={{ title: () => {}, roles: () => {} }} />
        </Figure>
        <Figure bare caption="What each Edit opens, against it: the roles to tick, those not the viewer’s to change shown with a lock (none here, for the owner); and the title, up to 32 characters, cleared to have none.">
          <div class="gallery-roles__pops">
            <Pop title="Roles" subtitle="Devon Park in Acme">
              <div class="lu-pop__form">
                <RoleChoices label="Devon Park’s roles" roles={choicesFor(ACME.devon)} />
                <div class="lu-pop__actions">
                  <Button size="md">Cancel</Button>
                  <Button variant="primary" size="md">
                    Save
                  </Button>
                </div>
              </div>
            </Pop>
            <Pop title="Title" subtitle="Devon Park in Acme">
              <form class="lu-pop__form" onSubmit={(e) => e.preventDefault()}>
                <TextField label="Title" value="Staff Engineer" maxLength={32} placeholder="No title" autocomplete="off" />
                <p class="lu-pop__note">Up to 32 characters. A title grants nothing.</p>
                <div class="lu-pop__actions">
                  <Button size="md">Cancel</Button>
                  <Button variant="primary" size="md" type="submit">
                    Save
                  </Button>
                </div>
              </form>
            </Pop>
          </div>
        </Figure>
      </div>
      <div class="gallery-roles__more">
        <Figure bare caption="A member’s own card: their own title is theirs to edit. Their roles are not, unless they manage roles.">
          <ProfileCard name={ACME.sam.name} initials="SO" tone="mint" handle="sam" status="online" tag="You" title={ACME.sam.title} roles={rolesOf(ACME.sam)} edit={{ title: () => {} }} />
        </Figure>
        <Figure bare caption="Their own card with no title yet: “Add a title” in its place.">
          <ProfileCard name={ACME.priya.name} initials="PN" tone="amber" handle="priya" status="online" tag="You" roles={rolesOf(ACME.priya)} edit={{ title: () => {} }} />
        </Figure>
        <Figure bare caption="The owner, marked on the card. Being the owner is no role and no tag; Ryan holds no roles, so none are listed.">
          <ProfileCard name={ACME.ryan.name} initials="RW" tone="indigo" handle="ryan" status="online" tag="Owner" title={ACME.ryan.title} action={{ label: "Message Ryan" }} secondary={{ label: "Message privately" }} />
        </Figure>
        <Figure bare caption="A guest, to someone who may edit: a title, and no roles to give.">
          <ProfileCard name={ACME.jordan.name} initials="JL" tone="stone" handle="jordan" status={null} tag="Guest" title={ACME.jordan.title} edit={{ title: () => {} }} action={{ label: "Message Jordan" }} />
        </Figure>
        <Figure bare caption="A role that is not shown as a tag is still listed on the card, on the plain chip when it has no colour.">
          <ProfileCard name={ACME.leo.name} initials="LM" tone="sky" handle="leo" status={null} title={ACME.leo.title} roles={rolesOf(ACME.leo)} action={{ label: "Message Leo" }} />
        </Figure>
      </div>
    </div>
  );
}

// The phones: the sections as a screen of their own, then each section as
// a panel pushed over it. A role is moved from its menu, never dragged.
function RolePhonesBoard() {
  const roster = (people: AcmePerson[], open?: string) => <Roster members={people.map(rosterEntry)} open={open} editor={devonEditor} />;
  const editor = (label: string, parts: EditorPart[]) => (
    <PhoneFrame label={label}>
      <PhonePanel back="Roles" title="Moderator" subtitle="Acme · 3 members" actions={[{ icon: "check", label: "Save" }]}>
        <div class="phone__panel-body">
          <RoleEditor role={MODERATOR} on={MODERATOR_ON} holders={MODERATORS} parts={parts} />
        </div>
      </PhonePanel>
    </PhoneFrame>
  );
  const roles = () => (
    <div class="phone__panel-body">
      <p class="lu-settings__subtitle">In order, highest first. A role is moved from its menu. A new role starts last, just above Everyone.</p>
      <RoleList roles={ACME_ROLES} reorder="menu" onOpen={() => {}} onMenu={() => {}} />
      <Button size="sm" block icon="plus">
        New role
      </Button>
    </div>
  );
  return (
    <div class="phones">
      <PhoneFrame label="Server settings: its sections as a screen of their own">
        <PhonePanel back="Acme" title="Server settings" subtitle="Acme">
          <div class="phone__panel-body">
            <SectionList label="Server settings" sections={SETTINGS_SECTIONS} />
          </div>
        </PhonePanel>
      </PhoneFrame>
      <PhoneFrame label="Roles: no handles; each role has More, Everyone has none">
        <PhonePanel back="Server settings" title="Roles" subtitle="Acme · 5 roles">
          {roles()}
        </PhonePanel>
      </PhoneFrame>
      <PhoneFrame label="A role’s menu: Move up and Move down in place of a drag">
        <PhonePanel back="Server settings" title="Roles" subtitle="Acme · 5 roles">
          {roles()}
        </PhonePanel>
        <RoleMenu name="Design" class="phone__menu" />
      </PhoneFrame>
      {editor("A role’s editor: name, colour, tag and its preview", ["identity", "server"])}
      {editor("The same, further down: its permissions", ["channels", "messages"])}
      {editor("The same, at its end: who holds it, and Delete role", ["holders", "delete"])}
      <PhoneFrame label="Members: the roles under the name and handle">
        <PhonePanel back="Server settings" title="Members" subtitle="128 in Acme">
          <div class="phone__panel-body">
            <SearchField label="Search members" />
            {roster(ACME_MEMBERS)}
          </div>
        </PhonePanel>
      </PhoneFrame>
      <PhoneFrame label="A member’s editing state: roles, title, Kick">
        <PhonePanel back="Server settings" title="Members" subtitle="128 in Acme">
          <div class="phone__panel-body">
            <SearchField label="Search members" />
            {roster([ACME.maya, ACME.devon, ACME.sam], "devon")}
          </div>
        </PhonePanel>
      </PhoneFrame>
      <PhoneFrame label="Deleting a role asks first, in a sheet">
        <PhonePanel back="Roles" title="Moderator" subtitle="Acme · 3 members" actions={[{ icon: "check", label: "Save" }]}>
          <div class="phone__panel-body">
            <RoleEditor role={MODERATOR} on={MODERATOR_ON} holders={MODERATORS} parts={["holders", "delete"]} />
          </div>
        </PhonePanel>
        <div class="phone__scrim" aria-hidden="true"></div>
        <section class="phone__sheet" role="dialog" aria-modal="true" aria-labelledby="phone-delete-role-title">
          <span class="phone__sheet-grip" aria-hidden="true"></span>
          <div class="lu-card__head">
            <h2 id="phone-delete-role-title" class="lu-card__title">
              Delete Moderator?
            </h2>
            <p class="lu-card__subtitle">3 members hold this role. They lose what it allowed. This can’t be undone.</p>
          </div>
          <div class="pieces__row">
            <Button variant="danger" size="md">
              Delete role
            </Button>
            <Button size="md">Keep it</Button>
          </div>
        </section>
      </PhoneFrame>
    </div>
  );
}

type Board = "split" | "replies" | "home" | "requests" | "devices" | "signin" | "private" | "phone" | "folders" | "preferences" | "settings" | "roles" | "role" | "members" | "tags" | "pcard" | "rolephones" | "pieces";

export function DesignGallery() {
  const { theme, apply } = useTheme();
  const [board, setBoard] = createSignal<Board>("split");
  // A settings board's section list leads to the board that draws that section.
  const toSection = (id: string) => setBoard(id === "roles" ? "roles" : id === "members" ? "members" : "settings");
  const roleBoard = () => {
    const b = board();
    return b === "settings" ? <SettingsBoard onSection={toSection} /> : b === "roles" ? <RolesBoard onSection={toSection} /> : b === "role" ? <RoleEditorBoard onSection={toSection} /> : b === "members" ? <MembersBoard onSection={toSection} /> : b === "tags" ? <TagsBoard /> : b === "pcard" ? <ProfileCardBoard /> : b === "rolephones" ? <RolePhonesBoard /> : undefined;
  };
  return (
    <div class="gallery">
      <header class="gallery__bar">
        <strong>Lumia design system</strong>
        <nav class="gallery__nav" aria-label="Boards">
          <For each={[["split", "Quiet Split"], ["replies", "Inline replies"], ["home", "Home"], ["requests", "Requests"], ["devices", "Devices"], ["signin", "Sign in"], ["private", "Private messages"], ["phone", "Phone"], ["folders", "Server folders"], ["preferences", "Preferences"], ["settings", "Server settings"], ["roles", "Roles"], ["role", "Role editor"], ["members", "Members"], ["tags", "Tags"], ["pcard", "Profile card"], ["rolephones", "Roles on phones"], ["pieces", "Pieces"]] as const}>
            {([id, label]) => (
              <button type="button" class="gallery__tab" aria-pressed={board() === id} onClick={() => setBoard(id)}>
                {label}
              </button>
            )}
          </For>
        </nav>
        <nav class="gallery__nav" aria-label="Theme">
          <button type="button" class="gallery__tab" aria-pressed={theme() === "light"} onClick={() => apply("light")}>
            Light
          </button>
          <button type="button" class="gallery__tab" aria-pressed={theme() === "dark"} onClick={() => apply("dark")}>
            Dark
          </button>
        </nav>
      </header>
      <div class="gallery__board">
        {board() === "split" ? <QuietSplit /> : board() === "replies" ? <RepliesBoard /> : board() === "home" ? <HomeBoard view="dm" /> : board() === "requests" ? <HomeBoard view="requests" /> : board() === "devices" ? <HomeBoard view="devices" /> : board() === "signin" ? <SignIn /> : board() === "private" ? <PrivateBoard /> : board() === "phone" ? <PhoneBoard /> : board() === "folders" ? <FoldersBoard /> : board() === "preferences" ? <PreferencesBoard /> : board() === "pieces" ? <Pieces /> : roleBoard()}
      </div>
    </div>
  );
}

/** A sample paragraph as the core gives one: `text`, with each mark's words found in it styled or linked. */
function para(text: string, ...marks: { words: string; style?: BodyRun["style"]; url?: string }[]): BodyBlock {
  const runs: BodyRun[] = [];
  const links: BodyLink[] = [];
  for (const m of marks) {
    const start = text.indexOf(m.words);
    const end = start + m.words.length;
    if (m.style) runs.push({ start, end, style: m.style });
    if (m.url) links.push({ start, end, url: m.url });
  }
  return { kind: "para", text, runs, links };
}

// A long message, a code block with a line wider than any row, a quote, lists and the inline styles (issue #18).
const LONG_BODY: BodyBlock[] = [
  para("Notes from the planning call, so nobody has to scroll the recording:", { words: "planning call", style: { kind: "strong" } }),
  {
    kind: "list",
    start: 1,
    items: [
      [para("Onboarding ships Thursday, behind the flag until support has read the new copy.")],
      [para("The importer keeps its old path for one more release; after that it goes.")],
      [para("Search stays out of scope this quarter."), { kind: "list", items: [[para("Ask again in January.")], [para("Sam owns the write-up.")]] }],
      [para("Mobile gets the same composer: Return sends on the web and the desktop, the phones keep their own key.")],
    ],
  },
  para("Open questions\nWho reviews the copy before Thursday?\nDo we need a second migration window?\nCan the flag go per server rather than per instance?\nWhat does support want to see in the dashboard?", { words: "Open questions", style: { kind: "strong" } }),
  para("Decisions\nThe flag is per server.\nSupport reads the copy on Wednesday.\nThe second migration window waits until someone asks for it.", { words: "Decisions", style: { kind: "strong" } }),
  para("Everything else is in the doc, and the recording is pinned in #planning for anyone who wants the long version."),
];
const CODE_BODY: BodyBlock[] = [
  para("The handler that answers it, and the line that is too long for any screen:"),
  {
    kind: "code",
    lang: "rust",
    text: "fn refuse(body: &str, max: u32) -> Result<(), ClientError> {\n    if !body::fits(body, max) {\n        return Err(ClientError::TooLong { over: body::room(body, max).left.unsigned_abs(), max }); // the instance would answer PAYLOAD_TOO_LARGE all the same, so the core says it before the round trip\n    }\n    Ok(())\n}",
  },
];
const QUOTE_BODY: BodyBlock[] = [
  { kind: "quote", blocks: [para("A message is someone else’s text: draw what they wrote, not what a document would be.")] },
  para("Agreed, and the quote keeps its own lines too."),
];
const LONG_URL = "https://example.com/a/very/long/path/that/keeps/going/and/going/and/going/until/it/is/wider/than/the/row";
const STYLED_BODY: BodyBlock[] = [
  para(
    "@Ryan the build is mostly green: run it again after Friday today, with bun run check, and the notes are on lumia.chat/docs.",
    { words: "@Ryan", style: { kind: "mention", index: 0 } },
    { words: "mostly", style: { kind: "emphasis" } },
    { words: "run it again", style: { kind: "strong" } },
    { words: "Friday", style: { kind: "strike" } },
    { words: "bun run check", style: { kind: "code" } },
    { words: "lumia.chat/docs", url: "https://lumia.chat/docs" },
  ),
  para(`A word with no room to break: ${LONG_URL}`, { words: LONG_URL, url: LONG_URL }),
];
const EDITED_CODE: BodyBlock[] = [para("Fixed the typo:"), { kind: "code", text: "bun run e2e" }];
const PASTE = "A paste of many lines keeps them all:\nline two\nline three\nline four\nline five\nline six\nline seven\nline eight\nline nine, past which the field scrolls\nline ten\nline eleven";

/** Minimal's lines; with reactions hidden a message is handed no chips, as the timeline hands it none. */
function OneLine(props: { label: string; reactions: boolean }) {
  return (
    <MessageList label={props.label} layout="one_line" spacing="tight" avatars="hidden" class="pieces__list">
      <NoticeLine time="10:02 AM" datetime="2026-11-02T10:02">
        Leo Martins joined
      </NoticeLine>
      <Message
        layout="one_line"
        author={PEOPLE.leo.name}
        initials="LM"
        tone="sky"
        time="10:08 AM"
        datetime="2026-11-02T10:08"
        actions={false}
        onOpenProfile={() => {}}
        parts={[{ time: "10:09 AM", datetime: "2026-11-02T10:09", children: "Also: who do I ask about a desk by the windows? The ones by the plants get the morning light and I would happily trade a monitor for one.", reactions: props.reactions ? [{ emoji: "👋", label: "Wave", count: 3 }] : [] }]}
      >
        Thank you all, what a warm welcome! My calendar is open for coffee chats this week.
      </Message>
      <Message
        layout="one_line"
        author={PEOPLE.priya.name}
        initials="PR"
        tone="peach"
        time="10:11 AM"
        datetime="2026-11-02T10:11"
        actions={false}
        mentioned
        onOpenProfile={() => {}}
        reference={<ReplyRef name={PEOPLE.leo.name} initials="LM" tone="sky" text="Also: who do I ask about a desk by the windows?" />}
        reactions={props.reactions ? [{ emoji: "👍", label: "Thumbs up", count: 1, defaultSelected: true }] : []}
      >
        Ask Sam, they keep the seating plan.
      </Message>
    </MessageList>
  );
}

/** Preferences' pieces on their own: picture tiles (one chosen, and none), the compact segmented control, switches, an outlined group of rows, and the preview at two settings. */
function SettingsPieces() {
  const [theme, setTheme] = createSignal<"light" | "dark" | "system">("system");
  const [spacing, setSpacing] = createSignal("cozy");
  const [size, setSize] = createSignal(15);
  const [on, setOn] = createSignal(true);
  return (
    <>
      <section>
        <h2>Picture tiles: a theme chosen; presets with none chosen (Custom)</h2>
        <div class="pieces__settings">
          <PictureChoices
            label="Appearance"
            value={theme()}
            onChange={setTheme}
            options={[
              { value: "light", label: "Light", picture: <ThemeSketch theme="light" /> },
              { value: "dark", label: "Dark", picture: <ThemeSketch theme="dark" /> },
              { value: "system", label: "System", picture: <ThemeSketch theme="system" /> },
            ]}
          />
          <PictureChoices
            label="Preset"
            value={undefined}
            options={[
              { value: "comfortable", label: "Comfortable", caption: "Large avatars, room to breathe", picture: <LayoutSketch layout="comfortable" /> },
              { value: "compact", label: "Compact", caption: "Small avatars, more on screen", picture: <LayoutSketch layout="compact" /> },
              { value: "minimal", label: "Minimal", caption: "A line each, media as links", picture: <LayoutSketch layout="minimal" /> },
            ]}
          />
        </div>
      </section>
      <section>
        <h2>An outlined group: compact segmented controls (words, and numbers of one width) and switches, each caption saying what its state does</h2>
        <div class="pieces__settings">
          <SettingsGroup>
            <SettingRow
              name="Spacing"
              caption="Room between messages."
              control={(ids) => (
                <Segmented
                  labelledBy={ids.name}
                  options={[
                    { value: "roomy", label: "Roomy" },
                    { value: "cozy", label: "Cozy" },
                    { value: "tight", label: "Tight" },
                  ]}
                  value={spacing()}
                  onChange={setSpacing}
                />
              )}
            />
            <SettingRow
              name="Font size"
              caption="Messages and names, in points."
              control={(ids) => <Segmented even labelledBy={ids.name} options={[13, 14, 15, 16, 18, 20].map((s) => ({ value: s, label: String(s), name: `${s} points` }))} value={size()} onChange={setSize} />}
            />
          </SettingsGroup>
          <SettingsGroup>
            <SettingRow name="Embeds" caption={on() ? "A card under each link." : "As links: no cards."} control={(ids) => <Toggle labelledBy={ids.name} describedBy={ids.caption} checked={on()} onChange={setOn} />} />
            <SettingRow name="Reactions" caption="Hidden. You can still add one from a message’s toolbar." control={(ids) => <Toggle labelledBy={ids.name} describedBy={ids.caption} checked={false} />} />
          </SettingsGroup>
        </div>
      </section>
      <section>
        <h2>The preview of messages: Comfortable, and Minimal</h2>
        <div class="pieces__row pieces__row--top">
          <div class="pieces__preview">
            <MessagesPreview display={PRESETS.comfortable} />
          </div>
          <div class="pieces__preview">
            <MessagesPreview display={PRESETS.minimal} />
          </div>
        </div>
      </section>
    </>
  );
}

// The "Preferences · Light" and "· Dark" boards (1440 by 900): the page
// drawn from values of its own, so a choice here changes nothing of this
// browser's; its preview at the font size chosen.
function PreferencesBoard() {
  const [value, setValue] = createSignal<Preferences>({ appearance: "light", ...PRESETS.comfortable, fontSize: 15, asideHidden: false });
  const set = (next: Partial<Preferences>) => setValue((v) => ({ ...v, ...next }));
  return (
    <div class="gallery-prefs" style={{ "--msg-scale": String(value().fontSize / 15) }}>
      <AppShell
        height={900}
        rail={
          <ServerRail
            logoSrc="/brand/lumia-original-icon.svg"
            homeCurrent
            servers={[
              { name: "Acme", initials: "AC" },
              { name: "Trail Runners", initials: "TR", ink: "amber" },
            ]}
            self={{ ...PEOPLE.self, status: "online" }}
          />
        }
        sidebar={<HomeSidebar view="requests" />}
        main={<PreferencesView value={value()} onAppearance={(appearance) => set({ appearance })} onPreset={(preset) => set(PRESETS[preset])} onChange={set} />}
      />
    </div>
  );
}

function Pieces() {
  return (
    <div class="pieces">
      <section>
        <h2>Icons</h2>
        <div class="pieces__row">
          <For each={icons}>{(name) => <span class="pieces__icon" title={name}><Icon name={name} size={20} /></span>}</For>
        </div>
      </section>
      <section>
        <h2>Buttons</h2>
        <div class="pieces__row">
          <Button variant="primary">Sign in</Button>
          <Button>Continue with SSO</Button>
          <Button size="sm">View all 128 members</Button>
          <Button variant="text">Show 10 earlier replies</Button>
          <IconButton icon="search" label="Search" />
          <IconButton icon="send" variant="send" size={40} label="Send message" />
        </div>
      </section>
      <section>
        <h2>People</h2>
        <div class="pieces__row">
          <Avatar initials="MC" tone="peach" status="online" label="Maya Chen, online" />
          <Avatar initials="DP" tone="purple" size={34} status="away" label="Devon Park, away" />
          <Avatar initials="SO" tone="mint" size={28} status="dnd" label="Sam Okafor, do not disturb" />
          <AvatarStack people={[PEOPLE.sam, PEOPLE.priya, PEOPLE.leo]} />
          <Badge count={3} label="3 unread" />
          <Badge dot />
          <Tag>Design</Tag>
          <Tag tone="accent">New</Tag>
          <ReactionChip icon="heart" label="Heart" count={2} />
          <ReactionChip icon="check-circle" label="Done" count={5} defaultSelected />
          <TypingIndicator name="Maya Chen" />
          <TypingIndicator name={["Maya Chen", "Devon Park"]} />
          <TypingIndicator name={["Maya Chen", "Devon Park", "Sam Okafor"]} />
        </div>
      </section>
      <section>
        <h2>A block: one person's quick messages</h2>
        <Message author={PEOPLE.leo.name} initials="LM" tone="sky" tag={{ label: "New", tone: "accent" }} time="10:08 AM" datetime="2026-11-02T10:08" actions={false} onOpenProfile={() => {}} parts={[{ time: "10:09", datetime: "2026-11-02T10:09", children: "Also: who do I ask about a desk by the windows?" }]}>
          Thank you all, what a warm welcome! My calendar is open for coffee chats this week.
        </Message>
      </section>
      <section>
        <h2>The same block, compact (a preference of this browser: Cozy, small avatars)</h2>
        <MessageList label="Compact" spacing="cozy" class="pieces__list">
          <Message author={PEOPLE.leo.name} initials="LM" tone="sky" time="10:08 AM" datetime="2026-11-02T10:08" actions={false} avatars="small" onOpenProfile={() => {}} parts={[{ time: "10:09", datetime: "2026-11-02T10:09", children: "Also: who do I ask about a desk by the windows?" }]}>
            Thank you all, what a warm welcome! My calendar is open for coffee chats this week.
          </Message>
        </MessageList>
      </section>
      <section>
        <h2>Grouped with avatars hidden (Roomy)</h2>
        <MessageList label="No avatars" avatars="hidden" class="pieces__list">
          <Message author={PEOPLE.leo.name} initials="LM" tone="sky" time="10:08 AM" datetime="2026-11-02T10:08" actions={false} avatars="hidden" onOpenProfile={() => {}} parts={[{ time: "10:09", datetime: "2026-11-02T10:09", children: "Also: who do I ask about a desk by the windows?" }]}>
            Thank you all, what a warm welcome! My calendar is open for coffee chats this week.
          </Message>
        </MessageList>
      </section>
      <section>
        <h2>Minimal: one line, Tight, reactions hidden</h2>
        <OneLine label="Minimal" reactions={false} />
      </section>
      <section>
        <h2>The same lines with reactions shown (Custom)</h2>
        <OneLine label="One line" reactions />
      </section>
      <SettingsPieces />
      <section>
        <h2>Over the frame: the account menu, a server's menu, the profile card, a popover</h2>
        <div class="pieces__row pieces__row--top">
          <Menu label="Your account">
            <MenuHead name={PEOPLE.self.name} sub="@ryan · Online" initials="RW" tone="indigo" status="online" />
            <MenuRule />
            <MenuLabel>Status</MenuLabel>
            <MenuRadio status="online" checked>
              Online
            </MenuRadio>
            <MenuRadio status="away" checked={false}>
              Away
            </MenuRadio>
            <MenuRadio status="dnd" checked={false}>
              Do not disturb
            </MenuRadio>
            <MenuRule />
            <MenuItem icon="user">View profile</MenuItem>
            <MenuItem icon="preferences">Preferences</MenuItem>
            <MenuItem icon="shield">Devices</MenuItem>
            <MenuRule />
            <MenuItem icon="sign-out">Sign out</MenuItem>
          </Menu>
          <Menu label="Acme" compact>
            <MenuItem icon="member-joined">Invite people</MenuItem>
            <MenuItem icon="hash">New channel</MenuItem>
            <MenuItem icon="settings">Server settings</MenuItem>
          </Menu>
          <ProfileCard
            name={PEOPLE.maya.name}
            initials="MC"
            tone="peach"
            handle="maya"
            status="online"
            details={[
              { icon: "emoji", label: "Status", text: "Heads down until 2 PM" },
              { icon: "user", label: "Pronouns", text: "she/her" },
              { icon: "mention", label: "About", text: "People Ops. Ask me about benefits." },
            ]}
            action={{ label: "Message Maya" }}
          />
          <Pop title="Invite people" subtitle="A link to join Acme.">
            <div class="lu-pop__form">
              <Choice label="Expires" value={24} options={[{ value: 0, label: "Never" }, { value: 24, label: "In a day" }, { value: 168, label: "In a week" }]} />
              <Choice label="Uses" value={0} options={[{ value: 0, label: "Unlimited" }, { value: 1, label: "Once" }, { value: 10, label: "Ten" }]} />
              <Button variant="primary" size="md" block>
                Make a link
              </Button>
            </div>
          </Pop>
        </div>
      </section>
      <section>
        <h2>Being in channels: an invitation, Browse, a picker, notices, the join bar, a server DM's card</h2>
        <div class="pieces__row pieces__row--top">
          <div style={{ width: "240px" }}>
            <InvitationRow channel="research" by={PEOPLE.maya.name} />
          </div>
          <ul class="lu-browselist" style={{ width: "420px" }}>
            <BrowseRow name="random" topic="Anything at all" count={12} />
            <BrowseRow name="music" count={1} />
          </ul>
          <Pop title="Add people" subtitle="To #general.">
            <div class="lu-pop__form">
              <ul class="lu-picklist">
                <PickRow name={PEOPLE.leo.name} initials="LM" tone="sky" note="Will be invited" checked />
                <PickRow name={PEOPLE.maya.name} initials="MC" tone="peach" checked={false} />
                <PickRow name="Cy Twombly" initials="CT" tone="amber" note="Unavailable" disabled />
              </ul>
              <Button variant="primary" size="md" block>
                Add
              </Button>
            </div>
          </Pop>
          <ProfileCard name={PEOPLE.maya.name} initials="MC" tone="peach" handle="maya" status="online" action={{ label: "Message Maya" }} secondary={{ label: "Message privately" }} />
        </div>
        <div class="pieces__row" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "640px" }}>
          <NoticeLine time="9:41 AM">{PEOPLE.maya.name} added {PEOPLE.leo.name}</NoticeLine>
          <NoticeLine time="9:44 AM">{PEOPLE.leo.name} left</NoticeLine>
          <JoinBar channel="random" />
        </div>
      </section>
      <section>
        <h2>Replies, reactions, mentions and read state: the line of what a reply quotes, mention pills by name, a row that names you, reactions, “(edited)”, the toolbar, the “New” line</h2>
        <div class="pieces__row" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "720px" }}>
          <MessageList>
            <Message
              author={PEOPLE.maya.name}
              initials="MC"
              tone="peach"
              time="9:04 AM"
              datetime="2026-11-13T09:04"
              actions={false}
              onOpenProfile={() => {}}
              id="g1"
              reactions={[
                { emoji: "👍", label: "👍", count: 3, selected: true, who: "You, Devon and Sam reacted with 👍" },
                { emoji: "🎉", label: "🎉", count: 5, who: "Devon, Sam and 3 others reacted with 🎉" },
              ]}
              onAddReaction={() => {}}
              parts={[
                {
                  time: "9:05",
                  datetime: "2026-11-13T09:05",
                  children: (
                    <p>
                      The flows are in the design channel.{" "}
                      <span class="lu-msg__edited">
                        <span aria-hidden="true">(edited)</span>
                        <span class="lu-sr">edited</span>
                      </span>
                    </p>
                  ),
                  id: "g2",
                },
              ]}
            >
              Morning all. The new onboarding flow is ready for a look before standup.
            </Message>
            <Divider variant="new" />
            <Message
              author={PEOPLE.devon.name}
              initials="DP"
              tone="purple"
              time="9:12 AM"
              datetime="2026-11-13T09:12"
              actions={false}
              onOpenProfile={() => {}}
              mentioned
              reference={<ReplyRef {...PEOPLE.maya} text="Morning all. The new onboarding flow is ready for a look before standup, and the copy is in the doc beside it so we can go through both together." onOpen={() => {}} />}
            >
              <p>
                Looks great. <Mention onOpen={() => {}}>@{PEOPLE.self.name}</Mention> can you check the copy on the second screen?
              </p>
            </Message>
            <Message author={PEOPLE.sam.name} initials="SO" tone="mint" time="9:20 AM" datetime="2026-11-13T09:20" actions={false} onOpenProfile={() => {}} reference={<ReplyRef deleted text="" />}>
              Agreed with the above.
            </Message>
            <Message author={PEOPLE.leo.name} initials="LM" tone="sky" time="9:24 AM" datetime="2026-11-13T09:24" actions={false} onOpenProfile={() => {}} reference={<ReplyRef text="The quote of someone a personal conversation does not name." onOpen={() => {}} />}>
              A personal conversation’s reply to someone unknown: no avatar, no name.
            </Message>
          </MessageList>
          <div class="pieces__row pieces__row--top">
            <MessageActions floating={false} onReply={() => {}} />
            <MessageActions floating={false} onReply={() => {}} replying />
            <QuoteReplyButton inline />
            <MessageMenu onEdit={() => {}} onCopy={() => {}} onDelete={() => {}} />
            <ConfirmDelete text="Morning all. The new onboarding flow is ready for a look before standup." />
          </div>
        </div>
        <h2>A right-click’s menus: this account’s own message, someone else’s to a member, a person in a server, a person at Home, this account itself</h2>
        <div class="pieces__row pieces__row--top">
          <MessageContextMenu onEdit={() => {}} onDelete={() => {}} />
          <MessageContextMenu />
          <PersonMenu name={PEOPLE.maya.name} ways={[{ label: "Message Maya" }, { label: "Message privately" }]} />
          <PersonMenu name={PEOPLE.maya.name} ways={[{ label: "Message Maya" }]} />
          <PersonMenu name={PEOPLE.self.name} />
        </div>
        <h2>Editing a message where it is drawn</h2>
        <div class="pieces__row" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "720px" }}>
          <MessageList>
            <Message author={PEOPLE.self.name} initials="RW" tone="indigo" time="9:30 AM" datetime="2026-11-13T09:30" actions={false} onOpenProfile={() => {}}>
              <MessageEdit value="Sure, after standup works. I’ll bring the second screen." onSave={() => {}} onCancel={() => {}} />
            </Message>
          </MessageList>
        </div>
        <h2>Long messages, code blocks and long pastes: a long body collapsed and the same opened, a code block with a line too long for the row (Copy under the pointer), a quote, lists, the inline styles, “(edited)” after a code block</h2>
        {/* Not wrapped: a wrapping row is as wide as its widest line, and a code block's is wider than any row. */}
        <div class="pieces__row" style={{ "flex-direction": "column", "flex-wrap": "nowrap", "align-items": "stretch", "max-width": "720px" }}>
          <MessageList>
            <Message author={PEOPLE.maya.name} initials="MC" tone="peach" time="9:40 AM" datetime="2026-11-14T09:40" actions={false} onOpenProfile={() => {}}>
              <MessageBody blocks={LONG_BODY} expanded={false} />
            </Message>
            <Message author={PEOPLE.maya.name} initials="MC" tone="peach" time="9:40 AM" datetime="2026-11-14T09:40" actions={false} onOpenProfile={() => {}} mentioned>
              <MessageBody blocks={LONG_BODY} expanded />
            </Message>
            <Message author={PEOPLE.devon.name} initials="DP" tone="purple" time="9:44 AM" datetime="2026-11-14T09:44" actions={false} onOpenProfile={() => {}} parts={[{ time: "9:45", datetime: "2026-11-14T09:45", children: <MessageBody blocks={EDITED_CODE} edited /> }]}>
              <MessageBody blocks={CODE_BODY} />
            </Message>
            <Message author={PEOPLE.sam.name} initials="SO" tone="mint" time="9:50 AM" datetime="2026-11-14T09:50" actions={false} onOpenProfile={() => {}}>
              <MessageBody blocks={QUOTE_BODY} />
            </Message>
            <Message author={PEOPLE.leo.name} initials="LM" tone="sky" time="9:52 AM" datetime="2026-11-14T09:52" actions={false} onOpenProfile={() => {}}>
              <MessageBody blocks={STYLED_BODY} mention={() => <Mention onOpen={() => {}}>@{PEOPLE.self.name}</Mention>} edited />
            </Message>
          </MessageList>
        </div>
        <h2>The composer near the limit and over it (Send off), a paste of many lines (eight shown, then it scrolls), an edit over the limit</h2>
        <div class="pieces__row" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "720px" }}>
          <Composer placeholder="Message #design" tools={false} value="The release notes, nearly at the limit." room={{ shown: true, left: 1204 }} />
          <Composer placeholder="Message #design" tools={false} value="The release notes, past the limit." room={{ shown: true, left: -37 }} />
          <Composer placeholder="Message #design" tools={false} value={PASTE} />
          <MessageList>
            <Message author={PEOPLE.self.name} initials="RW" tone="indigo" time="9:55 AM" datetime="2026-11-14T09:55" actions={false} onOpenProfile={() => {}}>
              <MessageEdit value="The whole log, pasted in again." room={{ shown: true, left: -1 }} onSave={() => {}} onCancel={() => {}} />
            </Message>
          </MessageList>
        </div>
        <h2>Over the composer: the reply bar atop its frame (on, off, to oneself), a note after sending, the @ list</h2>
        <div class="pieces__row" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "720px" }}>
          <div style={{ position: "relative", "padding-top": "300px" }}>
            <Composer
              placeholder="Message #design"
              tools={false}
              hint="Return to send · Esc to cancel the reply"
              above={
                <MentionList
                  id="gallery-mentions"
                  active={1}
                  options={[
                    { id: "everyone", name: "everyone", everyone: true, note: "Notify everyone in #design" },
                    { id: "1", name: PEOPLE.maya.name, handle: "maya", initials: "MC", tone: "peach" },
                    { id: "2", name: "Mateo Ruiz", handle: "mateo", initials: "MR", tone: "sky", note: "Not in #design" },
                  ]}
                />
              }
              bar={<ReplyBar name={PEOPLE.maya.name} mention />}
            />
          </div>
          <Composer placeholder="Message #design" tools={false} hint="Return to send · Esc to cancel the reply" bar={<ReplyBar name={PEOPLE.maya.name} mention={false} />} />
          <Composer placeholder="Message #design" tools={false} hint="Return to send · Esc to cancel the reply" bar={<ReplyBar name={PEOPLE.self.name} />} />
          <Composer
            placeholder="Message #design"
            tools={false}
            above={
              <ComposerNote actions={[{ label: "Add to #design", primary: true }, { label: "Dismiss" }]}>
                Mateo and Priya aren’t in #design, so they won’t be notified.
              </ComposerNote>
            }
          />
          <Composer placeholder="Message #design" tools={false} above={<ComposerNote>Invited Mateo to #design.</ComposerNote>} />
          <Composer placeholder="Message #design" tools={false} above={<ComposerNote error="You cannot do that in this channel." actions={[{ label: "Add to #design", primary: true }, { label: "Dismiss" }]}>Mateo isn’t in #design, so they won’t be notified.</ComposerNote>} />
        </div>
        <h2>The emoji picker, Mentions as a server’s quick tile and Home’s row, unread in the sidebar, a Mentions page’s item</h2>
        <div class="pieces__row pieces__row--top">
          <EmojiPicker frequent={QUICK_REACTIONS} />
          <div style={{ width: "240px", display: "flex", "flex-direction": "column", gap: "12px" }}>
            <QuickTiles items={[{ icon: "mention", label: "Mentions", count: 3, countLabel: "3 mentions" }]} />
            <ChannelList
              sections={[
                { items: [{ name: "Mentions", icon: "mention", mentions: 3 }] },
                {
                  title: "Channels",
                  items: [{ name: "general", active: true }, { name: "design", unread: true, mentions: 2 }, { name: "random", unread: true }, { name: "announcements" }],
                },
              ]}
            />
          </div>
          <ul class="lu-mentionentries" style={{ width: "420px" }}>
            <MentionEntry where="Acme · #design">
              <Message author={PEOPLE.devon.name} initials="DP" tone="purple" time="Yesterday, 4:30 PM" actions={false} reference={<ReplyRef {...PEOPLE.self} self text="Can someone look at the second screen?" />}>
                <p>
                  <Mention>@{PEOPLE.self.name}</Mention> done, it reads well now.
                </p>
              </Message>
            </MentionEntry>
          </ul>
        </div>
      </section>
      <MediaPieces />
    </div>
  );
}

/** A sample picture: a sky over hills, in two colours, at a shape. Data, not a product asset. */
function samplePicture(width: number, height: number, sky: string, hill: string, blur = false): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">${blur ? '<filter id="b"><feGaussianBlur stdDeviation="18"/></filter>' : ""}<g${blur ? ' filter="url(#b)"' : ""}><defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${sky}"/><stop offset="1" stop-color="#fff4e0"/></linearGradient></defs><rect width="100%" height="100%" fill="url(#s)"/><circle cx="${width * 0.7}" cy="${height * 0.32}" r="${Math.min(width, height) * 0.1}" fill="#ffd56b"/><path d="M0 ${height * 0.72} Q ${width * 0.3} ${height * 0.5} ${width * 0.55} ${height * 0.7} T ${width} ${height * 0.62} V ${height} H 0 Z" fill="${hill}"/></g></svg>`;
  return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
}

const PICTURES = {
  tall: samplePicture(900, 1200, "#7aa7e8", "#4f8a5b"),
  wide: samplePicture(1600, 900, "#f0a38a", "#6c5aa8"),
  square: samplePicture(800, 800, "#8fd3c9", "#2f6f73"),
  dusk: samplePicture(1200, 800, "#b28be0", "#3b3f7a"),
  placeholder: samplePicture(1200, 800, "#9fb6d8", "#5b7f6a", true),
};

/** The shapes of a shown player and post, as the core gives them, for the embeds' boxes. */
const GALLERY_PLAYER: EmbedSummary = { provider: "youtube", providerName: "YouTube", kind: "player", url: "", blueskyHandle: "", blueskyRkey: "", needsParent: false, aspectWidth: 16, aspectHeight: 9, fixedHeight: 0, resizes: false, maxWidth: 0, label: "Play video from YouTube" };
const GALLERY_POST: EmbedSummary = { ...GALLERY_PLAYER, provider: "x", providerName: "X", kind: "post", aspectWidth: 0, aspectHeight: 0, fixedHeight: 250, resizes: true, maxWidth: 550, label: "Show post from X" };

function MediaPieces() {
  const grid = (n: number): MediaTileProps[] =>
    [PICTURES.wide, PICTURES.square, PICTURES.dusk, PICTURES.tall, PICTURES.wide].slice(0, n).map((src, i) => ({ kind: "image", name: `photo-${i + 1}.jpg`, width: 1200, height: 800, src, onOpen: () => {} }));
  return (
    <section>
      <h2>Attachments in a message (specs/media.md 5): a picture alone at its shape, at most 400 tall; grids of two, three and five; a GIF held still for reduced motion with its badge; a video’s poster, a video with none, and a picture still drawn as its placeholder</h2>
      <div class="pieces__row pieces__row--top" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "760px" }}>
        <MessageList>
          <Message {...PEOPLE.maya} author={PEOPLE.maya.name} time="10:02 AM" actions={false}>
            <MessageBody blocks={[para("The view from the ridge this morning.")]} />
            <div class="lu-mediablock">
              <MediaGrid items={[{ kind: "image", name: "ridge.jpg", alt: "A blue sky over green hills, the sun high on the right", width: 900, height: 1200, src: PICTURES.tall, onOpen: () => {} }]} />
            </div>
          </Message>
          <Message {...PEOPLE.devon} author={PEOPLE.devon.name} time="10:05 AM" actions={false}>
            <div class="lu-mediablock">
              <MediaGrid items={grid(2)} />
              <MediaGrid items={grid(3)} />
              <MediaGrid items={grid(5)} />
            </div>
          </Message>
          <Message {...PEOPLE.sam} author={PEOPLE.sam.name} time="10:09 AM" actions={false}>
            <div class="lu-mediablock">
              <MediaGrid
                items={[
                  { kind: "gif", name: "wave.gif", width: 480, height: 360, src: PICTURES.square, still: true, onOpen: () => {} },
                  { kind: "video", name: "launch.mp4", width: 1600, height: 900, src: PICTURES.wide, onOpen: () => {} },
                  { kind: "video", name: "screen.webm", width: 1280, height: 720, onOpen: () => {} },
                  { kind: "image", name: "late.jpg", width: 1200, height: 800, placeholder: PICTURES.placeholder, onOpen: () => {} },
                ]}
              />
            </div>
          </Message>
        </MessageList>
      </div>
      <h2>Audio and files (with Images “As links”, every attachment is such a row, its name opening the viewer), and link cards: a picture beside, a wide picture under, a video’s, none; the site is always the link’s host</h2>
      <div class="pieces__row pieces__row--top" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "760px" }}>
        <MessageList>
          <Message {...PEOPLE.priya} author={PEOPLE.priya.name} time="11:20 AM" actions={false}>
            <MessageBody blocks={[para("Notes and the recording from Thursday.")]} />
            <div class="lu-mediablock">
              <AudioAttachment name="standup-thursday.m4a" size="3.1 MB" />
              <AudioAttachment name="standup-friday.m4a" size="2.8 MB" busy />
              <Attachment kind="file" name="Q4-roadmap.pdf" size="1.2 MB" />
              <Attachment kind="image" name="ridge.jpg" size="412 KB" onOpen={() => {}} />
              <Attachment kind="video" name="launch.mp4" size="8.4 MB" onOpen={() => {}} />
              <Attachment kind="file" name="Q4-roadmap.pdf" size="1.2 MB" note="Couldn’t be opened." />
              <Attachment kind="image" name="offsite.jpg" note="No longer available" gone />
            </div>
          </Message>
          <Message {...PEOPLE.leo} author={PEOPLE.leo.name} time="11:31 AM" actions={false}>
            <MessageBody blocks={[para("The write-up: https://example.com/posts/field-notes", { words: "https://example.com/posts/field-notes", url: "https://example.com/posts/field-notes" })]} />
            <div class="lu-mediablock">
              <LinkCard url="https://example.com/posts/field-notes" host="example.com" title="Field notes from a week of building in the open" description="What went well, what did not, and the three things we would do again." color="#2f6fde" image={PICTURES.square} imageWidth={400} imageHeight={400} />
              <LinkCard url="https://video.example.org/watch" host="video.example.org" title="The launch, in four minutes" description="A short walk through what shipped." color="#e0484f" image={PICTURES.wide} imageWidth={400} imageHeight={225} video />
              <LinkCard url="https://docs.example.net/start" host="docs.example.net" title="Getting started" description="Install, sign in, and make your first server." />
            </div>
          </Message>
        </MessageList>
      </div>
      <h2>Embeds (specs/media.md 6.6): a link to a player or audio has Play on its picture, a post “Show post”; one being found, one Bluesky could not find; pressed, the site’s embed in the card’s place (blank here: nothing is loaded from a site in the gallery); with Images “As links”, a video’s row plays under it</h2>
      <div class="pieces__row pieces__row--top" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "760px" }}>
        <MessageList>
          <Message {...PEOPLE.leo} author={PEOPLE.leo.name} time="2:05 PM" actions={false}>
            <MessageBody blocks={[para("Watch this, then read the thread.")]} />
            <div class="lu-mediablock">
              <LinkCard url="https://www.youtube.com/watch?v=dQw4w9WgXcQ" host="youtube.com" title="The launch, in four minutes" description="A short walk through what shipped." color="#e0484f" image={PICTURES.wide} imageWidth={400} imageHeight={225} video embed={{ press: "play", label: "Play video from YouTube, The launch, in four minutes", onPress: () => {} }} />
              <LinkCard url="https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC" host="open.spotify.com" title="Field recordings, vol. 2" description="An album by Ridge." image={PICTURES.square} imageWidth={400} imageHeight={400} embed={{ press: "play", label: "Play audio from Spotify, Field recordings, vol. 2", onPress: () => {} }} />
              <LinkCard url="https://x.com/example/status/20" host="x.com" title="Ridge on X" description="Shipping the field notes today. Thread below." embed={{ press: "show", label: "Show post from X, Ridge on X", onPress: () => {} }} />
              <LinkCard url="https://bsky.app/profile/ridge.example/post/3k2la3rsg5w2y" host="bsky.app" title="Ridge (@ridge.example)" description="The write-up is up." embed={{ press: "show", label: "Show post from Bluesky, Ridge (@ridge.example)", busy: true, onPress: () => {} }} />
              <LinkCard url="https://bsky.app/profile/gone.example/post/3k2la3rsg5w2y" host="bsky.app" title="A post" description="Its account was not found." note="Bluesky couldn’t find @gone.example. The link opens it on Bluesky." />
              <EmbedFrame src="about:blank" title="The launch, in four minutes" kind="player" box={frameBox(GALLERY_PLAYER)} />
              <EmbedFrame src="about:blank" title="Ridge on X" kind="post" box={frameBox(GALLERY_POST, 320)} />
              <AudioAttachment kind="video" name="launch.mp4" size="8.4 MB" />
            </div>
          </Message>
        </MessageList>
      </div>
      <h2>Editing a message of one’s own: × on each attachment and card</h2>
      <div class="pieces__row" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "760px" }}>
        <MessageList>
          <Message author={PEOPLE.self.name} initials="RW" tone="indigo" time="12:10 PM" actions={false}>
            <MessageEdit value="Two from the ridge, and the write-up." onSave={() => {}} onCancel={() => {}} />
            <div class="lu-mediablock">
              <MediaGrid items={grid(2).map((t) => ({ ...t, onRemove: () => {} }))} />
              <Attachment kind="file" name="Q4-roadmap.pdf" size="1.2 MB" onRemove={() => {}} />
              <LinkCard url="https://example.com/posts/field-notes" host="example.com" title="Field notes from a week of building in the open" image={PICTURES.square} imageWidth={400} imageHeight={400} onRemove={() => {}} />
            </div>
          </Message>
        </MessageList>
      </div>
      <h2>Over the composer: the files being attached (preparing, uploading, ready with its alt text, refused in words) and the card of a link being written, each with its ×; and a conversation with files dragged over it</h2>
      <div class="pieces__row" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "760px" }}>
        <Composer
          placeholder="Message #design"
          tools={false}
          onAttach={() => {}}
          value="The write-up: https://example.com/posts/field-notes"
          above={
            <ComposerMedia
              drafts={[
                { id: "1", name: "clip.mov", kind: "video", size: "8.4 MB", state: "processing", onRemove: () => {} },
                { id: "2", name: "ridge.jpg", kind: "image", size: "2.1 MB", preview: PICTURES.tall, state: "uploading", onAlt: () => {}, onRemove: () => {} },
                { id: "3", name: "dusk.jpg", kind: "image", size: "640 KB", preview: PICTURES.dusk, state: "ready", alt: "The hills at dusk", onAlt: () => {}, onRemove: () => {} },
                { id: "4", name: "archive.zip", kind: "file", size: "48 MB", state: "failed", error: "Too large: files can be up to 10 MB here.", onRemove: () => {} },
              ]}
              cards={[{ url: "https://example.com/posts/field-notes", host: "example.com", title: "Field notes from a week of building in the open", description: "What went well, what did not, and the three things we would do again.", color: "#2f6fde", image: PICTURES.square, imageWidth: 400, imageHeight: 400, onRemove: () => {} }]}
            />
          }
        />
        <Composer placeholder="Message #design" tools={false} onAttach={() => {}} above={<ComposerMedia drafts={[{ id: "5", name: "ridge.jpg", kind: "image", size: "2.1 MB", preview: PICTURES.tall, state: "uploading", onAlt: () => {}, onRemove: () => {} }]} note="Sending when the files are uploaded…" />} />
        <DropZone label="Drop to attach to #design" />
      </div>
      <h2>The viewer: the picture over the window (here in a frame), zoomed by a press, the wheel or its bar up to four times its own size, Save, the alt text, Escape to close</h2>
      <div class="pieces__row" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "960px" }}>
        <Viewer inline name="ridge.jpg" kind="image" alt="A blue sky over green hills, the sun high on the right" src={PICTURES.wide} width={1600} height={900} saveHref={PICTURES.wide} onClose={() => {}} />
      </div>
    </section>
  );
}
