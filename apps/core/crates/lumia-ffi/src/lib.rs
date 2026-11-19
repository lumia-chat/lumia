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

#[cfg(target_os = "android")]
mod android;

use std::path::PathBuf;
use std::sync::Arc;

use lumia_client::app::{self, App, EventSink, Opener, Place, StoreName};
use lumia_client::body;
use lumia_client::roles;
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
    /// An identity operation needs a fresh reauth: ask for the password, a
    /// code, a passkey or an OIDC login, then call again.
    #[error("confirm it is you first")]
    ReauthNeeded,
    /// The account has private messages on a device this one is not: offer
    /// link and restore, never a second identity.
    #[error("this account already has private messages set up on another device")]
    AccountHasIdentity,
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
            ClientError::ReauthNeeded => Self::ReauthNeeded,
            ClientError::AccountHasIdentity => Self::AccountHasIdentity,
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
    /// Whether a deleted message keeps its place as "(deleted)" there
    /// (architecture 6.7); off, the core's reads leave deleted ones out.
    pub show_deleted: bool,
    /// How long the server keeps its attachments, in days; 0 for ever
    /// (specs/media.md 4.5).
    pub attachment_retention_days: u32,
    /// A full server lets its oldest attachments go; false, it refuses
    /// uploads.
    pub delete_oldest_when_full: bool,
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
    /// The server's owner, whom a profile card and the settings' member
    /// list mark (architecture 7.5), and whether this account is it.
    pub owner_account_id: String,
    pub is_owner: bool,
    /// Whether it may create roles and put them in order (`MANAGE_ROLES`);
    /// which roles it may change is each role's `can_manage`.
    pub can_manage_roles: bool,
    /// Whether it may remove members and set their titles
    /// (`MANAGE_MEMBERS`); which members is each member's `can_manage`.
    pub can_manage_members: bool,
    /// Whether the tag beside a name there is the member's title; false,
    /// their role.
    pub tag_titles: bool,
    /// The server's roles, highest first, `Everyone` last.
    pub roles: Vec<RoleSummary>,
    /// The offered permissions this account may switch on for a role, by
    /// their wire names.
    pub grantable: Vec<String>,
    /// This account's own roles there, its own title (empty for none), and
    /// whether it is a guest there.
    pub role_ids: Vec<String>,
    pub title: String,
    pub guest: bool,
}

/// A role of a server (architecture 7.5).
#[derive(Debug, Clone, uniffi::Record)]
pub struct RoleSummary {
    pub role_id: String,
    /// "Everyone" for the role every member holds.
    pub name: String,
    /// One of the nine tones a rail folder may have, or empty for none.
    pub color: String,
    /// Lower is higher; the list a server gives is already in order.
    pub position: u32,
    pub is_everyone: bool,
    /// Never a member's tag ("Show as tag" off).
    pub hide_tag: bool,
    /// Every permission it grants, by its wire name.
    pub permissions: Vec<String>,
    /// Whether this account may change it, delete it, give it and take it
    /// away; of `Everyone`, whether it may change its permissions.
    pub can_manage: bool,
}

impl From<roles::RoleSummary> for RoleSummary {
    fn from(r: roles::RoleSummary) -> Self {
        Self {
            role_id: r.role_id,
            name: r.name,
            color: r.color,
            position: r.position,
            is_everyone: r.is_everyone,
            hide_tag: r.hide_tag,
            permissions: r.permissions,
            can_manage: r.can_manage,
        }
    }
}

/// A role as its editor holds it (`change_role`): what the editor opened
/// on, and what it holds at Save.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RoleDraft {
    pub name: String,
    /// One of the nine tones, or empty for none.
    pub color: String,
    /// Never a member's tag ("Show as tag" off).
    pub hide_tag: bool,
    /// The offered permissions switched on, by their wire names.
    pub permissions: Vec<String>,
}

impl From<RoleDraft> for roles::RoleDraft {
    fn from(r: RoleDraft) -> Self {
        Self {
            name: r.name,
            color: r.color,
            hide_tag: r.hide_tag,
            permissions: r.permissions,
        }
    }
}

/// The tag beside a member's name: a role in its colour, or a title in none.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TagSummary {
    pub label: String,
    /// One of the nine tones, or empty for the plain chip.
    pub color: String,
}

impl From<roles::TagSummary> for TagSummary {
    fn from(t: roles::TagSummary) -> Self {
        Self {
            label: t.label,
            color: t.color,
        }
    }
}

/// A heading of a role's editor and the permissions under it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PermissionGroup {
    pub title: String,
    pub permissions: Vec<PermissionChoice>,
}

/// A permission a role's editor offers.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PermissionChoice {
    /// Its wire name, as a role's `permissions` has it.
    pub name: String,
    pub label: String,
    /// What it allows, in a line.
    pub detail: String,
}

impl From<roles::PermissionGroup> for PermissionGroup {
    fn from(g: roles::PermissionGroup) -> Self {
        Self {
            title: g.title,
            permissions: g
                .permissions
                .into_iter()
                .map(|p| PermissionChoice {
                    name: p.name,
                    label: p.label,
                    detail: p.detail,
                })
                .collect(),
        }
    }
}

/// What a server's files take (specs/media.md 4.5).
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct StorageSummary {
    pub used_bytes: u64,
    /// 0 when the instance sets no limit.
    pub max_bytes: u64,
}

impl From<app::StorageSummary> for StorageSummary {
    fn from(s: app::StorageSummary) -> Self {
        Self {
            used_bytes: s.used_bytes,
            max_bytes: s.max_bytes,
        }
    }
}

