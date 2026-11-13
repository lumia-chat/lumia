//! UniFFI surface for the native shells (architecture section 13): one
//! [`Client`] per instance over `lumia_client::app::App`, and a
//! [`Listener`] the shell implements to receive what the gateway loop and
//! the DM engine produce.
//!
//! Marshalling only. Every method is synchronous and blocking, meant to be
//! called from a background thread on the shell's side; the gateway loop
//! runs on the client's own tokio runtime and reports through the listener
//! from a runtime thread. Ids cross the boundary as UUID strings and times
//! as microseconds since the epoch. The records here mirror the app's
//! summaries one for one.
//!
//! The stores are SQLite files under the shell's data directory, in the
//! clear: the platform keystore for the at-rest key is the shells' next
//! step (plan step 7).

use std::path::PathBuf;
use std::sync::Arc;

use lumia_client::app::{self, App, EventSink, Opener, Place, StoreName};
use lumia_client::sqlite::SqliteStore;
use lumia_client::ClientError;
use lumia_core::proto::PushKind;

uniffi::setup_scaffolding!();

/// Why a call failed. The shell gets the message; the variant names the
/// cause a caller can act on.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum LumiaError {
    /// The instance answered with the protocol's error envelope.
    #[error("{code}: {detail}")]
    Api {
        /// `lumia.v1.ErrorCode` as its name.
        code: String,
        /// The server's message.
        detail: String,
        /// `Error.details`, keyed as docs/protocol.md documents per code
        /// (`mfa`, `state`, ...).
        details: std::collections::HashMap<String, String>,
    },
    /// This device is revoked in the roster it now holds.
    #[error("this device was revoked (roster v{roster_version})")]
    DeviceRevoked {
        /// The adopted roster's version.
        roster_version: u64,
    },
    /// Not signed in to this instance.
    #[error("not signed in")]
    NotSignedIn,
    /// A malformed argument, a lifecycle mistake, or any other failure.
    #[error("{detail}")]
    Other { detail: String },
}

impl From<ClientError> for LumiaError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::Api {
                code,
                message,
                details,
                ..
            } => Self::Api {
                code,
                detail: message,
                details,
            },
            ClientError::DeviceRevoked { roster_version } => Self::DeviceRevoked { roster_version },
            ClientError::NotSignedIn => Self::NotSignedIn,
            other => Self::Other {
                detail: other.to_string(),
            },
        }
    }
}

// ---- Records, mirroring lumia_client::app -------------------------------------

/// The trust card.
#[derive(Debug, Clone, uniffi::Record)]
pub struct InstanceSummary {
    pub host: String,
    pub operator_name: String,
    pub operator_url: String,
    pub terms_url: String,
    pub privacy_url: String,
    pub software_version: String,
    pub identity_proof_sign_in_enabled: bool,
    pub identity_proof_create_enabled: bool,
    pub open_registration: bool,
    pub web_push_public_key: String,
    pub magic_link_enabled: bool,
    pub oidc_name: String,
}

impl From<app::InstanceSummary> for InstanceSummary {
    fn from(s: app::InstanceSummary) -> Self {
        Self {
            host: s.host,
            operator_name: s.operator_name,
            operator_url: s.operator_url,
            terms_url: s.terms_url,
            privacy_url: s.privacy_url,
            software_version: s.software_version,
            identity_proof_sign_in_enabled: s.identity_proof_sign_in_enabled,
            identity_proof_create_enabled: s.identity_proof_create_enabled,
            open_registration: s.open_registration,
            web_push_public_key: s.web_push_public_key,
            magic_link_enabled: s.magic_link_enabled,
            oidc_name: s.oidc_name,
        }
    }
}

/// A server the account belongs to.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ServerSummary {
    pub server_id: String,
    pub name: String,
    /// The server's picture, a path of this instance; empty for none.
    pub icon_url: String,
    /// How long the server keeps its messages, in days; 0 for ever.
    pub retention_days: u32,
    /// Whether this account may change the server's settings (architecture 6.7).
    pub can_manage: bool,
    /// Whether it may create channels there and remove others from them
    /// (`MANAGE_CHANNELS`, architecture 6.8).
    pub can_manage_channels: bool,
    /// Whether it may start server DMs there (`START_SERVER_DMS`).
    pub can_start_dms: bool,
    /// Whether it may mention everyone there (`MENTION_EVERYONE`).
    pub can_mention_everyone: bool,
    /// Whether it may delete others' messages there (`MANAGE_MESSAGES`).
    pub can_manage_messages: bool,
    /// This account's own switches there: adds to channels wait as
    /// invitations; nobody may start a server DM with it.
    pub channel_adds_by_invitation: bool,
    pub server_dms_off: bool,
}

impl From<app::ServerSummary> for ServerSummary {
    fn from(s: app::ServerSummary) -> Self {
        Self {
            server_id: s.server_id,
            name: s.name,
            icon_url: s.icon_url,
            retention_days: s.retention_days,
            can_manage: s.can_manage,
            can_manage_channels: s.can_manage_channels,
            can_start_dms: s.can_start_dms,
            can_mention_everyone: s.can_mention_everyone,
            can_manage_messages: s.can_manage_messages,
            channel_adds_by_invitation: s.channel_adds_by_invitation,
            server_dms_off: s.server_dms_off,
        }
    }
}

/// What an invite shows before joining.
#[derive(Debug, Clone, uniffi::Record)]
pub struct InvitePreviewSummary {
    pub code: String,
    pub server_id: String,
    pub server_name: String,
    pub channel_name: String,
    pub member_count: u32,
    pub as_guest: bool,
    pub host: String,
    pub operator_name: String,
    pub open_registration: bool,
    pub identity_proof_create_enabled: bool,
}

impl From<app::InvitePreviewSummary> for InvitePreviewSummary {
    fn from(p: app::InvitePreviewSummary) -> Self {
        Self {
            code: p.code,
            server_id: p.server_id,
            server_name: p.server_name,
            channel_name: p.channel_name,
            member_count: p.member_count,
            as_guest: p.as_guest,
            host: p.host,
            operator_name: p.operator_name,
            open_registration: p.open_registration,
            identity_proof_create_enabled: p.identity_proof_create_enabled,
        }
    }
}

/// An invite this account minted: the code and the link that carries it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct InviteSummary {
    pub code: String,
    pub url: String,
    pub server_id: String,
    pub expires_at_micros: Option<i64>,
    pub max_uses: u32,
    pub uses: u32,
    /// The address an email invite went to; empty for a link.
    pub email: String,
}

impl From<app::InviteSummary> for InviteSummary {
    fn from(i: app::InviteSummary) -> Self {
        Self {
            code: i.code,
            url: i.url,
            server_id: i.server_id,
            expires_at_micros: i.expires_at_micros,
            max_uses: i.max_uses,
            uses: i.uses,
            email: i.email,
        }
    }
}

