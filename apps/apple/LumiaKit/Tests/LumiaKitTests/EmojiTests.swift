// A server's own emoji on the phone (specs/emoji.md): a body's emoji as
// pieces of their own heard by their present name, the `:` the composer's
// list completes, "Frequently used" kept by server, a chip of one, a
// picture's frames by time, the caret's fallback, and the Emoji section's
// words and refusals.
import CoreGraphics
import Foundation
@testable import LumiaKit
import LumiaFFI
import XCTest

final class EmojiTests: XCTestCase {
    private func para(_ blocks: [BodyBlock]) -> BodyInline? {
        guard case let .para(inline)? = blocks.first else { return nil }
        return inline
    }

    private func summary(_ id: String, _ name: String, animated: Bool = false, width: UInt32 = 128, height: UInt32 = 128) -> EmojiSummary {
        EmojiSummary(emojiId: id, name: name, imageUrl: "/api/v1/avatars/" + String(repeating: "a", count: 64), animated: animated, width: width, height: height, createdByAccountId: "ada")
    }

    func testAnEmojiIsAPieceOfItsOwnHeardByItsPresentName() throws {
        // "Shipped " is 8 bytes and ":partyparrot:" 13; "é" before ":lgtm:" is 2 bytes.
        let body = "Shipped :partyparrot: é :lgtm: `:hype:`"
        let refs = [EmojiRefSummary(emojiId: "e8", start: 8, end: 21), EmojiRefSummary(emojiId: "e2", start: 25, end: 31)]
        let blocks = parseBodyMarked(body: body, mentions: [], emoji: refs)
        let pieces = BodyText.pieces(try XCTUnwrap(para(blocks)))
        XCTAssertEqual(pieces.map(\.kind), [.words, .emoji(0), .words, .emoji(1), .words, .code])
        XCTAssertEqual(pieces.map { String($0.text.characters) }, ["Shipped ", ":partyparrot:", " é ", ":lgtm:", " ", ":hype:"])
        // The first renamed since (heard by its name now), the second not held (its text as typed).
        let heard = BodyText.plain(blocks, mentions: [], name: { _ in "" }) { $0 == 0 ? ":party_parrot:" : nil }
        XCTAssertEqual(heard, "Shipped :party_parrot: é :lgtm: :hype:")
    }

    func testTheUnderscoreOfANameIsNotEmphasis() throws {
        let body = "a :party_blob: and :party_blob: b"
        let refs = [EmojiRefSummary(emojiId: "e10", start: 2, end: 14), EmojiRefSummary(emojiId: "e10", start: 19, end: 31)]
        let blocks = BodyBlocks.parsed(body, mentions: [], emoji: refs)
        let pieces = BodyText.pieces(try XCTUnwrap(para(blocks)))
        XCTAssertEqual(pieces.map(\.kind), [.words, .emoji(0), .words, .emoji(1), .words])
        XCTAssertEqual(String(pieces[1].text.characters), ":party_blob:")
    }

    func testTheComposersColonWordIsTheOneTheDraftEndsIn() {
        XCTAssertEqual(EmojiQuery.at("Shipped the importer :par"), EmojiQuery(before: "Shipped the importer ", query: "par"))
        XCTAssertEqual(EmojiQuery.at(":pa"), EmojiQuery(before: "", query: "pa"))
        XCTAssertEqual(EmojiQuery.at("x\n:party_b")?.query, "party_b")
        // A time and a face are left alone; so is a colon with nothing after it.
        XCTAssertNil(EmojiQuery.at("at 10:30"))
        XCTAssertNil(EmojiQuery.at("nice :)"))
        XCTAssertNil(EmojiQuery.at("say :"))
        XCTAssertNil(EmojiQuery.at(":par ty"))
        XCTAssertNil(EmojiQuery.at("no colon"))
        XCTAssertEqual(EmojiQuery.at("Shipped :par")?.picking("partyparrot"), "Shipped :partyparrot: ")
        XCTAssertEqual(emojiSearchPrefix(" :par: "), "par")
        XCTAssertEqual(emojiSearchPrefix("::"), "")
    }

