// The session: one instance's client with the state the screens show,
// kept fresh by the gateway's events the way the web's queries are
// (apps/web/src/client/state.ts) and the Swift LumiaSession is. Compose
// state, written on the main thread; every client call goes off it.
package chat.lumia.kit

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import uniffi.lumia_ffi.ClientEvent
import uniffi.lumia_ffi.DmChannelSummary
import uniffi.lumia_ffi.DmLogSummary
import uniffi.lumia_ffi.DmResponse
import uniffi.lumia_ffi.InstanceSummary
import uniffi.lumia_ffi.LumiaException
import uniffi.lumia_ffi.MemberSummary
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

class LumiaSession(val client: LumiaClient) {
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

    suspend fun signIn(handle: String, password: String, totpCode: String?) {
        client.call { signIn(handle, password, totpCode) }
        signedIn = true
    }

    suspend fun createAccount(handle: String, password: String) {
        client.call { createAccount(handle, password) }
        signedIn = true
    }

    suspend fun signOut() {
        stop()
        runCatching { client.call { signOut() } }
        signedIn = false
        snapshot = null
        messages.clear()
        dmChannels = emptyList()
        dmLogs.clear()
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
            snapshot = snap
            hasIdentity = client.hasIdentity
            refreshPeople(snap.servers.map { it.serverId })
            refreshDmChannels()
            if (events == null) {
                start(snap.channels.filter { it.kind !in personalKinds }.map { it.channelId })
            }
        } catch (e: Exception) {
            lastError = e.lumiaMessage
            if (e is LumiaException.NotSignedIn) signedIn = false
        }
    }

    suspend fun refreshSnapshot() {
        runCatching { client.call { snapshot() } }.getOrNull()?.let { snap ->
            snapshot = snap
            refreshPeople(snap.servers.map { it.serverId })
        }
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

    suspend fun pushSubscribe(platform: String, token: String) {
        runCatching { client.call { pushSubscribe(platform, token) } }
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
        }
    }

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
            is ClientEvent.Welcomed -> { connection = ConnectionState.Connected; refreshSnapshot() }
            is ClientEvent.Applied -> refreshMessages(event.channelId)
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
            is ClientEvent.Acked, is ClientEvent.Error, is ClientEvent.SubscriptionFailed, is ClientEvent.DmNote -> Unit
        }
    }
}