/// A channel the account can view.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChannelSummary {
    pub channel_id: String,
    pub server_id: String,
    pub name: String,
    pub kind: String,
    pub last_seq: u64,
    pub participant_account_ids: Vec<String>,
    pub host: String,
    /// `TEXT`: made private, so entered only by being added (architecture 6.8).
    pub private: bool,
    /// `TEXT`: joined by every newcomer to the server.
    pub is_default: bool,
    /// How many are in it, where a listing says (Browse); 0 elsewhere.
    pub member_count: u32,
    /// The channel's topic; empty for none.
    pub topic: String,
}

impl From<app::ChannelSummary> for ChannelSummary {
    fn from(c: app::ChannelSummary) -> Self {
        Self {
            channel_id: c.channel_id,
            server_id: c.server_id,
            name: c.name,
            kind: c.kind,
            last_seq: c.last_seq,
            participant_account_ids: c.participant_account_ids,
            host: c.host,
            private: c.private,
            is_default: c.is_default,
            member_count: c.member_count,
            topic: c.topic,
        }
    }
}

/// An invitation into a channel, waiting for this account's yes
/// (architecture 6.8): join the channel to take it, dismiss it to say no.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChannelInvitationSummary {
    pub channel: ChannelSummary,
    pub invited_by_account_id: String,
    pub created_at_micros: i64,
}

impl From<app::ChannelInvitationSummary> for ChannelInvitationSummary {
    fn from(i: app::ChannelInvitationSummary) -> Self {
        Self {
            channel: i.channel.into(),
            invited_by_account_id: i.invited_by_account_id,
            created_at_micros: i.created_at_micros,
        }
    }
}

/// Who an add brought in at once, and who it invited instead.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChannelAddSummary {
    pub added_account_ids: Vec<String>,
    pub invited_account_ids: Vec<String>,
}

impl From<app::ChannelAddSummary> for ChannelAddSummary {
    fn from(a: app::ChannelAddSummary) -> Self {
        Self {
            added_account_ids: a.added_account_ids,
            invited_account_ids: a.invited_account_ids,
        }
    }
}

/// A DM request waiting for an answer.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DmRequestSummary {
    pub request_id: String,
    pub from_account_id: String,
    pub to_account_id: String,
    pub state: String,
    pub shared_server_ids: Vec<String>,
    pub mutual_friend_count: u32,
}

impl From<app::DmRequestSummary> for DmRequestSummary {
    fn from(r: app::DmRequestSummary) -> Self {
        Self {
            request_id: r.request_id,
            from_account_id: r.from_account_id,
            to_account_id: r.to_account_id,
            state: r.state,
            shared_server_ids: r.shared_server_ids,
            mutual_friend_count: r.mutual_friend_count,
        }
    }
}

/// The account snapshot.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SnapshotSummary {
    pub account_id: String,
    pub handle: String,
    pub display_name: String,
    /// The account's picture, as `MemberSummary.avatar_url`; empty for none.
    pub avatar_url: String,
    pub servers: Vec<ServerSummary>,
    pub channels: Vec<ChannelSummary>,
    pub pending_requests: Vec<DmRequestSummary>,
    /// Channels this account was asked into and has not answered.
    pub channel_invitations: Vec<ChannelInvitationSummary>,
    /// Where this account has read each conversation (architecture 6.6).
    pub read_states: Vec<ReadStateSummary>,
}

impl From<app::SnapshotSummary> for SnapshotSummary {
    fn from(s: app::SnapshotSummary) -> Self {
        Self {
            account_id: s.account_id,
            handle: s.handle,
            display_name: s.display_name,
            avatar_url: s.avatar_url,
            servers: s.servers.into_iter().map(Into::into).collect(),
            channels: s.channels.into_iter().map(Into::into).collect(),
            pending_requests: s.pending_requests.into_iter().map(Into::into).collect(),
            channel_invitations: s.channel_invitations.into_iter().map(Into::into).collect(),
            read_states: s.read_states.into_iter().map(Into::into).collect(),
        }
    }
}

/// A person: a member of a server, or a public profile.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MemberSummary {
    pub account_id: String,
    pub handle: String,
    pub display_name: String,
    pub nickname: String,
    /// From the account's verified profile, as the display name is.
    pub pronouns: String,
    pub bio: String,
    /// The account's picture: a path on this instance (`/api/v1/avatars/…`,
    /// to resolve against the instance's address), an absolute URL from a
    /// signed profile issued elsewhere (not to be fetched unasked), or empty.
    pub avatar_url: String,
    /// Their switches in the server listed (architecture 6.8): an add
    /// becomes an invitation; nobody may start a server DM with them.
    /// False for a public profile.
    pub channel_adds_by_invitation: bool,
    pub server_dms_off: bool,
}

impl From<app::MemberSummary> for MemberSummary {
    fn from(m: app::MemberSummary) -> Self {
        Self {
            account_id: m.account_id,
            handle: m.handle,
            display_name: m.display_name,
            nickname: m.nickname,
            pronouns: m.pronouns,
            bio: m.bio,
            avatar_url: m.avatar_url,
            channel_adds_by_invitation: m.channel_adds_by_invitation,
            server_dms_off: m.server_dms_off,
        }
    }
}

/// A message as the local store holds it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MessageSummary {
    pub message_id: String,
    pub channel_id: String,
    pub seq: u64,
    pub author_account_id: String,
    pub body: String,
    pub created_at_micros: i64,
    pub deleted: bool,
    /// A notice the instance posted (architecture 6.8): `joined`, `added`,
    /// `left` or `removed`, with an empty body; empty for a message. The
    /// author is who did it, `notice_account_id` who it is about.
    pub notice: String,
    pub notice_account_id: String,
    /// Whether its author has edited it ("(edited)").
    pub edited: bool,
    /// What it quotes (architecture 6.6).
    pub reply: Option<QuoteSummary>,
    /// Whom it mentions, with UTF-8 byte ranges into the body.
    pub mentions: Vec<MentionSummary>,
    /// Its reactions, in the order each emoji was first used.
    pub reactions: Vec<ReactionSummary>,
    /// It mentions this account or everyone, or quotes this account.
    pub mentions_me: bool,
}

/// A quote of another message of the conversation (architecture 6.6).
#[derive(Debug, Clone, uniffi::Record)]
pub struct QuoteSummary {
    pub message_id: String,
    /// Its author; in a personal conversation, empty where unknown.
    pub author_account_id: String,
    /// What of it was quoted; empty once it is deleted.
    pub text: String,
    pub original_deleted: bool,
    /// Sent without notifying the author quoted; on a send, the reply bar's
    /// "@ On" switch turned off.
    pub silent: bool,
}

impl From<app::QuoteSummary> for QuoteSummary {
    fn from(q: app::QuoteSummary) -> Self {
        Self {
            message_id: q.message_id,
            author_account_id: q.author_account_id,
            text: q.text,
            original_deleted: q.original_deleted,
            silent: q.silent,
        }
    }
}

impl From<QuoteSummary> for app::QuoteSummary {
    fn from(q: QuoteSummary) -> Self {
        Self {
            message_id: q.message_id,
            author_account_id: q.author_account_id,
            text: q.text,
            original_deleted: q.original_deleted,
            silent: q.silent,
        }
    }
}

