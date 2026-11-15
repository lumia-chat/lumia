// Setting up private messages as values (issue #20): which screen a phone
// gets from the account's identity and its own, what a failed setup
// leaves, the Home reminder's seven days, the phrase's columns, page and
// file, the three-word check's fields against the core's wordlist,
// Restore's count and Start over's words for who sees the key change.
import Foundation
@testable import LumiaKit
import LumiaFFI
import XCTest

final class PrivateMessagesTests: XCTestCase {
    /// A store of its own for each test, gone when the test is.
    private func scratch() -> UserDefaults {
        let name = "lumia-phrase-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        addTeardownBlock { defaults.removePersistentDomain(forName: name) }
        return defaults
    }

    // MARK: Which screen

    func testAPhoneWithTheAccountsIdentityIsReady() {
        XCTAssertEqual(PrivateMessages.step(account: "ab12", device: "ab12"), .ready)
    }

    func testAnAccountWithNoIdentityIsSetUpHereWhateverThePhoneHolds() {
        // Neither has one: made here and bound. Only the phone: bound to it.
        XCTAssertEqual(PrivateMessages.step(account: nil, device: nil), .setUp)
        XCTAssertEqual(PrivateMessages.step(account: nil, device: "ab12"), .setUp)
    }

    func testAnAccountWhoseIdentityIsElsewhereNeedsThisPhoneBroughtIn() {
        // No identity here, or a different one: Link, Restore or Start over.
        XCTAssertEqual(PrivateMessages.step(account: "ab12", device: nil), .bringIn)
        XCTAssertEqual(PrivateMessages.step(account: "ab12", device: "cd34"), .bringIn)
    }

    func testAFailedSetupAsksForReauthOffersBringingInOrSaysWhy() {
        XCTAssertEqual(PrivateMessages.after(LumiaError.ReauthNeeded), .confirm)
        XCTAssertEqual(PrivateMessages.after(LumiaError.AccountHasIdentity), .bringIn)
        XCTAssertEqual(PrivateMessages.after(LumiaError.Other(detail: "no network")), .failed("no network"))
    }

    func testOnlyAKnownNeedStandsInTheWay() {
        XCTAssertTrue(PrivateMessages.ready.ready)
        XCTAssertFalse(PrivateMessages.bringIn.ready)
        for state in [PrivateMessages.confirm, .bringIn, .failed("x")] { XCTAssertTrue(state.needsSomething) }
        // Still being found out, or set up already: nothing to offer.
        for state in [PrivateMessages.unknown, .ready, .setUp] { XCTAssertFalse(state.needsSomething) }
    }

    // MARK: The Home reminder

    func testTheReminderShowsWhileThePhraseWaitsUnlessPutOff() {
        let defaults = scratch()
        var reminder = PhraseReminder(defaults: defaults)
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        XCTAssertTrue(reminder.shows(unconfirmed: true, now: now))
        XCTAssertFalse(reminder.shows(unconfirmed: false, now: now))

        reminder.putOff(now: now, defaults: defaults)
        XCTAssertFalse(reminder.shows(unconfirmed: true, now: now))
        XCTAssertFalse(reminder.shows(unconfirmed: true, now: now.addingTimeInterval(6 * 24 * 3600 + 23 * 3600)))
        XCTAssertTrue(reminder.shows(unconfirmed: true, now: now.addingTimeInterval(7 * 24 * 3600)))
    }

    func testInAWeekIsKeptOnThisPhoneAndAConfirmedPhraseLetsItGo() {
        let defaults = scratch()
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        var reminder = PhraseReminder(defaults: defaults)
        reminder.putOff(now: now, defaults: defaults)
        // The next launch reads it back.
        let again = PhraseReminder(defaults: defaults)
        XCTAssertEqual(again.until, now.addingTimeInterval(PhraseReminder.putOffFor))
        XCTAssertFalse(again.shows(unconfirmed: true, now: now.addingTimeInterval(3600)))

        reminder.clear(defaults: defaults)
        XCTAssertNil(PhraseReminder(defaults: defaults).until)
        XCTAssertTrue(PhraseReminder(defaults: defaults).shows(unconfirmed: true, now: now))
    }

    // MARK: The phrase

    private let phrase = "orbit cactus timber velvet harbor pencil meadow copper rescue glimpse fabric lunar anchor whisper salad ticket marble frozen clever sunny violin garden token mirror"

    func testTheWordsAreInOrderInLowerCase() {
        let words = PhraseWords.words("  Orbit  CACTUS\ntimber ")
        XCTAssertEqual(words, ["orbit", "cactus", "timber"])
        XCTAssertEqual(PhraseWords.words(phrase).count, 24)
    }