    func testAChoiceIsTypedAsItselfOrItsNameAndASpace() {
        XCTAssertEqual(PickedEmoji.unicode("👍").typed, "👍")
        XCTAssertEqual(PickedEmoji.custom(summary("e8", "partyparrot")).typed, ":partyparrot: ")
        XCTAssertEqual(PickedEmoji.custom(summary("e8", "partyparrot")).kept(serverID: "acme"), "emoji:acme:e8")
        XCTAssertEqual(PickedEmoji.unicode("👍").kept(serverID: "acme"), "👍")
    }

    func testFrequentlyUsedHoldsAServersEmojiInThatServerOnly() {
        let recent = ["👀", FrequentEmoji.custom(serverID: "acme", emojiID: "e8"), FrequentEmoji.custom(serverID: "beta", emojiID: "b1"), FrequentEmoji.custom(serverID: "acme", emojiID: "gone")]
        let heldInAcme: (String) -> Bool = { $0 == "e8" }
        let acme = FrequentEmoji.list(recent: recent, serverID: "acme", held: heldInAcme)
        XCTAssertEqual(Array(acme.prefix(2)), ["👀", "emoji:acme:e8"])
        XCTAssertFalse(acme.contains("emoji:beta:b1"))
        XCTAssertFalse(acme.contains("emoji:acme:gone"))
        // In a personal conversation, Unicode's alone; the quick six after.
        let home = FrequentEmoji.list(recent: recent, serverID: "", held: { _ in true })
        XCTAssertEqual(home, ["👀"] + Replies.quick)
        XCTAssertEqual(FrequentEmoji.customParts("emoji:acme:e8")?.emojiID, "e8")
        XCTAssertNil(FrequentEmoji.customParts("👍"))
        XCTAssertNil(FrequentEmoji.customParts("emoji::e8"))
    }

    func testAChipOfARemovedEmojiTakesOnlyThisAccountsBack() {
        let drawing = EmojiDrawing(emojiID: "e8", name: "partyparrot", width: 128, height: 128, animated: true, picture: nil)
        let held = ReactionDisplay(emoji: ":partyparrot:", count: 5, me: false, words: "", emojiID: "e8", custom: drawing)
        XCTAssertEqual(held.id, "e8")
        XCTAssertTrue(held.tappable)
        let removed = ReactionDisplay(emoji: ":oldlogo:", count: 4, me: false, words: "", emojiID: "e0")
        XCTAssertFalse(removed.tappable)
        XCTAssertTrue(ReactionDisplay(emoji: ":oldlogo:", count: 1, me: true, words: "", emojiID: "e0").tappable)
        let unicode = ReactionDisplay(emoji: "🎉", count: 3, me: false, words: "")
        XCTAssertEqual(unicode.id, "🎉")
        XCTAssertTrue(unicode.tappable)
    }

    func testADrawingKeepsItsShape() {
        let wide = EmojiDrawing(emojiID: "e2", name: "lgtm", width: 128, height: 64, animated: false, picture: nil)
        XCTAssertEqual(wide.fitted(in: 32), CGSize(width: 32, height: 16))
        XCTAssertEqual(wide.width(forHeight: 19.5), 39)
        XCTAssertEqual(wide.label, ":lgtm:")
        let tall = EmojiDrawing(emojiID: "e", name: "t", width: 64, height: 128, animated: false, picture: nil)
        XCTAssertEqual(tall.fitted(in: 32), CGSize(width: 16, height: 32))
    }

