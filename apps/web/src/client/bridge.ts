// The page's side of the client worker: typed async calls and a stream of
// gateway events. One bridge per instance; `connect` elects the owning
// tab with a Web Lock first (architecture 11.4), so two tabs never open
// the same OPFS store.

import { plain } from "./live";
import type { RoleDraft } from "./roles";

export interface InstanceSummary {
  host: string;
  operatorName: string;
  operatorUrl: string;
  termsUrl: string;
  privacyUrl: string;
  softwareVersion: string;
  identityProofSignInEnabled: boolean;
  identityProofCreateEnabled: boolean;
  openRegistration: boolean;
  webPushPublicKey: string;
  magicLinkEnabled: boolean;
  /** The OIDC provider's name for the sign-in button; empty without one. */
  oidcName: string;
}
export interface ServerSummary {
  serverId: string;
  name: string;
  /** The server's picture, a path of this instance (`avatarSrc` makes it a URL); empty for none. */
  iconUrl: string;
  /** How long the server keeps its messages, in days; 0 for ever. */
  retentionDays: number;
  /** Whether a deleted message keeps its place as "(deleted)" (architecture 6.7); off, the core's reads leave it out. */
  showDeleted: boolean;
  /** How long the server keeps its attachments, in days; 0 for ever (specs/media.md 4.5). */
  attachmentRetentionDays: number;
  /** A full server lets its oldest attachments go; false, it refuses uploads. */
  deleteOldestWhenFull: boolean;
  /** Whether this account may change the server's settings (architecture 6.7). */
  canManage: boolean;
  /** Whether it may create channels there (architecture 6.8). */
  canManageChannels: boolean;
  /** Whether it may start server DMs there. */
  canStartDms: boolean;
  /** Whether it may mention everyone there (`MENTION_EVERYONE`), and so is offered `@everyone` (architecture 6.6). */
  canMentionEveryone: boolean;
  /** Whether it may delete others' messages there (`MANAGE_MESSAGES`); its own it always may. */
  canManageMessages: boolean;
  /** Its own switches there: adds to channels wait as invitations; nobody may start a server DM with it. */
  channelAddsByInvitation: boolean;
  serverDmsOff: boolean;
  /** The server's owner, whom a profile card and the settings' member list mark (architecture 7.5). */
  ownerAccountId: string;
  /** Whether this account is that owner. */
  isOwner: boolean;
  /** Whether it may create roles and put them in order (`MANAGE_ROLES`); which roles it may change is each role's `canManage`. */
  canManageRoles: boolean;
  /** Whether it may remove members and set their titles (`MANAGE_MEMBERS`); which members is each member's `canManage`. */
  canManageMembers: boolean;
  /** Whether the tag beside a name there is the member's title; false, the default, their role. */
  tagTitles: boolean;
  /** The server's roles, highest first, Everyone last. */
  roles: RoleSummary[];
  /** The offered permissions this account may switch on for a role, by their wire names: the ones it holds itself. */
  grantable: string[];
  /** This account's own roles there, its own title (empty for none), and whether it is a guest. */
  roleIds: string[];
  title: string;
  guest: boolean;
  /** The server's live custom emoji, oldest first (specs/emoji.md 8). */
  emoji: EmojiSummary[];
  /** How many it may have, as the instance says (specs/emoji.md 5). */
  emojiLimit: number;
  /** Whether this account may upload, rename and remove them (`MANAGE_EMOJI`, or the owner). */
  canManageEmoji: boolean;
}
/** One of a server's custom emoji (specs/emoji.md 2), as the core's `EmojiSummary` serializes it. */
export interface EmojiSummary {
  emojiId: string;
  /** Written in messages as `:name:`. */
  name: string;
  /** The picture, a path of this instance as an avatar's is (`avatarSrc` makes it a URL). */
  imageUrl: string;
  /** Whether the picture is an animated GIF; otherwise it is a PNG. */
  animated: boolean;
  /** Its pixels, at most 128 each way: its shape. */
  width: number;
  height: number;
  createdByAccountId: string;
}
/** Where a message's body uses one of its server's custom emoji: UTF-8 byte offsets into the body, half-open, over its `:name:`. */
export interface EmojiRefSummary {
  emojiId: string;
  start: number;
  end: number;
}
/** A role of a server (architecture 7.5), as the snapshot's servers list theirs. */
export interface RoleSummary {
  roleId: string;
  /** "Everyone" for the role every member holds. */
  name: string;
  /** One of the nine tones, or empty for none. */
  color: string;
  /** Lower is higher; the instance's. The list is already in order. */
  position: number;
  isEveryone: boolean;
  /** Never a member's tag ("Show as tag" off). */
  hideTag: boolean;
  /** Every permission it grants, by its wire name (`MANAGE_ROLES`). */
  permissions: string[];
  /** Whether this account may change it, delete it, give it and take it away; of Everyone, change its permissions. */
  canManage: boolean;
}
/** The tag beside a member's name: a role in its colour, or a title in none (empty). */
export interface TagSummary {
  label: string;
  color: string;
}
/** A heading of a role's editor and the permissions under it; the core's list, the same on every client. */
export interface PermissionGroup {
  title: string;
  permissions: { name: string; label: string; detail: string }[];
}
/** A WebAuthn ceremony's start: the challenge's id and the options as the browser's JSON. */
export interface PasskeyChallengeSummary {
  challengeId: string;
  optionsJson: string;
}
/** One of the account's passkeys. */
export interface PasskeySummary {
  passkeyId: string;
  label: string;
  createdAtMicros: number | null;
  lastUsedAtMicros: number | null;
}
/** The signed profile this device's identity carries (architecture 6.4). */
export interface ProfileSummary {
  version: number;
  displayName: string;
  pronouns: string;
  bio: string;
  avatarUrl: string;
}
/** One account's presence in a server (`ONLINE`, `AWAY`, `DO_NOT_DISTURB`). */
export interface PresenceSummary {
  accountId: string;
  status: string;
  customStatus: string;
}
/** What an invite shows before joining (`GET /invites/{code}`). */
export interface InvitePreviewSummary {
  code: string;
  serverId: string;
  serverName: string;
  channelName: string;
  memberCount: number;
  asGuest: boolean;
  host: string;
  operatorName: string;
  openRegistration: boolean;
  identityProofCreateEnabled: boolean;
}
/** An invite this account minted: the code and the link that carries it. */
export interface InviteSummary {
  code: string;
  url: string;
  serverId: string;
  expiresAtMicros: number | null;
  maxUses: number;
  uses: number;
  /** The address an email invite went to; empty for a link. */
  email: string;
}
export interface ChannelSummary {
  channelId: string;
  serverId: string;
  name: string;
  kind: string;
  lastSeq: number;
  participantAccountIds: string[];
  host: string;
  /** TEXT: entered only by being added (architecture 6.8). */
  private: boolean;
  /** TEXT: joined by every newcomer to the server. */
  isDefault: boolean;
  /** How many are in it, where a listing says (Browse); 0 elsewhere. */
  memberCount: number;
  topic: string;
}
/** An invitation into a channel, waiting for this account's yes (architecture 6.8). */
export interface ChannelInvitationSummary {
  channel: ChannelSummary;
  invitedByAccountId: string;
  createdAtMicros: number;
}
/** Who an add brought in at once, and who it invited instead. */
export interface ChannelAddSummary {
  addedAccountIds: string[];
  invitedAccountIds: string[];
}
export interface DmRequestSummary {
  requestId: string;
  fromAccountId: string;
  toAccountId: string;
  state: string;
  sharedServerIds: string[];
  mutualFriendCount: number;
}
export interface SnapshotSummary {
  accountId: string;
  handle: string;
  displayName: string;
  /** The account's picture, as `MemberSummary.avatarUrl`; empty for none. */
  avatarUrl: string;
  servers: ServerSummary[];
  channels: ChannelSummary[];
  pendingRequests: DmRequestSummary[];
  /** Channels this account was asked into and has not answered. */
  channelInvitations: ChannelInvitationSummary[];
  /** Where this account has read each conversation, as the instance counted it; `readStates` keeps it current. */
  readStates: ReadStateSummary[];
  /** The rail as this account arranged it, ready to draw (architecture 6.9): every server of `servers` placed once, which are in its order. */
  rail: RailItem[];
}
/** One tile of the rail between Home and +: a server, or a folder of servers (architecture 6.9). */
export type RailItem =
  | { kind: "server"; serverId: string }
  | {
      kind: "folder";
      folderId: string;
      /** Empty for none: it reads as its servers (`railFolderWords`). */
      name: string;
      /** One of the nine avatar tones. */
      tone: string;
      serverIds: string[];
    };