/// A mention: an account, or everyone, and its byte range in the body.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MentionSummary {
    /// Empty when it is everyone.
    pub account_id: String,
    pub everyone: bool,
    pub start: u32,
    pub end: u32,
}

impl From<app::MentionSummary> for MentionSummary {
    fn from(m: app::MentionSummary) -> Self {
        Self {
            account_id: m.account_id,
            everyone: m.everyone,
            start: m.start,
            end: m.end,
        }
    }
}

impl From<MentionSummary> for app::MentionSummary {
    fn from(m: MentionSummary) -> Self {
        Self {
            account_id: m.account_id,
            everyone: m.everyone,
            start: m.start,
            end: m.end,
        }
    }
}

/// One emoji's reactions to a message.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ReactionSummary {
    pub reaction: String,
    pub count: u32,
    /// Whether this account is among them.
    pub me: bool,
    /// Who reacted, as many as are known.
    pub account_ids: Vec<String>,
}

impl From<app::ReactionSummary> for ReactionSummary {
    fn from(r: app::ReactionSummary) -> Self {
        Self {
            reaction: r.reaction,
            count: r.count,
            me: r.me,
            account_ids: r.account_ids,
        }
    }
}

/// A conversation's read state for this account: the last event read,
/// how many messages others sent after it, how many of those notify it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ReadStateSummary {
    pub channel_id: String,
    pub read_seq: u64,
    pub unread: u32,
    pub mentions: u32,
}

impl From<app::ReadStateSummary> for ReadStateSummary {
    fn from(r: app::ReadStateSummary) -> Self {
        Self {
            channel_id: r.channel_id,
            read_seq: r.read_seq,
            unread: r.unread,
            mentions: r.mentions,
        }
    }
}

/// A page of a channel's history from the instance, oldest first.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MessagePage {
    pub messages: Vec<MessageSummary>,
    pub has_more: bool,
}

/// A page of the messages that notify this account, newest first.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MentionsPage {
    pub messages: Vec<MessageSummary>,
    pub has_more: bool,
}

impl From<app::MessageSummary> for MessageSummary {
    fn from(m: app::MessageSummary) -> Self {
        Self {
            message_id: m.message_id,
            channel_id: m.channel_id,
            seq: m.seq,
            author_account_id: m.author_account_id,
            body: m.body,
            created_at_micros: m.created_at_micros,
            deleted: m.deleted,
            notice: m.notice,
            notice_account_id: m.notice_account_id,
            edited: m.edited,
            reply: m.reply.map(Into::into),
            mentions: m.mentions.into_iter().map(Into::into).collect(),
            reactions: m.reactions.into_iter().map(Into::into).collect(),
            mentions_me: m.mentions_me,
        }
    }
}

/// One applied channel event.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EventSummary {
    pub seq: u64,
    pub kind: String,
    pub message: Option<MessageSummary>,
}

impl From<app::EventSummary> for EventSummary {
    fn from(e: app::EventSummary) -> Self {
        Self {
            seq: e.seq,
            kind: e.kind,
            message: e.message.map(Into::into),
        }
    }
}

/// A device of the identity's roster.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DeviceSummary {
    pub device_id: String,
    pub kind: String,
    pub label: String,
    pub web_origin: String,
    pub added_at_micros: i64,
    pub this_device: bool,
    pub revoked: bool,
}

impl From<app::DeviceSummary> for DeviceSummary {
    fn from(d: app::DeviceSummary) -> Self {
        Self {
            device_id: d.device_id,
            kind: d.kind,
            label: d.label,
            web_origin: d.web_origin,
            added_at_micros: d.added_at_micros,
            this_device: d.this_device,
            revoked: d.revoked,
        }
    }
}

/// The identity's roster.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RosterSummary {
    pub identity_id: String,
    pub version: u64,
    pub devices: Vec<DeviceSummary>,
}

impl From<app::RosterSummary> for RosterSummary {
    fn from(r: app::RosterSummary) -> Self {
        Self {
            identity_id: r.identity_id,
            version: r.version,
            devices: r.devices.into_iter().map(Into::into).collect(),
        }
    }
}

/// What creating an identity gives back: the id, and the phrase shown once.
#[derive(Debug, Clone, uniffi::Record)]
pub struct IdentityCreated {
    pub identity_id: String,
    pub phrase: String,
}

/// A personal conversation this device knows.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DmChannelSummary {
    pub channel_id: String,
    pub participant_account_ids: Vec<String>,
    pub member: bool,
}

impl From<app::DmChannelSummary> for DmChannelSummary {
    fn from(c: app::DmChannelSummary) -> Self {
        Self {
            channel_id: c.channel_id,
            participant_account_ids: c.participant_account_ids,
            member: c.member,
        }
    }
}

/// A message of a personal conversation, as projected.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DmMessageSummary {
    pub message_id: String,
    pub seq: u64,
    pub sender_identity_id: String,
    pub sender_device_id: String,
    pub sender_account_id: String,
    pub body: String,
    pub sent_at_micros: i64,
    pub edited: bool,
    pub deleted: bool,
    pub mine: bool,
    /// What it quotes; the author is known when the original is in the
    /// conversation.
    pub reply: Option<QuoteSummary>,
    /// Its reactions, each emoji with who reacted where known.
    pub reactions: Vec<ReactionSummary>,
}

impl From<app::DmMessageSummary> for DmMessageSummary {
    fn from(m: app::DmMessageSummary) -> Self {
        Self {
            message_id: m.message_id,
            seq: m.seq,
            sender_identity_id: m.sender_identity_id,
            sender_device_id: m.sender_device_id,
            sender_account_id: m.sender_account_id,
            body: m.body,
            sent_at_micros: m.sent_at_micros,
            edited: m.edited,
            deleted: m.deleted,
            mine: m.mine,
            reply: m.reply.map(Into::into),
            reactions: m.reactions.into_iter().map(Into::into).collect(),
        }
    }
}

/// What a push should say, decided on the device.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NotificationContent {
    pub title: String,
    pub body: String,
    pub channel_id: String,
    pub kind: String,
}

impl From<app::NotificationContent> for NotificationContent {
    fn from(c: app::NotificationContent) -> Self {
        Self {
            title: c.title,
            body: c.body,
            channel_id: c.channel_id,
            kind: c.kind,
        }
    }
}

/// A personal conversation as this device can read it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DmLogSummary {
    pub messages: Vec<DmMessageSummary>,
    pub before_join: u32,
    pub undecryptable: u32,
}

impl From<app::DmLogSummary> for DmLogSummary {
    fn from(l: app::DmLogSummary) -> Self {
        Self {
            messages: l.messages.into_iter().map(Into::into).collect(),
            before_join: l.before_join,
            undecryptable: l.undecryptable,
        }
    }
}

/// What opening a conversation found or did.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DmOpened {
    pub channel_id: String,
    pub existed: bool,
    pub member: bool,
    pub creating: bool,
}

impl From<app::DmOpened> for DmOpened {
    fn from(o: app::DmOpened) -> Self {
        Self {
            channel_id: o.channel_id,
            existed: o.existed,
            member: o.member,
            creating: o.creating,
        }
    }
}

