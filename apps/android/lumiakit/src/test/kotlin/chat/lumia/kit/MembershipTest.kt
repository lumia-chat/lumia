// Channel membership and server DMs as values: what a sidebar lists, the
// words, who the pickers offer, the default channels' choices, the
// refusals' words, and which channels the gateway is still to be told of.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.lumia_ffi.ChannelInvitationSummary
import uniffi.lumia_ffi.ChannelSummary
import uniffi.lumia_ffi.ClientEvent
import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.SnapshotSummary

private fun channel(id: String, server: String = "s", kind: String = TEXT, name: String = id, private: Boolean = false, isDefault: Boolean = false, participants: List<String> = emptyList()) =
    ChannelSummary(id, server, name, kind, 0uL, participants, "", private, isDefault, 0u, "")

private fun member(id: String, display: String = "", handle: String = id, nickname: String = "", byInvitation: Boolean = false, dmsOff: Boolean = false) =
    MemberSummary(id, handle, display, nickname, "", "", "", byInvitation, dmsOff)

class MembershipTest {
    @Test
    fun aSidebarListsItsChannelsThenItsServerDmsAndItsInvitations() {
        val all = listOf(
            channel("general"),
            channel("dm", kind = SERVER_DM, participants = listOf("me", "ada")),
            channel("personal", server = "", kind = "PERSONAL_DM"),
            channel("elsewhere", server = "t"),
            channel("leads", private = true),
        )
        assertEquals(listOf("general", "leads"), sidebarChannels(all, "s").map { it.channelId })
        assertEquals(listOf("dm"), serverDms(all, "s").map { it.channelId })
        assertEquals("ada", otherIn(all[1], "me"))
        assertNull(otherIn(channel("alone", kind = SERVER_DM, participants = listOf("me")), "me"))
        val invitations = listOf(ChannelInvitationSummary(channel("design"), "ada", 0), ChannelInvitationSummary(channel("x", server = "t"), "ada", 0))
        assertEquals(listOf("design"), invitationsIn(invitations, "s").map { it.channel.channelId })
    }

    @Test
    fun noticesCountsAndNotesReadAsDecided() {
        assertEquals("Grace joined", noticeText("joined", "Grace", "Grace"))
        assertEquals("Grace left", noticeText("left", "Grace", "Grace"))
        assertEquals("Ada added Grace", noticeText("added", "Ada", "Grace"))
        assertEquals("Ada removed Grace", noticeText("removed", "Ada", "Grace"))
        assertNull(noticeText("renamed", "Ada", "Grace"))
        assertEquals("1 member", memberCount(1))
        assertEquals("0 members", memberCount(0))
        assertEquals("12 members", memberCount(12))
        assertEquals(
            "Messages here are kept by Acme Works and follow its message history settings. For an encrypted conversation, message privately.",
            serverDmNote("Acme Works"),
        )
        assertEquals("You’re not in #design", notInChannel("design"))
        assertEquals("#design · Ada invited you", invitationLine("design", "Ada"))
    }

    @Test
    fun addPeopleOffersTheServersMembersNotInTheChannel() {
        val members = listOf(
            member("me", "Robin"),
            member("ada", "Ada Lovelace", byInvitation = true),
            member("grace", "Grace Hopper"),
            member("alan", handle = "turing"),
        )
        assertEquals(listOf("ada", "alan"), addCandidates(members, setOf("grace"), "me", "").map { it.accountId })
        // By display name, handle or nickname, in any case.
        assertEquals(listOf("alan"), addCandidates(members, emptySet(), "me", "TUR").map { it.accountId })
        assertEquals(listOf("ada"), addCandidates(members, emptySet(), "me", "love").map { it.accountId })
        assertEquals("Will be invited", addTag(members[1]))
        assertNull(addTag(members[2]))
    }

    @Test
    fun theDmPickerListsEveryoneButMarksWhoTakesNone() {
        val members = listOf(member("me"), member("grace", "Grace Hopper", dmsOff = true), member("ada", "Ada Lovelace"))
        assertEquals(listOf("ada", "grace"), dmCandidates(members, "me", "").map { it.accountId })
        assertFalse(takesServerDms(members[1]))
        assertTrue(takesServerDms(members[2]))
    }

    @Test
    fun defaultChannelsAreChosenAmongThePublicTextChannelsOnce() {
        val joined = listOf(
            channel("general", isDefault = true),
            channel("leads", private = true),
            channel("dm", kind = SERVER_DM),
            channel("other", server = "t"),
        )
        val browsed = listOf(channel("design"), channel("general", isDefault = true))
        val choices = defaultChoices(joined, browsed, "s")
        assertEquals(listOf("general", "design"), choices.map { it.channelId })
        assertEquals(setOf("general"), defaultsOf(choices))
    }