impl From<app::ServerSummary> for ServerSummary {
    fn from(s: app::ServerSummary) -> Self {
        Self {
            server_id: s.server_id,
            name: s.name,
            icon_url: s.icon_url,
            retention_days: s.retention_days,
            show_deleted: s.show_deleted,
            attachment_retention_days: s.attachment_retention_days,
            delete_oldest_when_full: s.delete_oldest_when_full,
            can_manage: s.can_manage,
            can_manage_channels: s.can_manage_channels,
            can_start_dms: s.can_start_dms,
            can_mention_everyone: s.can_mention_everyone,
            can_manage_messages: s.can_manage_messages,
            channel_adds_by_invitation: s.channel_adds_by_invitation,
            server_dms_off: s.server_dms_off,
            owner_account_id: s.owner_account_id,
            is_owner: s.is_owner,
            can_manage_roles: s.can_manage_roles,
            can_manage_members: s.can_manage_members,
            tag_titles: s.tag_titles,
            roles: s.roles.into_iter().map(Into::into).collect(),
            grantable: s.grantable,
            role_ids: s.role_ids,
            title: s.title,
            guest: s.guest,
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
    /// The rail as this account arranged it, ready to draw: every server
    /// placed once, `servers` in its order (architecture 6.9).
    pub rail: Vec<RailItem>,
}

/// One tile of the rail between Home and +: a server, or a folder of servers
/// (architecture 6.9). Edit a rail with `Client`'s `rail…` calls and keep it
/// with `arrangeRail`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RailItem {
    Server {
        server_id: String,
    },
    Folder {
        folder_id: String,
        /// Empty for none: it reads as its servers (`railFolderWords`).
        name: String,
        /// One of the nine avatar tones.
        tone: String,
        server_ids: Vec<String>,
    },
}

impl From<lumia_client::rail::RailItem> for RailItem {
    fn from(item: lumia_client::rail::RailItem) -> Self {
        use lumia_client::rail::RailItem as R;
        match item {
            R::Server { server_id } => Self::Server { server_id },
            R::Folder {
                folder_id,
                name,
                tone,
                server_ids,
            } => Self::Folder {
                folder_id,
                name,
                tone,
                server_ids,
            },
        }
    }
}

impl From<RailItem> for lumia_client::rail::RailItem {
    fn from(item: RailItem) -> Self {
        match item {
            RailItem::Server { server_id } => Self::Server { server_id },
            RailItem::Folder {
                folder_id,
                name,
                tone,
                server_ids,
            } => Self::Folder {
                folder_id,
                name,
                tone,
                server_ids,
            },
        }
    }
}

fn rail_in(items: Vec<RailItem>) -> Vec<lumia_client::rail::RailItem> {
    items.into_iter().map(Into::into).collect()
}

fn rail_out(items: Vec<lumia_client::rail::RailItem>) -> Vec<RailItem> {
    items.into_iter().map(Into::into).collect()
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
            rail: rail_out(s.rail),
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
    /// Their roles in the server listed, by id, and their title there,
    /// empty for none (architecture 7.5). Empty for a public profile.
    pub role_ids: Vec<String>,
    pub title: String,
    /// Whether they are a guest there (a title, and no roles offered), and
    /// whether they own the server.
    pub guest: bool,
    pub owner: bool,
    /// The tag beside their name there, their role or their title as the
    /// server shows, or none; read the list again when `TagsChanged` says to.
    pub tag: Option<TagSummary>,
    /// Whether this account may set their title and remove them from the
    /// server; its own title is its own whatever this says.
    pub can_manage: bool,
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
            role_ids: m.role_ids,
            title: m.title,
            guest: m.guest,
            owner: m.owner,
            tag: m.tag.map(Into::into),
            can_manage: m.can_manage,
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
    /// What it carries (specs/media.md); empty once deleted.
    #[uniffi(default)]
    pub attachments: Vec<AttachmentSummary>,
    /// Its link cards, in the order their links appear.
    #[uniffi(default)]
    pub cards: Vec<CardSummary>,
}

/// One blob of an attachment or a card, with what opens it: handed back to
/// [`Client::attachment_bytes`] as it was given, never decrypted by a shell.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BlobSummary {
    pub blob_id: String,
    /// The stored ciphertext's length.
    pub size_bytes: u64,
    pub key: Vec<u8>,
    pub nonce: Vec<u8>,
    pub hash: Vec<u8>,
}

impl From<app::BlobSummary> for BlobSummary {
    fn from(b: app::BlobSummary) -> Self {
        Self {
            blob_id: b.blob_id,
            size_bytes: b.size_bytes,
            key: b.key,
            nonce: b.nonce,
            hash: b.hash,
        }
    }
}

impl From<BlobSummary> for app::BlobSummary {
    fn from(b: BlobSummary) -> Self {
        Self {
            blob_id: b.blob_id,
            size_bytes: b.size_bytes,
            key: b.key,
            nonce: b.nonce,
            hash: b.hash,
        }
    }
}

/// An attachment as a message carries it (specs/media.md 5).
#[derive(Debug, Clone, uniffi::Record)]
pub struct AttachmentSummary {
    pub attachment_id: String,
    /// `image`, `gif`, `video`, `audio` or `file`.
    pub kind: String,
    pub name: String,
    pub media_type: String,
    /// After orientation; 0 when unknown.
    pub width: u32,
    pub height: u32,
    /// Video and audio; 0 when unknown.
    pub duration_ms: u32,
    /// ThumbHash, drawn until the thumbnail arrives ([`placeholder_image`]).
    pub placeholder: Vec<u8>,
    pub alt: String,
    /// The file's own size, for a file row.
    pub size_bytes: u64,
    pub main: BlobSummary,
    /// At most 400 px: an image's, a GIF's first frame, a video's poster.
    pub thumbnail: Option<BlobSummary>,
    /// The server let it go (specs/media.md 4.5): drawn as a file row with
    /// its name and "No longer available", opening nothing.
    pub expired: bool,
}

impl From<app::AttachmentSummary> for AttachmentSummary {
    fn from(a: app::AttachmentSummary) -> Self {
        Self {
            attachment_id: a.attachment_id,
            kind: a.kind,
            name: a.name,
            media_type: a.media_type,
            width: a.width,
            height: a.height,
            duration_ms: a.duration_ms,
            placeholder: a.placeholder,
            alt: a.alt,
            size_bytes: a.size_bytes,
            main: a.main.into(),
            thumbnail: a.thumbnail.map(Into::into),
            expired: a.expired,
        }
    }
}

/// A link's card (specs/media.md 6): drawn under the link, named by
/// `host` (never `site_name`), opening `url`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CardSummary {
    pub url: String,
    pub title: String,
    pub description: String,
    pub site_name: String,
    /// The site to show: the link's host.
    pub host: String,
    /// `#rrggbb`, or empty.
    pub color: String,
    /// Server content: the picture's path ([`Client::card_image`]).
    pub image_url: String,
    /// Personal DMs: the picture as a blob ([`Client::attachment_bytes`]).
    pub image: Option<BlobSummary>,
    pub image_width: u32,
    pub image_height: u32,
    /// The page is a video: a play symbol over its picture.
    pub video: bool,
}

impl From<app::CardSummary> for CardSummary {
    fn from(c: app::CardSummary) -> Self {
        Self {
            url: c.url,
            title: c.title,
            description: c.description,
            site_name: c.site_name,
            host: c.host,
            color: c.color,
            image_url: c.image_url,
            image: c.image.map(Into::into),
            image_width: c.image_width,
            image_height: c.image_height,
            video: c.video,
        }
    }
}

/// A link's embed (specs/media.md 6.6): what pressing its card loads in the
/// card's place, from the link in the text, never from the card.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EmbedSummary {
    /// `youtube`, `vimeo`, `twitch`, `soundcloud`, `spotify`, `x`,
    /// `bluesky`, `facebook`, `instagram`, `tiktok` or `reddit`.
    pub provider: String,
    /// The site's name as a person reads it: `YouTube`, `X`.
    pub provider_name: String,
    /// `player` (back to its card when its row scrolls away or the conversation
    /// changes), `audio` (plays on until another starts or the conversation
    /// is left) or `post`.
    pub kind: String,
    /// The address to load; empty for a Bluesky post named by handle.
    pub url: String,
    /// A Bluesky post named by handle: [`Client::resolve_bluesky_handle`]
    /// on the press, then [`bluesky_embed_url`] of the DID and
    /// `bluesky_rkey`. Empty otherwise.
    pub bluesky_handle: String,
    pub bluesky_rkey: String,
    /// Twitch: append `&parent=<the instance's host>` to `url`.
    pub needs_parent: bool,
    /// A player's shape, at the column's width at most and at most 400
    /// tall; both 0 when it has none.
    pub aspect_width: u32,
    pub aspect_height: u32,
    /// The height when it has no shape: fixed (the rest scrolls inside), or
    /// held until the embed reports its own when `resizes`.
    pub fixed_height: u32,
    /// Grows to the height the embed reports by `postMessage`.
    pub resizes: bool,
    /// The widest to draw it: 550 for a post, 0 for the column's width.
    pub max_width: u32,
    /// The press's accessible name before the card's title:
    /// `Play video from YouTube`, `Show post from X`.
    pub label: String,
}