/** One rail edit (`railEdit`) and what it takes. */
export type RailEdit =
  | { op: "moveTo"; args: { id: string; folder?: string; before?: string } }
  | { op: "merge"; args: { dragged: string; onto: string; folderId: string } }
  | { op: "intoFolder"; args: { serverId: string; folder: string } }
  | { op: "outOfFolder"; args: { serverId: string } }
  | { op: "newFolder"; args: { serverId: string; folderId: string } }
  | { op: "deleteFolder"; args: { folder: string } }
  | { op: "setFolder"; args: { folder: string; name: string; tone: string } }
  | { op: "step"; args: { id: string; by: -1 | 1 } };
/** A quote of another message of the conversation (architecture 6.6). */
export interface QuoteSummary {
  messageId: string;
  /** Its author; in a personal conversation, empty where unknown. */
  authorAccountId: string;
  /** What of it was quoted; empty once it is deleted. */
  text: string;
  originalDeleted: boolean;
  /** The reply notifies nobody: sent with the reply bar's "@ On" switch off. */
  silent: boolean;
}
/** What was typed into a conversation's composer and not sent, as this browser keeps it. */
export interface DraftSummary {
  body: string;
  /** The message the words were being written in reply to, if any. */
  reply: QuoteSummary | null;
}
/** A mention: an account, or everyone, and its UTF-8 byte range in the body (half-open). */
export interface MentionSummary {
  /** Empty when it is everyone. */
  accountId: string;
  everyone: boolean;
  start: number;
  end: number;
}
/**
 * A message body read by the core (`client.parseBody`, protocol 14): its
 * blocks, as `lumia_core::body::DisplayBlock` serializes them. Ranges are
 * UTF-16 code units of the paragraph's `text`, as JavaScript counts.
 */
export type BodyBlock =
  | { kind: "para"; text: string; runs: BodyRun[]; links: BodyLink[] }
  /** A fenced code block: the first word after its fence, if any, and its lines. */
  | { kind: "code"; lang?: string | null; text: string }
  | { kind: "quote"; blocks: BodyBlock[] }
  /** Ordered from `start` when it has one; each item a run of blocks. */
  | { kind: "list"; start?: number | null; items: BodyBlock[][] };
/** A styled stretch of a paragraph. Runs may overlap (a mention in bold). */
export interface BodyRun {
  start: number;
  end: number;
  /** `mention`: `index` is its place in the message's `mentions`, and its text is the mention as typed; `emoji`: its place in the message's `emoji`, its text the `:name:` as typed (specs/emoji.md 6). */
  style: { kind: "emphasis" | "strong" | "strike" | "code" } | { kind: "mention"; index: number } | { kind: "emoji"; index: number };
}
/** A web link: the stretch of text it is, and where it goes (only ever http or https; the core keeps no other). */
export interface BodyLink {
  start: number;
  end: number;
  url: string;
}
/** How much room a draft has under the instance's limit (`client.messageRoom`): shown from 90% of it; `left` negative is how many characters too long. */
export interface MessageRoom {
  shown: boolean;
  left: number;
}
/** One emoji's reactions to a message, in the order each was first used. */
export interface ReactionSummary {
  reaction: string;
  count: number;
  /** Whether this account is among them. */
  me: boolean;
  /** Who reacted, as many as are known. */
  accountIds: string[];
  /** The server's custom emoji it is, whose `:name:` `reaction` then is (specs/emoji.md 9); empty for a Unicode emoji. */
  emojiId: string;
}
/** A conversation's read state: the last event read, how many messages others sent after it, and how many of those notify (in a DM, every one). */
export interface ReadStateSummary {
  channelId: string;
  readSeq: number;
  unread: number;
  mentions: number;
}
/** A page of the messages that notify this account, newest first. */
export interface MentionsPage {
  messages: MessageSummary[];
  hasMore: boolean;
}
export interface StatusSummary {
  status: string;
  customStatus: string;
}
export interface MemberSummary {
  accountId: string;
  handle: string;
  displayName: string;
  nickname: string;
  /** From the account's verified profile, as the display name is. */
  pronouns: string;
  bio: string;
  /** The account's picture: a path on this instance (`/api/v1/avatars/…`); empty for none. */
  avatarUrl: string;
  /** Their switches in the server listed (architecture 6.8). */
  channelAddsByInvitation: boolean;
  serverDmsOff: boolean;
  /** Their roles in the server listed, by id, and their title there, empty for none (architecture 7.5); empty for a public profile. */
  roleIds: string[];
  title: string;
  /** Whether they are a guest there, and whether they own it. */
  guest: boolean;
  owner: boolean;
  /** The tag beside their name there, as the server shows it; read again when `tagsChanged` says to. */
  tag: TagSummary | null;
  /** Whether this account may set their title and remove them from the server. */
  canManage: boolean;
}
export interface DmChannelSummary {
  channelId: string;
  participantAccountIds: string[];
  member: boolean;
}
export interface DmMessageSummary {
  messageId: string;
  seq: number;
  senderIdentityId: string;
  senderDeviceId: string;
  senderAccountId: string;
  body: string;
  sentAtMicros: number;
  edited: boolean;
  deleted: boolean;
  mine: boolean;
  reply: QuoteSummary | null;
  reactions: ReactionSummary[];
  /** What it carries (specs/media.md). */
  attachments: AttachmentSummary[];
  /** Its link cards: only those under a link its text holds. */
  cards: CardSummary[];
  /** The body read into blocks, which the worker adds to every message it hands the page. */
  blocks: BodyBlock[];
}
export interface DmLogSummary {
  messages: DmMessageSummary[];
  beforeJoin: number;
  undecryptable: number;
}
export interface DmOpened {
  channelId: string;
  existed: boolean;
  member: boolean;
  creating: boolean;
}
export interface MessageSummary {
  messageId: string;
  channelId: string;
  seq: number;
  authorAccountId: string;
  body: string;
  createdAtMicros: number;
  deleted: boolean;
  /** A notice (architecture 6.8): `joined`, `added`, `left` or `removed`, done by the author, about `noticeAccountId`; empty for a message. */
  notice: string;
  noticeAccountId: string;
  reply: QuoteSummary | null;
  mentions: MentionSummary[];
  reactions: ReactionSummary[];
  /** It mentions this account or everyone, or quotes one of its messages. */
  mentionsMe: boolean;
  /** Its author has edited it ("(edited)"). */
  edited: boolean;
  /** What it carries (specs/media.md); empty once deleted. */
  attachments: AttachmentSummary[];
  /** Its link cards, in the order their links appear. */
  cards: CardSummary[];
  /** The server's custom emoji its body uses, by byte range (specs/emoji.md 6); none in a personal conversation. */
  emoji?: EmojiRefSummary[];
  /** The body read into blocks, which the worker adds to every message it hands the page. */
  blocks: BodyBlock[];
}
/** One blob of an attachment or a DM card, with what opens it: handed back to `attachmentBytes` as given, never decrypted by the page. */
export interface BlobSummary {
  blobId: string;
  /** The stored ciphertext's length. */
  sizeBytes: number;
  /** Base64. */
  key: string;
  nonce: string;
  hash: string;
}
/** An attachment as a message carries it (specs/media.md 5). */
export interface AttachmentSummary {
  attachmentId: string;
  kind: "image" | "gif" | "video" | "audio" | "file";
  name: string;
  mediaType: string;
  /** After orientation; 0 when unknown. */
  width: number;
  height: number;
  /** Video and audio; 0 when unknown. */
  durationMs: number;
  /** ThumbHash in base64, drawn until the thumbnail arrives (`placeholderImage`); empty for none. */
  placeholder: string;
  alt: string;
  /** The file's own size, for a file row. */
  sizeBytes: number;
  main: BlobSummary;
  /** At most 400 px: an image's, a GIF's first frame, a video's poster. */
  thumbnail: BlobSummary | null;
  /** The server let it go (specs/media.md 4.5): drawn as its name and "No longer available", opening nothing. */
  expired: boolean;
}
/** What a server's files take (specs/media.md 4.5). */
export interface StorageSummary {
  usedBytes: number;
  /** 0 when the instance sets no limit. */
  maxBytes: number;
}
/** A link's card (specs/media.md 6): drawn under the link, named by `host` (never `siteName`), opening `url`. */
export interface CardSummary {
  url: string;
  title: string;
  description: string;
  siteName: string;
  /** The site to show: the link's host. */
  host: string;
  /** `#rrggbb`, or empty. */
  color: string;
  /** Server content: the picture, a path of this instance (`cardImage`, or load it). */
  imageUrl: string;
  /** Personal DMs: the picture as a blob (`attachmentBytes`). */
  image: BlobSummary | null;
  imageWidth: number;
  imageHeight: number;
  /** The page is a video: a play symbol over its picture. */
  video: boolean;
}
/**
 * A link's embed (specs/media.md 6.6): what pressing its card loads in the card's place. It comes
 * from the link in the text (`embedFor`), never from the card.
 */