/// The answer to a DM request: the opened conversation when accepted,
/// otherwise the request as it now stands.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum DmResponse {
    Opened { opened: DmOpened },
    Answered { request: DmRequestSummary },
}

/// What the gateway loop and the DM engine report, in order.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum ClientEvent {
    Welcomed {
        account_id: String,
    },
    Applied {
        channel_id: String,
        events: Vec<EventSummary>,
    },
    AccountApplied {
        count: u64,
    },
    Acked {
        command_id: String,
        seq: u64,
    },
    Error {
        code: String,
        message: String,
        command_id: String,
    },
    SubscriptionFailed {
        channel_id: String,
        message: String,
    },
    SnapshotTaken {
        channel_id: String,
        seq: u64,
    },
    Disconnected {
        reason: String,
        reconnect_after_seconds: u32,
    },
    Reconnecting {
        attempt: u32,
        delay_millis: u64,
        reason: String,
    },
    Closed {
        reason: String,
    },
    DmNote {
        message: String,
    },
    DmError {
        message: String,
    },
    DmSubscribed {
        channel_id: String,
    },
    DmOperation {
        channel_id: String,
        seq: u64,
        sender_identity_id: String,
        sender_device_id: String,
    },
    DmReady {
        channel_id: String,
    },
    DmDelivered {
        operation_id: String,
    },
    PresenceDiff {
        server_id: String,
        joined: Vec<PresenceSummary>,
        updated: Vec<PresenceSummary>,
        left_account_ids: Vec<String>,
    },
    Typing {
        channel_id: String,
        account_id: String,
        until_micros: i64,
    },
}

/// The identity's signed profile, as this device keeps it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct StatusSummary {
    pub status: String,
    pub custom_status: String,
}

impl From<app::StatusSummary> for StatusSummary {
    fn from(s: app::StatusSummary) -> Self {
        Self {
            status: s.status,
            custom_status: s.custom_status,
        }
    }
}

/// The identity's signed profile, as this device keeps it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ProfileSummary {
    pub version: u64,
    pub display_name: String,
    pub pronouns: String,
    pub bio: String,
    pub avatar_url: String,
}

impl From<app::ProfileSummary> for ProfileSummary {
    fn from(p: app::ProfileSummary) -> Self {
        Self {
            version: p.version,
            display_name: p.display_name,
            pronouns: p.pronouns,
            bio: p.bio,
            avatar_url: p.avatar_url,
        }
    }
}

/// A WebAuthn ceremony's start: the challenge's id and the options JSON.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PasskeyChallengeSummary {
    pub challenge_id: String,
    pub options_json: String,
}

impl From<app::PasskeyChallengeSummary> for PasskeyChallengeSummary {
    fn from(c: app::PasskeyChallengeSummary) -> Self {
        Self {
            challenge_id: c.challenge_id,
            options_json: c.options_json,
        }
    }
}

/// One of the account's passkeys.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PasskeySummary {
    pub passkey_id: String,
    pub label: String,
    pub created_at_micros: Option<i64>,
    pub last_used_at_micros: Option<i64>,
}

impl From<app::PasskeySummary> for PasskeySummary {
    fn from(p: app::PasskeySummary) -> Self {
        Self {
            passkey_id: p.passkey_id,
            label: p.label,
            created_at_micros: p.created_at_micros,
            last_used_at_micros: p.last_used_at_micros,
        }
    }
}

/// One account's presence in a server.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PresenceSummary {
    pub account_id: String,
    pub status: String,
    pub custom_status: String,
}

impl From<app::PresenceSummary> for PresenceSummary {
    fn from(p: app::PresenceSummary) -> Self {
        Self {
            account_id: p.account_id,
            status: p.status,
            custom_status: p.custom_status,
        }
    }
}

impl From<app::AppEvent> for ClientEvent {
    fn from(e: app::AppEvent) -> Self {
        use app::AppEvent as E;
        match e {
            E::Welcomed { account_id } => Self::Welcomed { account_id },
            E::Applied { channel_id, events } => Self::Applied {
                channel_id,
                events: events.into_iter().map(Into::into).collect(),
            },
            E::AccountApplied { count } => Self::AccountApplied {
                count: count as u64,
            },
            E::Acked { command_id, seq } => Self::Acked { command_id, seq },
            E::Error {
                code,
                message,
                command_id,
            } => Self::Error {
                code,
                message,
                command_id,
            },
            E::SubscriptionFailed {
                channel_id,
                message,
            } => Self::SubscriptionFailed {
                channel_id,
                message,
            },
            E::SnapshotTaken { channel_id, seq } => Self::SnapshotTaken { channel_id, seq },
            E::Disconnected {
                reason,
                reconnect_after_seconds,
            } => Self::Disconnected {
                reason,
                reconnect_after_seconds,
            },
            E::Reconnecting {
                attempt,
                delay_millis,
                reason,
            } => Self::Reconnecting {
                attempt,
                delay_millis,
                reason,
            },
            E::Closed { reason } => Self::Closed { reason },
            E::DmNote { message } => Self::DmNote { message },
            E::DmError { message } => Self::DmError { message },
            E::DmSubscribed { channel_id } => Self::DmSubscribed { channel_id },
            E::DmOperation {
                channel_id,
                seq,
                sender_identity_id,
                sender_device_id,
            } => Self::DmOperation {
                channel_id,
                seq,
                sender_identity_id,
                sender_device_id,
            },
            E::DmReady { channel_id } => Self::DmReady { channel_id },
            E::DmDelivered { operation_id } => Self::DmDelivered { operation_id },
            E::PresenceDiff {
                server_id,
                joined,
                updated,
                left_account_ids,
            } => Self::PresenceDiff {
                server_id,
                joined: joined.into_iter().map(Into::into).collect(),
                updated: updated.into_iter().map(Into::into).collect(),
                left_account_ids,
            },
            E::Typing {
                channel_id,
                account_id,
                until_micros,
            } => Self::Typing {
                channel_id,
                account_id,
                until_micros,
            },
        }
    }
}

/// The shell's side of the gateway loop.
#[uniffi::export(callback_interface)]
pub trait Listener: Send + Sync {
    /// Called from a runtime thread for every event, in order.
    fn on_event(&self, event: ClientEvent);
}

// ---- The client ------------------------------------------------------------------

/// One instance's client: HTTP API, gateway loop, local store, identity,
/// personal DMs.
#[derive(uniffi::Object)]
pub struct Client {
    runtime: tokio::runtime::Runtime,
    app: Arc<App>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.app.fmt(f)
    }
}

type Res<T> = Result<T, LumiaError>;