impl From<app::EmbedSummary> for EmbedSummary {
    fn from(e: app::EmbedSummary) -> Self {
        Self {
            provider: e.provider,
            provider_name: e.provider_name,
            kind: e.kind,
            url: e.url,
            bluesky_handle: e.bluesky_handle,
            bluesky_rkey: e.bluesky_rkey,
            needs_parent: e.needs_parent,
            aspect_width: e.aspect_width,
            aspect_height: e.aspect_height,
            fixed_height: e.fixed_height,
            resizes: e.resizes,
            max_width: e.max_width,
            label: e.label,
        }
    }
}

/// An attachment uploaded and waiting in the composer.
#[derive(Debug, Clone, uniffi::Record)]
pub struct AttachmentDraft {
    pub draft_id: String,
    pub channel_id: String,
    pub kind: String,
    pub name: String,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u32,
    pub placeholder: Vec<u8>,
    pub alt: String,
    pub size_bytes: u64,
}

impl From<app::AttachmentDraft> for AttachmentDraft {
    fn from(d: app::AttachmentDraft) -> Self {
        Self {
            draft_id: d.draft_id,
            channel_id: d.channel_id,
            kind: d.kind,
            name: d.name,
            media_type: d.media_type,
            width: d.width,
            height: d.height,
            duration_ms: d.duration_ms,
            placeholder: d.placeholder,
            alt: d.alt,
            size_bytes: d.size_bytes,
        }
    }
}

/// Decoded pixels: RGBA8, row by row, upright.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Pixels {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A file to attach and what the platform knows of it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct AttachmentInput {
    pub bytes: Vec<u8>,
    pub name: String,
    /// Sent as a file: a picture keeps its pixels, loses its metadata.
    #[uniffi(default = false)]
    pub as_file: bool,
    /// The picture decoded by the platform (HEIC, HEIF), upright.
    #[uniffi(default)]
    pub pixels: Option<Pixels>,
    /// A video's poster frame.
    #[uniffi(default)]
    pub poster: Option<Pixels>,
    /// A video's or audio's length, when the platform read it.
    #[uniffi(default)]
    pub duration_ms: Option<u32>,
    /// A video's size after rotation, when the platform read it; 0 if not.
    #[uniffi(default = 0)]
    pub video_width: u32,
    #[uniffi(default = 0)]
    pub video_height: u32,
    /// An image's accessibility text.
    #[uniffi(default = "")]
    pub alt: String,
}

impl From<AttachmentInput> for app::AttachmentInput {
    fn from(i: AttachmentInput) -> Self {
        let pixels = |p: Pixels| app::PixelsInput {
            rgba: p.rgba,
            width: p.width,
            height: p.height,
        };
        Self {
            bytes: i.bytes,
            name: i.name,
            as_file: i.as_file,
            pixels: i.pixels.map(pixels),
            poster: i.poster.map(pixels),
            duration_ms: i.duration_ms,
            video_width: i.video_width,
            video_height: i.video_height,
            alt: i.alt,
        }
    }
}

/// The largest blob of each role the instance takes, in bytes.
#[derive(Debug, Clone, uniffi::Record)]
pub struct UploadLimits {
    pub image_max_bytes: u64,
    pub file_max_bytes: u64,
    pub thumbnail_max_bytes: u64,
}

impl From<app::UploadLimitsSummary> for UploadLimits {
    fn from(l: app::UploadLimitsSummary) -> Self {
        Self {
            image_max_bytes: l.image_max_bytes,
            file_max_bytes: l.file_max_bytes,
            thumbnail_max_bytes: l.thumbnail_max_bytes,
        }
    }
}

/// What was typed into a conversation's composer and not sent, as this
/// device keeps it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DraftSummary {
    /// The words, as typed.
    pub body: String,
    /// The message they were being written in reply to, if any.
    pub reply: Option<QuoteSummary>,
}

impl From<app::DraftSummary> for DraftSummary {
    fn from(d: app::DraftSummary) -> Self {
        Self {
            body: d.body,
            reply: d.reply.map(Into::into),
        }
    }
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

/// A block of a message body as the core reads its Markdown subset
/// (protocol 14): the same grammar on every client.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BodyBlock {
    Para {
        inline: BodyInline,
    },
    /// A fenced code block: the first word after its fence, if any, and
    /// its lines.
    Code {
        lang: Option<String>,
        text: String,
    },
    Quote {
        blocks: Vec<BodyBlock>,
    },
    /// A list, ordered from `start` when it has one.
    List {
        start: Option<u64>,
        items: Vec<BodyListItem>,
    },
}

/// One item of a list: its blocks.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BodyListItem {
    pub blocks: Vec<BodyBlock>,
}