export interface EmbedSummary {
  provider: "youtube" | "vimeo" | "twitch" | "soundcloud" | "spotify" | "x" | "bluesky" | "facebook" | "instagram" | "tiktok" | "reddit";
  /** The site's name as a person reads it: "YouTube", "X". */
  providerName: string;
  /** A player goes back to its card when its row scrolls away or the conversation changes; audio plays on until another starts or the conversation is left. */
  kind: "player" | "audio" | "post";
  /** The address to load; empty for a Bluesky post named by handle. */
  url: string;
  /** A Bluesky post named by handle: `resolveBlueskyHandle` on the press, then `blueskyEmbedUrl(did, blueskyRkey)`. Empty otherwise. */
  blueskyHandle: string;
  blueskyRkey: string;
  /** Twitch: append `&parent=<location.hostname>` to `url`. */
  needsParent: boolean;
  /** A player's shape, at the column's width at most and at most 400 tall; both 0 when it has none. */
  aspectWidth: number;
  aspectHeight: number;
  /** The height when it has no shape: fixed (the rest scrolls inside), or held until the embed reports its own when `resizes`. */
  fixedHeight: number;
  /** Grows to the height the embed reports by `postMessage`. */
  resizes: boolean;
  /** The widest to draw it: 550 for a post, 0 for the column's width. */
  maxWidth: number;
  /** The press's accessible name before the card's title: "Play video from YouTube", "Show post from X". */
  label: string;
}
/** An attachment uploaded and waiting in the composer; a send names it by `draftId`. */
export interface AttachmentDraft {
  draftId: string;
  channelId: string;
  kind: AttachmentSummary["kind"];
  name: string;
  mediaType: string;
  width: number;
  height: number;
  durationMs: number;
  placeholder: string;
  alt: string;
  sizeBytes: number;
}
/** Decoded pixels, RGBA8 (an `ImageData`'s). */
export interface Pixels {
  rgba: Uint8Array | Uint8ClampedArray;
  width: number;
  height: number;
}
/** What the page knows of a file to attach. */
export interface AttachOptions {
  name: string;
  /** Sent as a file: a picture keeps its pixels, loses its metadata. */
  asFile?: boolean;
  alt?: string;
  /** A video's or audio's length, from a `<video>` or `<audio>` element. */
  durationMs?: number;
  videoWidth?: number;
  videoHeight?: number;
  /** A picture the browser decoded that the core cannot (HEIC in Safari). */
  pixels?: Pixels;
  /** A video's poster frame, drawn from a `<video>` to a canvas. */
  poster?: Pixels;
}
export interface UploadLimits {
  imageMaxBytes: number;
  fileMaxBytes: number;
  thumbnailMaxBytes: number;
}
/** One applied channel event: its kind (`message_created`, `message_edited`, `message_cards_set`, `message_deleted`, `reaction_added`, `reaction_removed`, `message_pinned`, `message_unpinned`, `thread_updated`, `attachments_expired`, `payloads_purged`, `redacted`, `member_joined`, `member_left`, `channel_updated`, `acl_changed`, `encrypted`, `empty`). */
export interface EventSummary {
  seq: number;
  kind: string;
  message?: MessageSummary;
  /** The message it creates or changes (a thread's root for `thread_updated`); empty for none. */
  messageId: string;
  /** For `attachments_expired` and `payloads_purged`, said of every message below this seq; 0 otherwise. */
  beforeSeq: number;
}
export interface DeviceSummary {
  deviceId: string;
  kind: string;
  label: string;
  webOrigin: string;
  addedAtMicros: number;
  thisDevice: boolean;
  revoked: boolean;
}
export interface RosterSummary {
  identityId: string;
  version: number;
  devices: DeviceSummary[];
}

/** Where linking this browser by a prompt stands (identity spec 4.2.1; the core's `LinkProgress`). */
export type LinkStage = "waiting" | "compare" | "linked" | "declined" | "expired";
export interface LinkProgress {
  stage: LinkStage;
  /** The six digits to compare, as `482 913`, at `compare`; empty otherwise. */
  digits: string;
}

