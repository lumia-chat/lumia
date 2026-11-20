// The session: one instance's client with the state the screens show,
// kept fresh by the gateway's events the way the web's queries are
// (apps/web/src/client/state.ts) and the Swift LumiaSession is, with who is
// typing in the servers' channels as the desktop shell's session keeps it.
// Compose state, written on the main thread; every client call goes off it.
package chat.lumia.kit

import android.os.SystemClock
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uniffi.lumia_ffi.ClientEvent
import uniffi.lumia_ffi.DeviceSummary
import uniffi.lumia_ffi.DmChannelSummary
import uniffi.lumia_ffi.DmLogSummary
import uniffi.lumia_ffi.DmResponse
import uniffi.lumia_ffi.EmojiSummary
import uniffi.lumia_ffi.InvitePreviewSummary
import uniffi.lumia_ffi.InviteSummary
import uniffi.lumia_ffi.InstanceSummary
import uniffi.lumia_ffi.LinkPrompt
import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.PasskeySummary
import uniffi.lumia_ffi.PermissionGroup
import uniffi.lumia_ffi.ProfileSummary
import uniffi.lumia_ffi.StatusSummary
import uniffi.lumia_ffi.StorageSummary
import uniffi.lumia_ffi.MessageRoom
import uniffi.lumia_ffi.MessageSummary
import uniffi.lumia_ffi.RoleDraft
import uniffi.lumia_ffi.RoleSummary
import uniffi.lumia_ffi.RosterSummary
import uniffi.lumia_ffi.TagLook
import uniffi.lumia_ffi.rolesTagLook
import uniffi.lumia_ffi.SnapshotSummary
import uniffi.lumia_ffi.ChannelSummary
import uniffi.lumia_ffi.ChannelAddSummary
import uniffi.lumia_ffi.MentionSummary
import uniffi.lumia_ffi.QuoteSummary
import uniffi.lumia_ffi.RailItem
import uniffi.lumia_ffi.ReadStateSummary
import uniffi.lumia_ffi.Client
import uniffi.lumia_ffi.AttachmentInput
import uniffi.lumia_ffi.BlobSummary
import uniffi.lumia_ffi.CardSummary
import uniffi.lumia_ffi.UploadLimits

sealed class ConnectionState(val label: String) {
    data object Idle : ConnectionState("not connected")
    data object Connecting : ConnectionState("connecting")
    data object Connected : ConnectionState("connected")
    data class Reconnecting(val attempt: UInt) : ConnectionState("reconnecting (attempt $attempt)")
    data class Disconnected(val reason: String) : ConnectionState("disconnected: $reason")
    data class Closed(val reason: String) : ConnectionState("closed: $reason")
}

val personalKinds = setOf("PERSONAL_DM", "PERSONAL_GROUP_DM")

/** How soon after a gateway loop's end another end does not start the loop again: the longest the desktop shell waits between its attempts. */
private const val RESTART_EVERY_MS = 30_000L

/** How many of a channel's newest messages are read from the store at a time. */
private const val MESSAGE_PAGE = 200u

/** Where a sign-in through the system browser returns to, with `magic={token}` (architecture 6.3). */
const val BROWSER_SIGN_IN_RETURN = "lumia://sign-in"

/** Where a reauth at the account's OIDC provider returns to, with `reauth={token}` (identity spec 10). */
const val REAUTH_RETURN = "lumia://reauth"

