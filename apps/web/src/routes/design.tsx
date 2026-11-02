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
  Divider,
  Icon,
  IconButton,
  MemberList,
  Mention,
  Message,
  MessageList,
  QuickTiles,
  ReactionChip,
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
  const [board, setBoard] = createSignal<"split" | "signin" | "pieces">("split");
  return (
    <div class="gallery">
      <header class="gallery__bar">
        <strong>Lumia design system</strong>
        <nav class="gallery__nav" aria-label="Boards">
          <For each={[["split", "Quiet Split"], ["signin", "Sign in"], ["pieces", "Pieces"]] as const}>
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
        {board() === "split" ? <QuietSplit /> : board() === "signin" ? <SignIn /> : <Pieces />}
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