/// A paragraph's text and what styles it. Ranges are UTF-16 code units of
/// `text`, as `NSRange` and `AnnotatedString` count.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BodyInline {
    pub text: String,
    pub runs: Vec<BodyRun>,
    /// Web links, which the client opens.
    pub links: Vec<BodyLink>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BodyRun {
    pub start: u32,
    pub end: u32,
    pub style: BodyStyle,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BodyLink {
    pub start: u32,
    pub end: u32,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BodyStyle {
    Emphasis,
    Strong,
    Strike,
    Code,
    /// The mention at `index` of those the body was read with: its text is
    /// the mention as typed, which the client replaces with the name.
    Mention {
        index: u32,
    },
}

/// How much room a draft has under the instance's limit, as the composer's
/// counter shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct MessageRoom {
    /// Whether the counter shows: from 90% of the limit.
    pub shown: bool,
    /// Characters left, or when negative, how many too long: Send is
    /// refused below zero.
    pub left: i64,
}

impl From<body::DisplayBlock> for BodyBlock {
    fn from(block: body::DisplayBlock) -> Self {
        let blocks = |blocks: Vec<body::DisplayBlock>| blocks.into_iter().map(Into::into).collect();
        match block {
            body::DisplayBlock::Para(inline) => Self::Para {
                inline: BodyInline {
                    text: inline.text,
                    runs: inline
                        .runs
                        .into_iter()
                        .map(|r| BodyRun {
                            start: r.start,
                            end: r.end,
                            style: match r.style {
                                body::DisplayStyle::Emphasis => BodyStyle::Emphasis,
                                body::DisplayStyle::Strong => BodyStyle::Strong,
                                body::DisplayStyle::Strike => BodyStyle::Strike,
                                body::DisplayStyle::Code => BodyStyle::Code,
                                body::DisplayStyle::Mention { index } => {
                                    BodyStyle::Mention { index }
                                }
                            },
                        })
                        .collect(),
                    links: inline
                        .links
                        .into_iter()
                        .map(|l| BodyLink {
                            start: l.start,
                            end: l.end,
                            url: l.url,
                        })
                        .collect(),
                },
            },
            body::DisplayBlock::Code { lang, text } => Self::Code { lang, text },
            body::DisplayBlock::Quote { blocks: inner } => Self::Quote {
                blocks: blocks(inner),
            },
            body::DisplayBlock::List { start, items } => Self::List {
                start,
                items: items
                    .into_iter()
                    .map(|item| BodyListItem {
                        blocks: blocks(item),
                    })
                    .collect(),
            },
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
            attachments: m.attachments.into_iter().map(Into::into).collect(),
            cards: m.cards.into_iter().map(Into::into).collect(),
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

/// What a link request asks to add, shown before approving it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct LinkRequestSummary {
    pub device_id: String,
    /// `native` or `web`.
    pub kind: String,
    pub label: String,
    pub web_origin: String,
}

impl From<app::LinkRequestSummary> for LinkRequestSummary {
    fn from(r: app::LinkRequestSummary) -> Self {
        Self {
            device_id: r.device_id,
            kind: r.kind,
            label: r.label,
            web_origin: r.web_origin,
        }
    }
}

/// Where linking this device by prompt stands: `waiting`, `compare` (show
/// `digits`), `linked`, `declined` or `expired`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct LinkProgress {
    pub stage: String,
    pub digits: String,
}

impl From<app::LinkProgress> for LinkProgress {
    fn from(p: app::LinkProgress) -> Self {
        Self {
            stage: p.stage,
            digits: p.digits,
        }
    }
}

/// Another device asks to be linked: what it would add, and where it stands
/// here (`new`, `waiting`, `compare` with `digits`, or `refused`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct LinkPrompt {
    pub session_id: String,
    pub request: LinkRequestSummary,
    pub stage: String,
    pub digits: String,
}

impl From<app::LinkPrompt> for LinkPrompt {
    fn from(p: app::LinkPrompt) -> Self {
        Self {
            session_id: p.session_id,
            request: p.request.into(),
            stage: p.stage,
            digits: p.digits,
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
    /// What it carries.
    #[uniffi(default)]
    pub attachments: Vec<AttachmentSummary>,
    /// Its link cards: only those under a link its text holds.
    #[uniffi(default)]
    pub cards: Vec<CardSummary>,
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
            attachments: m.attachments.into_iter().map(Into::into).collect(),
            cards: m.cards.into_iter().map(Into::into).collect(),
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
    /// Someone's name or picture changed: the account as others see it.
    /// The kept member lists have it already; what shows the person draws
    /// it again.
    ProfileUpdated {
        account_id: String,
        handle: String,
        display_name: String,
        avatar_url: String,
        pronouns: String,
        bio: String,
    },
    /// What a member's tag is in a server may have changed, after the
    /// `AccountApplied` it came in (architecture 7.5): its roles, some
    /// members' roles or titles, or which of the two it shows. Read the
    /// server's members again (`kept_members`) for their tags.
    TagsChanged {
        server_id: String,
    },
    /// A link session moved, after the `AccountApplied` it came in: this
    /// device's own as the new device, or one that prompts here.
    LinkSessionUpdated {
        session_id: String,
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
        removed: bool,
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
            E::ProfileUpdated {
                account_id,
                handle,
                display_name,
                avatar_url,
                pronouns,
                bio,
            } => Self::ProfileUpdated {
                account_id,
                handle,
                display_name,
                avatar_url,
                pronouns,
                bio,
            },
            E::TagsChanged { server_id } => Self::TagsChanged { server_id },
            E::LinkSessionUpdated { session_id } => Self::LinkSessionUpdated { session_id },
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
            E::Closed { reason, removed } => Self::Closed { reason, removed },
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

/// `body` read into blocks with `mentions` marked, as
/// [`Client::parse_body`], for a view that has no client.
#[uniffi::export]
pub fn parse_body(body: String, mentions: Vec<MentionSummary>) -> Vec<BodyBlock> {
    let ranges: Vec<std::ops::Range<usize>> = mentions
        .iter()
        .map(|m| m.start as usize..m.end as usize)
        .collect();
    body::display(&body, &ranges)
        .into_iter()
        .map(Into::into)
        .collect()
}

/// What a link request, as the new device shows it, asks to add (identity
/// spec 4.2 step 2): read before approving. Nothing is sent.
#[uniffi::export]
pub fn link_request_summary(request: String) -> Result<LinkRequestSummary, LumiaError> {
    Ok(app::App::identity_link_request_summary(&request)?.into())
}

/// The links of a message body that may have cards, at most three, in order
/// (specs/media.md 6.1): what a composer previews with
/// [`Client::card_preview`].
#[uniffi::export]
pub fn card_links(body: String) -> Vec<String> {
    App::card_links(&body)
}

/// The embed of a link in a message's text (specs/media.md 6.6), or none.
/// Nothing is fetched; the card's press loads it.
#[uniffi::export]
pub fn embed_for(url: String) -> Option<EmbedSummary> {
    App::embed_for(&url).map(Into::into)
}

/// The address of a Bluesky post's embed from its account's DID and its
/// record key; none when either is not one.
#[uniffi::export]
pub fn bluesky_embed_url(did: String, rkey: String) -> Option<String> {
    App::bluesky_embed_url(&did, &rkey)
}

/// The origins an embed loads from: what a web view holding one may load.
#[uniffi::export]
pub fn embed_origins() -> Vec<String> {
    App::embed_origins()
}

/// The height a post's embed reported (specs/media.md 6.6), from what its
/// window posted as text (a string as posted, an object as its JSON), held to
/// 50..=5000; none for any other message or site. `provider` is
/// [`EmbedSummary::provider`]. Pass on only messages from the embed's own
/// window and origin.
#[uniffi::export]
pub fn reported_embed_height(provider: String, message: String) -> Option<u32> {
    App::reported_embed_height(&provider, &message)
}

/// What is playing or shown, for the one-at-a-time rules (specs/media.md 5
/// and 6.6).
#[derive(Debug, Clone, uniffi::Record)]
pub struct PlayingSummary {
    /// A site's embed in its card's place, rather than an attachment.
    pub embed: bool,
    /// `video` (an embed's `player` too), `audio` or `post`.
    pub kind: String,
}

/// Whether `starting` stops `other`, already playing or shown: a card
/// pressed stops everything else; an attachment started stops every other
/// attachment and an embed that plays, and leaves a post where it is.
#[uniffi::export]
pub fn playback_stops(starting: PlayingSummary, other: PlayingSummary) -> bool {
    App::playback_stops((starting.embed, &starting.kind), (other.embed, &other.kind))
}

/// Whether what is playing stops when its row scrolls away: a video (an
/// attachment pauses, an embed's player goes back to its card). Audio plays
/// on and a post stays.
#[uniffi::export]
pub fn playback_stops_off_screen(playing: PlayingSummary) -> bool {
    App::playback_stops_off_screen(playing.embed, &playing.kind)
}

/// A picture fitted into the viewer's room, in points (specs/media.md 5).
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct ViewerFrame {
    /// The fitted picture: shrunk into the room, never grown.
    pub width: f32,
    pub height: f32,
    /// The room it zooms in.
    pub room_width: f32,
    pub room_height: f32,
    /// The scale of the picture's own size, a pixel a point: 1 for one that
    /// fits as it is.
    pub own: f32,
    /// The furthest scale.
    pub max_scale: f32,
    /// Where two taps on a fitted picture zoom to.
    pub detail_scale: f32,
}

impl From<lumia_core::viewer::Frame> for ViewerFrame {
    fn from(f: lumia_core::viewer::Frame) -> Self {
        Self {
            width: f.width,
            height: f.height,
            room_width: f.room_width,
            room_height: f.room_height,
            own: f.own,
            max_scale: f.max_scale(),
            detail_scale: f.detail_scale(),
        }
    }
}

impl From<ViewerFrame> for lumia_core::viewer::Frame {
    fn from(f: ViewerFrame) -> Self {
        Self {
            width: f.width,
            height: f.height,
            room_width: f.room_width,
            room_height: f.room_height,
            own: f.own,
        }
    }
}

/// How the viewer's picture is zoomed and moved: `scale` in times the
/// fitted size, `x` and `y` its centre from the room's, in points.
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct ViewerView {
    pub scale: f32,
    pub x: f32,
    pub y: f32,
}

impl From<lumia_core::viewer::View> for ViewerView {
    fn from(v: lumia_core::viewer::View) -> Self {
        Self {
            scale: v.scale,
            x: v.x,
            y: v.y,
        }
    }
}

impl From<ViewerView> for lumia_core::viewer::View {
    fn from(v: ViewerView) -> Self {
        Self {
            scale: v.scale,
            x: v.x,
            y: v.y,
        }
    }
}

/// A picture of `own_width` by `own_height` pixels fitted into the
/// viewer's room, `margin` points kept clear on each side of it.
#[uniffi::export]
pub fn viewer_frame(
    own_width: f32,
    own_height: f32,
    room_width: f32,
    room_height: f32,
    margin: f32,
) -> ViewerFrame {
    App::viewer_frame([own_width, own_height], [room_width, room_height], margin).into()
}

/// The viewer's picture zoomed to `scale` about the point (`x`, `y`), from
/// the room's centre: the point stays where it is, as under a pinch.
#[uniffi::export]
pub fn viewer_zoom_at(
    view: ViewerView,
    scale: f32,
    x: f32,
    y: f32,
    frame: ViewerFrame,
) -> ViewerView {
    App::viewer_zoom_at(view.into(), scale, [x, y], &frame.into()).into()
}

/// The viewer's picture moved by a drag, stopped at its edges.
#[uniffi::export]
pub fn viewer_pan_by(view: ViewerView, dx: f32, dy: f32, frame: ViewerFrame) -> ViewerView {
    App::viewer_pan_by(view.into(), [dx, dy], &frame.into()).into()
}

/// Two taps on the viewer's picture at (`x`, `y`): in to its own size about
/// the point, or back to fitted.
#[uniffi::export]
pub fn viewer_pressed(view: ViewerView, x: f32, y: f32, frame: ViewerFrame) -> ViewerView {
    App::viewer_pressed(view.into(), [x, y], &frame.into()).into()
}

/// The viewer's picture kept inside a frame that changed, as when the
/// screen turns.
#[uniffi::export]
pub fn viewer_held(view: ViewerView, frame: ViewerFrame) -> ViewerView {
    App::viewer_held(view.into(), &frame.into()).into()
}

/// Whether a picture someone sent may be handed to a decoder: its header
/// names a size within the bounds (specs/media.md 5). One that is not is
/// drawn as a file row.
#[uniffi::export]
pub fn attachment_drawable(bytes: Vec<u8>) -> bool {
    App::attachment_drawable(&bytes)
}

/// An attachment's placeholder as a tiny picture (RGBA8) to scale up and
/// draw until its thumbnail arrives; none for an empty or malformed one.
#[uniffi::export]
pub fn placeholder_image(placeholder: Vec<u8>) -> Option<Pixels> {
    App::placeholder_image(&placeholder).map(|p| Pixels {
        rgba: p.rgba,
        width: p.width,
        height: p.height,
    })
}

/// The permissions a role's editor offers, under their headings: the same
/// on every shell (architecture 7.5). Which of them this account may switch
/// on in a server is that server's `grantable`.
#[uniffi::export]
pub fn role_permissions() -> Vec<PermissionGroup> {
    App::role_permissions()
        .into_iter()
        .map(Into::into)
        .collect()
}

/// The words of the recovery phrase's wordlist that begin with `prefix`, at
/// most four: what a phrase field offers as the person types.
#[uniffi::export]
pub fn phrase_completions(prefix: String) -> Vec<String> {
    lumia_core::identity::phrase::completions(&prefix, 4)
        .into_iter()
        .map(str::to_string)
        .collect()
}

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
        // Opened attachments are kept beside the stores until the shell
        // names a cache directory of its platform's.
        app.set_media_cache_dir(lock_dir.join("media"));
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

    /// Whether a server's deleted messages keep their place as "(deleted)"
    /// (architecture 6.7). Returns the snapshot as it then is.
    pub fn set_show_deleted(&self, server_id: String, show: bool) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.set_show_deleted(&server_id, show))?
            .into())
    }

    /// How long a server keeps its attachments in days (0 for ever), and
    /// whether a full one lets its oldest go; `None` leaves one as it is
    /// (specs/media.md 4.5). Returns the snapshot as it then is.
    pub fn set_server_files(
        &self,
        server_id: String,
        attachment_retention_days: Option<u32>,
        delete_oldest_when_full: Option<bool>,
    ) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.set_server_files(
                &server_id,
                attachment_retention_days,
                delete_oldest_when_full,
            ))?
            .into())
    }

    /// What a server's files take, for its settings (`MANAGE_SERVER`).
    pub fn server_storage(&self, server_id: String) -> Res<StorageSummary> {
        Ok(self
            .runtime
            .block_on(self.app.server_storage(&server_id))?
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

    /// A new role in a server, placed last above `Everyone` (architecture
    /// 7.5): `color` one of the nine tones or empty, `hide_tag` for one that
    /// is never a member's tag, `permissions` the offered ones switched on.
    /// Returns the snapshot as it then is.
    pub fn create_role(
        &self,
        server_id: String,
        name: String,
        color: String,
        hide_tag: bool,
        permissions: Vec<String>,
    ) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(
                self.app
                    .create_role(&server_id, &name, &color, hide_tag, &permissions),
            )?
            .into())
    }

    /// What an editor changed of a role, and nothing else of it: `was` is
    /// the role as the editor opened on it, `now` what it holds at Save,
    /// and the difference is what is sent (the name, the colour, the tag
    /// switch, the offered permissions switched on and off). What the
    /// editor did not touch stays as the instance has it, whatever someone
    /// else changed meanwhile. An editor saved as it opened asks nothing.
    /// Returns the snapshot as it then is.
    pub fn change_role(
        &self,
        server_id: String,
        role_id: String,
        was: RoleDraft,
        now: RoleDraft,
    ) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(
                self.app
                    .change_role(&server_id, &role_id, &was.into(), &now.into()),
            )?
            .into())
    }

    /// A role's name, colour, tag switch and permissions, whole; a
    /// permission no editor offers is kept as the role has it, and of
    /// `Everyone` only the permissions change. Returns the snapshot as it
    /// then is. Going, once the shells save through `change_role`.
    pub fn update_role(
        &self,
        server_id: String,
        role_id: String,
        name: String,
        color: String,
        hide_tag: bool,
        permissions: Vec<String>,
    ) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.update_role(
                &server_id,
                &role_id,
                &name,
                &color,
                hide_tag,
                &permissions,
            ))?
            .into())
    }

    /// Deletes a role, from whoever held it too. Returns the snapshot as it
    /// then is.
    pub fn delete_role(&self, server_id: String, role_id: String) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.delete_role(&server_id, &role_id))?
            .into())
    }

    /// A server's roles in the order of `role_ids`: every role but
    /// `Everyone` once, highest first. Returns the snapshot as it then is.
    pub fn reorder_roles(&self, server_id: String, role_ids: Vec<String>) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.reorder_roles(&server_id, &role_ids))?
            .into())
    }

    /// What was changed of a member, in one request the instance does whole
    /// or not at all: the roles to give them (`give`) and to take away
    /// (`take`), their other roles left as the instance has them, and their
    /// title when one is given (empty clears it; none leaves it as it is).
    /// Returns the member as the server now shows them.
    pub fn change_member(
        &self,
        server_id: String,
        account_id: String,
        give: Vec<String>,
        take: Vec<String>,
        title: Option<String>,
    ) -> Res<MemberSummary> {
        Ok(self
            .runtime
            .block_on(self.app.change_member(
                &server_id,
                &account_id,
                &give,
                &take,
                title.as_deref(),
            ))?
            .into())
    }

    /// The roles a member is to hold, the whole set. Returns the member as
    /// the server now shows them. Going, once the shells save through
    /// `change_member`.
    pub fn set_member_roles(
        &self,
        server_id: String,
        account_id: String,
        role_ids: Vec<String>,
    ) -> Res<MemberSummary> {
        Ok(self
            .runtime
            .block_on(
                self.app
                    .set_member_roles(&server_id, &account_id, &role_ids),
            )?
            .into())
    }

    /// A member's title in a server, up to 32 characters; empty clears it.
    /// This account's own is its own to set. Returns the member as the
    /// server now shows them.
    pub fn set_member_title(
        &self,
        server_id: String,
        account_id: String,
        title: String,
    ) -> Res<MemberSummary> {
        Ok(self
            .runtime
            .block_on(self.app.set_member_title(&server_id, &account_id, &title))?
            .into())
    }

    /// Removes a member from a server (`MANAGE_MEMBERS`, and a member below
    /// this account).
    pub fn remove_member(&self, server_id: String, account_id: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.remove_member(&server_id, &account_id))?)
    }

    /// Whether the tag beside a member's name in a server is their title;
    /// false, their role. Returns the snapshot as it then is.
    pub fn set_tag_titles(&self, server_id: String, titles: bool) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.set_tag_titles(&server_id, titles))?
            .into())
    }

    /// The members last listed for a server, from the local store, each
    /// with its tag as the server now shows it; empty when it was never
    /// listed.
    pub fn kept_members(&self, server_id: String) -> Res<Vec<MemberSummary>> {
        Ok(self
            .app
            .kept_members(&server_id)?
            .into_iter()
            .map(Into::into)
            .collect())
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

    /// What was typed into a conversation's composer and not sent, from
    /// the local store; none when nothing is kept. Read when the
    /// conversation comes on screen, at a launch too.
    pub fn draft(&self, channel_id: String) -> Res<Option<DraftSummary>> {
        Ok(self.app.draft(&channel_id)?.map(Into::into))
    }

    /// Keeps what is typed in a conversation's composer, with the message
    /// it is a reply to, in place of what was kept before; words that are
    /// empty or all blank keep nothing. On this device only. Called as the
    /// words are typed, once they rest, and when the conversation leaves
    /// the screen.
    pub fn set_draft(
        &self,
        channel_id: String,
        body: String,
        reply: Option<QuoteSummary>,
    ) -> Res<()> {
        let reply = reply.map(app::QuoteSummary::from);
        Ok(self.app.set_draft(&channel_id, &body, reply.as_ref())?)
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

    /// Keeps `items` as this account's rail, whole (architecture 6.9).
    /// Returns the snapshot as it then is.
    pub fn arrange_rail(&self, items: Vec<RailItem>) -> Res<SnapshotSummary> {
        Ok(self
            .runtime
            .block_on(self.app.arrange_rail(&rail_in(items)))?
            .into())
    }

    /// A new folder's id (UUIDv7).
    pub fn rail_new_folder_id(&self) -> String {
        lumia_client::rail::new_folder_id()
    }

    /// A server or folder moved: into `folder` before `before` (or at its
    /// end), or onto the rail before `before` (or at its end).
    pub fn rail_move_to(
        &self,
        items: Vec<RailItem>,
        id: String,
        folder: Option<String>,
        before: Option<String>,
    ) -> Vec<RailItem> {
        rail_out(lumia_client::rail::move_to(
            &rail_in(items),
            &id,
            folder.as_deref(),
            before.as_deref(),
        ))
    }

    /// `dragged` dropped onto `onto`: a new folder of the two, or into
    /// `onto`'s folder.
    pub fn rail_merge(
        &self,
        items: Vec<RailItem>,
        dragged: String,
        onto: String,
        folder_id: String,
    ) -> Vec<RailItem> {
        rail_out(lumia_client::rail::merge(
            &rail_in(items),
            &dragged,
            &onto,
            &folder_id,
        ))
    }

    /// A server put at the end of a folder.
    pub fn rail_into_folder(
        &self,
        items: Vec<RailItem>,
        server_id: String,
        folder: String,
    ) -> Vec<RailItem> {
        rail_out(lumia_client::rail::into_folder(
            &rail_in(items),
            &server_id,
            &folder,
        ))
    }

    /// A server taken out of its folder, to just after it.
    pub fn rail_out_of_folder(&self, items: Vec<RailItem>, server_id: String) -> Vec<RailItem> {
        rail_out(lumia_client::rail::out_of_folder(
            &rail_in(items),
            &server_id,
        ))
    }

    /// A server in a folder of its own where it is ("New folder").
    pub fn rail_new_folder(
        &self,
        items: Vec<RailItem>,
        server_id: String,
        folder_id: String,
    ) -> Vec<RailItem> {
        rail_out(lumia_client::rail::new_folder(
            &rail_in(items),
            &server_id,
            &folder_id,
        ))
    }

    /// A folder taken away, its servers left in its place.
    pub fn rail_delete_folder(&self, items: Vec<RailItem>, folder: String) -> Vec<RailItem> {
        rail_out(lumia_client::rail::delete_folder(&rail_in(items), &folder))
    }

    /// A folder's name and tone, from Folder settings.
    pub fn rail_set_folder(
        &self,
        items: Vec<RailItem>,
        folder: String,
        name: String,
        tone: String,
    ) -> Vec<RailItem> {
        rail_out(lumia_client::rail::set_folder(
            &rail_in(items),
            &folder,
            &name,
            &tone,
        ))
    }

    /// Move up (`by` -1) or down (+1) where it is.
    pub fn rail_step(&self, items: Vec<RailItem>, id: String, by: i32) -> Vec<RailItem> {
        rail_out(lumia_client::rail::step(&rail_in(items), &id, by))
    }

    /// Where a tile stands, 1-based: on the rail, or in its folder.
    pub fn rail_position_of(&self, items: Vec<RailItem>, id: String) -> Option<u32> {
        lumia_client::rail::position_of(&rail_in(items), &id).and_then(|p| u32::try_from(p).ok())
    }

    /// The folder a server is in, if any.
    pub fn rail_folder_of(&self, items: Vec<RailItem>, server_id: String) -> Option<String> {
        lumia_client::rail::folder_of(&rail_in(items), &server_id)
    }

    /// The tone an id is drawn in (one of the nine), as every shell hashes it.
    pub fn rail_tone_of(&self, id: String) -> String {
        lumia_client::rail::tone_of(&id)
    }

    /// How a folder with no name reads: "Acme, Lumia and 2 more".
    pub fn rail_folder_words(&self, names: Vec<String>) -> String {
        lumia_client::rail::folder_words(&names)
    }

    /// The largest message body the instance takes, in bytes of UTF-8.
    pub fn max_message_bytes(&self) -> u32 {
        self.app.max_message_bytes()
    }

    /// The room `body` has under the instance's limit, for the composer's
    /// counter.
    pub fn message_room(&self, body: String) -> MessageRoom {
        let room = body::room(&body, self.app.max_message_bytes());
        MessageRoom {
            shown: room.shown,
            left: room.left,
        }
    }

    /// `body` read into blocks, with `mentions` (the message's, by their
    /// byte ranges) marked as runs.
    pub fn parse_body(&self, body: String, mentions: Vec<MentionSummary>) -> Vec<BodyBlock> {
        parse_body(body, mentions)
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
                role_ids: m.role_ids,
                title: m.title,
                guest: m.guest,
                ..Default::default()
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

    /// Linking by prompt, as the new device: opens a session, so every device
    /// that holds the identity prompts.
    pub fn link_start(&self, label: String) -> Res<LinkProgress> {
        Ok(self.runtime.block_on(self.app.link_start(&label))?.into())
    }

    /// Where this device's link session stands; takes its next step.
    pub fn link_progress(&self) -> Res<LinkProgress> {
        Ok(self.runtime.block_on(self.app.link_progress())?.into())
    }

    /// Gives up this device's link session.
    pub fn link_cancel(&self) {
        self.runtime.block_on(self.app.link_cancel());
    }

    /// On a device that holds the identity: the link sessions that prompt here.
    pub fn link_prompts(&self) -> Res<Vec<LinkPrompt>> {
        Ok(self
            .runtime
            .block_on(self.app.link_prompts())?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Answers a prompt, so both devices show their digits.
    pub fn link_prompt_answer(&self, session_id: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.link_prompt_answer(&session_id))?)
    }

    /// The digits matched: approves, after the device's own unlock.
    pub fn link_prompt_confirm(&self, session_id: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.link_prompt_confirm(&session_id))?)
    }

    /// "Not me".
    pub fn link_prompt_decline(&self, session_id: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.link_prompt_decline(&session_id))?)
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

    // ---- Setting up (identity spec 4.1, 4.3, 4.7) ----

    /// The identity the signed-in account is bound to here, hex, or none.
    pub fn account_identity(&self) -> Res<Option<String>> {
        Ok(self.runtime.block_on(self.app.account_identity())?)
    }

    /// Whether a reauth token is in hand for identity operations.
    pub fn has_reauth(&self) -> bool {
        self.app.has_reauth()
    }

    /// Reauth by the account's password.
    pub fn reauth_password(&self, password: String) -> Res<()> {
        Ok(self.runtime.block_on(self.app.reauth_password(&password))?)
    }

    /// Reauth by a TOTP code, or by the mailed code when `mailed`.
    pub fn reauth_code(&self, code: String, mailed: bool) -> Res<()> {
        Ok(self.runtime.block_on(self.app.reauth_code(&code, mailed))?)
    }

    /// Mails six digits to the account's address for `reauth_code`.
    pub fn reauth_mail_code(&self) -> Res<()> {
        Ok(self.runtime.block_on(self.app.reauth_mail_code())?)
    }

    /// The URL of a fresh OIDC login landing at `return_to` with `?reauth=`.
    pub fn reauth_oidc_url(&self, return_to: String) -> Res<String> {
        Ok(self
            .runtime
            .block_on(self.app.reauth_oidc_url(&return_to))?)
    }

    /// Holds the reauth token an OIDC login landed with.
    pub fn reauth_take(&self, token: String) {
        self.app.reauth_take(&token);
    }

    /// The phrase kept until confirmed, or none once it is. Shown only after
    /// the device's own unlock.
    pub fn identity_phrase(&self) -> Res<Option<String>> {
        Ok(self.runtime.block_on(self.app.identity_phrase())?)
    }

    /// Whether the phrase still waits to be confirmed (the reminders).
    pub fn identity_phrase_unconfirmed(&self) -> Res<bool> {
        Ok(self
            .runtime
            .block_on(self.app.identity_phrase_unconfirmed())?)
    }

    /// Three positions, 1 to 24, to ask the person for.
    pub fn identity_phrase_quiz(&self) -> Res<Vec<u32>> {
        Ok(self.runtime.block_on(self.app.identity_phrase_quiz())?)
    }

    /// Checks the words for the quiz's positions; right ends the reminders.
    pub fn identity_phrase_confirm(&self, words: Vec<String>) -> Res<bool> {
        Ok(self
            .runtime
            .block_on(self.app.identity_phrase_confirm(words))?)
    }

    /// Sets up private messages here: an identity made unless held, the
    /// account bound to it under the reauth in hand. Returns its id, hex.
    pub fn identity_setup(&self, label: String) -> Res<String> {
        Ok(self.runtime.block_on(self.app.identity_setup(&label))?)
    }

    /// Starting over: the account moved to this device's identity.
    pub fn identity_start_over(&self, label: String) -> Res<String> {
        Ok(self
            .runtime
            .block_on(self.app.identity_start_over(&label))?)
    }

    /// The devices restoring from `phrase` would stop, read first so the
    /// person can keep one they still have.
    pub fn identity_recover_devices(&self, phrase: String) -> Res<Vec<DeviceSummary>> {
        Ok(self
            .runtime
            .block_on(self.app.identity_recover_devices(&phrase))?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Restores from the phrase, keeping the devices in `keep` (their ids).
    pub fn identity_recover_keeping(
        &self,
        phrase: String,
        label: String,
        keep: Vec<String>,
    ) -> Res<RosterSummary> {
        Ok(self
            .runtime
            .block_on(self.app.identity_recover_keeping(&phrase, &label, keep))?
            .into())
    }

    /// Restores the identity from its phrase here and signs in by it.
    pub fn identity_recover(&self, phrase: String, label: String) -> Res<RosterSummary> {
        Ok(self
            .runtime
            .block_on(self.app.identity_recover(&phrase, &label))?
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

    // ---- Attachments and link cards (specs/media.md) ----

    /// Keeps opened attachments in `path` (up to 512 MiB, least recently
    /// used out first) instead of beside the stores: a platform cache
    /// directory the system may clear.
    pub fn set_media_cache_dir(&self, path: String) {
        self.app.set_media_cache_dir(PathBuf::from(path));
    }

    /// What the instance takes of attachments; none when it keeps none, and
    /// the composer then offers no way to attach.
    pub fn upload_limits(&self) -> Res<Option<UploadLimits>> {
        Ok(self
            .runtime
            .block_on(self.app.upload_limits())?
            .map(Into::into))
    }

    /// Makes an attachment of a file and uploads it into the conversation:
    /// processed here, sealed, stored. Answers the draft the composer
    /// shows; a send names it by `draft_id`. Slow for a large file: call it
    /// off the main thread, as every call here.
    pub fn attach(&self, channel_id: String, input: AttachmentInput) -> Res<AttachmentDraft> {
        Ok(self
            .runtime
            .block_on(self.app.attach(&channel_id, input.into()))?
            .into())
    }

    /// Sets a waiting attachment's alt text.
    pub fn set_attachment_alt(&self, draft_id: String, alt: String) -> Res<AttachmentDraft> {
        Ok(self.app.set_attachment_alt(&draft_id, &alt)?.into())
    }

    /// Takes a waiting attachment out of the composer (its blobs off the
    /// instance, best effort).
    pub fn discard_attachment(&self, draft_id: String) {
        self.runtime
            .block_on(self.app.discard_attachment(&draft_id));
    }

    /// Sends a message in a server's channel with its mentions, its quote,
    /// the waiting attachments `attachments` names (draft ids) and the
    /// cards of `card_urls` (the links of [`card_links`] the person kept).
    /// Returns the message id.
    pub fn send_message_with_media(
        &self,
        channel_id: String,
        body: String,
        mentions: Vec<MentionSummary>,
        reply_to: Option<QuoteSummary>,
        attachments: Vec<String>,
        card_urls: Vec<String>,
    ) -> Res<String> {
        let mentions: Vec<app::MentionSummary> = mentions.into_iter().map(Into::into).collect();
        let reply_to: Option<app::QuoteSummary> = reply_to.map(Into::into);
        Ok(self.app.send_message_with_media(
            &channel_id,
            &body,
            &mentions,
            reply_to.as_ref(),
            &attachments,
            &card_urls,
        )?)
    }

    /// Edits one of this account's messages in a server's channel, dropping
    /// the attachments named and, when `card_urls` is given, keeping only
    /// those links' cards. Returns the command id.
    pub fn edit_message_with_media(
        &self,
        channel_id: String,
        message_id: String,
        body: String,
        mentions: Vec<MentionSummary>,
        remove_attachment_ids: Vec<String>,
        card_urls: Option<Vec<String>>,
    ) -> Res<String> {
        let mentions: Vec<app::MentionSummary> = mentions.into_iter().map(Into::into).collect();
        Ok(self.app.edit_message_with_media(
            &channel_id,
            &message_id,
            &body,
            &mentions,
            &remove_attachment_ids,
            card_urls.as_deref(),
        )?)
    }

    /// Sends a message in a personal DM with its quote, the waiting
    /// attachments named and the cards of `card_urls` this device built
    /// ([`Client::card_preview`]). Returns the message id.
    pub fn dm_send_with_media(
        &self,
        channel_id: String,
        body: String,
        quote: Option<QuoteSummary>,
        attachments: Vec<String>,
        card_urls: Vec<String>,
    ) -> Res<String> {
        let quote: Option<app::QuoteSummary> = quote.map(Into::into);
        Ok(self.runtime.block_on(self.app.dm_send_with_media(
            &channel_id,
            &body,
            quote.as_ref(),
            &attachments,
            &card_urls,
        ))?)
    }

    /// Edits one of this identity's messages in a personal DM, dropping the
    /// attachments named and, when `card_urls` is given, keeping only those
    /// links' cards.
    pub fn dm_edit_with_media(
        &self,
        channel_id: String,
        message_id: String,
        body: String,
        remove_attachment_ids: Vec<String>,
        card_urls: Option<Vec<String>>,
    ) -> Res<()> {
        Ok(self.runtime.block_on(self.app.dm_edit_with_media(
            &channel_id,
            &message_id,
            &body,
            &remove_attachment_ids,
            card_urls.as_deref(),
        ))?)
    }

    /// The links a composer previews in a conversation, at most three: none
    /// where it cannot have cards (a server channel of an instance that
    /// makes none), so nothing is shown waiting for one.
    pub fn preview_links(&self, channel_id: String, body: String) -> Vec<String> {
        self.app.preview_links(&channel_id, &body)
    }

    /// The card to show under a link while composing: the instance's in a
    /// server channel, this device's own in a personal one. None when the
    /// page gives none.
    pub fn card_preview(&self, channel_id: String, url: String) -> Res<Option<CardSummary>> {
        Ok(self
            .runtime
            .block_on(self.app.card_preview(&channel_id, &url))?
            .map(Into::into))
    }

    /// An attachment's or a DM card's blob, fetched, opened and kept. Check
    /// [`attachment_drawable`] before decoding a picture.
    pub fn attachment_bytes(&self, blob: BlobSummary) -> Res<Vec<u8>> {
        Ok(self
            .runtime
            .block_on(self.app.attachment_bytes(&blob.into()))?)
    }

    /// A server card's picture, by its `image_url`.
    pub fn card_image(&self, image_url: String) -> Res<Vec<u8>> {
        Ok(self.runtime.block_on(self.app.card_image(&image_url))?)
    }

    /// A Bluesky handle's DID, asked of Bluesky (not the instance) on the
    /// press of a post named by handle ([`EmbedSummary::bluesky_handle`]).
    /// None when Bluesky has no account for it (the card opens the browser);
    /// an error when Bluesky gave no answer, and the press may be made again.
    pub fn resolve_bluesky_handle(&self, handle: String) -> Res<Option<String>> {
        Ok(self
            .runtime
            .block_on(self.app.resolve_bluesky_handle(&handle))?)
    }
}