class LumiaSession(
    val client: LumiaClient,
    /**
     * What leaving does about this phone's push subscription, which is
     * remembered where the session cannot reach (the phone's preferences),
     * so `AppStore` gives it: `chosen` when the person signed out, and the
     * session can still ask the instance to drop it; not when the instance
     * ended the session. A session made without one has none to see to.
     */
    private val pushLeft: suspend (chosen: Boolean) -> Unit = {},
    /**
     * What this phone being removed from the account does about what the
     * app keeps of it beyond the core (`ERASED_WITH_THE_DEVICE`, the
     * pictures fetched), which `AppStore` holds.
     */
    private val erased: () -> Unit = {},
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val host: String = client.host

    var signedIn by mutableStateOf(client.signedIn)
        private set
    var hasIdentity by mutableStateOf(client.hasIdentity)
        private set
    var snapshot by mutableStateOf<SnapshotSummary?>(null)
        private set
    var connection by mutableStateOf<ConnectionState>(ConnectionState.Idle)
        private set
    val messages = mutableStateMapOf<String, List<MessageSummary>>()
    var dmChannels by mutableStateOf<List<DmChannelSummary>>(emptyList())
        private set
    val dmLogs = mutableStateMapOf<String, DmLogSummary>()
    var roster by mutableStateOf<RosterSummary?>(null)
        private set
    /** Everyone this account shares a server with, by account id. */
    var people by mutableStateOf<Map<String, MemberSummary>>(emptyMap())
        private set
    /** A server's members, by server id: the names a channel's members list and the pickers show. */
    var serverMembers by mutableStateOf<Map<String, List<MemberSummary>>>(emptyMap())
        private set
    /**
     * Channels opened that the snapshot does not hold (yet): a public one
     * previewed from Browse, a server DM just started. The channel screen
     * finds one here when the snapshot has none by its id.
     */
    val previews = mutableStateMapOf<String, ChannelSummary>()
    /** The server channels the gateway loop has been told of, so one gained later is subscribed to once. */
    private val subscribed = mutableSetOf<String>()
    /** Public profiles looked up one at a time; null once the instance refused one. */
    private val profiles = mutableStateMapOf<String, MemberSummary?>()
    private val profilesInFlight = mutableSetOf<String>()
    var lastError by mutableStateOf<String?>(null)
        private set
    private var events: Job? = null

    // Sign in

    suspend fun instanceInfo(): InstanceSummary = client.call { instanceInfo() }

    /** Asks the instance to mail a sign-in link to [email]. */
    suspend fun requestMagicLink(email: String) { client.call { requestMagicLink(email) } }

    /** Signs in with the token from a magic-link mail (the code, or the whole link). */
    suspend fun signInMagicLink(token: String) {
        client.call { signInMagicLink(token) }
        signedIn = true
    }

    suspend fun signIn(handle: String, password: String, totpCode: String?) {
        client.call { signIn(handle, password, totpCode) }
        signedIn = true
    }

    /** With an invite link or code, the account joins its server at creation. */
    suspend fun createAccount(handle: String, password: String, invite: String? = null) {
        client.call { createAccount(handle, password, invite) }
        signedIn = true
    }

    // Invites (architecture 6.1)

    /** What an invite link or code leads to, before anything is sent. */
    suspend fun invitePreview(linkOrCode: String): InvitePreviewSummary = client.call { invitePreview(linkOrCode) }

    /** An invite link for a server this account can invite to: unlimited unless [maxUses], open-ended unless [expiresInHours]. */
    suspend fun createInvite(serverId: String, maxUses: UInt = 0u, expiresInHours: UInt? = null): InviteSummary = client.call { createInvite(serverId, maxUses, expiresInHours) }

    /** A one-use invite good for a week, mailed to the address as a link (architecture 6.2). */
    suspend fun emailInvite(serverId: String, email: String): InviteSummary = client.call { emailInvite(serverId, email) }

    /** The server's invites, newest first, for a member who may invite. */
    suspend fun invites(serverId: String): List<InviteSummary> = client.call { invites(serverId) }

    /** Revokes an invite this account made, or any with manage_server. */
    suspend fun revokeInvite(linkOrCode: String) { client.call { revokeInvite(linkOrCode) } }

    /** Joins the invite's server and refreshes the snapshot so it is listed; returns its id. */
    suspend fun acceptInvite(linkOrCode: String): String {
        val serverId = client.call { acceptInvite(linkOrCode) }
        refreshSnapshot()
        return serverId
    }

    // Servers and channels

    /** A server this account owns, listed once the snapshot is refreshed; returns its id. */
    suspend fun createServer(name: String): String {
        val server = client.call { createServer(name) }
        refreshSnapshot()
        return server.serverId
    }

    /** A text channel in the server, private when asked (architecture 6.8), listed once the snapshot is refreshed; returns its id. */
    suspend fun createChannel(serverId: String, name: String, private: Boolean = false): String {
        val channel = client.call { createChannel(serverId, name, private) }
        refreshSnapshot()
        return channel.channelId
    }

    // Channel membership (architecture 6.8)

    /** A channel by id: the snapshot's, else one opened without being in it. */
    fun channelOf(channelId: String): ChannelSummary? =
        snapshot?.channels?.firstOrNull { it.channelId == channelId } ?: previews[channelId]

    /** Whether this account is in a channel: the snapshot holds the channels it is in. */
    fun isIn(channelId: String): Boolean = snapshot?.channels?.any { it.channelId == channelId } == true

    /** The public channels of a server this account is not in, each with how many are in it. */
    suspend fun browseChannels(serverId: String): List<ChannelSummary> = client.call { browseChannels(serverId) }

    /**
     * Opens a channel this account is not in, read-only (as `READ_HISTORY`
     * allows): kept among the previews and subscribed to, so its messages
     * come as any channel's do.
     */
    fun preview(channel: ChannelSummary) {
        previews[channel.channelId] = channel
        if (events != null && subscribed.add(channel.channelId)) {
            scope.launch { runCatching { client.call { subscribe(channel.channelId) } } }
        }
    }

    /** Joins a public channel, or one this account was invited to; the snapshot it answers with lists it. */
    suspend fun joinChannel(channelId: String) { took(client.call { joinChannel(channelId) }) }

    /** Leaves a channel, a default one included. */
    suspend fun leaveChannel(channelId: String) { took(client.call { leaveChannel(channelId) }) }

    /** No to an invitation. */
    suspend fun dismissInvitation(channelId: String) { took(client.call { dismissChannelInvitation(channelId) }) }

    /** Who is in a channel, as account ids. */
    suspend fun channelMembers(channelId: String): List<String> = client.call { channelMembers(channelId) }

    /** Members of the server added to a channel, or invited when they take adds that way. */
    suspend fun addChannelMembers(channelId: String, accountIds: List<String>): ChannelAddSummary = client.call { addChannelMembers(channelId, accountIds) }

    /** Someone taken out of a channel (`MANAGE_CHANNELS`). */
    suspend fun removeChannelMember(channelId: String, accountId: String) { client.call { removeChannelMember(channelId, accountId) } }

    /** This account's own switches in a server; null leaves one as it is. */
    suspend fun updateMembership(serverId: String, channelAddsByInvitation: Boolean?, serverDmsOff: Boolean?) {
        took(client.call { updateMembership(serverId, channelAddsByInvitation, serverDmsOff) })
    }

    /** The server's default channels, exactly [channelIds]. */
    suspend fun setDefaultChannels(serverId: String, channelIds: List<String>) {
        took(client.call { setDefaultChannels(serverId, channelIds) })
    }

    /**
     * The 1:1 server DM with a member, started if there is none (architecture
     * 6.4): kept among the previews until the snapshot has it, and subscribed
     * to; returns its id.
     */
    suspend fun openServerDm(serverId: String, accountId: String): String {
        val channel = client.call { openServerDm(serverId, accountId) }
        preview(channel)
        refreshSnapshot()
        return channel.channelId
    }

    // Server settings (architecture 6.7): each call answers with the
    // snapshot as it then is, which is taken as it comes; other members hear
    // of the change through their account logs.

    /** A server's name, how long it keeps its messages in days (0 for ever), or both; null leaves one as it is. */
    suspend fun updateServer(serverId: String, name: String?, retentionDays: Int?) {
        took(client.call { updateServer(serverId, name, retentionDays?.toUInt()) })
    }

    /** A file's bytes as chosen, made the server's picture by the instance. */
    suspend fun setServerIcon(serverId: String, picture: ByteArray, mediaType: String) {
        took(client.call { setServerIcon(serverId, picture, mediaType) })
    }

    /** The server goes back to having no picture. */
    suspend fun removeServerIcon(serverId: String) {
        took(client.call { removeServerIcon(serverId) })
    }

    /** Whether the server's deleted messages keep their place as "(deleted)"; off, they are gone from its conversations. */
    suspend fun setShowDeleted(serverId: String, show: Boolean) {
        took(client.call { setShowDeleted(serverId, show) })
    }

    /** How long a server keeps its attachments (0 for ever) and whether a full one lets its oldest go; none leaves one as it is (specs/media.md 4.5). */
    suspend fun setServerFiles(serverId: String, attachmentRetentionDays: Int?, deleteOldestWhenFull: Boolean?) {
        took(client.call { setServerFiles(serverId, attachmentRetentionDays?.toUInt(), deleteOldestWhenFull) })
    }

    /** What a server's files take, for its settings; only someone who may change them is answered (`MANAGE_SERVER`). */
    suspend fun serverStorage(serverId: String): StorageSummary = client.call { serverStorage(serverId) }

    // Roles, titles and tags (architecture 7.5). The core holds every rule:
    // what this account may do is in the snapshot's servers and in each
    // member, and a tag is the member's own `tag`, already worked out. A
    // change to a server's roles answers with the snapshot, a change to a
    // member with that member; either way the members are then read again
    // from what the core kept (`keptMembers`, no request), since a tag may
    // have changed with it. Someone else's change comes as `TagsChanged`.

    /** The permissions a role's editor offers, under their headings: the core's list, the same on every client; none when it cannot be read. */
    val rolePermissions: List<PermissionGroup> by lazy { runCatching { uniffi.lumia_ffi.rolePermissions() }.getOrDefault(emptyList()) }

    /** A new role, placed last above Everyone by the instance; answers its id, by its name, which is its alone in the server whatever its capitals. */
    suspend fun createRole(serverId: String, draft: RoleDraft): String? {
        val name = draft.name.trim()
        val snap = client.call { createRole(serverId, name, draft.color, draft.hideTag, draft.permissions) }
        took(snap)
        refreshTags(serverId)
        return snap.servers.firstOrNull { it.serverId == serverId }?.roles?.firstOrNull { !it.isEveryone && it.name.lowercase() == name.lowercase() }?.roleId
    }

    /**
     * What an editor changed of a role, and nothing else of it: [was] is
     * the role as the editor opened on it and [now] what it holds at Save,
     * and the core sends what differs (`changeRole`), so whatever someone
     * else made of the role meanwhile stays. Answers the role as the
     * snapshot it answered with has it, which the editor then opens on;
     * none when it is gone.
     */
    suspend fun changeRole(serverId: String, roleId: String, was: RoleDraft, now: RoleDraft): RoleSummary? {
        val snap = client.call { changeRole(serverId, roleId, was, now) }
        took(snap)
        refreshTags(serverId)
        return snap.servers.firstOrNull { it.serverId == serverId }?.roles?.firstOrNull { it.roleId == roleId }
    }

    /** A role gone, from whoever held it too. */
    suspend fun deleteRole(serverId: String, roleId: String) {
        took(client.call { deleteRole(serverId, roleId) })
        refreshTags(serverId)
    }

    /** A server's roles in the order of [roleIds]: every role but Everyone once, highest first. */
    suspend fun reorderRoles(serverId: String, roleIds: List<String>) {
        took(client.call { reorderRoles(serverId, roleIds) })
        refreshTags(serverId)
    }

    /** Whether the tag beside a name in a server is the member's title; false, their role. */
    suspend fun setTagTitles(serverId: String, titles: Boolean) {
        took(client.call { setTagTitles(serverId, titles) })
        refreshTags(serverId)
    }

    /**
     * What was changed of a member, in one request the instance does whole
     * or not at all (the core's `changeMember`): the roles to [give] them
     * and to [take] away, their others left as the instance has them, and
     * their [title] (empty for none; none leaves it as it is), this
     * account's own being its own to set. Never the whole set of roles a
     * list a moment old shows them with.
     */
    suspend fun changeMember(serverId: String, accountId: String, give: List<String> = emptyList(), take: List<String> = emptyList(), title: String? = null) {
        client.call { changeMember(serverId, accountId, give, take, title) }
        memberChanged(serverId, accountId)
    }

    /** Someone removed from a server (`MANAGE_MEMBERS`, a member below this account). */
    suspend fun removeMember(serverId: String, accountId: String) {
        client.call { removeMember(serverId, accountId) }
        refreshTags(serverId)
    }

    /** A member as the core now keeps them; this account's own roles and title are the snapshot's too, which is taken again. */
    private suspend fun memberChanged(serverId: String, accountId: String) {
        refreshTags(serverId)
        if (accountId == me) refreshSnapshot()
    }

    /**
     * A server's members read again from what the core kept, each with
     * their tag as the server now shows it: after a change made here, and
     * when the core says one came in (`TagsChanged`, after the
     * `AccountApplied` it came in, the snapshot taken again by then).
     * Nothing is asked of the instance. A list never read is left for the
     * snapshot's own read of it.
     */
    private suspend fun refreshTags(serverId: String) {
        if (leaving || !signedIn || serverId !in serverMembers) return
        val kept = runCatching { client.call { keptMembers(serverId) } }.getOrNull() ?: return
        if (kept.isEmpty()) return
        val byServer = serverMembers + (serverId to kept)
        serverMembers = byServer
        people = (snapshot?.servers?.map { it.serverId } ?: byServer.keys.toList()).flatMap { byServer[it].orEmpty() }.associateBy { it.accountId }
    }

    /** A member of a server as the list held has them. */
    fun memberOf(serverId: String, accountId: String): MemberSummary? = serverMembers[serverId]?.firstOrNull { it.accountId == accountId }

    /** The tag beside a member's name in a server, their role or their title as the server shows; none for none. */
    fun tagOf(serverId: String, accountId: String): TagLook? = rolesTagLook(memberOf(serverId, accountId)?.tag)

    // A server's own emoji (specs/emoji.md). The core holds them, from the
    // snapshot and the account log, and every rule: which a typed prefix
    // matches, the refs a send carries (found by the core from the words),
    // and the custom reactions kept by id. A change made here answers with
    // the snapshot; one made elsewhere comes as `EmojiChanged`, after the
    // `AccountApplied` whose snapshot the session has taken by then.

    /** A server's live emoji, oldest first, as the snapshot has them; none for a server not held. */
    fun serverEmoji(serverId: String?): List<EmojiSummary> =
        serverId?.let { id -> snapshot?.servers?.firstOrNull { it.serverId == id }?.emoji } ?: emptyList()

    /** A server's emoji as drawn, by id: each with its picture's address (only the instance's own, `pictureOf`). */
    fun drawnEmoji(serverId: String?): Map<String, DrawnEmoji> =
        serverEmoji(serverId).mapNotNull { drawnEmoji(it, ::pictureOf) }.associateBy { it.id }

    /** The server's emoji the `:word` typed after a colon matches, in the order the composer lists them (the core's; nothing under two characters). */
    fun emojiMatches(serverId: String, prefix: String): List<EmojiSummary> = client.now { emojiMatches(serverId, prefix) }

    /** A picture's bytes as chosen, made one of the server's emoji by the instance under [name]. */
    suspend fun uploadEmoji(serverId: String, name: String, picture: ByteArray, mediaType: String) {
        took(client.call { uploadEmoji(serverId, name, picture, mediaType) })
    }

    /** A new name for one of a server's emoji; messages keep theirs and draw it by its new one. */
    suspend fun renameEmoji(serverId: String, emojiId: String, name: String) {
        took(client.call { renameEmoji(serverId, emojiId, name) })
    }

    /** One of a server's emoji removed: its picture goes, its name stays in what used it. */
    suspend fun removeEmoji(serverId: String, emojiId: String) {
        took(client.call { removeEmoji(serverId, emojiId) })
        keepListedEmoji()
    }

    /**
     * A server's emoji changed: read again (the core's `serverEmoji`), the
     * snapshot taken again should it not have them yet; the server's
     * conversations held read again from the core, which has named its
     * reactions anew (one left after a removal reads as the emoji was last
     * called); and the pictures of those no server lists any more let go.
     */
    private suspend fun emojiChanged(serverId: String) {
        if (leaving || !signedIn) return
        val now = runCatching { client.call { serverEmoji(serverId) } }.getOrNull() ?: return
        if (now != serverEmoji(serverId)) refreshSnapshot()
        for (channel in snapshot?.channels.orEmpty()) {
            if (channel.serverId == serverId && channel.channelId in messages) refreshMessages(channel.channelId)
        }
        keepListedEmoji()
    }

    /** Lets go of every emoji picture held that no server of the snapshot lists (specs/emoji.md 7). */
    private fun keepListedEmoji() {
        val listed = snapshot?.servers?.flatMap { s -> s.emoji.mapNotNull { pictureOf(it.imageUrl) } }?.toSet() ?: return
        forgetEmojiPictures { it in listed }
    }

    // The rail (architecture 6.9)

    /** An arrangement sent and not yet answered, drawn in the snapshot's place so a move shows at once. */
    private var railSent by mutableStateOf<List<RailItem>?>(null)
    private var railSends = 0

    /** The rail as drawn: every server placed once, the arrangement last sent while it is on its way, else the snapshot's. */
    val rail: List<RailItem> get() = railSent ?: snapshot?.rail ?: emptyList()

    /** Why the last arrangement sent was not kept, for the rail to say once (`takeRailError`). */
    var railError by mutableStateOf<String?>(null)
        private set

    fun takeRailError(): String? = railError.also { railError = null }

    /**
     * An edit of the rail, one of the client's `rail…` calls on the rail as
     * drawn: drawn at once and kept by `arrangeRail`, whose snapshot is
     * taken (the account log carries it to the account's other devices).
     * Returns the rail it made, or null when it changed nothing. Refused or
     * not reached, the snapshot's rail is drawn again and `railError` says
     * why; a later edit sent meanwhile is drawn until it is answered.
     */
    fun arrange(edit: Client.(List<RailItem>) -> List<RailItem>): List<RailItem>? {
        val before = rail
        val next = client.now { edit(before) }
        if (next == before) return null
        railSent = next
        val sent = ++railSends
        scope.launch {
            try {
                val snap = client.call { arrangeRail(next) }
                if (sent == railSends) railSent = null
                took(snap)
            } catch (e: Exception) {
                if (sent == railSends) railSent = null
                if (e.endsSession) sessionEnded() else railError = railRefusal(e)
            }
        }
        return next
    }

    /** One of the client's calls that are values only (`railPositionOf`, `railFolderWords`, …). */
    fun <T> railNow(block: Client.() -> T): T = client.now(block)

    /**
     * The room [draft] has under the instance's limit (the core's
     * `messageRoom`, protocol 14) while its counter shows, from 90% of the
     * limit; null before. Asked at every key, so a draft too short to be
     * near (`mayNearLimit`) is not handed to the core at all.
     */
    fun roomOf(draft: String): MessageRoom? {
        if (!mayNearLimit(draft.length, client.now { maxMessageBytes() })) return null
        return client.now { messageRoom(draft) }.takeIf { it.shown }
    }

    private fun took(snap: SnapshotSummary) {
        if (leaving || !signedIn) return
        val before = snapshot
        snapshot = snap
        subscribeGained()
        scope.launch {
            refreshReads()
            rereadShown(before, snap)
        }
    }

    /**
     * The conversations held of a server whose "Show deleted messages" the
     * snapshot changed, read again from the store, which leaves deleted
     * messages out where it is off: one on screen gains or loses its
     * "(deleted)" lines at once, here and on every member's phone, which
     * has the change by its account log.
     */
    private suspend fun rereadShown(before: SnapshotSummary?, after: SnapshotSummary) {
        val changed = showDeletedChanged(
            before?.servers?.associate { it.serverId to it.showDeleted } ?: return,
            after.servers.associate { it.serverId to it.showDeleted },
        )
        if (changed.isEmpty()) return
        for (channel in after.channels) {
            if (channel.serverId in changed && channel.channelId in messages) refreshMessages(channel.channelId)
        }
    }

    /**
     * The gateway loop subscribes to the channels the snapshot had when it
     * started; one gained since (created, joined, added, a server DM begun
     * or received) is subscribed to as the snapshot that lists it is taken,
     * once, so its messages come live and not only after a restart.
     */
    private fun subscribeGained() {
        val snap = snapshot ?: return
        if (events == null) return
        for (id in toSubscribe(snap.channels, subscribed)) {
            subscribed.add(id)
            scope.launch { runCatching { client.call { subscribe(id) } } }
        }
        // A preview the account has since joined is the snapshot's now.
        for (c in snap.channels) previews.remove(c.channelId)
    }

    /** Where a picture this instance names is, to draw; none for none, or for one that is not the instance's own (`pictureAddress`). */
    fun pictureOf(path: String): String? = pictureAddress(client.baseUrl, path)

    /**
     * Where a person's picture is, to draw over their initials: their own
     * from the snapshot, anyone else's from the servers' member lists or a
     * looked-up profile (`avatarPathOf`, the lookup `nameOf` already asks
     * for); none for none or one not on this instance.
     */
    fun avatarOf(accountId: String): String? =
        pictureOf(avatarPathOf(accountId, me, snapshot?.avatarUrl ?: "", people[accountId], ::profileOf))

    /**
     * Signs out because the person chose to: the phone's push subscription
     * removed, the session revoked on the instance, the tokens and what was
     * held of the account forgotten, and no line on the sign-in screen,
     * which is for a session the instance ended. The identity and this
     * device's keys stay on the phone, as on the web and the desktop.
     * Nothing for a session already over or on its way out.
     */
    suspend fun signOut() {
        if (signedIn) leave(notice = null)
    }

    /**
     * The instance no longer takes this session (it was revoked, or ran
     * out): what the phone held of the account goes, as on signing out, and
     * the sign-in screen, which is what shows once `signedIn` is false, says
     * what happened. Nothing for a session already over on this phone.
     */
    private suspend fun sessionEnded() {
        if (signedIn) leave(notice = SIGNED_OUT)
    }

    /**
     * Another device removed this one from the identity (identity spec
     * 4.4), and the core has erased what it kept and given it a new device
     * id: what the app held goes as for a session that ended, and what it
     * keeps of the account beyond the session too, and the sign-in screen
     * says so (`closedNotice`). Signing in again is as a new device.
     */
    private suspend fun removed(notice: String) {
        erased()
        if (signedIn) leave(notice) else endedNotice = notice
    }

    /** The line for the sign-in screen, taken: the next one starts without it. */
    fun takeEndedNotice(): String? = endedNotice.also { endedNotice = null }

    private var endedNotice: String? = null
    private val departure = Leaving()

    /** Whether the session is on its way out: nothing is subscribed or shown for it meanwhile. */
    val leaving: Boolean get() = departure.underWay

    /**
     * Leaves the session, in `Leaving`'s order and by its rules: push seen
     * to first, the gateway stopped, the tokens gone, the account's state
     * cleared; the stores stay. One at a time: a call refused while this is
     * under way (the snapshot, by the sign-out itself) does not start it
     * again, and leaves the sign-out the person chose without a line.
     */
    private suspend fun leave(notice: String?) {
        departure.leave(
            push = { pushLeft(notice == null) },
            stop = {
                endedNotice = notice
                stop()
            },
            revoke = { client.call { signOut() } },
            forget = {
                signedIn = false
                snapshot = null
                messages.clear()
                dmChannels = emptyList()
                dmLogs.clear()
                roster = null
                people = emptyMap()
                serverMembers = emptyMap()
                peopleWithStart = false
                previews.clear()
                profiles.clear()
                serverPresence.clear()
                lastError = null
                showTyping(Typing())
                readStates = emptyMap()
                badge = 0
                marked.clear()
                mentionLists.clear()
                channelMemberIds.clear()
                messageLimits.clear()
                railSent = null
                railError = null
                hasIdentity = client.hasIdentity
                restoring = null
                privateMessages = null
                phraseUnconfirmed = false
                phraseUnlocked = false
                setupAsked = false
                approval = null
                linking = null
                linkPrompts = emptyList()
                promptShown = null
                promptFailure = null
                promptDone = null
                promptLoop?.cancel()
                promptDoneJob?.cancel()
                attaching.clear()
                uploadLimits = null
                limitsAsked = false
            },
        )
    }

    // Identity

    suspend fun identityLinkRequest(label: String): String = client.call { identityLinkRequest(label) }

    suspend fun identityClaim(code: String) {
        client.call { identityClaim(code) }
        hasIdentity = true
        client.call { signInIdentity() }
        signedIn = true
        identityChanged()
    }

    suspend fun identityRevoke(deviceId: String, password: String) {
        roster = client.call { identityRevoke(deviceId, password) }
    }

    /**
     * Approving another device (identity spec 4.2 steps 3 to 6): none, or
     * the one under way, answered or refused. Held here rather than by the
     * screen, so the phone turned or the screen left while the roster is
     * published keeps the code it answers.
     */
    var approval by mutableStateOf<Approval?>(null)
        private set

    /**
     * Approves [request] (the device's own unlock already given): the
     * roster with the new device published, the bundle uploaded, and the
     * code it answers kept in [approval]; once at a time. In the session's
     * scope, so nothing that goes from the screen stops it half-way.
     */
    fun approve(request: String) {
        if (approval?.underWay == true) return
        val started = Approval(request)
        approval = started
        scope.launch {
            val answered = try {
                Approval(request, code = client.call { identityApprove(request) })
            } catch (e: Exception) {
                Approval(request, error = approveError(e.lumiaCode, e.lumiaMessage))
            }
            // Not when the session was left meanwhile, which forgot it.
            if (approval !== started) return@launch
            approval = answered
            refreshRoster()
        }
    }

    /** The approve screen left: an answered or refused approval is forgotten; one under way is kept for when it is opened again. */
    fun approvalLeft() {
        if (approval?.underWay != true) approval = null
    }

    // Linking by prompt (identity spec 4.2.1), as the new device

    /**
     * This phone's link by prompt, none until the link screen starts one:
     * the session's, so the phone turned keeps it, as the core keeps the
     * session's secret for the process.
     */
    var linking by mutableStateOf<Linking?>(null)
        private set
    private var linkAsking = false

    /**
     * Opens a link session ([label] is this phone's name in the roster):
     * every device of the account that holds the identity prompts. A
     * session already open is given up first. In the session's scope, so
     * nothing that goes from the screen stops it half-way.
     */
    fun linkStart(label: String) {
        if (linking?.stage == LinkStage.STARTING) return
        val started = Linking(LinkStage.STARTING)
        linking = started
        scope.launch {
            val answered = try {
                client.call { linkCancel() }
                val p = client.call { linkStart(label) }
                Linking(linkStageOf(p.stage), p.digits)
            } catch (e: Exception) {
                Linking(LinkStage.FAILED, error = linkStartError(e.lumiaCode, e.lumiaMessage))
            }
            if (linking === started) linking = answered
        }
    }

    /**
     * Asks where the link stands, which also takes this phone's next step
     * (revealing its secret, claiming the code): when the link screen
     * opens, every [LINK_POLL_MS] while it is open, and whenever the
     * session moves. One ask at a time, in the session's scope: a claim cut short
     * would leave the code taken and this phone without the identity.
     * Linked, this phone holds the identity, is signed in by it, and the
     * gateway starts again under that session.
     */
    fun linkFollow() {
        val now = linking ?: return
        if (!now.stage.polls || linkAsking) return
        linkAsking = true
        scope.launch {
            try {
                val p = client.call { linkProgress() }
                val stage = linkStageOf(p.stage)
                // Not when it was cancelled or started again meanwhile; a claim that went through is taken either way.
                if (stage == LinkStage.LINKED) {
                    hasIdentity = true
                    signedIn = true
                    identityChanged()
                }
                if (linking === now || stage == LinkStage.LINKED) linking = Linking(stage, p.digits)
            } catch (e: Exception) {
                if (e.endsSession) sessionEnded()
                else if (linking === now) linking = now.copy(error = e.lumiaMessage)
            } finally {
                linkAsking = false
            }
        }
    }

    /** Gives the link up (the other devices' prompts go), and forgets it here. */
    fun linkCancel() {
        val open = linking?.stage?.let { it.polls || it == LinkStage.STARTING } == true
        linking = null
        if (open) scope.launch { runCatching { client.call { linkCancel() } } }
    }

    /** The linked phone gone on to the app: the link is forgotten. */
    fun linkDone() {
        if (linking?.stage == LinkStage.LINKED) linking = null
    }

    // Linking by prompt (identity spec 4.2.1), on a device that holds the identity

    /** The account's link sessions that prompt here, oldest first; none on a phone without the identity. */
    var linkPrompts by mutableStateOf<List<LinkPrompt>>(emptyList())
        private set
    /** The session the prompt shows, one at a time (`promptToShow`). */
    var promptShown by mutableStateOf<String?>(null)
        private set
    /** Whether a call of the prompt's is under way: its buttons wait. */
    var promptBusy by mutableStateOf(false)
        private set
    /** What the prompt's last call that failed says, under its buttons, until the next. */
    var promptFailure by mutableStateOf<String?>(null)
        private set
    /** "Pixel 9 is linked." or "Linking was stopped.", shown for [PROMPT_DONE_MS] after the prompt's session ended here. */
    var promptDone by mutableStateOf<String?>(null)
        private set
    private var promptsAsking = false
    private var promptLoop: Job? = null
    private var promptDoneJob: Job? = null
    /** Whether the app is in the foreground: an open prompt is asked about every [LINK_POLL_MS] only then. */
    private var inForeground = true

    /** The prompt on screen, while its session is listed. */
    val prompt: LinkPrompt? get() = linkPrompts.firstOrNull { it.sessionId == promptShown }

    /**
     * Asks which link sessions prompt here: at launch, back in the
     * foreground, whenever a link session moves, and every [LINK_POLL_MS] while one
     * is listed and the app is in front. A prompt whose session is no longer
     * listed closes; the next, oldest first, takes its place.
     */
    suspend fun refreshPrompts() {
        if (promptsAsking || !signedIn || leaving) return
        if (!client.hasIdentity) {
            linkPrompts = emptyList()
            promptShown = null
            return
        }
        promptsAsking = true
        try {
            val listed = client.call { linkPrompts() }
            if (!signedIn || leaving) return
            linkPrompts = listed
            val next = promptToShow(listed.map { it.sessionId }, promptShown)
            if (next != promptShown) promptFailure = null
            promptShown = next
        } catch (e: Exception) {
            if (e.endsSession) sessionEnded()
        } finally {
            promptsAsking = false
        }
        keepAskingPrompts()
    }

    private fun keepAskingPrompts() {
        if (promptLoop?.isActive == true || linkPrompts.isEmpty() || !inForeground) return
        promptLoop = scope.launch {
            while (linkPrompts.isNotEmpty() && inForeground && signedIn) {
                delay(LINK_POLL_MS)
                refreshPrompts()
            }
        }
    }

    /** Continue: this phone answers, so the new device reveals and both show the digits. */
    fun promptAnswer(sessionId: String) = promptCall { linkPromptAnswer(sessionId) }

    /**
     * They match (the phone's own unlock already given): the request
     * approved as Approve a device does and its code handed over through the
     * session; then "Pixel 9 is linked." and Devices' roster read again.
     */
    fun promptConfirm(sessionId: String) {
        val p = linkPrompts.firstOrNull { it.sessionId == sessionId } ?: return
        promptCall(done = promptLinkedWords(p.request.kind, p.request.label, p.request.webOrigin)) {
            linkPromptConfirm(sessionId)
        }
    }

    /** Not me, or They don't match ([mismatch]: "Linking was stopped." after). The new device is told it was declined. */
    fun promptDecline(sessionId: String, mismatch: Boolean = false) =
        promptCall(done = if (mismatch) PROMPT_STOPPED else null) { linkPromptDecline(sessionId) }

    private fun promptCall(done: String? = null, call: Client.() -> Unit) {
        if (promptBusy) return
        promptBusy = true
        promptFailure = null
        scope.launch {
            try {
                client.call(call)
                if (done != null) showPromptDone(done)
            } catch (e: Exception) {
                if (e.endsSession) sessionEnded() else promptFailure = promptError(e.lumiaCode, e.lumiaDetails["state"], e.lumiaMessage)
            } finally {
                promptBusy = false
            }
            refreshPrompts()
            if (done != null) refreshRoster()
        }
    }

    private fun showPromptDone(words: String) {
        promptDone = words
        promptDoneJob?.cancel()
        promptDoneJob = scope.launch {
            delay(PROMPT_DONE_MS)
            promptDone = null
        }
    }

    // Setting up private messages (identity spec 4.1, 4.3, 4.7; issue #20)

    /** What this phone needs before it can send a private message; null until asked (`refreshPrivateMessages`). */
    var privateMessages by mutableStateOf<PrivateMessages?>(null)
        private set
    /** Whether this phone keeps a phrase not yet confirmed: Home's reminder and Devices' card. */
    var phraseUnconfirmed by mutableStateOf(false)
        private set
    /**
     * Whether the phrase was unlocked by the phone's own unlock for the
     * screens showing it: for the process, so a turn of the phone does
     * not ask again, and dropped once neither screen is open.
     */
    var phraseUnlocked by mutableStateOf(false)
    /** Whether a screen asked for "Private messages need this phone" (or setting up), which the shell draws over whatever is open. */
    var setupAsked by mutableStateOf(false)

    /** Whether this phone can send private messages; when it cannot, the shell is asked to say what it needs. */
    fun readyForPrivate(): Boolean {
        if (hasIdentity && privateMessages == PrivateMessages.READY) return true
        // Never asked, with the identity held: the account's is the one most likely, and a send says otherwise if not.
        if (hasIdentity && privateMessages == null) return true
        setupAsked = true
        return false
    }

    /** Asks the instance which identity the account is bound to, and the phone whether its phrase waits. */
    suspend fun refreshPrivateMessages() {
        hasIdentity = client.hasIdentity
        val held = if (hasIdentity) runCatching { client.call { identityShow() } }.getOrNull()?.identityId else null
        val bound = runCatching { client.call { accountIdentity() } }.getOrElse { return }
        privateMessages = privateMessagesFor(held, bound)
        phraseUnconfirmed = hasIdentity && runCatching { client.call { identityPhraseUnconfirmed() } }.getOrDefault(false)
    }

    /**
     * Sets up private messages here (identity spec 4.1): an identity made
     * unless this phone holds one, and the account bound to it under the
     * reauth the sign-in carried or one just asked for. Throws
     * `ReauthNeeded` without one, `AccountHasIdentity` for an account
     * bound elsewhere.
     */
    suspend fun identitySetUp(label: String) {
        client.call { identitySetup(label) }
        identityChanged()
    }

    /**
     * After a sign-in, or an account made: set up as sign-up does when the
     * account has no identity, and with no question. Nothing is thrown:
     * what is left to do (a reauth, link or restore) is what Home says.
     */
    suspend fun setUpQuietly(label: String) {
        refreshPrivateMessages()
        if (privateMessages != PrivateMessages.SET_UP) return
        runCatching { identitySetUp(label) }
    }

    /** The phrase kept until confirmed, 24 words separated by spaces, or none once it is. Asked for only after the phone's own unlock. */
    suspend fun identityPhrase(): String? = client.call { identityPhrase() }

    /** The three positions (1 to 24) the check asks for; the same until answered. */
    suspend fun identityPhraseQuiz(): List<Int> = client.call { identityPhraseQuiz() }.map { it.toInt() }

    /** The check answered: true ends the reminders; false means the phrase again, and the next check asks other words. */
    suspend fun identityPhraseConfirm(words: List<String>): Boolean {
        val right = client.call { identityPhraseConfirm(words) }
        if (right) phraseUnconfirmed = false
        return right
    }

    /**
     * A restore between the phrase and "Keep any devices?" (`Restoring`):
     * held here, in memory only, so turning the phone keeps the step and
     * what was ticked; the restore screen drops it as the flow ends or is
     * left, and leaving the session drops it too.
     */
    var restoring by mutableStateOf<Restoring?>(null)

    /** The devices of the phrase's identity that restoring would stop, read from the instance before anything changes. */
    suspend fun identityRecoverDevices(phrase: String): List<DeviceSummary> = client.call { identityRecoverDevices(phrase) }

    /**
     * Restores the identity from its phrase on this phone (identity spec
     * 4.3), keeping the devices in [keep] and revoking every other, and
     * signs in by it. Signed in already, the gateway starts again under
     * the session it took.
     */
    suspend fun identityRecover(phrase: String, label: String, keep: List<String> = emptyList()) {
        client.call { identityRecoverKeeping(phrase, label, keep) }
        signedIn = true
        identityChanged()
    }

    /** Starting over (identity spec 4.7): the account moved to an identity of this phone's, under a reauth in hand. */
    suspend fun identityStartOver(label: String) {
        client.call { identityStartOver(label) }
        identityChanged()
    }

    /** What an identity set up, restored or started over changes: the roster, what the phone needs, and the gateway's subscriptions. */
    private suspend fun identityChanged() {
        hasIdentity = client.hasIdentity
        refreshRoster()
        if (events != null) {
            stop()
            load()
        } else {
            refreshPrivateMessages()
        }
    }

    // Reauth (identity spec 10): a credential proved again, held by the core for the next setup or start over.

    suspend fun reauthPassword(password: String) { client.call { reauthPassword(password) } }
    suspend fun reauthMailCode() { client.call { reauthMailCode() } }
    suspend fun reauthMailed(code: String) { client.call { reauthCode(code, true) } }

    /** Where the system browser logs in at the account's provider afresh, coming back to [REAUTH_RETURN] with `reauth={token}`. */
    suspend fun reauthOidcUrl(): String = client.call { reauthOidcUrl(REAUTH_RETURN) }

    /** The `lumia:` URL the browser came back on, its token held for the next call. */
    fun reauthTake(returnUrl: String) {
        val token = android.net.Uri.parse(returnUrl).getQueryParameter("reauth") ?: return
        client.now { reauthTake(token) }
    }

    // Passkeys, and sign-in through the browser (architecture 6.3)

    suspend fun passkeys(): List<PasskeySummary> = client.call { passkeys() }
    suspend fun removePasskey(id: String) { client.call { removePasskey(id) } }

    /**
     * Where the system browser signs in: [oidc] to the provider, else the
     * instance's web sign-in page (a passkey there, or any method it
     * offers). It comes back to [BROWSER_SIGN_IN_RETURN] with a one-time
     * token, which [completeBrowserSignIn] takes.
     */
    suspend fun browserSignInUrl(oidc: Boolean): String = client.call {
        if (oidc) oidcStartUrl(BROWSER_SIGN_IN_RETURN) else webSignInUrl(BROWSER_SIGN_IN_RETURN)
    }

    /** The `lumia:` URL the browser came back on, completed as a magic-link token. */
    suspend fun completeBrowserSignIn(returnUrl: String) {
        client.call { signInMagicLink(returnUrl) }
        signedIn = true
    }

    /** Where the instance's web page opens signed in as this account by a one-time token, on [next] (`devices` for a passkey). */
    suspend fun webHandoffUrl(next: String): String = client.call { webHandoffUrl(handoffToken(), next) }

    // Profile (architecture 6.4)

    /** The signed profile this identity carries: this device's, else the one the instance holds from another device. */
    suspend fun profile(): ProfileSummary? = client.call { identityProfile() }

    /** A new version, signed by this device and sent to the instance; the snapshot follows, so the name on screen does. */
    suspend fun setProfile(displayName: String, pronouns: String, bio: String): ProfileSummary {
        val current = runCatching { client.call { identityProfile() } }.getOrNull()
        val p = client.call { identitySetProfile(displayName, pronouns, bio, current?.avatarUrl ?: "") }
        refreshSnapshot()
        return p
    }

    suspend fun refreshRoster() {
        hasIdentity = client.hasIdentity
        if (!hasIdentity) { roster = null; return }
        roster = runCatching { client.call { identityShow() } }.getOrNull()
    }

    // Loading

    /**
     * Whether `load` is reading every member list beside a loop it just
     * started, and no connection has been lost or refused since: that
     * loop's first welcome then reads none of them a second time. Any
     * later welcome is a reconnect, which may have missed who came and
     * went, and reads them all.
     */
    private var peopleWithStart = false

    /**
     * Loads the snapshot and starts the gateway on every server channel.
     * The loop is started as soon as the snapshot says which channels, and
     * before anything else is asked of the instance: until it runs nothing
     * on the phone is live and the core takes no send ("not started"), and
     * what else a launch reads is not quick, every server's members being
     * up to fifty requests a server. Those are read beside the rest, in the
     * session's own time: this answers without them, and each list is
     * drawn as it comes.
     */
    suspend fun load() {
        try {
            val snap = client.call { snapshot() }
            // Answered while the session was being left (asked just before, by a
            // screen made again or the app coming forward): nothing of the account
            // is taken up again, and the gateway the leaving stopped stays stopped.
            if (leaving || !signedIn) return
            snapshot = snap
            hasIdentity = client.hasIdentity
            val starting = events == null
            if (starting) {
                peopleWithStart = true
                // With any channel opened to read before the loop ran.
                val held = snap.channels.filter { it.kind !in personalKinds }.map { it.channelId }
                start(held + previews.keys.filter { it !in held })
            } else {
                subscribeGained()
            }
            // Every list beside a loop just started. One already running (a screen made
            // again, the phone turned) has kept the lists it holds as they are told, so
            // only one not held is read.
            readPeopleBeside(snap, everyone = starting)
            refreshPrivateMessages()
            // A device asking to be linked while this phone was closed prompts as it opens.
            scope.launch { refreshPrompts() }
            refreshReads()
            refreshDmChannels()
        } catch (e: Exception) {
            // A session that is over goes back to sign-in. Anything else is said
            // only with nothing held to show; what is held stays on screen.
            if (e.endsSession) sessionEnded() else if (snapshot == null) lastError = e.lumiaMessage
        }
    }

    /**
     * The snapshot again, after something that changed it. A refusal that
     * says the session is over ends it here as it does in `load`; any other
     * failure says nothing and leaves what is held on screen.
     */
    /**
     * Reads the snapshot again. Member lists are read only for servers newly
     * in it, or for every server when [everyone]: a reconnect may have missed
     * what changed. A name or picture changed since arrives by itself
     * (`ProfileUpdated`), so another account event (a read on another
     * device, say) costs no list.
     */
    suspend fun refreshSnapshot(everyone: Boolean = false) {
        val snap = try {
            client.call { snapshot() }
        } catch (e: Exception) {
            if (e.endsSession) sessionEnded()
            return
        }
        if (leaving || !signedIn) return
        val before = snapshot
        snapshot = snap
        subscribeGained()
        refreshReads()
        rereadShown(before, snap)
        readPeopleBeside(snap, everyone)
    }

    /**
     * The member lists of a snapshot's servers read in the session's own
     * time, which whoever took the snapshot does not wait for. The
     * gateway's events are handled one at a time, each after the one
     * before, and a welcome and every account event take the snapshot:
     * waited for there, a list of thousands held back every message that
     * arrived behind it.
     */
    private fun readPeopleBeside(snap: SnapshotSummary, everyone: Boolean) {
        val ids = snap.servers.map { it.serverId }
        scope.launch { refreshPeople(ids, everyone) }
    }

    /** How many of a channel's newest messages are read from the store: a page, and more once a jump looks further back. */
    private val messageLimits = mutableMapOf<String, UInt>()

    suspend fun refreshMessages(channelId: String) {
        val limit = messageLimits[channelId] ?: MESSAGE_PAGE
        runCatching { client.call { messages(channelId, limit) } }.getOrNull()?.let { messages[channelId] = it }
    }

    suspend fun refreshDmChannels() {
        if (!client.dmAvailable) return
        runCatching { client.call { dmChannels() } }.getOrNull()?.let { dmChannels = it }
    }

    suspend fun refreshDmLog(channelId: String) {
        if (!client.dmAvailable) return
        runCatching { client.call { dmMessages(channelId) } }.getOrNull()?.let { dmLogs[channelId] = it }
    }

    /** The reads of member lists, one at a time: a screen that asks for a list while the snapshot's own read of it is on its way waits for that one and asks nothing. */
    private val peopleReads = Mutex()

    /**
     * The member lists of [serverIds], read again for [everyone] of them
     * or only where none is held, and the rest kept; a server left goes.
     * Which are not held is asked once the read before this one is over,
     * so two asked for together (a launch's and its first event's) read
     * each list once. A list that could not be read is as it was held, and
     * not there where none was (`membersAfter`): a server whose members
     * are not known has no list, and what counts them (a role's holders,
     * Members' heading) says nothing instead of none.
     */
    private suspend fun refreshPeople(serverIds: List<String>, everyone: Boolean) = peopleReads.withLock {
        readPeople(serverIds, if (everyone) serverIds else serverIds.filter { it !in serverMembers })
    }

    private suspend fun readPeople(serverIds: List<String>, fetch: List<String>) {
        val read = fetch.associateWith { id -> runCatching { client.call { members(id, "") } }.getOrNull() }
        if (leaving || !signedIn) return
        // Over what is held now, which a tag or a name changed meanwhile may have written, and for the
        // servers the snapshot has now: one joined or left while the lists were read is the next read's.
        val servers = snapshot?.servers?.map { it.serverId } ?: serverIds
        val byServer = membersAfter(serverMembers, servers, read)
        serverMembers = byServer
        people = servers.flatMap { byServer[it].orEmpty() }.associateBy { it.accountId }
    }

    /**
     * A server's members asked for where the phone holds no list of them,
     * its read with the snapshot having failed: the screens that count
     * them (Roles, a role's editor, Members) ask as they open, so
     * "Reading the members…" is true and a count is not unknown for good.
     * With a list held, or one on its way, nothing is asked.
     */
    suspend fun askMembers(serverId: String) {
        if (leaving || !signedIn || serverId in serverMembers) return
        peopleReads.withLock {
            val ids = snapshot?.servers?.map { it.serverId } ?: return
            if (serverId in ids && serverId !in serverMembers) readPeople(ids, listOf(serverId))
        }
    }

    // Actions

    /**
     * Sends a message to a server's channel or server DM, quoting [reply]
     * or nothing, with the mentions its body makes of the server's members
     * (and of everyone, for one who may), as the core finds them; a server
     * DM mentions nobody, both being told of every message anyway. Answers
     * who it mentioned that is not in the channel, so not notified, as far
     * as who is in it is known (`loadChannelMembers`). It answers when the
     * core's send has, without error, and throws when that did: the
     * composer's words and their draft are let go on the one and kept on
     * the other (`DraftKeeper.sending`). The channel is read again from the
     * store in the session's own time, which the answer does not wait for.
     */
    suspend fun sendMessageWith(channelId: String, body: String, reply: QuoteSummary?, cardUrls: List<String> = emptyList()): List<String> {
        val mentions = mentionsIn(channelId, body)
        val outside = outsideOf(mentions, channelMemberIds[channelId], me)
        // The waiting attachments ready to go (`draftIds`) and the links whose cards were kept.
        val going = readyAttachments(channelId)
        if (going.isEmpty() && cardUrls.isEmpty()) {
            client.call { sendMessageWith(channelId, body, mentions, reply) }
        } else {
            client.call { sendMessageWithMedia(channelId, body, mentions, reply, draftIds(going), cardUrls) }
            sent(channelId, going)
        }
        scope.launch { refreshMessages(channelId) }
        return outside
    }

    /** Sends to a personal conversation, quoting [quote] or nothing (inside the encryption), with its attachments and cards as a channel's, and answering as a channel's send does. */
    suspend fun dmSendWith(channelId: String, body: String, quote: QuoteSummary?, cardUrls: List<String> = emptyList()) {
        val going = readyAttachments(channelId)
        if (going.isEmpty() && cardUrls.isEmpty()) {
            client.call { dmSendWith(channelId, body, quote) }
        } else {
            client.call { dmSendWithMedia(channelId, body, quote, draftIds(going), cardUrls) }
            sent(channelId, going)
        }
        scope.launch { refreshDmLog(channelId) }
    }

    // Drafts (issue #23)

    /**
     * The store's draft calls, one at a time and in the order they were
     * asked, off the main thread: a draft kept as a conversation leaves the
     * screen is in the store before the next screen reads one, and a send's
     * clearing is never overtaken by a save handed over before it.
     */
    private val draftCalls = Dispatchers.IO.limitedParallelism(1)

    /**
     * What keeps the draft of a conversation while its screen is open
     * (`DraftKeeper`): a server's channel, a server DM or a personal
     * conversation, by its channel id, in the core's local store and
     * nowhere else. The saves are the session's own work, so one handed
     * over as the screen goes is not cut short with it, and a failure of
     * the store is said nowhere. Nothing is kept once the session is over,
     * a screen still open then flushing into nothing: the core forgets
     * every draft at a sign-out and at a removed device's erasure, and what
     * was typed is not for whoever signs in next.
     */
    fun draftKeeper(channelId: String): DraftKeeper = DraftKeeper(
        read = { withContext(draftCalls) { client.now { draft(channelId) } } },
        keep = { body, reply -> if (signedIn) scope.launch(draftCalls) { runCatching { client.now { setDraft(channelId, body, reply) } } } },
        after = { ms, run -> scope.launch { delay(ms); run() }.let { job -> { job.cancel() } } },
        // The saves go one at a time in the order asked: whatever is asked after them comes back once they are in.
        settled = { withContext(draftCalls) {} },
    )

    // Attachments and link cards (specs/media.md)

    /** What the instance takes of attachments, once asked (`askUploadLimits`); none when it keeps none, and the composer then offers no way to attach. */
    var uploadLimits by mutableStateOf<UploadLimits?>(null)
        private set
    private var limitsAsked = false

    /** Asks the instance what it takes of attachments, once a session (again after a failure to reach it). */
    fun askUploadLimits() {
        if (limitsAsked || !signedIn) return
        limitsAsked = true
        scope.launch {
            try { uploadLimits = client.call { uploadLimits() } } catch (e: Exception) { limitsAsked = false }
        }
    }

    /** The files in each conversation's composer, by channel id: kept here so turning the phone or leaving the conversation keeps them, and their uploads go on in the session's scope. */
    val attaching = mutableStateMapOf<String, List<PendingAttachment>>()

    private var attachedCount = 0

    /**
     * Adds a file to [channelId]'s composer and makes an attachment of it:
     * [describe] says what it is at once (name, size, type, read without
     * its bytes), [input] reads and decodes it off the main thread, and
     * the core processes, seals and uploads it (`attach`). A failure is
     * said in words on the file; one taken out meanwhile is discarded.
     */
    fun attach(channelId: String, name: String, sizeBytes: Long, mediaType: String, input: suspend () -> AttachmentInput) {
        val localId = "local-${++attachedCount}"
        attaching[channelId] = attaching[channelId].orEmpty() + PendingAttachment(localId, name, sizeBytes, mediaType)
        scope.launch {
            val outcome = try {
                val made = input()
                Result.success(client.call { attach(channelId, made) })
            } catch (e: OutOfMemoryError) {
                Result.failure(IllegalStateException("too large to read on this phone"))
            } catch (e: Exception) {
                Result.failure(e)
            }
            val now = attaching[channelId].orEmpty()
            if (now.none { it.localId == localId }) {
                // Taken out while it uploaded: its blobs go too.
                outcome.getOrNull()?.let { draft -> client.call { discardAttachment(draft.draftId) } }
                return@launch
            }
            attaching[channelId] = now.map { p ->
                if (p.localId != localId) p
                else outcome.fold({ p.copy(draft = it) }, { p.copy(error = uploadRefused(it.lumiaCode, it.lumiaDetails["state"], it.lumiaMessage)) })
            }
        }
    }

    /** Says a file could not even be read, on itself in the strip. */
    fun attachFailed(channelId: String, name: String, words: String) {
        val localId = "local-${++attachedCount}"
        attaching[channelId] = attaching[channelId].orEmpty() + PendingAttachment(localId, name, 0, "", error = attachRefused(words))
    }

    /** Takes a file out of the composer, and its blobs off the instance when it was uploaded. */
    fun removeAttachment(channelId: String, localId: String) {
        val gone = attaching[channelId].orEmpty().firstOrNull { it.localId == localId } ?: return
        attaching[channelId] = attaching[channelId].orEmpty().filter { it.localId != localId }
        gone.draft?.let { d -> scope.launch { runCatching { client.call { discardAttachment(d.draftId) } } } }
    }

    /** A waiting picture's alt text, as typed; given to the core as it is sent. */
    fun setAlt(channelId: String, localId: String, alt: String) {
        attaching[channelId] = attaching[channelId].orEmpty().map { if (it.localId == localId) it.copy(alt = alt) else it }
    }

    /** The attachments ready to send, their alt text given to the core first. */
    private suspend fun readyAttachments(channelId: String): List<PendingAttachment> {
        val ready = attaching[channelId].orEmpty().filter { it.ready }
        for (p in ready) {
            val d = p.draft ?: continue
            if (p.alt != d.alt) client.call { setAttachmentAlt(d.draftId, p.alt) }
        }
        return ready
    }

    /** Those sent, out of the strip; a refused one stays, with its words, until taken out. */
    private fun sent(channelId: String, going: List<PendingAttachment>) {
        val ids = going.map { it.localId }.toSet()
        attaching[channelId] = attaching[channelId].orEmpty().filter { it.localId !in ids }
    }

    /** The draft's links a composer previews here: none where the conversation cannot have cards (an instance that makes none), so nothing waits for one. */
    suspend fun previewLinks(channelId: String, body: String): List<String> =
        runCatching { client.call { previewLinks(channelId, body) } }.getOrDefault(emptyList())

    /** The card a link would have while composing: the instance's in a server channel, this phone's in a personal one; none when the page gives none or it cannot be had. */
    suspend fun cardPreview(channelId: String, url: String): CardSummary? =
        runCatching { client.call { cardPreview(channelId, url) } }.getOrNull()

    /** An attachment's or a personal card's blob, fetched, opened and kept by the core. */
    suspend fun attachmentBytes(blob: BlobSummary): ByteArray = client.call { attachmentBytes(blob) }

    /** A server card's picture, by its address. */
    suspend fun cardImage(imageUrl: String): ByteArray = client.call { cardImage(imageUrl) }

    /**
     * A Bluesky handle's account (its DID), asked of Bluesky's public API
     * on the press of a post named by handle (specs/media.md 6.6), never
     * through the instance. None when Bluesky has no account for it; throws
     * when Bluesky gave no answer.
     */
    suspend fun resolveBlueskyHandle(handle: String): String? = client.call { resolveBlueskyHandle(handle) }

    /** The instance's origin, `https://chat.example` (its port when it names one): the base of the page an embed is held in. */
    val instanceOrigin: String get() = originOf(client.baseUrl) ?: client.baseUrl.trimEnd('/')

    // Reactions (architecture 6.6)

    /**
     * Adds this account's reaction to a message of a server's channel, or
     * takes it away when it has it already. Sent through the gateway; the
     * change comes back as the channel's event, and a refusal (not one
     * emoji, the message gone) as an error the screen shows.
     */
    fun react(channelId: String, messageId: String, emoji: String) {
        val remove = removes(messages[channelId]?.firstOrNull { it.messageId == messageId }?.reactions ?: emptyList(), emoji)
        scope.launch {
            try { client.call { react(channelId, messageId, emoji, remove) } } catch (e: Exception) { lastError = e.lumiaMessage }
        }
    }

    /**
     * Adds this account's reaction with one of the server's emoji, by its
     * id, or takes it away when it has it (specs/emoji.md 9); a removed
     * one can only be taken away, which the instance holds to.
     */
    fun reactEmoji(channelId: String, messageId: String, emojiId: String) {
        val remove = messages[channelId]?.firstOrNull { it.messageId == messageId }?.reactions?.any { it.emojiId == emojiId && it.me } == true
        scope.launch {
            try { client.call { reactEmoji(channelId, messageId, emojiId, remove) } } catch (e: Exception) { lastError = e.lumiaMessage }
        }
    }

    /** The same in a personal conversation, inside its encryption. */
    fun dmReact(channelId: String, messageId: String, emoji: String) {
        val remove = removes(dmLogs[channelId]?.messages?.firstOrNull { it.messageId == messageId }?.reactions ?: emptyList(), emoji)
        scope.launch {
            try {
                client.call { dmReact(channelId, messageId, emoji, remove) }
                refreshDmLog(channelId)
            } catch (e: Exception) {
                lastError = e.lumiaMessage
            }
        }
    }

    // Editing and deleting (architecture 6.6)

    /**
     * Edits refused, by message id: what to say under the message ("Not
     * saved: …"), until it is edited again or the edit comes back.
     */
    val editRefusals = mutableStateMapOf<String, String>()

    /** Edits sent, by the command id `editMessage` answers, which a refusal names: the message each was for. */
    private val editsSent = mutableMapOf<String, String>()

    /**
     * Edits one of this account's messages of a server's channel, with the
     * mentions its new words make, found as a send's are. Sent through the
     * gateway; the edit comes back as the channel's event, and a refusal as
     * one this message shows.
     */
    fun editMessage(channelId: String, messageId: String, body: String) {
        editRefusals.remove(messageId)
        scope.launch {
            try {
                val mentions = mentionsIn(channelId, body)
                editsSent[client.call { editMessage(channelId, messageId, body, mentions) }] = messageId
            } catch (e: Exception) {
                editRefusals[messageId] = notSaved(e.lumiaMessage)
            }
        }
    }

    /**
     * The same, dropping the attachments [removeIds] names and, with
     * [cardUrls], keeping only those links' cards (spec 4.4, 6.4).
     */
    fun editMessageWithMedia(channelId: String, messageId: String, body: String, removeIds: List<String>, cardUrls: List<String>?) {
        editRefusals.remove(messageId)
        scope.launch {
            try {
                val mentions = mentionsIn(channelId, body)
                editsSent[client.call { editMessageWithMedia(channelId, messageId, body, mentions, removeIds, cardUrls) }] = messageId
            } catch (e: Exception) {
                editRefusals[messageId] = notSaved(e.lumiaMessage)
            }
        }
    }

    /** Deletes a message of a server's channel: this account's own, or anyone's for a moderator. A refusal is shown as `lastError`. */
    fun deleteMessage(channelId: String, messageId: String) {
        scope.launch {
            try { client.call { deleteMessage(channelId, messageId) } } catch (e: Exception) { lastError = e.lumiaMessage }
        }
    }

    /** Edits one of this account's messages of a personal conversation, inside its encryption. */
    fun dmEdit(channelId: String, messageId: String, body: String) {
        editRefusals.remove(messageId)
        scope.launch {
            try {
                client.call { dmEdit(channelId, messageId, body) }
                refreshDmLog(channelId)
            } catch (e: Exception) {
                editRefusals[messageId] = notSaved(e.lumiaMessage)
            }
        }
    }

    /** The same, dropping attachments and cards as a channel's edit does; the core takes their blobs off the instance. */
    fun dmEditWithMedia(channelId: String, messageId: String, body: String, removeIds: List<String>, cardUrls: List<String>?) {
        editRefusals.remove(messageId)
        scope.launch {
            try {
                client.call { dmEditWithMedia(channelId, messageId, body, removeIds, cardUrls) }
                refreshDmLog(channelId)
            } catch (e: Exception) {
                editRefusals[messageId] = notSaved(e.lumiaMessage)
            }
        }
    }

    /** Deletes one of this account's messages of a personal conversation. */
    fun dmDelete(channelId: String, messageId: String) {
        scope.launch {
            try {
                client.call { dmDelete(channelId, messageId) }
                refreshDmLog(channelId)
            } catch (e: Exception) {
                lastError = e.lumiaMessage
            }
        }
    }

    /** The mentions a body makes in a server's text channel, as the core finds them; none anywhere else. */
    private suspend fun mentionsIn(channelId: String, body: String): List<MentionSummary> {
        val channel = channelOf(channelId)
        if (channel?.kind != TEXT) return emptyList()
        val everyone = snapshot?.servers?.any { it.serverId == channel.serverId && it.canMentionEveryone } == true
        val members = serverMembers[channel.serverId] ?: emptyList()
        return client.call { findMentions(body, members, everyone) }
    }

    /** The error a screen showed, gone. */
    fun clearError() { lastError = null }

    // Read state (architecture 6.6)

    /** Where this account has read each conversation, by channel id: the core's live count. */
    var readStates by mutableStateOf<Map<String, ReadStateSummary>>(emptyMap())
        private set

    /** Everything unread that is for this account: the app's badge. */
    var badge by mutableStateOf(0)
        private set

    /**
     * What the counts changing does beyond the screens: the delivered
     * notifications of conversations now read go, and the rest carry the
     * badge (`PushRegistrar.settle`). The store's to say, holding the context.
     */
    var readsChanged: ((Collection<ReadStateSummary>, Int) -> Unit)? = null

    private val marked = mutableMapOf<String, Marked>()

    /** Reads the counts again from the core: after a snapshot, every event applied, and every place this phone moves. */
    suspend fun refreshReads() {
        if (leaving || !signedIn) return
        val states = runCatching { client.call { readStates() } }.getOrNull() ?: return
        val now = states.associateBy { it.channelId }
        val total = runCatching { client.call { badge() }.toInt() }.getOrDefault(badgeOf(states))
        if (now == readStates && total == badge) return
        readStates = now
        badge = total
        readsChanged?.invoke(states, total)
    }

    /** What is unread in a conversation, and how much of it is for this account. */
    fun unread(channelId: String): Unread = unreadOf(readStates, channelId)

    /**
     * This account has read a conversation through [seq], its newest message
     * on screen with the app in front. Nothing when that moves nothing, or
     * when it was marked through there less than a second ago (`shouldMark`).
     */
    fun markRead(channelId: String, seq: ULong) {
        val now = SystemClock.elapsedRealtime()
        if (!shouldMark(readStates[channelId], seq, marked[channelId], now)) return
        marked[channelId] = Marked(seq, now)
        scope.launch {
            try {
                client.call { markRead(channelId, seq) }
                refreshReads()
            } catch (e: Exception) {
                if (e.endsSession) sessionEnded()
            }
        }
    }

    /** A Mentions list as far as it has been read: the messages, whether the instance has older ones, and how the last read went. */
    data class MentionsList(
        val messages: List<MessageSummary> = emptyList(),
        val more: Boolean = false,
        val loading: Boolean = false,
        val loaded: Boolean = false,
        val error: String? = null,
    )

    /** The Mentions lists, by server id; `""` is every server's (Home's). */
    val mentionLists = mutableStateMapOf<String, MentionsList>()

    /** Reads the first page of a Mentions list, or ([more]) the next after what it holds. Null [serverId] is every server's. */
    fun loadMentions(serverId: String?, more: Boolean) {
        val key = serverId ?: ""
        val list = mentionLists[key] ?: MentionsList()
        if (list.loading || (more && !list.more)) return
        mentionLists[key] = list.copy(loading = true)
        val before = if (more) list.messages.lastOrNull()?.messageId else null
        scope.launch {
            try {
                val page = client.call { mentions(serverId, before) }
                val held = mentionLists[key] ?: MentionsList()
                mentionLists[key] = MentionsList(if (more) held.messages + page.messages else page.messages, page.hasMore, loading = false, loaded = true)
            } catch (e: Exception) {
                mentionLists[key] = (mentionLists[key] ?: MentionsList()).copy(loading = false, loaded = true, error = e.lumiaMessage)
            }
        }
    }

    /**
     * Finds a message of a channel for a jump (a quote's original, a mention
     * opened from its page): true once it is among the messages held,
     * reading further back in the store a page at a time, up to
     * [SEEK_PAGES] pages, for one that is not.
     */
    suspend fun reveal(channelId: String, messageId: String): Boolean {
        repeat(SEEK_PAGES + 1) { page ->
            val held = messages[channelId]
            if (held?.any { it.messageId == messageId } == true) return true
            if (page == SEEK_PAGES) return false
            val limit = messageLimits[channelId] ?: MESSAGE_PAGE
            // The store has nothing older than what it gave.
            if (held != null && held.size.toUInt() < limit) return false
            messageLimits[channelId] = limit + MESSAGE_PAGE
            refreshMessages(channelId)
        }
        return false
    }

    /** Who is in each channel whose members were asked for, by channel id: the @ list marks those not in it, and a send names them. */
    val channelMemberIds = mutableStateMapOf<String, Set<String>>()

    /** Asks who is in a channel, for its @ list and the bar after a send. */
    suspend fun loadChannelMembers(channelId: String) {
        runCatching { client.call { channelMembers(channelId) } }.getOrNull()?.let { channelMemberIds[channelId] = it.toSet() }
    }

    /** A person's name when one is known, else none: who reacted, named or "Someone". */
    fun knownNameOf(accountId: String): String? {
        if (accountId.isNotEmpty() && accountId == me) return selfName
        val p = people[accountId] ?: profileOf(accountId) ?: return null
        return chat.lumia.kit.nameOf(p).ifEmpty { null }
    }

    suspend fun dmRejoin(channelId: String) {
        client.call { dmRejoin(channelId) }
        refreshDmChannels()
    }

    /** Opens a conversation with `accountId`, or asks when the instance wants a request first: the channel id when opened, null when a request went out. */
    suspend fun dmOpenOrRequest(accountId: String): String? {
        return try {
            val opened = client.call { dmOpen(listOf(accountId)) }
            refreshSnapshot()
            refreshDmChannels()
            opened.channelId
        } catch (e: Exception) {
            if (e.lumiaCode != "FAILED_PRECONDITION") throw e
            client.call { dmRequest(accountId) }
            null
        }
    }

    /** Answers a request; the channel id when accepting opened one. */
    suspend fun dmRespond(requestId: String, response: String): String? {
        val r = client.call { dmRespond(requestId, response) }
        refreshSnapshot()
        refreshDmChannels()
        return (r as? DmResponse.Opened)?.opened?.channelId
    }

    // Names

    val me: String get() = snapshot?.accountId ?: ""

    val selfName: String
        get() {
            val s = snapshot
            return s?.displayName?.ifEmpty { null } ?: s?.handle?.ifEmpty { null } ?: "You"
        }

    /** A person's name: from the shared servers, else a looked-up profile, else the id's start. */
    fun nameOf(accountId: String): String {
        if (accountId.isNotEmpty() && accountId == me) return selfName
        val p = people[accountId] ?: profileOf(accountId)
        if (p != null) {
            for (candidate in listOf(p.displayName, p.nickname, p.handle)) if (candidate.isNotEmpty()) return candidate
        }
        return if (accountId.isEmpty()) "Someone" else accountId.take(8)
    }

    fun profileOf(accountId: String): MemberSummary? {
        if (accountId.isEmpty()) return null
        if (profiles.containsKey(accountId)) return profiles[accountId]
        if (profilesInFlight.add(accountId)) {
            scope.launch {
                try {
                    profiles[accountId] = client.call { account(accountId) }
                } catch (e: Exception) {
                    // Not visible stays null; anything else is retried on the next read.
                    if (e.lumiaCode == "NOT_FOUND") profiles[accountId] = null
                }
                profilesInFlight.remove(accountId)
            }
        }
        return null
    }

    /** A person's pronouns, from the verified profile the instance showed; empty when none. */
    fun pronounsOf(accountId: String): String {
        if (accountId.isEmpty()) return ""
        return (people[accountId] ?: profileOf(accountId))?.pronouns ?: ""
    }

    /** How often this account's own name or picture has changed while the session ran, here or elsewhere: Profile reads its profile again. */
    var ownProfileChanges by mutableStateOf(0)
        private set

    /**
     * Someone's name or picture changed (issue #15): they are written as
     * they now are into everything held of them, every server's member
     * list, the people, a looked-up profile and, for this account, the
     * snapshot, so whatever draws them draws them again at once with
     * nothing asked of the instance. Each server's nickname stays. A new
     * picture is a new address, fetched as any is; none is the initials.
     */
    private fun profileUpdated(profile: ClientEvent.ProfileUpdated) {
        people = people.updatedBy(profile)
        serverMembers = serverMembers.updatedBy(profile)
        profiles[profile.accountId]?.let { profiles[profile.accountId] = it.updatedBy(profile) }
        val snap = snapshot ?: return
        if (snap.accountId != profile.accountId) return
        snapshot = snap.updatedBy(profile)
        ownProfileChanges++
    }

    fun conversationName(channel: ChannelSummary): String {
        val others = channel.participantAccountIds.filter { it != me }
        if (others.isEmpty()) return "Just you"
        return others.joinToString(", ") { nameOf(it) }
    }

    // Gateway

    private fun start(channelIds: List<String>) {
        val stream = client.start(channelIds)
        subscribed.clear()
        subscribed.addAll(channelIds)
        connection = ConnectionState.Connecting
        events = scope.launch {
            stream.collect { handle(it) }
            events = null
            // The stream ends by itself only when the loop has (`Closed`; `stop()`
            // cancels this job instead), so here and not in `handle`: the loop
            // this may start is not the one `events` was just cleared for. Not
            // for a phone the loop's end removed, which has nothing left to ask.
            if (signedIn) loopEnded()
        }
    }

    /**
     * The loop has ended and will not come back by itself. Why is in words
     * only, so the snapshot is asked: a session the instance has revoked
     * answers so there and the phone goes to sign-in; anything else is a
     * snapshot later, which starts the loop again. Asked once and no more:
     * a failure that is not the session's end is left at that, and a loop
     * that ends again within [RESTART_EVERY_MS] of the last time is asked
     * about but not started, so one that ends as soon as it starts does not
     * go round.
     */
    private suspend fun loopEnded() {
        val now = SystemClock.elapsedRealtime()
        val before = loopEndedAt
        loopEndedAt = now
        if (before != null && now - before < RESTART_EVERY_MS) refreshSnapshot() else load()
    }

    private var loopEndedAt: Long? = null

    fun stop() {
        events?.cancel()
        events = null
        client.stop()
        connection = ConnectionState.Idle
    }

    /** Back in the foreground: the lock is taken again; when another process wrote, the loop restarts. */
    fun foreground() {
        inForeground = true
        scope.launch {
            val wrote = runCatching { client.call { resume() } }.getOrDefault(false)
            if (wrote) {
                stop()
                load()
            } else {
                runCatching { client.call { foreground() } }
                // What moved while the app was away: a device asking to be linked, or this phone's own link.
                refreshPrompts()
                linkFollow()
            }
        }
    }

    fun background() {
        inForeground = false
        scope.launch { runCatching { client.call { background() } } }
    }

    private suspend fun handle(event: ClientEvent) {
        when (event) {
            is ClientEvent.Welcomed -> {
                connection = ConnectionState.Connected
                // Every list again after a reconnect; not at the welcome of a loop `load` just started, which is reading them.
                val read = peopleWithStart
                peopleWithStart = false
                refreshSnapshot(everyone = !read)
                serverPresence.clear()
                watchPresence(watchingServerIds)
            }
            is ClientEvent.Applied -> {
                refreshMessages(event.channelId)
                refreshReads()
                // Someone joined, left, was added or removed: who the @ list marks has changed.
                if (event.channelId in channelMemberIds && event.events.any { it.message?.notice?.isNotEmpty() == true }) loadChannelMembers(event.channelId)
                // Whoever's message arrived has finished typing it. Written with the
                // messages, nothing suspending between, so no frame shows one without the other.
                var now = typing
                for (m in event.events.mapNotNull { it.message }) now = now.stopped(event.channelId, m.authorAccountId)
                showTyping(now)
            }
            is ClientEvent.SnapshotTaken -> refreshMessages(event.channelId)
            is ClientEvent.AccountApplied -> { refreshSnapshot(); refreshDmChannels() }
            // A link session moved (identity spec 4.2.1): this phone's own, or another's prompting here.
            is ClientEvent.LinkSessionUpdated -> { linkFollow(); refreshPrompts() }
            is ClientEvent.ProfileUpdated -> profileUpdated(event)
            // A server's roles, some members' roles or titles, or which of the two it shows (architecture 7.5):
            // what the core keeps has it, and the snapshot was taken again for the `AccountApplied` before this.
            is ClientEvent.TagsChanged -> refreshTags(event.serverId)
            is ClientEvent.EmojiChanged -> emojiChanged(event.serverId)
            is ClientEvent.Reconnecting -> { peopleWithStart = false; connection = ConnectionState.Reconnecting(event.attempt) }
            is ClientEvent.Disconnected -> { peopleWithStart = false; connection = ConnectionState.Disconnected(event.reason) }
            is ClientEvent.Closed -> {
                peopleWithStart = false
                connection = ConnectionState.Closed(event.reason)
                events = null
                closedNotice(event.removed)?.let { removed(it) }
            }
            is ClientEvent.DmSubscribed -> { refreshDmChannels(); refreshDmLog(event.channelId) }
            is ClientEvent.DmReady -> { refreshDmChannels(); refreshDmLog(event.channelId) }
            is ClientEvent.DmOperation -> { refreshDmLog(event.channelId); refreshReads() }
            is ClientEvent.DmDelivered -> for (id in dmLogs.keys.toList()) refreshDmLog(id)
            is ClientEvent.DmError -> lastError = event.message
            is ClientEvent.PresenceDiff -> {
                val server = (serverPresence[event.serverId] ?: emptyMap()).toMutableMap()
                for (p in event.joined + event.updated) server[p.accountId] = p.status
                for (id in event.leftAccountIds) server.remove(id)
                serverPresence[event.serverId] = server
            }
            // Never this account's own: the instance does not send it back, and it is not shown if one does.
            is ClientEvent.Typing -> if (event.accountId != me) {
                showTyping(typing.began(event.channelId, event.accountId, event.untilMicros))
            }
            // A command refused: a reaction that is not one emoji, @everyone without the
            // permission, a quote of a message that is gone. The screen says it; an
            // edit's refusal, under its message.
            is ClientEvent.Error -> {
                val edited = editsSent.remove(event.commandId)
                if (edited != null) editRefusals[edited] = notSaved(event.message) else lastError = event.message
            }
            is ClientEvent.Acked, is ClientEvent.SubscriptionFailed, is ClientEvent.DmNote -> Unit
        }
    }

    // Typing (protocol 6.7)

    /** Who is typing in the servers' channels, whoever began first at the front; never this account. */
    private var typing by mutableStateOf(Typing())
    private var typingExpiry: Job? = null

    /**
     * This person changed what the composer of a server's channel holds and
     * left something in it: says so to the channel. The core sends it at most
     * once in five seconds and holds back anything that is not a server's
     * channel. A personal conversation's screen does not call this at all:
     * the instance is not to learn who is writing there, and whether typing
     * travels inside one is an open item of the E2EE spec (section 13).
     */
    fun typed(channelId: String) {
        if (connection != ConnectionState.Connected) return
        scope.launch { runCatching { client.call { typing(channelId) } } }
    }

    /** The names of those typing in a channel, whoever began first at the front. */
    fun typingIn(channelId: String): List<String> = typing.inChannel(channelId).map { nameOf(it) }

    /**
     * Shows who is typing now, and drops each when their time runs out:
     * nothing arrives when someone stops, so one job sleeps until the first
     * end there is, and ends itself once nobody is typing.
     */
    private fun showTyping(now: Typing) {
        if (now == typing) return
        typing = now
        typingExpiry?.cancel()
        typingExpiry = scope.launch {
            while (true) {
                val ends = typing.ends ?: break
                delay(((ends - nowMicros()) / 1000 + 1).coerceAtLeast(1))
                typing = typing.expired(nowMicros())
            }
        }
    }

    private fun nowMicros(): Long = System.currentTimeMillis() * 1000

    // Presence (protocol 6.7)

    /** Who is present in the servers on screen, by server then account: ONLINE, AWAY or DO_NOT_DISTURB. */
    val serverPresence = mutableStateMapOf<String, Map<String, String>>()
    private var watchingServerIds: List<String> = emptyList()

    /** The servers on screen, whose presence this client wants; none for Home. Re-sent on every welcome. */
    fun watchPresence(serverIds: List<String>) {
        watchingServerIds = serverIds
        if (connection != ConnectionState.Connected) return
        scope.launch { runCatching { client.call { watchPresence(serverIds) } } }
    }

    /** The status this person chose and their custom status text. */
    suspend fun status(): StatusSummary? = runCatching { client.call { status() } }.getOrNull()

    /** Chooses the status (ONLINE, AWAY, DO_NOT_DISTURB) and custom text; the core keeps it and sends it with the servers watched. */
    suspend fun setStatus(status: String, customStatus: String) {
        client.call { setStatus(status, customStatus) }
    }

    /** An account's presence in a server, or null when it is not there. */
    fun presenceOf(accountId: String, serverId: String): String? = serverPresence[serverId]?.get(accountId)
}