    private func image() -> CGImage {
        CGContext(data: nil, width: 2, height: 2, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!.makeImage()!
    }

    func testAPlayingPictureShowsTheFrameItsTimeFallsIn() {
        let frames = [image(), image(), image()]
        let picture = EmojiPicture(frames: frames, delays: [0.1, 0.2, 0.1])
        XCTAssertTrue(picture.animated)
        XCTAssertEqual(picture.duration, 0.4, accuracy: 1e-9)
        XCTAssertEqual(picture.shortest, 0.1, accuracy: 1e-9)
        XCTAssertTrue(picture.frame(at: nil) === frames[0], "still under Reduce Motion: the first frame")
        XCTAssertTrue(picture.frame(at: 0.05) === frames[0])
        XCTAssertTrue(picture.frame(at: 0.15) === frames[1])
        XCTAssertTrue(picture.frame(at: 0.35) === frames[2])
        XCTAssertTrue(picture.frame(at: 0.45) === frames[0], "it loops")
        let still = EmojiPicture(frames: [frames[0]], delays: [0])
        XCTAssertFalse(still.animated)
        XCTAssertTrue(still.frame(at: 3) === frames[0])
    }

    func testTheInstancesPicturesDecode() throws {
        // The core's own fixtures: a still PNG and an animated GIF.
        let fixtures = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../../../../core/crates/lumia-client/tests/fixtures").standardized
        let png = try XCTUnwrap(EmojiPicture.decode(Data(contentsOf: fixtures.appendingPathComponent("logo.png"))))
        XCTAssertFalse(png.animated)
        let gif = try XCTUnwrap(EmojiPicture.decode(Data(contentsOf: fixtures.appendingPathComponent("animated.gif"))))
        XCTAssertTrue(gif.animated)
        XCTAssertTrue(gif.delays.allSatisfy { $0 >= 0.02 })
        XCTAssertNil(EmojiPicture.decode(Data("not a picture".utf8)))
        XCTAssertNotNil(EmojiPicture.preview(try Data(contentsOf: fixtures.appendingPathComponent("animated.gif"))))
    }

    @MainActor
    func testAPictureThatCannotBeHadIsItsName() async {
        let pictures = EmojiPictures()
        pictures.fetch = { _ in throw URLError(.notConnectedToInternet) }
        let url = URL(string: "https://chat.acme.dev/api/v1/avatars/" + String(repeating: "b", count: 64))!
        XCTAssertNil(pictures.picture(url))
        await pictures.load(url)
        XCTAssertTrue(pictures.failed(url))
        pictures.keep(only: [])
        XCTAssertFalse(pictures.failed(url), "tried again after the servers' emoji change")
    }

    /// A picture that could not be had (the instance busy, the network gone
    /// a moment) is its name meanwhile and is fetched again later by itself,
    /// not only when some server's emoji next change.
    @MainActor
    func testAPictureThatCouldNotBeHadIsTriedAgainLater() async throws {
        let png = try Data(contentsOf: fixtures.appendingPathComponent("logo.png"))
        final class Tries: @unchecked Sendable { var n = 0 }
        let tries = Tries()
        let pictures = EmojiPictures()
        pictures.retryAfter = { _ in .milliseconds(50) }
        pictures.fetch = { _ in
            tries.n += 1
            if tries.n == 1 { throw URLError(.timedOut) }
            return png
        }
        let url = URL(string: "https://chat.acme.dev/api/v1/avatars/" + String(repeating: "c", count: 64))!
        await pictures.load(url)
        XCTAssertTrue(pictures.failed(url), "drawn as its name while it cannot be had")
        for _ in 0..<40 where pictures.picture(url) == nil { try await Task.sleep(for: .milliseconds(50)) }
        XCTAssertNotNil(pictures.picture(url), "a picture that failed once was never fetched again (\(tries.n) tries)")
        XCTAssertFalse(pictures.failed(url))

        // A reconnect tries every one that failed at once, whatever the wait.
        let other = URL(string: "https://chat.acme.dev/api/v1/avatars/" + String(repeating: "d", count: 64))!
        tries.n = 0
        pictures.retryAfter = { _ in .seconds(3600) }
        await pictures.load(other)
        XCTAssertTrue(pictures.failed(other))
        pictures.retryFailed()
        for _ in 0..<40 where pictures.picture(other) == nil { try await Task.sleep(for: .milliseconds(50)) }
        XCTAssertNotNil(pictures.picture(other), "a reconnect did not try it again")
    }

    private var fixtures: URL {
        URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../../../../core/crates/lumia-client/tests/fixtures").standardized
    }

    private func server(_ id: String, emoji: [EmojiSummary], limit: UInt32 = 50) -> ServerSummary {
        ServerSummary(serverId: id, name: "Acme", iconUrl: "", retentionDays: 0, showDeleted: false, attachmentRetentionDays: 90, deleteOldestWhenFull: false, canManage: true, canManageChannels: true, canStartDms: true, canMentionEveryone: true, canManageMessages: true, channelAddsByInvitation: false, serverDmsOff: false, ownerAccountId: "me", isOwner: true, canManageRoles: true, canManageMembers: true, tagTitles: false, roles: [], grantable: [], roleIds: [], title: "", guest: false, emoji: emoji, emojiLimit: limit, canManageEmoji: true)
    }

    private func snapshot(_ servers: [ServerSummary]) -> SnapshotSummary {
        SnapshotSummary(accountId: "me", handle: "me", displayName: "Me", avatarUrl: "", servers: servers, channels: [], pendingRequests: [], channelInvitations: [], readStates: [], rail: [])
    }

    /// A session over an instance that answers nothing, so a snapshot asked
    /// for over HTTP fails, as one that times out does.
    @MainActor
    private func offline() throws -> (LumiaSession, LumiaClient, URL) {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        let client = try LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: dir)
        return (LumiaSession(client: client, host: "127.0.0.1:1", signedIn: true, hasIdentity: false), client, dir)
    }

