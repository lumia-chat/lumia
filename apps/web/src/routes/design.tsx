// The design gallery: the Quiet Split board and the sign-in card from the
// design canvas, rebuilt from the ported components with the same sample
// data, plus the smaller pieces in isolation. Theme and layout toggles at
// the top. Not a product screen.
import { For, Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { applyPreferences } from "../client/preferences";
import { QUICK_REACTIONS } from "../design/emoji";
import {
  AppShell,
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
  MessageList,
  Notice,
  NoticeLine,
  PickRow,
  Pop,
  ProfileCard,
  QuickTiles,
  ConfirmDelete,
  MessageEdit,
  MessageMenu,
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
  icons,
  type IconName,
  type Tone,
} from "../design/components";

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
                  { name: "leadership", kind: "private" },
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
                      { ...PEOPLE.maya, status: "online", tag: "Admin" },
                      { ...PEOPLE.devon, status: "away", subtitle: "In a meeting" },
                      { ...PEOPLE.sam, status: "online" },
                    ],
                  },
                  { title: "Offline · 96", members: [{ ...PEOPLE.priya }, { ...PEOPLE.leo }] },
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
        <Notice icon="shield">Messages here are end-to-end encrypted. Only your devices and Maya's can read them; chat.acme.dev delivers them without seeing them.</Notice>
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
          <Notice icon="shield">Messages with Maya are end-to-end encrypted. Only your devices and Maya's can read them.</Notice>
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

export function DesignGallery() {
  const { theme, apply } = useTheme();
  const [board, setBoard] = createSignal<"split" | "replies" | "home" | "requests" | "devices" | "signin" | "phone" | "pieces">("split");
  return (
    <div class="gallery">
      <header class="gallery__bar">
        <strong>Lumia design system</strong>
        <nav class="gallery__nav" aria-label="Boards">
          <For each={[["split", "Quiet Split"], ["replies", "Inline replies"], ["home", "Home"], ["requests", "Requests"], ["devices", "Devices"], ["signin", "Sign in"], ["phone", "Phone"], ["pieces", "Pieces"]] as const}>
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
        {board() === "split" ? <QuietSplit /> : board() === "replies" ? <RepliesBoard /> : board() === "home" ? <HomeBoard view="dm" /> : board() === "requests" ? <HomeBoard view="requests" /> : board() === "devices" ? <HomeBoard view="devices" /> : board() === "signin" ? <SignIn /> : board() === "phone" ? <PhoneBoard /> : <Pieces />}
      </div>
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
        <h2>The same block, compact (a preference of this browser)</h2>
        <div class="lu-msglist--compact">
          <Message author={PEOPLE.leo.name} initials="LM" tone="sky" time="10:08 AM" datetime="2026-11-02T10:08" actions={false} density="compact" onOpenProfile={() => {}} parts={[{ time: "10:09", datetime: "2026-11-02T10:09", children: "Also: who do I ask about a desk by the windows?" }]}>
            Thank you all, what a warm welcome! My calendar is open for coffee chats this week.
          </Message>
        </div>
      </section>
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
        <h2>Editing a message where it is drawn</h2>
        <div class="pieces__row" style={{ "flex-direction": "column", "align-items": "stretch", "max-width": "720px" }}>
          <MessageList>
            <Message author={PEOPLE.self.name} initials="RW" tone="indigo" time="9:30 AM" datetime="2026-11-13T09:30" actions={false} onOpenProfile={() => {}}>
              <MessageEdit value="Sure, after standup works. I’ll bring the second screen." onSave={() => {}} onCancel={() => {}} />
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
    </div>
  );
}