    @Test
    fun refusalsReadAsDecided() {
        assertEquals("You cannot add people to this channel.", addRefused("PERMISSION_DENIED"))
        assertEquals("You cannot add people to this channel.", addRefused("NOT_FOUND"))
        assertEquals("Someone chosen cannot be added to this channel.", addRefused("INVALID_ARGUMENT"))
        assertNull(addRefused("UNAVAILABLE"))
        assertEquals("You cannot remove people from this channel.", removeRefused("PERMISSION_DENIED"))
        assertEquals("Grace doesn’t take messages here.", serverDmRefused("FAILED_PRECONDITION", "Grace"))
        assertEquals("You cannot start conversations in this server.", serverDmRefused("PERMISSION_DENIED", "Grace"))
        assertEquals("Grace is not in this server.", serverDmRefused("NOT_FOUND", "Grace"))
        assertEquals("You cannot change this server's settings.", serverSettingsRefused("PERMISSION_DENIED", null))
        assertEquals("Default channels must be public channels of this server.", serverSettingsRefused("INVALID_ARGUMENT", "default_channels"))
    }

    @Test
    fun aChannelGainedLaterIsSubscribedOnce() {
        val all = listOf(channel("general"), channel("dm", kind = SERVER_DM), channel("p", server = "", kind = "PERSONAL_DM"), channel("new"))
        assertEquals(listOf("dm", "new"), toSubscribe(all, setOf("general")))
        assertEquals(emptyList<String>(), toSubscribe(all, setOf("general", "dm", "new")))
    }

    private val newPicture = "/api/v1/avatars/" + "3".repeat(64)

    private fun ada(display: String = "Ada Lovelace", avatar: String = newPicture) =
        ClientEvent.ProfileUpdated("ada", "ada", display, avatar, "she/her", "Engines.")

    @Test
    fun aChangedNameAndPictureAreWrittenIntoEveryListHeldWithEachServersNickname() {
        val byServer = mapOf(
            "s" to listOf(member("ada", "Ada"), member("grace", "Grace")),
            "t" to listOf(member("ada", "Ada", nickname = "Countess", byInvitation = true, dmsOff = true)),
            "u" to listOf(member("grace", "Grace")),
        )
        val people = mapOf("ada" to member("ada", "Ada", nickname = "Countess"), "grace" to member("grace", "Grace"))
        val lists = byServer.updatedBy(ada())
        val held = people.updatedBy(ada())

        // Drawn as they now are, by name and by picture, in every list that has them.
        assertEquals("Ada Lovelace", nameOf(lists.getValue("s")[0]))
        assertEquals("Ada Lovelace", nameOf(held.getValue("ada")))
        assertEquals(newPicture, avatarPathOf("ada", "me", "", held["ada"]) { null })
        assertEquals(newPicture, lists.getValue("t")[0].avatarUrl)
        assertEquals(listOf("she/her", "she/her"), listOf(lists.getValue("s")[0].pronouns, held.getValue("ada").pronouns))
        assertEquals("Engines.", lists.getValue("t")[0].bio)
        // The server's own: the nickname and the switches stay.
        val t = lists.getValue("t")[0]
        assertEquals(Triple("Countess", true, true), Triple(t.nickname, t.channelAddsByInvitation, t.serverDmsOff))
        assertEquals("Countess", held.getValue("ada").nickname)
        // A picture cleared is none, and the avatar its initials.
        assertEquals("", avatarPathOf("ada", "me", "", people.updatedBy(ada(avatar = ""))["ada"]) { null })
    }

    @Test
    fun aChangedNameChangesNothingElse() {
        val grace = member("grace", "Grace")
        val byServer = mapOf("s" to listOf(member("ada", "Ada"), grace), "u" to listOf(grace))
        val people = mapOf("ada" to member("ada", "Ada"), "grace" to grace)
        val lists = byServer.updatedBy(ada())
        val held = people.updatedBy(ada())
        assertSame(grace, lists.getValue("s")[1])
        assertSame(byServer.getValue("u"), lists.getValue("u"))
        assertSame(grace, held.getValue("grace"))
        assertEquals(byServer.keys, lists.keys)
        assertEquals(people.keys, held.keys)
        // Someone nothing holds: the same lists and people, and nobody added.
        val stranger = ClientEvent.ProfileUpdated("eve", "eve", "Eve", newPicture, "", "")
        assertSame(byServer, byServer.updatedBy(stranger))
        assertSame(people, people.updatedBy(stranger))
        assertSame(grace, grace.updatedBy(ada()))
    }

    @Test
    fun thisAccountsOwnChangeIsTheSnapshotsToo() {
        val snap = SnapshotSummary("ada", "ada", "Ada", "", emptyList(), emptyList(), emptyList(), emptyList(), emptyList(), emptyList())
        val now = snap.updatedBy(ada())
        assertEquals(Triple("ada", "Ada Lovelace", newPicture), Triple(now.handle, now.displayName, now.avatarUrl))
        assertEquals(newPicture, avatarPathOf("ada", "ada", now.avatarUrl, null) { null })
        assertEquals("", snap.avatarUrl)
        // Someone else's change leaves this account's snapshot as it was.
        assertSame(snap, snap.updatedBy(ClientEvent.ProfileUpdated("grace", "grace", "Grace H", newPicture, "", "")))
    }
}