    /// `EmojiChanged` is said once the core has written the server's emoji
    /// from `EmojiUpdated` into what it keeps (specs/emoji.md 8): the session
    /// takes them from there, whether or not the snapshot asked for on the
    /// account event before it came. Here that snapshot fails, and the core,
    /// holding no emoji of Acme, says it has none: :partyparrot: goes.
    @MainActor
    func testAnEmojiChangeTakesTheServersEmojiAsTheCoreHoldsThem() async throws {
        let (session, client, dir) = try offline()
        defer { try? FileManager.default.removeItem(at: dir) }
        session.take(snapshot([server("acme", emoji: [summary("e8", "partyparrot")])]))
        XCTAssertNotNil(session.drawing(emojiID: "e8", in: "acme"))
        await session.handle(.accountApplied(count: 1))
        await session.handle(.emojiChanged(serverId: "acme"))
        let kept = await client.serverEmoji(serverID: "acme")
        XCTAssertEqual(session.emoji(of: "acme"), kept, "the session holds Acme's emoji as they were before the change, not as the core has them")
        XCTAssertNil(session.drawing(emojiID: "e8", in: "acme"), "a removed emoji is still drawn and offered")
    }

    private func elsewhere(_ instance: String, _ method: String, _ path: String, token: String? = nil, body: [String: Any]? = nil) async throws -> (status: Int, json: [String: Any]) {
        var request = URLRequest(url: URL(string: "\(instance)/api/v1\(path)")!)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "accept")
        if let token { request.setValue("Bearer \(token)", forHTTPHeaderField: "authorization") }
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "content-type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:])
    }

    /// A UUIDv7, as a command's id must be.
    private func uuid7() -> String {
        var b = [UInt8](repeating: 0, count: 16)
        let ms = UInt64(Date().timeIntervalSince1970 * 1000)
        for i in 0..<6 { b[i] = UInt8((ms >> (8 * (5 - i))) & 0xff) }
        for i in 6..<16 { b[i] = UInt8.random(in: 0...255) }
        b[6] = 0x70 | (b[6] & 0x0f)
        b[8] = 0x80 | (b[8] & 0x3f)
        return UUID(uuid: (b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15])).uuidString.lowercased()
    }

    // Against a running instance (`LUMIA_TEST_INSTANCE`): the Mentions page
    // is read over HTTP and kept by no store, so the core cannot name its
    // reactions anew on `EmojiUpdated`. A message mentioning the account
    // carries a reaction with Acme's :partyparrot:, which is renamed
    // :dancer: and then removed; at each `emojiChanged` the page held reads
    // the chip by the name the instance now gives it.
    @MainActor
    func testAnEmojiChangeReadsTheMentionsPagesAgain() async throws {
        guard let instance = ProcessInfo.processInfo.environment["LUMIA_TEST_INSTANCE"], let base = URL(string: instance) else {
            throw XCTSkip("LUMIA_TEST_INSTANCE names no instance")
        }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let stamp = UInt64(Date().timeIntervalSince1970 * 1000)
        let password = "a password for the kit's test"
        let owner = await LumiaSession(client: try LumiaClient(baseURL: base, dataDirectory: dir.appendingPathComponent("owner")))
        try await owner.createAccount(handle: "owner\(stamp)", password: password)
        let serverID = try await owner.client.createServer(name: "Acme").serverId
        let channelID = try await owner.client.createChannel(serverID: serverID, name: "general", isPrivate: false).channelId
        let invite = try await owner.client.createInvite(serverID: serverID)
        let member = await LumiaSession(client: try LumiaClient(baseURL: base, dataDirectory: dir.appendingPathComponent("member")))
        try await member.createAccount(handle: "member\(stamp)", password: password, invite: invite.url)
        _ = try await member.client.joinChannel(channelID: channelID)
        let png = try Data(contentsOf: fixtures.appendingPathComponent("logo.png"))
        try await owner.uploadEmoji(serverID: serverID, name: "partyparrot", picture: png, mediaType: "image/png")
        await owner.refreshSnapshot()
        let emojiID = try XCTUnwrap(owner.emoji(of: serverID).first?.emojiId)

        let signedIn = try await elsewhere(instance, "POST", "/auth/sign-in/password", body: ["handleOrEmail": "member\(stamp)", "password": password]).json
        let token = try XCTUnwrap(signedIn["accessToken"] as? String)
        let body = "Shipped, @owner\(stamp)"
        let start = body.utf8.count - "@owner\(stamp)".utf8.count
        let sent = try await elsewhere(instance, "POST", "/channels/\(channelID)/commands", token: token, body: ["commandId": uuid7(), "channelId": channelID, "sendMessage": ["body": body, "mentions": [["accountId": owner.me, "start": start, "end": body.utf8.count]]]])
        XCTAssertEqual(sent.status / 100, 2, "\(sent.json)")
        await owner.loadMentions(serverID: nil, more: false)
        let messageID = try XCTUnwrap(owner.mentionsPages[""]?.messages.first?.messageId, "the mention is not on the page")
        let reacted = try await elsewhere(instance, "POST", "/channels/\(channelID)/commands", token: token, body: ["commandId": uuid7(), "channelId": channelID, "addReaction": ["messageId": messageID, "emojiId": emojiID]])
        XCTAssertEqual(reacted.status / 100, 2, "\(reacted.json)")
        await owner.loadMentions(serverID: nil, more: false)
        func chip() -> String? { owner.mentionsPages[""]?.messages.first?.reactions.first { $0.emojiId == emojiID }?.reaction }
        XCTAssertEqual(chip(), ":partyparrot:")

        try await owner.renameEmoji(serverID: serverID, emojiID: emojiID, name: "dancer")
        await owner.handle(.emojiChanged(serverId: serverID))
        XCTAssertEqual(chip(), ":dancer:", "the Mentions page reads the chip by the emoji's old name after a rename")
        try await owner.removeEmoji(serverID: serverID, emojiID: emojiID)
        await owner.handle(.emojiChanged(serverId: serverID))
        XCTAssertEqual(chip(), ":dancer:", "a removed emoji's chip on the Mentions page is not named as it was last called")
        XCTAssertTrue(owner.drawing(emojiID: emojiID, in: serverID) == nil, "a removed emoji is drawn")
    }

    /// What the `:` list or the picker drew may be renamed or removed before
    /// it is chosen (its name even taken by another): what goes in is the
    /// emoji chosen by its id as the server has it now, under its present
    /// name, which is the name the core finds at the send; or nothing.
    @MainActor
    func testAChoiceGoesInAsTheServersEmojiNowAre() throws {
        let (session, _, dir) = try offline()
        defer { try? FileManager.default.removeItem(at: dir) }
        session.take(snapshot([server("acme", emoji: [summary("e8", "partyparrot"), summary("e9", "parrotwave")])]))
        let drawn = PickedEmoji.custom(summary("e8", "partyparrot"))
        session.take(snapshot([server("acme", emoji: [summary("e8", "dancer"), summary("e9", "parrotwave")])]))
        XCTAssertEqual(session.current(drawn, in: "acme")?.typed, ":dancer: ", "a renamed emoji goes in by the name it no longer has")
        session.take(snapshot([server("acme", emoji: [summary("e9", "parrotwave"), summary("e10", "partyparrot")])]))
        XCTAssertNil(session.current(drawn, in: "acme"), "a removed emoji goes in by a name that is now another emoji's")
        XCTAssertNil(session.current(.custom(summary("e9", "parrotwave")), in: "beta"), "another server's emoji goes in")
        XCTAssertEqual(session.current(.unicode("👍"), in: "acme"), .unicode("👍"))
    }

    /// A list of a server's emoji (the `:` list, the picker's search) is
    /// shown only for what is asked now: another server, other words, or the
    /// server's emoji changed since show none of it until asked again.
    func testAListIsShownOnlyForWhatIsAskedNow() {
        let held = [summary("e8", "partyparrot"), summary("e9", "parrotwave")]
        let asked = EmojiAsked(serverID: "acme", words: "par", held: held)
        let found = EmojiFound(asked: asked, rows: held)
        XCTAssertEqual(found.rows(for: asked), held)
        XCTAssertEqual(found.rows(for: EmojiAsked(serverID: "acme", words: "parr", held: held)), [])
        XCTAssertEqual(found.rows(for: EmojiAsked(serverID: "beta", words: "par", held: held)), [])
        XCTAssertEqual(found.rows(for: EmojiAsked(serverID: "acme", words: "par", held: [held[0]])), [], "a removal since")
        XCTAssertEqual(found.rows(for: EmojiAsked(serverID: "acme", words: "par", held: [summary("e8", "dancer"), held[1]])), [], "a rename since")
        XCTAssertEqual(EmojiFound().rows(for: asked), [])
    }

    /// A picture over the instance's 8 MB is refused before it is read
    /// whole or drawn, from the photo picker as from Files; one under is
    /// read, named by its file and drawn.
    func testAPictureOverTheLimitIsRefusedBeforeItIsReadOrDrawn() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let png = try Data(contentsOf: fixtures.appendingPathComponent("logo.png"))
        // A picture that draws, made larger than the limit (as a 48-megapixel photo is).
        var big = png
        big.append(Data(count: ServerSettingsRefusal.largestPicture + 1 - png.count))
        let large = dir.appendingPathComponent("IMG_0001.png")
        try big.write(to: large)
        switch await EmojiUploadFile.read(large) {
        case let .refused(words): XCTAssertEqual(words, EmojiWords.tooLarge)
        case let .chosen(file): XCTFail("a picture of \(file.data.count) bytes was read and drawn (preview \(file.preview == nil ? "none" : "made")) before Save refuses it")
        }
        switch await EmojiUploadFile.make(data: big, fileName: "emoji.png") {
        case let .refused(words): XCTAssertEqual(words, EmojiWords.tooLarge)
        case .chosen: XCTFail("the photo picker's data over the limit was drawn")
        }
        let small = dir.appendingPathComponent("Acme-Heart v2.png")
        try png.write(to: small)
        guard case let .chosen(file) = await EmojiUploadFile.read(small) else { return XCTFail("a small picture was refused") }
        XCTAssertEqual(file.fileName, "Acme-Heart v2.png")
        XCTAssertEqual(file.mediaType, "image/png")
        XCTAssertEqual(file.data, png)
        XCTAssertNotNil(file.preview)
    }

    @MainActor
    func testTheCaretNotKnownPutsAChoiceAtTheEnd() {
        let caret = ComposerCaret()
        XCTAssertEqual(caret.insert(":partyparrot: ", into: "Shipped "), "Shipped :partyparrot: ")
    }

    func testTheSectionsWords() {
        XCTAssertEqual(EmojiWords.count(12, limit: 50), "12 of 50")
        XCTAssertFalse(EmojiWords.atLimit(count: 12, limit: 50))
        XCTAssertTrue(EmojiWords.atLimit(count: 50, limit: 50))
        XCTAssertTrue(EmojiWords.atLimit(count: 51, limit: 50), "a lowered limit keeps what is there and takes no more")
        // A limit of 0 is no room, never no limit.
        XCTAssertEqual(EmojiWords.count(0, limit: 0), "0 of 0")
        XCTAssertTrue(EmojiWords.atLimit(count: 0, limit: 0))
        XCTAssertEqual(EmojiWords.lead(server: "Acme", count: 0, limit: 0), "Acme has as many emoji as it may (0).")
        XCTAssertEqual(EmojiWords.lead(server: "Acme", count: 50, limit: 50), "Acme has as many emoji as it may (50). Remove one to upload another.")
        XCTAssertTrue(EmojiWords.lead(server: "Acme", count: 12, limit: 50).hasPrefix("Used as :name: in Acme’s channels"))
        XCTAssertEqual(EmojiWords.uploaded(by: "Devon Park", animated: true), "Uploaded by Devon Park · Animated")
        XCTAssertEqual(EmojiWords.uploaded(by: "Maya Chen", animated: false), "Uploaded by Maya Chen")
        XCTAssertEqual(EmojiWords.removeTitle("parrotwave"), "Remove :parrotwave:?")
        XCTAssertEqual(EmojiWords.nameHint("acme_heart_v2"), "2 to 32 letters, digits and _. It is typed as :acme_heart_v2: in a message.")
        XCTAssertEqual(EmojiWords.sent(" PartyParrot "), "partyparrot")
        // The core's name from a file, as the form starts from it.
        XCTAssertEqual(emojiNameFromFile(fileName: "Acme-Heart v2.png"), "acme_heart_v2")
    }

    func testRefusalsInTheBoardsWordsWhereTheyWereTried() {
        func said(_ code: String, _ details: [String: String]) -> (EmojiRefusalPlace, String) {
            let r = EmojiRefusal.words(code: code, details: details, message: "raw", server: "Acme", name: "partyparrot")
            return (r.place, r.words)
        }
        XCTAssertTrue(said("ALREADY_EXISTS", ["existing_id": "x"]) == (.name, "Acme already has :partyparrot:. Choose another name."))
        XCTAssertTrue(said("INVALID_ARGUMENT", ["field": "name"]) == (.name, "A name is 2 to 32 of a to z, 0 to 9 and _, with no spaces."))
        XCTAssertTrue(said("INVALID_ARGUMENT", ["field": "picture", "reason": "too_large"]) == (.picture, "This picture is too large for an emoji. An animated one can have at most 200 frames and come to 512 KB once made."))
        XCTAssertTrue(said("INVALID_ARGUMENT", ["field": "picture", "reason": "unsupported"]) == (.picture, "Lumia can’t use this file. Choose a PNG, JPEG, WebP or GIF."))
        XCTAssertTrue(said("INVALID_ARGUMENT", ["field": "picture", "reason": "invalid"]) == (.picture, "This picture couldn’t be read."))
        XCTAssertTrue(said("RESOURCE_EXHAUSTED", ["state": "emoji", "limit": "50"]) == (.form, "Acme has as many emoji as it may (50). Remove one to upload another."))
        XCTAssertTrue(said("PERMISSION_DENIED", [:]) == (.form, "You can’t manage Acme’s emoji."))
        XCTAssertTrue(said("NOT_FOUND", [:]) == (.form, "That emoji has been removed."))
        XCTAssertTrue(said("SOMETHING", [:]) == (.form, "raw"))
        // Before anything is sent: the name's rule, and a picture over 8 MB.
        XCTAssertEqual(EmojiRefusal.before(name: "party-time", bytes: 10)?.words, EmojiWords.nameRule)
        XCTAssertEqual(EmojiRefusal.before(name: "p", bytes: 10)?.words, EmojiWords.nameRule)
        XCTAssertNil(EmojiRefusal.before(name: "Party_Time", bytes: 10), "capitals are lowered, as the instance does")
        XCTAssertEqual(EmojiRefusal.before(name: "fireworks", bytes: 9 * 1024 * 1024)?.place, .picture)
    }
}