    func testTwoColumnsReadDownTheLeftFirst() {
        let columns = PhraseWords.columns(PhraseWords.words(phrase), 2)
        XCTAssertEqual(columns.count, 2)
        XCTAssertEqual(columns[0].map(\.number), Array(1...12))
        XCTAssertEqual(columns[1].map(\.number), Array(13...24))
        XCTAssertEqual(columns[0].first?.word, "orbit")
        XCTAssertEqual(columns[1].first?.word, "anchor")
        XCTAssertEqual(columns[1].last?.word, "mirror")
        // The web's three: eight to a column.
        XCTAssertEqual(PhraseWords.columns(PhraseWords.words(phrase), 3).map { $0.count }, [8, 8, 8])
    }

    func testTheFileAndThePageCarryTheWarningAndEveryNumberedWord() {
        let words = PhraseWords.words(phrase)
        let file = PhraseWords.file(words)
        XCTAssertTrue(file.contains(PhraseWords.warning))
        XCTAssertTrue(file.contains("\n1. orbit\n"))
        XCTAssertTrue(file.contains("\n24. mirror\n"))

        let page = PhraseWords.page(words)
        XCTAssertTrue(page.contains(PhraseWords.warning))
        // Row one has 1 and 13 side by side; row twelve 12 and 24.
        XCTAssertTrue(page.contains(">1</td><td") && page.contains(">orbit</td>"))
        XCTAssertTrue(page.contains(">13</td>") && page.contains(">anchor</td>"))
        XCTAssertEqual(page.components(separatedBy: "<tr>").count - 1, 12)
    }

    // MARK: Check your phrase

    func testAFieldOffersTheWordlistsWordsUntilItHoldsTheOneItLeadsTo() {
        XCTAssertEqual(PhraseCheck.suggestions(""), [])
        XCTAssertEqual(PhraseCheck.suggestions("   "), [])
        let fa = PhraseCheck.suggestions("Fa")
        XCTAssertTrue(fa.contains("fabric") && fa.contains("face"), "\(fa)")
        XCTAssertLessThanOrEqual(fa.count, 4)
        // The one word its letters lead to: nothing more to offer.
        XCTAssertEqual(PhraseCheck.suggestions("velvet"), [])
        // A word that begins others still offers them.
        XCTAssertTrue(PhraseCheck.suggestions("fat").contains("father"))
        XCTAssertEqual(PhraseCheck.suggestions("zzz"), [])
    }

    func testConfirmTakesAWordOfTheListInEveryField() {
        var check = PhraseCheck(positions: [4, 11, 19])
        XCTAssertEqual(check.answers, ["", "", ""])
        XCTAssertFalse(check.canConfirm())
        check.answers = ["velvet", "fa", ""]
        XCTAssertFalse(check.canConfirm())
        check.answers = ["velvet", "fabric", "notaword"]
        XCTAssertFalse(check.canConfirm())
        check.answers = [" Velvet", "fabric ", "clever"]
        XCTAssertTrue(check.canConfirm())
        XCTAssertEqual(check.words, ["velvet", "fabric", "clever"])
    }

    // MARK: Restore

    func testRestoreCountsTheWordsAndTurnsOnAtTwentyFour() {
        let twelve = PhraseWords.words(phrase).prefix(12).joined(separator: " ")
        XCTAssertEqual(RestoreWords.countLabel(""), "0 of 24")
        XCTAssertEqual(RestoreWords.countLabel(twelve), "12 of 24")
        XCTAssertFalse(RestoreWords.canRestore(twelve))
        XCTAssertTrue(RestoreWords.canRestore(phrase))
        XCTAssertFalse(RestoreWords.canRestore(phrase + " extra"))
        XCTAssertEqual(RestoreWords.phrase(" Orbit\n\ncactus  timber "), "orbit cactus timber")
    }

    // MARK: Start over

    private func channel(_ kind: String, _ people: [String]) -> ChannelSummary {
        ChannelSummary(channelId: UUID().uuidString, serverId: "", name: "", kind: kind, lastSeq: 0, participantAccountIds: people, host: "", private: false, isDefault: false, memberCount: 0, topic: "")
    }

    func testStartOverCountsThePeopleInPersonalConversationsOnce() {
        let channels = [
            channel("PERSONAL_DM", ["me", "ada"]),
            channel("PERSONAL_GROUP_DM", ["me", "ada", "bo"]),
            channel("SERVER_DM", ["me", "cy"]),
            channel("TEXT", []),
        ]
        XCTAssertEqual(StartOverWords.people(in: channels, me: "me"), 2)
        XCTAssertEqual(StartOverWords.people(in: [], me: "me"), 0)
    }

    func testStartOverSaysWhoWillSeeTheKeyChange() {
        XCTAssertEqual(StartOverWords.keyChange(people: 3), "The 3 people you talk to privately will see that your key changed.")
        XCTAssertEqual(StartOverWords.keyChange(people: 1), "The 1 person you talk to privately will see that your key changed.")
        XCTAssertEqual(StartOverWords.keyChange(people: 0), "Nobody has talked with you privately yet, so nobody will see a key change.")
    }
}