#[uniffi::export]
impl Client {
    /// Opens the client for the instance at `base_url` with state under
    /// `data_dir`: `<data_dir>/<host>/` for this instance and
    /// `<data_dir>/identity/` for the device's identity, shared by every
    /// instance opened from the same directory.
    ///
    /// # Errors
    /// When the directories or a store cannot be opened.
    #[uniffi::constructor]
    pub fn new(base_url: String, data_dir: String) -> Res<Arc<Self>> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| LumiaError::Other {
                detail: e.to_string(),
            })?;
        let host = app::host_of(&base_url);
        let data_dir = PathBuf::from(data_dir);
        let dir = data_dir.join(&host);
        let identity_dir = data_dir.join("identity");
        for d in [&dir, &identity_dir] {
            std::fs::create_dir_all(d).map_err(|e| LumiaError::Other {
                detail: e.to_string(),
            })?;
        }
        let lock_dir = dir.clone();
        let opener: Opener = Arc::new(move |name| {
            let path = match name {
                StoreName::Instance => dir.join("store.sqlite"),
                StoreName::Keys => dir.join("keys.sqlite"),
                StoreName::IdentityStore => identity_dir.join("store.sqlite"),
                StoreName::IdentityKeys => identity_dir.join("keys.sqlite"),
            };
            SqliteStore::open(&path)
        });
        // The single-writer lock with the notification extension sits
        // beside this instance's stores (architecture 15.5).
        let app = App::open_locked(
            &base_url,
            Place::Native,
            opener,
            &lock_dir.join("writer.lock"),
        )?;
        Ok(Arc::new(Self { runtime, app }))
    }

    /// The instance's host name (with the port when not the default).
    pub fn host(&self) -> String {
        self.app.host()
    }

    /// This device's id (UUID string).
    pub fn device_id(&self) -> String {
        self.app.device_id()
    }

    /// Whether an access token is held for this instance.
    pub fn signed_in(&self) -> bool {
        self.app.signed_in()
    }

    /// `GET /instance`: the trust card.
    pub fn instance_info(&self) -> Res<InstanceSummary> {
        Ok(self.runtime.block_on(self.app.instance_info())?.into())
    }

    /// Creates an account with a password and signs in; with an invite link
    /// or code, joins its server too (and is admitted when registration is closed).
    pub fn create_account(
        &self,
        handle: String,
        password: String,
        invite: Option<String>,
    ) -> Res<()> {
        Ok(self.runtime.block_on(
            self.app
                .create_account(&handle, &password, invite.as_deref()),
        )?)
    }

    /// `GET /invites/{code}`: what an invite link or code shows before joining. Public.
    pub fn invite_preview(&self, link_or_code: String) -> Res<InvitePreviewSummary> {
        Ok(self
            .runtime
            .block_on(self.app.invite_preview(&link_or_code))?
            .into())
    }

    /// `POST /invites`: an invite link for a server (0 uses is unlimited).
    pub fn create_invite(
        &self,
        server_id: String,
        max_uses: u32,
        expires_in_hours: Option<u32>,
    ) -> Res<InviteSummary> {
        Ok(self
            .runtime
            .block_on(
                self.app
                    .create_invite(&server_id, max_uses, expires_in_hours),
            )?
            .into())
    }

    /// `POST /invites/email`: a one-use invite good for a week, mailed to
    /// the address as a link (architecture 6.2).
    pub fn email_invite(&self, server_id: String, email: String) -> Res<InviteSummary> {
        Ok(self
            .runtime
            .block_on(self.app.email_invite(&server_id, &email))?
            .into())
    }

    /// `POST /invites/{code}/accept`: joins the invite's server; returns its id.
    pub fn accept_invite(&self, link_or_code: String) -> Res<String> {
        Ok(self
            .runtime
            .block_on(self.app.accept_invite(&link_or_code))?)
    }

    /// `GET /servers/{id}/invites`: the server's invites, newest first.
    pub fn invites(&self, server_id: String) -> Res<Vec<InviteSummary>> {
        Ok(self
            .runtime
            .block_on(self.app.invites(&server_id))?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// `DELETE /invites/{code}`: revokes an invite.
    pub fn revoke_invite(&self, link_or_code: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.revoke_invite(&link_or_code))?)
    }

    /// The servers this client has on screen, whose presence it wants
    /// (protocol 6.7); sent with the chosen status.
    pub fn watch_presence(&self, watch_server_ids: Vec<String>) -> Res<()> {
        Ok(self.app.watch_presence(&watch_server_ids)?)
    }

    /// `PATCH /account`: this account's handle, display name, or both; a
    /// value left out stays, an empty display name clears it. Returns the
    /// snapshot as it then is.
    pub fn update_account(
        &self,
        handle: Option<String>,
        display_name: Option<String>,
    ) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(
                self.app
                    .update_account(handle.as_deref(), display_name.as_deref()),
            )?
            .into())
    }

    /// `PUT /account/avatar`: `picture` is a file's bytes as chosen (PNG,
    /// JPEG, WebP or GIF, at most 8 MB); the instance keeps its own square
    /// of it. Returns the snapshot as it then is.
    pub fn set_avatar(&self, picture: Vec<u8>, media_type: String) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.set_avatar(picture, &media_type))?
            .into())
    }

    /// `DELETE /account/avatar`.
    pub fn remove_avatar(&self) -> Res<SnapshotSummary> {
        Ok(self.runtime.block_on(self.app.remove_avatar())?.into())
    }

    /// Says this person is typing in a server's channel (protocol 6.7);
    /// call it on every edit of the composer, the core spaces them out.
    pub fn typing(&self, channel_id: String) -> Res<()> {
        Ok(self.app.typing(&channel_id)?)
    }

    /// The status this person chose and their custom status text, kept
    /// across launches.
    pub fn status(&self) -> Res<StatusSummary> {
        Ok(self.app.status()?.into())
    }

    /// Chooses the status (`ONLINE`, `AWAY`, `DO_NOT_DISTURB`) and custom
    /// text; sent at once when the gateway is up, and on every welcome.
    pub fn set_status(&self, status: String, custom_status: String) -> Res<()> {
        Ok(self.app.set_status(&status, &custom_status)?)
    }

    /// `PATCH /servers/{id}`: a server's name, how long it keeps messages
    /// in days (0 for ever), or both; `None` leaves one as it is. Returns
    /// the snapshot as it then is.
    pub fn update_server(
        &self,
        server_id: String,
        name: Option<String>,
        retention_days: Option<u32>,
    ) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(
                self.app
                    .update_server(&server_id, name.as_deref(), retention_days),
            )?
            .into())
    }

    /// `PUT /servers/{id}/icon`: `picture` is a file's bytes as chosen,
    /// taken as the account's picture is. Returns the snapshot as it then is.
    pub fn set_server_icon(
        &self,
        server_id: String,
        picture: Vec<u8>,
        media_type: String,
    ) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.set_server_icon(&server_id, picture, &media_type))?
            .into())
    }

    /// `DELETE /servers/{id}/icon`.
    pub fn remove_server_icon(&self, server_id: String) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.remove_server_icon(&server_id))?
            .into())
    }

    /// `POST /servers`: a server this account owns.
    pub fn create_server(&self, name: String) -> Res<ServerSummary> {
        Ok(self.runtime.block_on(self.app.create_server(&name))?.into())
    }

    /// `POST /channels`: a text channel in a server this account may
    /// manage channels in, private when asked (architecture 6.8).
    pub fn create_channel(
        &self,
        server_id: String,
        name: String,
        private: bool,
    ) -> Res<ChannelSummary> {
        Ok(self
            .runtime
            .block_on(self.app.create_channel(&server_id, &name, private))?
            .into())
    }

    /// `GET /servers/{id}/channels/browse`: the public channels of a server
    /// this account can see and is not in, each with how many are in it.
    pub fn browse_channels(&self, server_id: String) -> Res<Vec<ChannelSummary>> {
        Ok(self
            .runtime
            .block_on(self.app.browse_channels(&server_id))?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// `POST /channels/{id}/join`. Returns the snapshot as it then is.
    pub fn join_channel(&self, channel_id: String) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.join_channel(&channel_id))?
            .into())
    }

    /// `POST /channels/{id}/leave`. Returns the snapshot as it then is.
    pub fn leave_channel(&self, channel_id: String) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.leave_channel(&channel_id))?
            .into())
    }

    /// `GET /channels/{id}/members`: who is in a channel, as account ids.
    pub fn channel_members(&self, channel_id: String) -> Res<Vec<String>> {
        Ok(self
            .runtime
            .block_on(self.app.channel_members(&channel_id))?)
    }

    /// `POST /channels/{id}/members`: members of the server added to a
    /// channel, or invited when they take adds that way.
    pub fn add_channel_members(
        &self,
        channel_id: String,
        account_ids: Vec<String>,
    ) -> Res<ChannelAddSummary> {
        Ok(self
            .runtime
            .block_on(self.app.add_channel_members(&channel_id, &account_ids))?
            .into())
    }

    /// `DELETE /channels/{id}/members/{account_id}`: someone taken out of a
    /// channel (`MANAGE_CHANNELS`).
    pub fn remove_channel_member(&self, channel_id: String, account_id: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.remove_channel_member(&channel_id, &account_id))?)
    }

    /// `DELETE /channel-invitations/{channel_id}`: no to an invitation.
    /// Returns the snapshot as it then is.
    pub fn dismiss_channel_invitation(&self, channel_id: String) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.dismiss_channel_invitation(&channel_id))?
            .into())
    }

    /// `PATCH /servers/{id}/membership`: this account's own switches in a
    /// server; `None` leaves one as it is. Returns the snapshot as it then is.
    pub fn update_membership(
        &self,
        server_id: String,
        channel_adds_by_invitation: Option<bool>,
        server_dms_off: Option<bool>,
    ) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.update_membership(
                &server_id,
                channel_adds_by_invitation,
                server_dms_off,
            ))?
            .into())
    }

    /// `POST /servers/{id}/dms`: the 1:1 server DM with a member, started
    /// if there is none (architecture 6.4).
    pub fn open_server_dm(&self, server_id: String, account_id: String) -> Res<ChannelSummary> {
        Ok(self
            .runtime
            .block_on(self.app.open_server_dm(&server_id, &account_id))?
            .into())
    }

    /// `PATCH /servers/{id}` with exactly `channel_ids` as the server's
    /// default channels. Returns the snapshot as it then is.
    pub fn set_default_channels(
        &self,
        server_id: String,
        channel_ids: Vec<String>,
    ) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.set_default_channels(&server_id, &channel_ids))?
            .into())
    }

    /// The creation options for a new passkey of this account (architecture 6.3).
    pub fn passkey_register_start(&self) -> Res<PasskeyChallengeSummary> {
        Ok(self
            .runtime
            .block_on(self.app.passkey_register_start())?
            .into())
    }

    /// The credential the platform made, kept under `label`.
    pub fn passkey_register_finish(
        &self,
        challenge_id: String,
        credential_json: String,
        label: String,
    ) -> Res<()> {
        Ok(self.runtime.block_on(self.app.passkey_register_finish(
            &challenge_id,
            &credential_json,
            &label,
        ))?)
    }

    /// The request options to sign in with a passkey, for a handle when given.
    pub fn passkey_assert_start(&self, handle_or_email: String) -> Res<PasskeyChallengeSummary> {
        Ok(self
            .runtime
            .block_on(self.app.passkey_assert_start(&handle_or_email))?
            .into())
    }

    /// The assertion signs this device in.
    pub fn sign_in_passkey(&self, challenge_id: String, credential_json: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.sign_in_passkey(&challenge_id, &credential_json))?)
    }

    /// This account's passkeys.
    pub fn passkeys(&self) -> Res<Vec<PasskeySummary>> {
        Ok(self
            .runtime
            .block_on(self.app.passkeys())?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Removes one of this account's passkeys.
    pub fn remove_passkey(&self, passkey_id: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.remove_passkey(&passkey_id))?)
    }

    /// Asks the instance to mail a sign-in link to `email`.
    pub fn request_magic_link(&self, email: String) -> Res<()> {
        Ok(self.runtime.block_on(self.app.request_magic_link(&email))?)
    }

    /// Signs in with the token from a magic-link mail (the code, or the whole link).
    pub fn sign_in_magic_link(&self, token: String) -> Res<()> {
        Ok(self.runtime.block_on(self.app.sign_in_magic_link(&token))?)
    }

    /// A one-time sign-in token for this account, for the instance's web
    /// page to sign in as it (a passkey is made there) or for another device.
    pub fn handoff_token(&self) -> Res<String> {
        Ok(self.runtime.block_on(self.app.handoff_token())?)
    }

    /// Where the system browser signs in with the instance's OIDC provider;
    /// it comes back to `return_to` with `magic={token}`.
    pub fn oidc_start_url(&self, return_to: String) -> String {
        self.app.oidc_start_url(&return_to)
    }

    /// Where the system browser signs in through the instance's web page
    /// (a passkey, say); it comes back to `return_to` with `magic={token}`.
    pub fn web_sign_in_url(&self, return_to: String) -> String {
        self.app.web_sign_in_url(&return_to)
    }

    /// Where the system browser lands signed in as this account by `token`
    /// (from `handoff_token`), on `next` (`devices` for the passkeys there).
    pub fn web_handoff_url(&self, token: String, next: String) -> String {
        self.app.web_handoff_url(&token, &next)
    }

    /// Signs in with a password, and a TOTP code when the account requires one.
    pub fn sign_in(&self, handle: String, password: String, totp_code: Option<String>) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.sign_in(&handle, &password, totp_code.as_deref()))?)
    }

    /// Signs in with a password and the passkey factor: the assertion the
    /// platform made for an assert challenge (protocol 6.1, `details.mfa`
    /// of `passkey`).
    pub fn sign_in_with_passkey(
        &self,
        handle: String,
        password: String,
        challenge_id: String,
        credential_json: String,
    ) -> Res<()> {
        Ok(self.runtime.block_on(self.app.sign_in_with_passkey(
            &handle,
            &password,
            &challenge_id,
            &credential_json,
        ))?)
    }

    /// Whether a password sign-in must also present one of the account's passkeys.
    pub fn passkey_required(&self) -> Res<bool> {
        Ok(self.runtime.block_on(self.app.passkey_required())?)
    }

    /// Requires, or stops requiring, one of the account's passkeys at a
    /// password sign-in.
    pub fn set_passkey_required(&self, required: bool) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.set_passkey_required(required))?)
    }

    /// Signs out: revokes the session and forgets the tokens.
    pub fn sign_out(&self) -> Res<()> {
        Ok(self.runtime.block_on(self.app.sign_out())?)
    }

    /// `GET /account/snapshot`.
    pub fn snapshot(&self) -> Res<SnapshotSummary> {
        Ok(self.runtime.block_on(self.app.snapshot())?.into())
    }

    /// `GET /accounts/{id}`: a public profile, when visible.
    pub fn account(&self, account_id: String) -> Res<MemberSummary> {
        Ok(self.runtime.block_on(self.app.account(&account_id))?.into())
    }

    /// `GET /servers/{id}/members`.
    pub fn members(&self, server_id: String, query: String) -> Res<Vec<MemberSummary>> {
        Ok(self
            .runtime
            .block_on(self.app.members(&server_id, &query))?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// The newest messages of a channel from the local store, oldest first.
    pub fn messages(&self, channel_id: String, limit: u32) -> Res<Vec<MessageSummary>> {
        Ok(self
            .app
            .messages(&channel_id, limit)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Starts the gateway loop; `listener` gets every event, in order,
    /// from a runtime thread, until `Closed` or `stop`.
    pub fn start(&self, channel_ids: Vec<String>, listener: Box<dyn Listener>) -> Res<()> {
        let listener: Arc<dyn Listener> = Arc::from(listener);
        let sink: EventSink = Arc::new(move |event| listener.on_event(event.into()));
        let app = Arc::clone(&self.app);
        // Spawning happens on the client's runtime.
        let _guard = self.runtime.enter();
        Ok(app.start(&channel_ids, sink)?)
    }

    /// Stops the gateway loop.
    pub fn stop(&self) {
        self.app.stop();
    }

    /// Subscribes to a channel from the store's cursor.
    pub fn subscribe(&self, channel_id: String) -> Res<()> {
        Ok(self.app.subscribe(&channel_id)?)
    }

    /// Sends a message; returns the client-minted message id.
    pub fn send_message(&self, channel_id: String, body: String) -> Res<String> {
        Ok(self.app.send_message(&channel_id, &body)?)
    }

    /// Sends a message with its mentions and what it quotes (only the
    /// quote's message id and text are read); returns the message id.
    pub fn send_message_with(
        &self,
        channel_id: String,
        body: String,
        mentions: Vec<MentionSummary>,
        reply_to: Option<QuoteSummary>,
    ) -> Res<String> {
        let mentions: Vec<app::MentionSummary> = mentions.into_iter().map(Into::into).collect();
        let reply_to: Option<app::QuoteSummary> = reply_to.map(Into::into);
        Ok(self
            .app
            .send_message_with(&channel_id, &body, &mentions, reply_to.as_ref())?)
    }

    /// The mentions `body` makes of `members`, and of everyone when
    /// `everyone` (this account may), as the core finds them.
    pub fn find_mentions(
        &self,
        body: String,
        members: Vec<MemberSummary>,
        everyone: bool,
    ) -> Vec<MentionSummary> {
        let members: Vec<app::MemberSummary> = members
            .into_iter()
            .map(|m| app::MemberSummary {
                account_id: m.account_id,
                handle: m.handle,
                display_name: m.display_name,
                nickname: m.nickname,
                pronouns: m.pronouns,
                bio: m.bio,
                avatar_url: m.avatar_url,
                channel_adds_by_invitation: m.channel_adds_by_invitation,
                server_dms_off: m.server_dms_off,
            })
            .collect();
        app::find_mentions(&body, &members, everyone)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Edits one of this account's messages in a server's channel; returns
    /// the command id a refusal's `Error` event names.
    pub fn edit_message(
        &self,
        channel_id: String,
        message_id: String,
        body: String,
        mentions: Vec<MentionSummary>,
    ) -> Res<String> {
        let mentions: Vec<app::MentionSummary> = mentions.into_iter().map(Into::into).collect();
        Ok(self
            .app
            .edit_message(&channel_id, &message_id, &body, &mentions)?)
    }

    /// Deletes a message of a server's channel: this account's own, or any
    /// for a moderator; returns the command id a refusal's `Error` event
    /// names.
    pub fn delete_message(&self, channel_id: String, message_id: String) -> Res<String> {
        Ok(self.app.delete_message(&channel_id, &message_id)?)
    }

    /// Adds this account's reaction to a message, or takes it away.
    pub fn react(
        &self,
        channel_id: String,
        message_id: String,
        emoji: String,
        remove: bool,
    ) -> Res<()> {
        Ok(self.app.react(&channel_id, &message_id, &emoji, remove)?)
    }

    /// Every conversation's read state as this client counts it.
    pub fn read_states(&self) -> Vec<ReadStateSummary> {
        self.app.read_states().into_iter().map(Into::into).collect()
    }

    /// The badge: what notifies this account and is unread, everywhere.
    pub fn badge(&self) -> u32 {
        self.app.badge()
    }

    /// `POST /channels/{id}/read`: read through `seq`; the state as the
    /// instance then counts it.
    pub fn mark_read(&self, channel_id: String, seq: u64) -> Res<ReadStateSummary> {
        Ok(self
            .runtime
            .block_on(self.app.mark_read(&channel_id, seq))?
            .into())
    }

    /// `GET /account/mentions`: newest first, in one server or (`None`)
    /// all; `before` is the last message id of the page before.
    pub fn mentions(&self, server_id: Option<String>, before: Option<String>) -> Res<MentionsPage> {
        let (messages, has_more) = self
            .runtime
            .block_on(self.app.mentions(server_id.as_deref(), before.as_deref()))?;
        Ok(MentionsPage {
            messages: messages.into_iter().map(Into::into).collect(),
            has_more,
        })
    }

    /// `GET /channels/{id}/messages`: history older than the store holds,
    /// the newest `limit` below `before_seq`, oldest first, and whether
    /// older remains. Not stored: keep it beside the store's messages.
    pub fn older_messages(
        &self,
        channel_id: String,
        before_seq: u64,
        limit: u32,
    ) -> Res<MessagePage> {
        let (messages, has_more) =
            self.runtime
                .block_on(self.app.older_messages(&channel_id, before_seq, limit))?;
        Ok(MessagePage {
            messages: messages.into_iter().map(Into::into).collect(),
            has_more,
        })
    }

    /// The app came back to the foreground: ask for every head.
    pub fn foreground(&self) -> Res<()> {
        Ok(self.app.foreground()?)
    }

    /// The app is going to the background: releases the single-writer lock
    /// so the notification extension can take it.
    pub fn background(&self) {
        self.app.background();
    }

    /// The app is back in the foreground: takes the single-writer lock
    /// again; true when the extension wrote meanwhile, so the gateway loop
    /// must be stopped and started to resume from the store.
    pub fn resume(&self) -> Res<bool> {
        Ok(self.app.resume()?)
    }

    /// For the notification extension: what the push should say, after
    /// syncing and decrypting under the exclusive writer lock (tried for
    /// `timeout_ms`); nil when the app holds the lock or nothing new was
    /// readable, so the generic notice stands.
    pub fn notification_content(&self, timeout_ms: u64) -> Res<Option<NotificationContent>> {
        Ok(self
            .runtime
            .block_on(
                self.app
                    .notification_content(std::time::Duration::from_millis(timeout_ms)),
            )?
            .map(Into::into))
    }

    // ---- Identity ----

    /// Whether this device holds an identity roster.
    pub fn has_identity(&self) -> bool {
        self.app.has_identity()
    }

    /// Creates a new identity on this device; the phrase is shown once.
    pub fn identity_create(&self, label: String) -> Res<IdentityCreated> {
        let (identity_id, phrase) = self.runtime.block_on(self.app.identity_create(&label))?;
        Ok(IdentityCreated {
            identity_id,
            phrase,
        })
    }

    /// Binds the signed-in password account to this device's identity.
    /// The identity's signed profile, if one was set on this device.
    pub fn identity_profile(&self) -> Res<Option<ProfileSummary>> {
        Ok(self
            .runtime
            .block_on(self.app.identity_profile())?
            .map(Into::into))
    }

    /// Signs a new profile with this device's key and, when signed in, sends it.
    pub fn identity_set_profile(
        &self,
        display_name: String,
        pronouns: String,
        bio: String,
        avatar_url: String,
    ) -> Res<ProfileSummary> {
        Ok(self
            .runtime
            .block_on(
                self.app
                    .identity_set_profile(&display_name, &pronouns, &bio, &avatar_url),
            )?
            .into())
    }

    pub fn identity_link_account(&self, password: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.identity_link_account(&password))?)
    }

    /// Step 1 of linking this device: the request for another device to approve.
    pub fn identity_link_request(&self, label: String) -> Res<String> {
        Ok(self
            .runtime
            .block_on(self.app.identity_link_request(&label))?)
    }

    /// On a device that holds the identity: approves a link request.
    pub fn identity_approve(&self, request: String) -> Res<String> {
        Ok(self.runtime.block_on(self.app.identity_approve(&request))?)
    }

    /// Claims the bundle an approving device uploaded, then signs in.
    pub fn identity_claim(&self, code: String) -> Res<RosterSummary> {
        Ok(self
            .runtime
            .block_on(self.app.identity_claim(&code))?
            .into())
    }

    /// Signs in by identity proof with the roster this device holds.
    pub fn sign_in_identity(&self) -> Res<()> {
        Ok(self.runtime.block_on(self.app.sign_in_identity())?)
    }

    /// The roster this device holds.
    pub fn identity_show(&self) -> Res<RosterSummary> {
        Ok(self.runtime.block_on(self.app.identity_show())?.into())
    }

    /// Revokes another device, signed by this one with the password as reauth.
    pub fn identity_revoke(&self, device_id: String, password: String) -> Res<RosterSummary> {
        Ok(self
            .runtime
            .block_on(self.app.identity_revoke(&device_id, &password))?
            .into())
    }

    // ---- Push ----

    /// Registers this device's push token with the instance: `platform`
    /// is `apns` or `fcm` (through the relay) or `unified_push` (the
    /// distributor's endpoint, with its Web Push keys when it gave any,
    /// base64url; empty otherwise).
    pub fn push_subscribe(
        &self,
        platform: String,
        token: String,
        p256dh: String,
        auth: String,
    ) -> Res<()> {
        let kind = match platform.as_str() {
            "apns" => PushKind::RelayApns,
            "fcm" => PushKind::RelayFcm,
            "unified_push" => PushKind::UnifiedPush,
            other => {
                return Err(LumiaError::Other {
                    detail: format!("unknown platform {other}"),
                })
            }
        };
        Ok(self
            .runtime
            .block_on(self.app.push_subscribe(kind, &token, &p256dh, &auth))?)
    }

    /// Removes this device's push subscription of a platform (`apns`,
    /// `fcm` through the relay; `unified_push` served by the instance).
    pub fn push_unsubscribe(&self, platform: String) -> Res<()> {
        let kind = match platform.as_str() {
            "unified_push" => platform,
            _ => format!("relay_{platform}"),
        };
        Ok(self.runtime.block_on(self.app.push_unsubscribe(&kind))?)
    }

    // ---- Personal DMs ----

    /// Whether personal DMs work on this device: it holds an identity.
    pub fn dm_available(&self) -> bool {
        self.app.dm_available()
    }

    /// The personal conversations this device knows.
    pub fn dm_channels(&self) -> Res<Vec<DmChannelSummary>> {
        Ok(self
            .runtime
            .block_on(self.app.dm_channels())?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Opens (or finds) the conversation with `account_ids`.
    pub fn dm_open(&self, account_ids: Vec<String>) -> Res<DmOpened> {
        Ok(self
            .runtime
            .block_on(self.app.dm_open(&account_ids))?
            .into())
    }

    /// Sends a DM request to an account.
    pub fn dm_request(&self, account_id: String) -> Res<DmRequestSummary> {
        Ok(self
            .runtime
            .block_on(self.app.dm_request(&account_id))?
            .into())
    }

    /// Answers a DM request: `accept`, `decline` or `block`.
    pub fn dm_respond(&self, request_id: String, response: String) -> Res<DmResponse> {
        Ok(
            match self
                .runtime
                .block_on(self.app.dm_respond(&request_id, &response))?
            {
                Ok(opened) => DmResponse::Opened {
                    opened: opened.into(),
                },
                Err(request) => DmResponse::Answered {
                    request: request.into(),
                },
            },
        )
    }

    /// Sends a message in a personal DM.
    pub fn dm_send(&self, channel_id: String, body: String) -> Res<()> {
        self.runtime
            .block_on(self.app.dm_send(&channel_id, &body))?;
        Ok(())
    }

    /// Sends a message in a personal DM quoting another, or nothing.
    pub fn dm_send_with(
        &self,
        channel_id: String,
        body: String,
        quote: Option<QuoteSummary>,
    ) -> Res<()> {
        let quote: Option<app::QuoteSummary> = quote.map(Into::into);
        self.runtime
            .block_on(self.app.dm_send_with(&channel_id, &body, quote.as_ref()))?;
        Ok(())
    }

    /// Edits one of this identity's messages in a personal DM.
    pub fn dm_edit(&self, channel_id: String, message_id: String, body: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.dm_edit(&channel_id, &message_id, &body))?)
    }

    /// Adds this identity's reaction to a message of a personal DM, or
    /// takes it away.
    pub fn dm_react(
        &self,
        channel_id: String,
        message_id: String,
        emoji: String,
        remove: bool,
    ) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.dm_react(&channel_id, &message_id, &emoji, remove))?)
    }

    /// Deletes one of this identity's messages in a personal DM.
    pub fn dm_delete(&self, channel_id: String, message_id: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.dm_delete(&channel_id, &message_id))?)
    }

    /// Rejoins a conversation by external commit.
    pub fn dm_rejoin(&self, channel_id: String) -> Res<()> {
        Ok(self.runtime.block_on(self.app.dm_rejoin(&channel_id))?)
    }

    /// The conversation as this device can read it.
    pub fn dm_messages(&self, channel_id: String) -> Res<DmLogSummary> {
        Ok(self
            .runtime
            .block_on(self.app.dm_messages(&channel_id))?
            .into())
    }
}
