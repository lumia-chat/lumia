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
import uniffi.lumia_ffi.ClientEvent
import uniffi.lumia_ffi.DmChannelSummary
import uniffi.lumia_ffi.DmLogSummary
import uniffi.lumia_ffi.DmResponse
import uniffi.lumia_ffi.InvitePreviewSummary
import uniffi.lumia_ffi.InviteSummary
import uniffi.lumia_ffi.InstanceSummary
import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.PasskeySummary
import uniffi.lumia_ffi.ProfileSummary
import uniffi.lumia_ffi.StatusSummary
import uniffi.lumia_ffi.MessageSummary
import uniffi.lumia_ffi.RosterSummary
import uniffi.lumia_ffi.SnapshotSummary
import uniffi.lumia_ffi.ChannelSummary
import uniffi.lumia_ffi.ChannelAddSummary
import uniffi.lumia_ffi.MentionSummary
import uniffi.lumia_ffi.QuoteSummary
import uniffi.lumia_ffi.ReadStateSummary

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

    private fun took(snap: SnapshotSummary) {
        if (leaving || !signedIn) return
        snapshot = snap
        subscribeGained()
        scope.launch { refreshReads() }
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
    }

    suspend fun identityRevoke(deviceId: String, password: String) {
        roster = client.call { identityRevoke(deviceId, password) }
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

    /** Loads the snapshot and starts the gateway on every server channel. */
    suspend fun load() {
        try {
            val snap = client.call { snapshot() }
            // Answered while the session was being left (asked just before, by a
            // screen made again or the app coming forward): nothing of the account
            // is taken up again, and the gateway the leaving stopped stays stopped.
            if (leaving || !signedIn) return
            snapshot = snap
            hasIdentity = client.hasIdentity
            refreshReads()
            refreshPeople(snap.servers.map { it.serverId })
            refreshDmChannels()
            if (events == null) {
                // With any channel opened to read before the loop ran.
                val held = snap.channels.filter { it.kind !in personalKinds }.map { it.channelId }
                start(held + previews.keys.filter { it !in held })
            } else {
                subscribeGained()
            }
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
    suspend fun refreshSnapshot() {
        val snap = try {
            client.call { snapshot() }
        } catch (e: Exception) {
            if (e.endsSession) sessionEnded()
            return
        }
        if (leaving || !signedIn) return
        snapshot = snap
        subscribeGained()
        refreshReads()
        refreshPeople(snap.servers.map { it.serverId })
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

    private suspend fun refreshPeople(serverIds: List<String>) {
        val found = mutableMapOf<String, MemberSummary>()
        val byServer = mutableMapOf<String, List<MemberSummary>>()
        for (id in serverIds) {
            val members = runCatching { client.call { members(id, "") } }.getOrDefault(emptyList())
            byServer[id] = members
            for (m in members) found[m.accountId] = m
        }
        people = found
        serverMembers = byServer
    }

    // Actions

    /**
     * Sends a message to a server's channel or server DM, quoting [reply]
     * or nothing, with the mentions its body makes of the server's members
     * (and of everyone, for one who may), as the core finds them; a server
     * DM mentions nobody, both being told of every message anyway. Answers
     * who it mentioned that is not in the channel, so not notified, as far
     * as who is in it is known (`loadChannelMembers`).
     */
    suspend fun sendMessageWith(channelId: String, body: String, reply: QuoteSummary?): List<String> {
        val mentions = mentionsIn(channelId, body)
        val outside = outsideOf(mentions, channelMemberIds[channelId], me)
        client.call { sendMessageWith(channelId, body, mentions, reply) }
        refreshMessages(channelId)
        return outside
    }

    /** Sends to a personal conversation, quoting [quote] or nothing (inside the encryption). */
    suspend fun dmSendWith(channelId: String, body: String, quote: QuoteSummary?) {
        client.call { dmSendWith(channelId, body, quote) }
        refreshDmLog(channelId)
    }

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

    /**
     * The edit sent last and not yet back: a refusal the gateway sends while
     * it waits is that edit's. The core's `editMessage` answers no command
     * id to match the refusal by, so this is the nearest the phone can say.
     */
    private var editWaiting: Pair<String, String>? = null

    /**
     * Edits one of this account's messages of a server's channel, with the
     * mentions its new words make, found as a send's are. Sent through the
     * gateway; the edit comes back as the channel's event, and a refusal as
     * one this message shows.
     */
    fun editMessage(channelId: String, messageId: String, body: String) {
        editRefusals.remove(messageId)
        editWaiting = channelId to messageId
        scope.launch {
            try {
                val mentions = mentionsIn(channelId, body)
                client.call { editMessage(channelId, messageId, body, mentions) }
            } catch (e: Exception) {
                if (editWaiting?.second == messageId) editWaiting = null
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
            // this may start is not the one `events` was just cleared for.
            loopEnded()
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
        scope.launch {
            val wrote = runCatching { client.call { resume() } }.getOrDefault(false)
            if (wrote) {
                stop()
                load()
            } else {
                runCatching { client.call { foreground() } }
            }
        }
    }

    fun background() {
        scope.launch { runCatching { client.call { background() } } }
    }

    private suspend fun handle(event: ClientEvent) {
        when (event) {
            is ClientEvent.Welcomed -> {
                connection = ConnectionState.Connected
                refreshSnapshot()
                serverPresence.clear()
                watchPresence(watchingServerIds)
            }
            is ClientEvent.Applied -> {
                // The edit waited for is back: its channel's edit (which names no message).
                editWaiting?.let { (channel, _) -> if (channel == event.channelId && event.events.any { it.kind == "message_edited" }) editWaiting = null }
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
            is ClientEvent.Reconnecting -> connection = ConnectionState.Reconnecting(event.attempt)
            is ClientEvent.Disconnected -> connection = ConnectionState.Disconnected(event.reason)
            is ClientEvent.Closed -> { connection = ConnectionState.Closed(event.reason); events = null }
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
                val edit = editWaiting
                if (edit != null) {
                    editWaiting = null
                    editRefusals[edit.second] = notSaved(event.message)
                } else {
                    lastError = event.message
                }
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