export type ClientEvent =
  | { kind: "welcomed"; accountId: string }
  | { kind: "applied"; channelId: string; events: EventSummary[] }
  | { kind: "accountApplied"; count: number }
  /** Someone's name or picture changed, this account's own included (issue #15): the account as others see it, after the `accountApplied` it came in. */
  | { kind: "profileUpdated"; accountId: string; handle: string; displayName: string; avatarUrl: string; pronouns: string; bio: string }
  /** A link session moved (identity spec 4.2.1), after the `accountApplied` it came in: this browser's own as the new device. */
  | { kind: "linkSessionUpdated"; sessionId: string }
  /** What a server's tags are drawn from changed (its roles, some members' roles or titles, or which of the two it shows), after the `accountApplied` it came in: what the core keeps has it already. */
  | { kind: "tagsChanged"; serverId: string }
  /** A server's custom emoji changed (specs/emoji.md 8), after the `accountApplied` it came in: what the core keeps has them already (`serverEmoji`). */
  | { kind: "emojiChanged"; serverId: string }
  | { kind: "acked"; commandId: string; seq: number }
  | { kind: "error"; code: string; message: string; commandId: string }
  | { kind: "subscriptionFailed"; channelId: string; message: string }
  | { kind: "presenceDiff"; serverId: string; joined: PresenceSummary[]; updated: PresenceSummary[]; leftAccountIds: string[] }
  /** Someone else is typing in a server's channel, until `untilMicros` on this device's clock. */
  | { kind: "typing"; channelId: string; accountId: string; untilMicros: number }
  | { kind: "snapshotTaken"; channelId: string; seq: number }
  | { kind: "disconnected"; reason: string; reconnectAfterSeconds: number }
  | { kind: "reconnecting"; attempt: number; delayMillis: number; reason: string }
  | { kind: "closed"; reason: string; removed: boolean }
  | { kind: "dmNote"; message: string }
  | { kind: "dmError"; message: string }
  | { kind: "dmSubscribed"; channelId: string }
  | { kind: "dmOperation"; channelId: string; seq: number; senderIdentityId: string; senderDeviceId: string }
  | { kind: "dmReady"; channelId: string }
  | { kind: "dmDelivered"; operationId: string };

export interface Opened {
  host: string;
  deviceId: string;
  signedIn: boolean;
}

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void };

/** The API error envelope as the worker reports it: `code: message`. */
export class ClientError extends Error {
  readonly code: string;
  /** `Error.details` as the protocol documents them per code (`mfa`, `state`, ...). */
  readonly details: Record<string, string>;
  constructor(text: string) {
    super(text);
    const m = /^api error ([A-Z_]+): /.exec(text);
    this.code = m?.[1] ?? "";
    this.details = {};
    const d = / details=(.*)$/.exec(text);
    for (const pair of d?.[1]?.split(",") ?? []) {
      const eq = pair.indexOf("=");
      if (eq > 0) this.details[pair.slice(0, eq)] = pair.slice(eq + 1);
    }
  }
}

/**
 * Whether a failure says the instance no longer takes this session
 * (protocol 10 and 12): it does not know the token, the refresh token ran
 * out with it, or the session or its device was revoked. Nothing but
 * signing in again follows any of them.
 */
export function signedOut(e: unknown): boolean {
  if (!(e instanceof Error)) return false;
  if (e instanceof ClientError && ["UNAUTHENTICATED", "TOKEN_EXPIRED", "SESSION_REVOKED", "DEVICE_REVOKED"].includes(e.code)) return true;
  return /\bnot signed in\b/i.test(e.message);
}

/**
 * Whether an identity operation wants a fresh reauth: none was in hand
 * (the core's `ReauthNeeded`), or the one in hand was spent or ran out
 * (the instance's `UNAUTHENTICATED` for the token). Asked only of the
 * calls that take one.
 */
export function reauthNeeded(e: unknown): boolean {
  if (!(e instanceof Error)) return false;
  if (e instanceof ClientError && e.code === "UNAUTHENTICATED") return true;
  return e.message === "confirm it is you first";
}

/** Whether setting up found the account bound to an identity this browser does not hold (the core's `AccountHasIdentity`). */
export function accountHasIdentity(e: unknown): boolean {
  return e instanceof Error && e.message === "this account already has private messages set up on another device";
}

export class Bridge {
  private worker: Worker;
  private next = 1;
  private pending = new Map<number, Pending>();
  private listeners = new Set<(e: ClientEvent) => void>();

  constructor() {
    this.worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    this.worker.onmessage = (e: MessageEvent) => {
      const data = e.data as { id?: number; ok?: boolean; result?: unknown; error?: string; event?: ClientEvent };
      if (data.event) {
        for (const l of this.listeners) l(data.event);
        return;
      }
      if (data.id === undefined) return;
      const p = this.pending.get(data.id);
      if (!p) return;
      this.pending.delete(data.id);
      if (data.ok) p.resolve(data.result);
      else p.reject(new ClientError(data.error ?? "unknown error"));
    };
  }

