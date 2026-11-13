// Channel membership and server DMs (architecture 6.8 and 6.4), as plain
// values the screens draw from and `src/test` checks: what a server's
// sidebar lists, the words a notice, a count, an invitation and a server
// DM's note read as, who the add-people and DM pickers offer, the default
// channels Server settings chooses among, the words a refusal reads as,
// and which channels the gateway is still to be told of. The same rules and
// words as the iOS app's `Membership.swift`.
package chat.lumia.kit

import uniffi.lumia_ffi.ChannelInvitationSummary
import uniffi.lumia_ffi.ChannelSummary
import uniffi.lumia_ffi.MemberSummary

/** A server's DM channel: one of its members with another (architecture 6.4). */
const val SERVER_DM = "SERVER_DM"

/** A server's text channel. */
const val TEXT = "TEXT"

// What a server's sidebar lists

/** The channels of [serverId] this account is in, in sidebar order: neither its server DMs nor anything personal. */
fun sidebarChannels(channels: List<ChannelSummary>, serverId: String): List<ChannelSummary> =
    channels.filter { it.serverId == serverId && it.kind != SERVER_DM && it.kind !in personalKinds }

/** The server DMs this account has in [serverId]. */
fun serverDms(channels: List<ChannelSummary>, serverId: String): List<ChannelSummary> =
    channels.filter { it.serverId == serverId && it.kind == SERVER_DM }

/** The invitations into [serverId]'s channels waiting for this account. */
fun invitationsIn(invitations: List<ChannelInvitationSummary>, serverId: String): List<ChannelInvitationSummary> =
    invitations.filter { it.channel.serverId == serverId }

/** The other person in a server DM; none for one with nobody else in it. */
fun otherIn(channel: ChannelSummary, me: String): String? = channel.participantAccountIds.firstOrNull { it != me }

// Words

/**
 * A notice as a channel's list reads it: [actor] is whoever did it (the
 * message's author), [subject] whom it is about. None for a kind this
 * build does not know, which is not drawn.
 */
fun noticeText(kind: String, actor: String, subject: String): String? = when (kind) {
    "joined" -> "$subject joined"
    "left" -> "$subject left"
    "added" -> "$actor added $subject"
    "removed" -> "$actor removed $subject"
    else -> null
}

/** How many are in a channel, as Browse says it. */
fun memberCount(count: Int): String = if (count == 1) "1 member" else "$count members"

/** What a server DM says once, at the top of its history (architecture 6.4). */
fun serverDmNote(serverName: String): String =
    "Messages here are kept by $serverName and follow its message history settings. For an encrypted conversation, message privately."

/** The bar in place of the composer in a channel this account is not in. */
fun notInChannel(name: String): String = "You’re not in #$name"

/** An invitation as the sidebar lists it. */
fun invitationLine(channelName: String, inviter: String): String = "#$channelName · $inviter invited you"

/** The line a profile card says in place of Message, for someone who takes no server DMs. */
const val TAKES_NO_SERVER_DMS = "Doesn’t take messages here"

// Pickers

/** A person as the pickers name them: the display name, else the nickname, else the handle. */
fun nameOf(member: MemberSummary): String = member.displayName.ifEmpty { member.nickname.ifEmpty { member.handle } }

/** Whether [member] answers [query]: by display name, nickname or handle, in any case; everyone answers an empty one. */
fun answers(member: MemberSummary, query: String): Boolean {
    val q = query.trim().lowercase()
    return q.isEmpty() || listOf(member.displayName, member.nickname, member.handle).any { it.lowercase().contains(q) }
}

/** Who Add people offers: the server's members not in the channel and not this account, answering [query], by name. */
fun addCandidates(members: List<MemberSummary>, inChannel: Set<String>, me: String, query: String): List<MemberSummary> =
    members.filter { it.accountId != me && it.accountId !in inChannel && answers(it, query) }.sortedBy { nameOf(it).lowercase() }

/** The tag Add people puts on someone whose adds become invitations; none for anyone else. */
fun addTag(member: MemberSummary): String? = if (member.channelAddsByInvitation) "Will be invited" else null

/** Who the server DM picker lists: the server's members but this account, answering [query], by name. */
fun dmCandidates(members: List<MemberSummary>, me: String, query: String): List<MemberSummary> =
    members.filter { it.accountId != me && answers(it, query) }.sortedBy { nameOf(it).lowercase() }

/** Whether the picker may start a server DM with [member]: not with someone who turned them off. */
fun takesServerDms(member: MemberSummary): Boolean = !member.serverDmsOff

// Default channels (Server settings)

/**
 * The channels a server's default channels are chosen among: its public
 * text channels, those this account is in ([joined], the snapshot's, in
 * sidebar order) and then those it is not ([browsed], Browse's), once each.
 */
fun defaultChoices(joined: List<ChannelSummary>, browsed: List<ChannelSummary>, serverId: String): List<ChannelSummary> {
    val seen = mutableSetOf<String>()
    return (joined.filter { it.serverId == serverId && it.kind == TEXT && !it.private } + browsed.filter { !it.private })
        .filter { seen.add(it.channelId) }
}

/** The default channels as they stand: those of [choices] marked default. */
fun defaultsOf(choices: List<ChannelSummary>): Set<String> = choices.filter { it.isDefault }.map { it.channelId }.toSet()

// Refusals

const val CANNOT_CREATE_CHANNELS = "You cannot add channels to this server."

/** The words for a refused add; none for a refusal that has no words of its own. */
fun addRefused(code: String?): String? = when (code) {
    "PERMISSION_DENIED", "NOT_FOUND" -> "You cannot add people to this channel."
    "INVALID_ARGUMENT" -> "Someone chosen cannot be added to this channel."
    else -> null
}

/** The words for a refused removal. */
fun removeRefused(code: String?): String? = if (code == "PERMISSION_DENIED") "You cannot remove people from this channel." else null

/** The words for a server DM [name]'s side refused. */
fun serverDmRefused(code: String?, name: String): String? = when (code) {
    "FAILED_PRECONDITION" -> "$name doesn’t take messages here."
    "PERMISSION_DENIED" -> "You cannot start conversations in this server."
    "NOT_FOUND" -> "$name is not in this server."
    else -> null
}

// The gateway

/**
 * The channels of a snapshot the gateway is still to be subscribed to,
 * once its loop is running: each server channel (a server DM among them)
 * that is not in [subscribed]. Personal conversations go by the DM engine.
 */
fun toSubscribe(channels: List<ChannelSummary>, subscribed: Set<String>): List<String> =
    channels.filter { it.kind !in personalKinds && it.channelId !in subscribed }.map { it.channelId }
