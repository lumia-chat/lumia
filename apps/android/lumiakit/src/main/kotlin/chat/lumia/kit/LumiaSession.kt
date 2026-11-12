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

    /** A text channel in the server, listed once the snapshot is refreshed; returns its id. */
    suspend fun createChannel(serverId: String, name: String): String {
        val channel = client.call { createChannel(serverId, name) }
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
                profiles.clear()
                serverPresence.clear()
                lastError = null
                showTyping(Typing())
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
            refreshPeople(snap.servers.map { it.serverId })
            refreshDmChannels()
            if (events == null) {
                start(snap.channels.filter { it.kind !in personalKinds }.map { it.channelId })
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
        refreshPeople(snap.servers.map { it.serverId })
    }

    suspend fun refreshMessages(channelId: String) {
        runCatching { client.call { messages(channelId, 200u) } }.getOrNull()?.let { messages[channelId] = it }
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
        for (id in serverIds) {
            for (m in runCatching { client.call { members(id, "") } }.getOrDefault(emptyList())) found[m.accountId] = m
        }
        people = found
    }

    // Actions

    suspend fun sendMessage(channelId: String, body: String) {
        client.call { sendMessage(channelId, body) }
        refreshMessages(channelId)
    }

    suspend fun dmSend(channelId: String, body: String) {
        client.call { dmSend(channelId, body) }
        refreshDmLog(channelId)
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
                refreshMessages(event.channelId)
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
            is ClientEvent.DmOperation -> refreshDmLog(event.channelId)
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
            is ClientEvent.Acked, is ClientEvent.Error, is ClientEvent.SubscriptionFailed, is ClientEvent.DmNote -> Unit
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
