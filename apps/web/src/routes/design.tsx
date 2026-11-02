// The design gallery: the Quiet Split board and the sign-in card from the
// design canvas, rebuilt from the ported components with the same sample
// data, plus the smaller pieces in isolation. Theme and layout toggles at
// the top. Not a product screen.
import { For, createSignal, onMount } from "solid-js";
import {
  AppShell,
  Avatar,
  AvatarStack,
  Badge,
  Button,
  Card,
  ChannelHeader,
  ChannelList,
  Checkbox,
  Composer,
  DeviceList,
  Divider,
  EmptyState,
  Icon,
  IconButton,
  MemberList,
  Mention,
  Message,
  MessageList,
  Notice,
  QuickTiles,
  ReactionChip,
  RequestCard,
  SearchTrigger,
  ServerHeader,
  ServerRail,
  SidePanel,
  SystemMessage,
  Tag,
  TextField,
  ThreadPanel,
  TypingIndicator,
  icons,
} from "../design/components";

type Theme = "light" | "dark";

function useTheme() {
  const [theme, setTheme] = createSignal<Theme>("light");
  onMount(() => {
    try {
      const saved = localStorage.getItem("lumia:theme");
      if (saved === "dark" || saved === "light") setTheme(saved);
    } catch {
      /* storage may be unavailable */
    }
  });
  const apply = (t: Theme) => {
    setTheme(t);
    document.documentElement.dataset.theme = t;
    try {
      localStorage.setItem("lumia:theme", t);
    } catch {
      /* ignore */
    }
  };
  onMount(() => apply(theme()));
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
          <ServerHeader name="Acme" />
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
      <ServerHeader name="Home" />
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

export function DesignGallery() {
  const { theme, apply } = useTheme();
  const [board, setBoard] = createSignal<"split" | "home" | "requests" | "devices" | "signin" | "pieces">("split");
  return (
    <div class="gallery">
      <header class="gallery__bar">
        <strong>Lumia design system</strong>
        <nav class="gallery__nav" aria-label="Boards">
          <For each={[["split", "Quiet Split"], ["home", "Home"], ["requests", "Requests"], ["devices", "Devices"], ["signin", "Sign in"], ["pieces", "Pieces"]] as const}>
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
        {board() === "split" ? <QuietSplit /> : board() === "home" ? <HomeBoard view="dm" /> : board() === "requests" ? <HomeBoard view="requests" /> : board() === "devices" ? <HomeBoard view="devices" /> : board() === "signin" ? <SignIn /> : <Pieces />}
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
        </div>
      </section>
    </div>
  );
}