  private call<T>(method: string, ...args: unknown[]): Promise<T> {
    const id = this.next++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      try {
        // What a query holds is a store, and a store's proxies cannot be posted: a member with
        // a tag or roles is objects within objects, which a store hands out wrapped (plan step 11).
        this.worker.postMessage({ id, method, args: args.map(plain) });
      } catch (e) {
        this.pending.delete(id);
        throw e;
      }
    });
  }

  onEvent(listener: (e: ClientEvent) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  open(baseUrl: string): Promise<Opened> {
    return this.call("open", baseUrl);
  }
  instanceInfo(): Promise<InstanceSummary> {
    return this.call("instanceInfo");
  }
  /** With an invite link or code, the account joins its server too. */
  createAccount(handle: string, password: string, invite?: string): Promise<void> {
    return this.call("createAccount", handle, password, invite ?? null);
  }
  invitePreview(linkOrCode: string): Promise<InvitePreviewSummary> {
    return this.call("invitePreview", linkOrCode);
  }
  createInvite(serverId: string, maxUses = 0, expiresInHours?: number): Promise<InviteSummary> {
    return this.call("createInvite", serverId, maxUses, expiresInHours ?? null);
  }
  /** A one-use invite good for a week, mailed to the address as a link (architecture 6.2). */
  emailInvite(serverId: string, email: string): Promise<InviteSummary> {
    return this.call("emailInvite", serverId, email);
  }
  /** Joins the invite's server; resolves to its id. */
  acceptInvite(linkOrCode: string): Promise<string> {
    return this.call("acceptInvite", linkOrCode);
  }
  /** This client's status and the servers it has on screen (protocol 6.7). */
  watchPresence(watchServerIds: string[]): Promise<void> {
    return this.call("watchPresence", watchServerIds);
  }
  /** The status this person chose and their custom status text, kept across launches. */
  /** Says this person is typing in a server's channel (protocol 6.7); call it on every edit, the core spaces them out. */
  typing(channelId: string): Promise<void> {
    return this.call("typing", channelId);
  }
  status(): Promise<StatusSummary> {
    return this.call("status");
  }
  /** Chooses the status (ONLINE, AWAY, DO_NOT_DISTURB) and custom text; sent at once when connected, and on every welcome. */
  setStatus(status: string, customStatus: string): Promise<void> {
    return this.call("setStatus", status, customStatus);
  }
  invites(serverId: string): Promise<InviteSummary[]> {
    return this.call("invites", serverId);
  }
  revokeInvite(linkOrCode: string): Promise<void> {
    return this.call("revokeInvite", linkOrCode);
  }
  createServer(name: string): Promise<ServerSummary> {
    return this.call("createServer", name);
  }
  createChannel(serverId: string, name: string, isPrivate = false): Promise<ChannelSummary> {
    return this.call("createChannel", serverId, name, isPrivate);
  }
  /** The public channels of a server this account can see and is not in, with their counts (architecture 6.8). */
  browseChannels(serverId: string): Promise<ChannelSummary[]> {
    return this.call("browseChannels", serverId);
  }
  joinChannel(channelId: string): Promise<SnapshotSummary> {
    return this.call("joinChannel", channelId);
  }
  leaveChannel(channelId: string): Promise<SnapshotSummary> {
    return this.call("leaveChannel", channelId);
  }
  /** Who is in a channel, as account ids. */
  channelMembers(channelId: string): Promise<string[]> {
    return this.call("channelMembers", channelId);
  }
  addChannelMembers(channelId: string, accountIds: string[]): Promise<ChannelAddSummary> {
    return this.call("addChannelMembers", channelId, accountIds);
  }
  removeChannelMember(channelId: string, accountId: string): Promise<void> {
    return this.call("removeChannelMember", channelId, accountId);
  }
  dismissChannelInvitation(channelId: string): Promise<SnapshotSummary> {
    return this.call("dismissChannelInvitation", channelId);
  }
  /** This account's switches in a server; one left out stays. */
  updateMembership(serverId: string, channelAddsByInvitation?: boolean, serverDmsOff?: boolean): Promise<SnapshotSummary> {
    return this.call("updateMembership", serverId, channelAddsByInvitation, serverDmsOff);
  }
  /** The 1:1 server DM with a member, started if need be (architecture 6.4). */
  openServerDm(serverId: string, accountId: string): Promise<ChannelSummary> {
    return this.call("openServerDm", serverId, accountId);
  }
  setDefaultChannels(serverId: string, channelIds: string[]): Promise<SnapshotSummary> {
    return this.call("setDefaultChannels", serverId, channelIds);
  }
  passkeyRegisterStart(): Promise<PasskeyChallengeSummary> {
    return this.call("passkeyRegisterStart");
  }
  passkeyRegisterFinish(challengeId: string, credentialJson: string, label: string): Promise<void> {
    return this.call("passkeyRegisterFinish", challengeId, credentialJson, label);
  }
  passkeyAssertStart(handleOrEmail: string): Promise<PasskeyChallengeSummary> {
    return this.call("passkeyAssertStart", handleOrEmail);
  }
  signInPasskey(challengeId: string, credentialJson: string): Promise<void> {
    return this.call("signInPasskey", challengeId, credentialJson);
  }
  /** A password sign-in with the passkey factor: the assertion made for an assert challenge. */
  signInWithPasskey(handle: string, password: string, challengeId: string, credentialJson: string): Promise<void> {
    return this.call("signInWithPasskey", handle, password, challengeId, credentialJson);
  }
  /** Whether a password sign-in must also present one of this account's passkeys. */
  passkeyRequired(): Promise<boolean> {
    return this.call("passkeyRequired");
  }
  setPasskeyRequired(required: boolean): Promise<void> {
    return this.call("setPasskeyRequired", required);
  }
  passkeys(): Promise<PasskeySummary[]> {
    return this.call("passkeys");
  }
  removePasskey(passkeyId: string): Promise<void> {
    return this.call("removePasskey", passkeyId);
  }
  /** A one-time sign-in token for this account, for a native app that signed in through this page to take back. */
  handoffToken(): Promise<string> {
    return this.call("handoffToken");
  }
  /** The signed profile this browser's identity holds, if any. */
  identityProfile(): Promise<ProfileSummary | null> {
    return this.call("identityProfile");
  }
  /** Signs a new profile with this browser's device key and, signed in, sends it as the account's. */
  identitySetProfile(displayName: string, pronouns: string, bio: string, avatarUrl: string): Promise<ProfileSummary> {
    return this.call("identitySetProfile", displayName, pronouns, bio, avatarUrl);
  }
  /** Asks the instance to mail a sign-in link to `email`. */
  /**
   * `PATCH /account`: this account's handle and display name on the
   * instance. A value left out stays as it is and an empty display name
   * clears it; resolves to the snapshot as it then is. `ALREADY_EXISTS`
   * for a handle that is taken, `INVALID_ARGUMENT` for one that is not a
   * handle or a name that is too long.
   */
  updateAccount(handle?: string, displayName?: string): Promise<SnapshotSummary> {
    return this.call("updateAccount", handle, displayName);
  }
  /**
   * `PUT /account/avatar`: this account's picture, from a file's bytes and
   * its media type; the instance makes its own square of it. Resolves to
   * the snapshot as it then is. `INVALID_ARGUMENT` with `details.reason`
   * (`unsupported`, `too_large`, `invalid`), `FAILED_PRECONDITION` on an
   * instance that keeps no pictures, `RATE_LIMITED`.
   */
  setAvatar(picture: Uint8Array, mediaType: string): Promise<SnapshotSummary> {
    return this.call("setAvatar", picture, mediaType);
  }
  /** `DELETE /account/avatar`; resolves to the snapshot as it then is. */
  removeAvatar(): Promise<SnapshotSummary> {
    return this.call("removeAvatar");
  }
  /**
   * `PATCH /servers/{id}`: a server's name, how long it keeps messages in
   * days (0 for ever), or both; one left out stays. Resolves to the snapshot
   * as it then is. `PERMISSION_DENIED` without `MANAGE_SERVER`,
   * `INVALID_ARGUMENT` with `details.field` `name`.
   */
  updateServer(serverId: string, name?: string, retentionDays?: number): Promise<SnapshotSummary> {
    return this.call("updateServer", serverId, name, retentionDays);
  }
  /** How long a server keeps its attachments in days (0 for ever), and whether a full one lets its oldest go; one left out stays (specs/media.md 4.5). */
  setServerFiles(serverId: string, attachmentRetentionDays?: number, deleteOldestWhenFull?: boolean): Promise<SnapshotSummary> {
    return this.call("setServerFiles", serverId, attachmentRetentionDays, deleteOldestWhenFull);
  }
  /** What a server's files take, for its settings (`MANAGE_SERVER`). */
  serverStorage(serverId: string): Promise<StorageSummary> {
    return this.call("serverStorage", serverId);
  }
  /** Whether a server's deleted messages keep their place as "(deleted)" (architecture 6.7). */
  setShowDeleted(serverId: string, show: boolean): Promise<SnapshotSummary> {
    return this.call("setShowDeleted", serverId, show);
  }
  /** The permissions a role's editor offers, under their headings (architecture 7.5): the core's list. */
  rolePermissions(): Promise<PermissionGroup[]> {
    return this.call("rolePermissions");
  }
  /**
   * A new role, placed last above Everyone: `color` one of the nine tones or
   * empty, `permissions` the offered ones switched on, by their wire names.
   * `PERMISSION_DENIED`; `INVALID_ARGUMENT` with `details.field` `name` or
   * `color`; `ALREADY_EXISTS` for a name another role has.
   */
  createRole(serverId: string, name: string, color: string, hideTag: boolean, permissions: string[]): Promise<SnapshotSummary> {
    return this.call("createRole", serverId, name, color, hideTag, permissions);
  }
  /**
   * What an editor changed of a role, and nothing else of it: `was` is the
   * role as the editor opened on it and `now` what it holds at Save. The
   * core sends the difference (the name when it differs once trimmed, the
   * colour, the tag switch, the offered permissions switched on and off),
   * so what the editor did not touch stays as the instance has it,
   * whatever someone else changed meanwhile; an editor saved as it opened
   * asks nothing. Of Everyone only the permissions change. Refused as
   * `createRole` is, and `NOT_FOUND` for a role the server no longer has.
   */
  changeRole(serverId: string, roleId: string, was: RoleDraft, now: RoleDraft): Promise<SnapshotSummary> {
    return this.call("changeRole", serverId, roleId, was, now);
  }
  deleteRole(serverId: string, roleId: string): Promise<SnapshotSummary> {
    return this.call("deleteRole", serverId, roleId);
  }
  /** The server's roles in this order: every role but Everyone once, highest first. `INVALID_ARGUMENT` with `details.field` `role_ids` for an order that is not the server's roles. */
  reorderRoles(serverId: string, roleIds: string[]): Promise<SnapshotSummary> {
    return this.call("reorderRoles", serverId, roleIds);
  }
  /**
   * What was changed of a member, in one request the instance does whole
   * or not at all: the roles to give them and to take away, by id, their
   * other roles left as the instance has them, and their title when it is
   * a string (empty clears it; null leaves it). This account's own title
   * is its own to set this way. The member as the server now shows them.
   * `PERMISSION_DENIED`; `INVALID_ARGUMENT` with `details.field` `roles`
   * or `title`; `RATE_LIMITED` for this account's own title changed too
   * often; `NOT_FOUND` for someone who is not a member.
   */
  changeMember(serverId: string, accountId: string, give: string[], take: string[], title: string | null): Promise<MemberSummary> {
    return this.call("changeMember", serverId, accountId, give, take, title);
  }
  /** Removes a member from a server (`MANAGE_MEMBERS`, and a member below this account). */
  removeMember(serverId: string, accountId: string): Promise<void> {
    return this.call("removeMember", serverId, accountId);
  }
  /** Whether the tag beside a name in a server is the member's title; false, their role (`MANAGE_SERVER`). */
  setTagTitles(serverId: string, titles: boolean): Promise<SnapshotSummary> {
    return this.call("setTagTitles", serverId, titles);
  }
  /**
   * `PUT /account/rail`: keeps `items` as this account's rail, whole
   * (architecture 6.9); its other devices hear of it through the account
   * log. Resolves to the snapshot, its rail as the instance kept it.
   * `INVALID_ARGUMENT` with `details.field` (`name`, `tone`, `folder_id`, `folders`).
   */
  arrangeRail(items: RailItem[]): Promise<SnapshotSummary> {
    return this.call("arrangeRail", items);
  }
  /** One rail edit by the core's rules, answering the rail as it is to be (nothing is sent; `arrangeRail` keeps it). */
  railEdit(items: RailItem[], edit: RailEdit): Promise<RailItem[]> {
    return this.call("railEdit", items, edit.op, edit.args);
  }
  /** A new folder's id (UUIDv7). */
  railNewFolderId(): Promise<string> {
    return this.call("railNewFolderId");
  }
  /** The tone a folder made from this server gets. */
  railToneOf(id: string): Promise<string> {
    return this.call("railToneOf", id);
  }
  /** How a folder with no name reads: "Acme, Lumia and 2 more". */
  railFolderWords(names: string[]): Promise<string> {
    return this.call("railFolderWords", names);
  }
  /** `PUT /servers/{id}/icon`: as `setAvatar`, for a server's picture. */
  setServerIcon(serverId: string, picture: Uint8Array, mediaType: string): Promise<SnapshotSummary> {
    return this.call("setServerIcon", serverId, picture, mediaType);
  }
  /** `DELETE /servers/{id}/icon`. */
  removeServerIcon(serverId: string): Promise<SnapshotSummary> {
    return this.call("removeServerIcon", serverId);
  }
  requestMagicLink(email: string): Promise<void> {
    return this.call("requestMagicLink", email);
  }
  /** Signs in with the token from a magic-link mail (the code, or the whole link). */
  signInMagicLink(token: string): Promise<void> {
    return this.call("signInMagicLink", token);
  }
  signIn(handle: string, password: string, totpCode?: string): Promise<void> {
    return this.call("signIn", handle, password, totpCode ?? null);
  }
  signOut(): Promise<void> {
    return this.call("signOut");
  }
  signedIn(): Promise<boolean> {
    return this.call("signedIn");
  }
  snapshot(): Promise<SnapshotSummary> {
    return this.call("snapshot");
  }
  messages(channelId: string, limit = 100): Promise<MessageSummary[]> {
    return this.call("messages", channelId, limit);
  }
  /** What was typed into a conversation's composer and not sent, from the store in this browser; null when nothing is kept. */
  draft(channelId: string): Promise<DraftSummary | null> {
    return this.call("draft", channelId);
  }
  /** Keeps what is typed in a conversation's composer, with the message it is a reply to, in place of what was kept; blank words keep nothing. In this browser only. */
  setDraft(channelId: string, body: string, reply: QuoteSummary | null): Promise<void> {
    return this.call("setDraft", channelId, body, reply);
  }
  /** Scrollback from the instance, older than the store holds: the newest `limit` below `beforeSeq`, oldest first. Not stored. */
  olderMessages(channelId: string, beforeSeq: number, limit = 100): Promise<{ messages: MessageSummary[]; hasMore: boolean }> {
    return this.call("olderMessages", channelId, beforeSeq, limit);
  }
  start(channelIds: string[]): Promise<void> {
    return this.call("start", channelIds);
  }
  stop(): Promise<void> {
    return this.call("stop");
  }
  subscribe(channelId: string): Promise<void> {
    return this.call("subscribe", channelId);
  }
  sendMessage(channelId: string, body: string): Promise<string> {
    return this.call("sendMessage", channelId, body);
  }
  /** Sends a message with its mentions and what it quotes (only `messageId` and `text` are read); the client-minted message id. */
  sendMessageWith(channelId: string, body: string, mentions: MentionSummary[], replyTo: QuoteSummary | null): Promise<string> {
    return this.call("sendMessageWith", channelId, body, mentions, replyTo);
  }
  /** The mentions a body makes: each `@handle` of one of `members`, and `@everyone` when `everyone`. */
  findMentions(body: string, members: MemberSummary[], everyone: boolean): Promise<MentionSummary[]> {
    // A query's members are a store's proxies, which a worker message cannot carry: plain copies go.
    return this.call("findMentions", body, members.map((m) => ({ ...m })), everyone);
  }
  /** The room a draft has under the instance's limit, for the composer's counter. */
  messageRoom(body: string): Promise<MessageRoom> {
    return this.call("messageRoom", body);
  }
  /** Edits one of this account's messages in a server's channel: its new body and the mentions it makes. Answers the command id an `error` event names if it is refused. */
  editMessage(channelId: string, messageId: string, body: string, mentions: MentionSummary[]): Promise<string> {
    return this.call("editMessage", channelId, messageId, body, mentions);
  }
  /** Deletes a message of a server's channel: this account's own, or another's with `MANAGE_MESSAGES`. Answers the command id an `error` event names if it is refused. */
  deleteMessage(channelId: string, messageId: string): Promise<string> {
    return this.call("deleteMessage", channelId, messageId);
  }
  /** Adds this account's reaction to a message of a server channel, or takes it away. */
  react(channelId: string, messageId: string, emoji: string, remove: boolean): Promise<void> {
    return this.call("react", channelId, messageId, emoji, remove);
  }
  /** Reacts to a message of a server's channel with one of the server's custom emoji, or takes the reaction away (specs/emoji.md 9). */
  reactEmoji(channelId: string, messageId: string, emojiId: string, remove: boolean): Promise<void> {
    return this.call("reactEmoji", channelId, messageId, emojiId, remove);
  }
  /** A server's live custom emoji as the core keeps them, oldest first: what is read again on `emojiChanged`. */
  serverEmoji(serverId: string): Promise<EmojiSummary[]> {
    return this.call("serverEmoji", serverId);
  }
  /** The server's emoji what follows a `:` in the composer matches, in the order the list shows them; none under two characters (the core's rule). */
  emojiMatches(serverId: string, prefix: string): Promise<EmojiSummary[]> {
    return this.call("emojiMatches", serverId, prefix);
  }
  /** `POST /servers/{id}/emoji`: a new custom emoji from a file's bytes as chosen, which the instance makes its own (specs/emoji.md 3). The snapshot as it then is. */
  uploadEmoji(serverId: string, name: string, picture: Uint8Array, mediaType: string): Promise<SnapshotSummary> {
    return this.call("uploadEmoji", serverId, name, picture, mediaType);
  }
  /** `PATCH /servers/{id}/emoji/{emoji_id}`: a new name. The snapshot as it then is. */
  renameEmoji(serverId: string, emojiId: string, name: string): Promise<SnapshotSummary> {
    return this.call("renameEmoji", serverId, emojiId, name);
  }
  /** `DELETE /servers/{id}/emoji/{emoji_id}`: removed, its name left behind (specs/emoji.md 7). The snapshot as it then is. */
  removeEmoji(serverId: string, emojiId: string): Promise<SnapshotSummary> {
    return this.call("removeEmoji", serverId, emojiId);
  }
  /** A custom emoji's name made from a picture's file name, for the upload form to start from (the core's rule). */
  emojiNameFromFile(fileName: string): Promise<string> {
    return this.call("emojiNameFromFile", fileName);
  }
  /** Whether a name is one a custom emoji may have (2 to 32 of a–z, 0–9 and _). */
  isEmojiName(name: string): Promise<boolean> {
    return this.call("isEmojiName", name);
  }
  /** Every conversation's read state as this client counts it. */
  readStates(): Promise<ReadStateSummary[]> {
    return this.call("readStates");
  }
  /** What notifies this account and is unread, everywhere. */
  badge(): Promise<number> {
    return this.call("badge");
  }
  /** Read through `seq`; the state as the instance then counts it. */
  markRead(channelId: string, seq: number): Promise<ReadStateSummary> {
    return this.call("markRead", channelId, seq);
  }
  /** The messages that notify this account, newest first, in one server or (`null`) all; `before` is the last message id of the page before. */
  mentions(serverId: string | null, before: string | null): Promise<MentionsPage> {
    return this.call("mentions", serverId, before);
  }
  foreground(): Promise<void> {
    return this.call("foreground");
  }
  hasIdentity(): Promise<boolean> {
    return this.call("hasIdentity");
  }
  identityLinkRequest(label: string): Promise<string> {
    return this.call("identityLinkRequest", label);
  }
  identityClaim(code: string): Promise<RosterSummary> {
    return this.call("identityClaim", code);
  }
  /** Linking by a prompt (identity spec 4.2.1): opens a session, so every device that holds the identity asks about this browser. */
  linkStart(label: string): Promise<LinkProgress> {
    return this.call("linkStart", label);
  }
  /** Where the session stands, the core taking its next step (revealing, then claiming the code and signing in by identity proof). */
  linkProgress(): Promise<LinkProgress> {
    return this.call("linkProgress");
  }
  /** Gives the session up: declined, so the other devices stop asking. */
  linkCancel(): Promise<void> {
    return this.call("linkCancel");
  }
  signInIdentity(): Promise<void> {
    return this.call("signInIdentity");
  }
  identityShow(): Promise<RosterSummary> {
    return this.call("identityShow");
  }
  identityRevoke(deviceId: string, password: string): Promise<RosterSummary> {
    return this.call("identityRevoke", deviceId, password);
  }
  /** The identity the signed-in account is bound to, hex, or null for none (identity spec 4.1). */
  accountIdentity(): Promise<string | null> {
    return this.call<string | null | undefined>("accountIdentity").then((id) => id ?? null);
  }
  /** Whether a reauth token is in hand: a credentialed sign-in carries one for five minutes (identity spec 10). */
  hasReauth(): Promise<boolean> {
    return this.call("hasReauth");
  }
  reauthPassword(password: string): Promise<void> {
    return this.call("reauthPassword", password);
  }
  /** Reauth by a TOTP code, or by the code `reauthMailCode` mailed when `mailed`. */
  reauthCode(code: string, mailed: boolean): Promise<void> {
    return this.call("reauthCode", code, mailed);
  }
  reauthMailCode(): Promise<void> {
    return this.call("reauthMailCode");
  }
  /** Where a fresh login at the account's OIDC provider starts; it lands at `returnTo` with `?reauth=`. */
  reauthOidcUrl(returnTo: string): Promise<string> {
    return this.call("reauthOidcUrl", returnTo);
  }
  reauthTake(token: string): Promise<void> {
    return this.call("reauthTake", token);
  }
  /** A new recovery phrase, 24 words, held by the worker until it is confirmed and set up; making another forgets the last. */
  identityNewPhrase(): Promise<string> {
    return this.call("identityNewPhrase");
  }
  /** Up to four words of the phrase's wordlist that begin with `prefix`. */
  phraseCompletions(prefix: string): Promise<string[]> {
    return this.call("phraseCompletions", prefix);
  }
  /** Three positions, 1 to 24, to ask for; the same until answered. */
  identityPhraseQuiz(): Promise<number[]> {
    return this.call<ArrayLike<number>>("identityPhraseQuiz").then((q) => Array.from(q));
  }
  /** Whether the words are the quiz's; false, and the next quiz asks other positions. */
  identityPhraseConfirm(words: string[]): Promise<boolean> {
    return this.call("identityPhraseConfirm", words);
  }
  /** Private messages set up here from the confirmed phrase, the account bound under the reauth in hand: the identity id. */
  identitySetup(label: string): Promise<string> {
    return this.call("identitySetup", label);
  }
  /** Starting over (identity spec 4.7): the account moved to an identity made here from the confirmed phrase. */
  identityStartOver(label: string): Promise<string> {
    return this.call("identityStartOver", label);
  }
  /** Restores the identity from its phrase and signs in by it; the page offers it only on the flagship's origin. */
  identityRecover(phrase: string, label: string): Promise<RosterSummary> {
    return this.call("identityRecover", phrase, label);
  }
  /** The devices restoring from the phrase would stop, read from the instance before anything changes; empty when it knows none. */
  identityRecoverDevices(phrase: string): Promise<DeviceSummary[]> {
    return this.call("identityRecoverDevices", phrase);
  }
  /** Restores as `identityRecover` does, keeping the devices in `keep` (their ids) and stopping the rest. */
  identityRecoverKeeping(phrase: string, label: string, keep: string[]): Promise<RosterSummary> {
    return this.call("identityRecoverKeeping", phrase, label, keep);
  }
  /** Lets go of the bodies the worker keeps read, for an account whose device was removed. */
  forgetBodies(): Promise<void> {
    return this.call("forgetBodies");
  }
  pushSubscribe(endpoint: string, p256dh: string, auth: string): Promise<void> {
    return this.call("pushSubscribe", endpoint, p256dh, auth);
  }
  pushUnsubscribe(): Promise<void> {
    return this.call("pushUnsubscribe");
  }
  account(accountId: string): Promise<MemberSummary> {
    return this.call("account", accountId);
  }
  members(serverId: string, query = ""): Promise<MemberSummary[]> {
    return this.call("members", serverId, query);
  }
  /** The members last listed for a server, from the store, each with its tag as the server now shows it; empty when it was never listed. What is read again on `tagsChanged`. */
  keptMembers(serverId: string): Promise<MemberSummary[]> {
    return this.call("keptMembers", serverId);
  }
  dmAvailable(): Promise<boolean> {
    return this.call("dmAvailable");
  }
  dmChannels(): Promise<DmChannelSummary[]> {
    return this.call("dmChannels");
  }
  dmOpen(accountIds: string[]): Promise<DmOpened> {
    return this.call("dmOpen", accountIds);
  }
  dmRequest(accountId: string): Promise<DmRequestSummary> {
    return this.call("dmRequest", accountId);
  }
  dmRespond(requestId: string, response: "accept" | "decline" | "block"): Promise<DmOpened | DmRequestSummary> {
    return this.call("dmRespond", requestId, response);
  }
  dmSend(channelId: string, body: string): Promise<void> {
    return this.call("dmSend", channelId, body);
  }
  /** Sends a message in a personal DM quoting another, or nothing. */
  dmSendWith(channelId: string, body: string, quote: QuoteSummary | null): Promise<void> {
    return this.call("dmSendWith", channelId, body, quote);
  }
  dmReact(channelId: string, messageId: string, emoji: string, remove: boolean): Promise<void> {
    return this.call("dmReact", channelId, messageId, emoji, remove);
  }
  /** Edits one of this identity's messages in a personal DM. */
  dmEdit(channelId: string, messageId: string, body: string): Promise<void> {
    return this.call("dmEdit", channelId, messageId, body);
  }
  dmDelete(channelId: string, messageId: string): Promise<void> {
    return this.call("dmDelete", channelId, messageId);
  }
  dmRejoin(channelId: string): Promise<void> {
    return this.call("dmRejoin", channelId);
  }
  dmMessages(channelId: string): Promise<DmLogSummary> {
    return this.call("dmMessages", channelId);
  }

  // ---- Attachments and link cards (specs/media.md) ----

  /** What the instance takes of attachments; null when it keeps none, and the composer offers no way to attach. */
  uploadLimits(): Promise<UploadLimits | null> {
    return this.call<UploadLimits | null | undefined>("uploadLimits").then((l) => l ?? null);
  }
  /** Makes an attachment of a file and uploads it into the conversation; the draft the composer strip shows. */
  attach(channelId: string, bytes: Uint8Array, options: AttachOptions): Promise<AttachmentDraft> {
    return this.call("attach", channelId, bytes, options);
  }
  setAttachmentAlt(draftId: string, alt: string): Promise<AttachmentDraft> {
    return this.call("setAttachmentAlt", draftId, alt);
  }
  /** Takes a waiting attachment out of the composer (its blobs off the instance, best effort). */
  discardAttachment(draftId: string): Promise<void> {
    return this.call("discardAttachment", draftId);
  }
  /** `sendMessageWith`, with waiting attachments by draft id and the links whose cards the person kept; the message id. */
  sendMessageWithMedia(channelId: string, body: string, mentions: MentionSummary[], replyTo: QuoteSummary | null, attachments: string[], cardUrls: string[]): Promise<string> {
    return this.call("sendMessageWithMedia", channelId, body, mentions, replyTo, attachments, cardUrls);
  }
  /** `editMessage`, dropping attachments and, with `cardUrls` given, keeping only those links' cards; the command id. */
  editMessageWithMedia(channelId: string, messageId: string, body: string, mentions: MentionSummary[], removeAttachmentIds: string[], cardUrls: string[] | null): Promise<string> {
    return this.call("editMessageWithMedia", channelId, messageId, body, mentions, removeAttachmentIds, cardUrls);
  }
  /** `dmSendWith`, with waiting attachments by draft id (the web makes no cards in personal DMs); the message id. */
  dmSendWithMedia(channelId: string, body: string, quote: QuoteSummary | null, attachments: string[], cardUrls: string[]): Promise<string> {
    return this.call("dmSendWithMedia", channelId, body, quote, attachments, cardUrls);
  }
  dmEditWithMedia(channelId: string, messageId: string, body: string, removeAttachmentIds: string[], cardUrls: string[] | null): Promise<void> {
    return this.call("dmEditWithMedia", channelId, messageId, body, removeAttachmentIds, cardUrls);
  }
  /** The links of a body that may have cards, at most three, in order. */
  cardLinks(body: string): Promise<string[]> {
    return this.call("cardLinks", body);
  }
  /** The links a composer previews in a conversation: none where it cannot have cards (an instance that makes none, or a personal conversation on the web). */
  previewLinks(channelId: string, body: string): Promise<string[]> {
    return this.call("previewLinks", channelId, body);
  }
  /** The card under a link while composing: the instance's in a server channel, none in a personal one on the web. */
  cardPreview(channelId: string, url: string): Promise<CardSummary | null> {
    return this.call<CardSummary | null | undefined>("cardPreview", channelId, url).then((c) => c ?? null);
  }
  /** A blob of an attachment or a DM card, fetched, opened and kept in memory. Check `attachmentDrawable` before decoding a picture. */
  attachmentBytes(blob: BlobSummary): Promise<Uint8Array> {
    // Copied: a summary read from a query is a store's proxy, which a worker's message cannot carry.
    return this.call("attachmentBytes", { blobId: blob.blobId, sizeBytes: blob.sizeBytes, key: blob.key, nonce: blob.nonce, hash: blob.hash });
  }
  /** A server card's picture by its `imageUrl`. */
  cardImage(imageUrl: string): Promise<Uint8Array> {
    return this.call("cardImage", imageUrl);
  }
  /** The embed of a link in a message's text, or null. Nothing is fetched. */
  embedFor(url: string): Promise<EmbedSummary | null> {
    return this.call<EmbedSummary | null | undefined>("embedFor", url).then((e) => e ?? null);
  }
  /** The address of a Bluesky post's embed from its account's DID and its record key; null when either is not one. */
  blueskyEmbedUrl(did: string, rkey: string): Promise<string | null> {
    return this.call<string | null | undefined>("blueskyEmbedUrl", did, rkey).then((u) => u ?? null);
  }
  /** The origins an embed loads from: the `frame-src` the page needs. */
  embedOrigins(): Promise<string[]> {
    return this.call("embedOrigins");
  }
  /**
   * The height a post's embed reported (specs/media.md 6.6), from what its window posted as text (a
   * string as posted, an object as its JSON): read in the site's own form and held to 50–5000; null
   * for any other message or site. Only what came from the embed's own window and origin is asked.
   */
  reportedEmbedHeight(provider: string, message: string): Promise<number | null> {
    return this.call<number | null | undefined>("reportedEmbedHeight", provider, message).then((h) => h ?? null);
  }
  /** A Bluesky handle's DID, asked of Bluesky (not the instance) on the press of a post named by handle. */
  /** A Bluesky handle's DID, asked of Bluesky on the press; undefined when Bluesky has no account for it, and a refusal when it gave no answer (the press may be made again). */
  resolveBlueskyHandle(handle: string): Promise<string | undefined> {
    return this.call("resolveBlueskyHandle", handle);
  }
  /** Whether a picture someone sent may be decoded: its header names a size within the bounds. */
  attachmentDrawable(bytes: Uint8Array): Promise<boolean> {
    return this.call("attachmentDrawable", bytes);
  }
  /** A placeholder as a tiny picture for an `ImageData`; null for none. */
  placeholderImage(placeholder: string): Promise<{ width: number; height: number; rgba: Uint8ClampedArray } | null> {
    return this.call<{ width: number; height: number; rgba: Uint8ClampedArray } | null | undefined>("placeholderImage", placeholder).then((p) => p ?? null);
  }
}

/**
 * Becomes the owning tab, or reports that another tab holds the lock. The
 * lock is held for the page's lifetime; the browser releases it when the
 * tab closes, and the next tab asking gets it.
 */
export function claimOwnership(): Promise<boolean> {
  if (!("locks" in navigator)) return Promise.resolve(true);
  return new Promise((resolve) => {
    void navigator.locks.request("lumia-owner", { ifAvailable: true }, (lock) => {
      if (!lock) {
        resolve(false);
        return Promise.resolve();
      }
      resolve(true);
      // Hold the lock until the page goes away.
      return new Promise<void>(() => {});
    });
  });
}
