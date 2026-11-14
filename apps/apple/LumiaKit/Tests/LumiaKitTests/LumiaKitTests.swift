// The kit's pure helpers, and that the core opens through the bindings.
import Foundation
@testable import LumiaKit
import LumiaFFI
import XCTest

final class LumiaKitTests: XCTestCase {
    func testInitialsAndTones() {
        XCTAssertEqual(initials(of: "Ada Lovelace"), "AL")
        XCTAssertEqual(initials(of: "ada"), "A")
        XCTAssertEqual(initials(of: "  "), "?")
        XCTAssertEqual(AvatarTone.of("abc"), AvatarTone.of("abc"))
    }

    // A person's picture: this account's own from its snapshot, anyone
    // else's from the servers' lists, and a profile looked up only when no
    // server lists them; only the instance's own pictures are fetched.
    func testAPersonsPictureIsOwnThenListedThenLookedUp() {
        let base = URL(string: "https://chat.example.com")!
        let own = "/api/v1/avatars/" + String(repeating: "a", count: 64)
        let ada = "/api/v1/avatars/" + String(repeating: "b", count: 64)
        let grace = "/api/v1/avatars/" + String(repeating: "c", count: 64)
        func person(_ id: String, _ avatar: String) -> MemberSummary {
            MemberSummary(accountId: id, handle: id, displayName: id, nickname: "", pronouns: "", bio: "", avatarUrl: avatar, channelAddsByInvitation: false, serverDmsOff: false)
        }
        let listed = ["me": person("me", "/api/v1/avatars/stale"), "ada": person("ada", ada), "bo": person("bo", "")]
        let profiles = ["ada": person("ada", "/api/v1/avatars/profile"), "bo": person("bo", grace), "grace": person("grace", grace), "eve": person("eve", "https://elsewhere.example/eve.png")]
        var askedFor: [String] = []
        func path(_ id: String) -> String {
            avatarPath(of: id, me: "me", own: own, listed: { listed[$0] }, lookedUp: { askedFor.append($0); return profiles[$0] })
        }
        XCTAssertEqual(path("me"), own, "this account's own, from the snapshot, before any list")
        XCTAssertEqual(path("ada"), ada, "a server's list before a profile")
        XCTAssertEqual(path("bo"), "", "listed with none: none, not a profile's")
        XCTAssertEqual(path("grace"), grace, "no server lists her: her profile's")
        XCTAssertEqual(path("nobody"), "")
        XCTAssertEqual(path(""), "", "no account")
        XCTAssertEqual(askedFor, ["grace", "nobody"], "profiles are looked up only for those no server lists")
        XCTAssertEqual(pictureURL(path("grace"), base: base)?.absoluteString, "https://chat.example.com\(grace)")
        XCTAssertNil(pictureURL(path("eve"), base: base), "a picture named on another host is not fetched")
        XCTAssertNil(pictureURL(path("bo"), base: base))
    }

    func testTokensResolve() {
        XCTAssertEqual(LumiaTokens.space16, 16)
        XCTAssertNotEqual(LumiaTokens.bg.light, LumiaTokens.bg.dark)
    }

    func testClientOpensAndReportsTransportFailures() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        let client = try LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: dir)
        let host = await client.host
        XCTAssertTrue(host.hasPrefix("127.0.0.1"), host)
        let signedIn = await client.signedIn
        XCTAssertFalse(signedIn)
        // Nothing listens on port 1: the failure is the transport's, as a readable message.
        do {
            _ = try await client.instanceInfo()
            XCTFail("instance info from a closed port")
        } catch {
            XCTAssertNil(error.lumiaCode)
            XCTAssertTrue(error.lumiaMessage.contains("127.0.0.1:1"), error.lumiaMessage)
        }
        try? FileManager.default.removeItem(at: dir)
    }

    // The four codes an instance answers a call on a dead session with
    // (protocol 10 and 12), and the core's own two words for the same.
    func testWhichFailuresSayTheSessionIsOver() {
        for code in ["UNAUTHENTICATED", "TOKEN_EXPIRED", "SESSION_REVOKED", "DEVICE_REVOKED"] {
            XCTAssertTrue(LumiaError.Api(code: code, detail: "session revoked", details: [:]).lumiaSignedOut, code)
        }
        XCTAssertTrue(LumiaError.DeviceRevoked(rosterVersion: 3).lumiaSignedOut)
        XCTAssertTrue(LumiaError.NotSignedIn.lumiaSignedOut)
        for code in ["FAILED_PRECONDITION", "PERMISSION_DENIED", "NOT_FOUND", "RATE_LIMITED", "IDENTITY_PROOF_INVALID", "INTERNAL", "UNAVAILABLE"] {
            XCTAssertFalse(LumiaError.Api(code: code, detail: "no", details: [:]).lumiaSignedOut, code)
        }
        XCTAssertFalse(LumiaError.Other(detail: "transport: error sending request").lumiaSignedOut)
        XCTAssertFalse(URLError(.notConnectedToInternet).lumiaSignedOut)
    }

    // Against a running instance with open registration
    // (`LUMIA_TEST_INSTANCE=http://localhost:4000 swift test`; skipped
    // without it): a session revoked from elsewhere (protocol 12) ends here
    // as signing out does, and the sign-in screen has its line to say, once.
    @MainActor
    func testARevokedSessionEndsAndSaysWhy() async throws {
        guard let instance = ProcessInfo.processInfo.environment["LUMIA_TEST_INSTANCE"], let base = URL(string: instance) else {
            throw XCTSkip("LUMIA_TEST_INSTANCE names no instance")
        }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let handle = "gone\(UInt64(Date().timeIntervalSince1970 * 1000))"
        let password = "a password for the kit's test"
        let session = await LumiaSession(client: try LumiaClient(baseURL: base, dataDirectory: dir))
        try await session.createAccount(handle: handle, password: password)
        await session.load()
        XCTAssertEqual(session.snapshot?.handle, handle)
        try await wait(for: "the gateway's welcome") { session.connection == .connected }

        // Signed in beside it, the test revokes every session but its own.
        let token = try await signInBeside(instance, handle: handle, password: password)
        let others = try await otherSessions(instance, token: token)
        XCTAssertEqual(others.count, 1)
        for other in others {
            let status = try await call(instance, "DELETE", "/sessions/\(other)", token: token).1
            XCTAssertEqual(status, 204)
        }

        // The gateway is told and its loop ends; the session asks for the
        // snapshot, is refused, and ends.
        try await wait(for: "the session to end") { !session.signedIn }
        XCTAssertNil(session.snapshot)
        XCTAssertEqual(session.connection, .idle)
        let held = await session.client.signedIn
        XCTAssertFalse(held)
        XCTAssertEqual(session.takeEndedNotice(), LumiaSession.signedOutNotice)
        XCTAssertNil(session.takeEndedNotice())
    }

    // The phone's token goes to the instance once per value; one the
    // instance did not take is still to be sent, and signing out starts over.
    func testATokenIsSentOnceAndAgainAfterSigningOut() {
        var ledger = PushLedger()
        XCTAssertTrue(ledger.claim("a"))
        XCTAssertFalse(ledger.claim("a"), "the same token, asked again")
        XCTAssertTrue(ledger.claim("b"), "the system gave another")
        XCTAssertFalse(ledger.claim("b"))
        ledger.failed("a")
        XCTAssertFalse(ledger.claim("b"), "an older token's failure says nothing of the one held")
        ledger.failed("b")
        XCTAssertTrue(ledger.claim("b"), "the instance did not take it")
        ledger.forget()
        XCTAssertTrue(ledger.claim("b"), "signed out, whoever signs in next is subscribed again")
    }

    /// What a test's session was asked of its client, and what the test has it answer.
    @MainActor
    private final class Asked {
        var calls: [LumiaSession.ClientCall] = []
        var refused: Set<LumiaSession.ClientCall> = []
        /// A call held until the test lets it go, as one on a slow network.
        var held: LumiaSession.ClientCall?
        var letGo: CheckedContinuation<Void, Never>?
    }

    /// A session that found its client signed in, over a client nothing
    /// answers, whose sign-out and push calls go to `asked` instead.
    @MainActor
    private func signedInSession(_ asked: Asked) throws -> (LumiaSession, URL) {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        let client = try LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: dir)
        let session = LumiaSession(client: client, host: "127.0.0.1:1", signedIn: true, hasIdentity: false)
        session.ask = { call in
            asked.calls.append(call)
            if asked.held == call { await withCheckedContinuation { asked.letGo = $0 } }
            if asked.refused.contains(call) { throw LumiaError.Other(detail: "transport: error sending request") }
        }
        return (session, dir)
    }

    // Signing out takes this phone's push subscription back while the
    // session can still say so, then revokes the session; a second sign-out
    // while the first is on its way does nothing, nor does one after it, and
    // the sign-in screen has no line to say.
    @MainActor
    func testSigningOutTakesThePushSubscriptionBackFirstAndHappensOnce() async throws {
        let asked = Asked()
        let (session, dir) = try signedInSession(asked)
        defer { try? FileManager.default.removeItem(at: dir) }
        await session.pushSubscribe(token: "c0ffee")
        await session.pushSubscribe(token: "c0ffee")
        XCTAssertEqual(asked.calls, [.pushSubscribe(token: "c0ffee")])

        asked.held = .pushUnsubscribe
        let first = Task { await session.signOut() }
        try await wait(for: "the subscription to be taken back") { asked.letGo != nil }
        XCTAssertTrue(session.signedIn, "the session is still there to ask with")
        await session.signOut()
        await session.pushSubscribe(token: "decaf")
        XCTAssertEqual(asked.calls, [.pushSubscribe(token: "c0ffee"), .pushUnsubscribe], "nothing more while one is on its way")
        asked.letGo?.resume()
        await first.value

        XCTAssertEqual(asked.calls, [.pushSubscribe(token: "c0ffee"), .pushUnsubscribe, .signOut])
        XCTAssertFalse(session.signedIn)
        XCTAssertNil(session.takeEndedNotice(), "nobody was sent back to sign-in")
        XCTAssertEqual(session.pushLedger, PushLedger(), "the next sign-in subscribes again")
        await session.signOut()
        await session.pushSubscribe(token: "c0ffee")
        XCTAssertEqual(asked.calls.count, 3, "signed out, nothing is asked")
    }

    // A subscription that cannot be taken back (no network, a session the
    // instance has just ended) does not keep the person signed in, and
    // neither does a refused revocation.
    @MainActor
    func testSigningOutGoesOnWhenTheInstanceDoesNotAnswer() async throws {
        let asked = Asked()
        let (session, dir) = try signedInSession(asked)
        defer { try? FileManager.default.removeItem(at: dir) }
        asked.refused = [.pushUnsubscribe, .signOut]
        await session.signOut()
        XCTAssertEqual(asked.calls, [.pushUnsubscribe, .signOut])
        XCTAssertFalse(session.signedIn)
        XCTAssertNil(session.takeEndedNotice())
    }

    // A token the instance did not take is sent the next time; and one on
    // its way when the person signs out lands before it is taken back, not after.
    @MainActor
    func testATokenOnItsWayLandsBeforeItIsTakenBack() async throws {
        let asked = Asked()
        let (session, dir) = try signedInSession(asked)
        defer { try? FileManager.default.removeItem(at: dir) }
        asked.refused = [.pushSubscribe(token: "c0ffee")]
        await session.pushSubscribe(token: "c0ffee")
        asked.refused = []
        asked.held = .pushSubscribe(token: "c0ffee")
        let sending = Task { await session.pushSubscribe(token: "c0ffee") }
        try await wait(for: "the token to be sent again") { asked.letGo != nil }
        let leaving = Task { await session.signOut() }
        try await Task.sleep(for: .milliseconds(200))
        XCTAssertEqual(asked.calls, [.pushSubscribe(token: "c0ffee"), .pushSubscribe(token: "c0ffee")], "signing out waits for the token")
        asked.letGo?.resume()
        await sending.value
        await leaving.value
        XCTAssertEqual(asked.calls.suffix(2), [.pushUnsubscribe, .signOut])
        XCTAssertFalse(session.signedIn)
    }

    // Against a running instance (`LUMIA_TEST_INSTANCE`, as above): signing
    // out leaves the instance without the session, the phone without what
    // it held of the account and with no line for the sign-in screen, and
    // the same session signs in again and is subscribed again.
    @MainActor
    func testSigningOutEndsTheSessionOnTheInstance() async throws {
        guard let instance = ProcessInfo.processInfo.environment["LUMIA_TEST_INSTANCE"], let base = URL(string: instance) else {
            throw XCTSkip("LUMIA_TEST_INSTANCE names no instance")
        }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let handle = "left\(UInt64(Date().timeIntervalSince1970 * 1000))"
        let password = "a password for the kit's test"
        let session = await LumiaSession(client: try LumiaClient(baseURL: base, dataDirectory: dir))
        try await session.createAccount(handle: handle, password: password)
        await session.load()
        try await wait(for: "the gateway's welcome") { session.connection == .connected }
        await session.pushSubscribe(token: "c0ffee")
        XCTAssertNotEqual(session.pushLedger, PushLedger(), "the instance did not take the token")
        let token = try await signInBeside(instance, handle: handle, password: password)
        let before = try await otherSessions(instance, token: token)
        XCTAssertEqual(before.count, 1)

        await session.signOut()
        let after = try await otherSessions(instance, token: token)
        XCTAssertEqual(after, [], "the instance still has the session")
        XCTAssertFalse(session.signedIn)
        XCTAssertNil(session.snapshot)
        XCTAssertTrue(session.people.isEmpty)
        XCTAssertEqual(session.connection, .idle)
        let held = await session.client.signedIn
        XCTAssertFalse(held)
        XCTAssertNil(session.takeEndedNotice())
        XCTAssertEqual(session.pushLedger, PushLedger())

        try await session.signIn(handle: handle, password: password, totpCode: nil)
        await session.load()
        XCTAssertEqual(session.snapshot?.handle, handle)
        await session.pushSubscribe(token: "c0ffee")
        XCTAssertNotEqual(session.pushLedger, PushLedger(), "signed in again, the token did not go again")
        let again = try await otherSessions(instance, token: token)
        XCTAssertEqual(again.count, 1)
        XCTAssertNotEqual(again, before, "the session signed out of is back")
        await session.signOut()
    }

    // A push that reaches a phone holding no session of the account becomes
    // no notice at all, whatever the instance: the extension's decision.
    func testAPushForNoSessionIsNoNotice() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let client = try LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: dir)
        XCTAssertEqual(client.notice(timeoutMs: 500), .nothing)
    }

    // Against a running instance (`LUMIA_TEST_INSTANCE`, as above): a push
    // for a signed-in phone with nothing new is the generic notice, and one
    // that arrives after the instance ended the session (a subscription that
    // outlived it) is none, though the phone has not yet heard of the end.
    @MainActor
    func testAPushAfterTheSessionEndedIsNoNotice() async throws {
        guard let instance = ProcessInfo.processInfo.environment["LUMIA_TEST_INSTANCE"], let base = URL(string: instance) else {
            throw XCTSkip("LUMIA_TEST_INSTANCE names no instance")
        }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let handle = "stale\(UInt64(Date().timeIntervalSince1970 * 1000))"
        let password = "a password for the kit's test"
        let client = try LumiaClient(baseURL: base, dataDirectory: dir)
        try await client.createAccount(handle: handle, password: password)
        let signedIn = await Task.detached { client.notice(timeoutMs: 3000) }.value
        XCTAssertEqual(signedIn, .generic)

        let token = try await signInBeside(instance, handle: handle, password: password)
        for other in try await otherSessions(instance, token: token) {
            let status = try await call(instance, "DELETE", "/sessions/\(other)", token: token).1
            XCTAssertEqual(status, 204)
        }
        let held = await client.signedIn
        XCTAssertTrue(held, "the client still holds the tokens of the ended session")
        let ended = await Task.detached { client.notice(timeoutMs: 3000) }.value
        XCTAssertEqual(ended, .nothing)
    }

    // Against a running instance (`LUMIA_TEST_INSTANCE`, as above): a
    // sign-in made while a session is held, which is what linking a phone
    // that signed in with a password is, leaves the instance with one
    // session for the phone, and the gateway carries on: the held session's
    // socket is told it was revoked, and the session here ends up connected
    // on the new one, neither sent back to sign-in nor left with a loop
    // that has ended. Both right after the loop began and well into it.
    @MainActor
    func testASignInOverAHeldSessionLeavesOneAndTheGatewayCarriesOn() async throws {
        guard let instance = ProcessInfo.processInfo.environment["LUMIA_TEST_INSTANCE"], let base = URL(string: instance) else {
            throw XCTSkip("LUMIA_TEST_INSTANCE names no instance")
        }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let handle = "swap\(UInt64(Date().timeIntervalSince1970 * 1000))"
        let password = "a password for the kit's test"
        let session = await LumiaSession(client: try LumiaClient(baseURL: base, dataDirectory: dir))
        try await session.createAccount(handle: handle, password: password)
        let server = try await session.createServer(name: "Swap")
        let channel = try await session.createChannel(serverID: server, name: "general")
        await session.load()
        try await wait(for: "the gateway's welcome") { session.connection == .connected }
        let token = try await signInBeside(instance, handle: handle, password: password)
        var held = try await otherSessions(instance, token: token)
        XCTAssertEqual(held.count, 1)

        for (round, pause) in [Duration.zero, .seconds(11)].enumerated() {
            try await Task.sleep(for: pause)
            try await session.signIn(handle: handle, password: password, totpCode: nil)
            let now = try await otherSessions(instance, token: token)
            XCTAssertEqual(now.count, 1, "round \(round): the phone has not one session but \(now.count)")
            XCTAssertNotEqual(now, held, "round \(round): the held session was not replaced")
            held = now
            // Whatever the old socket was told has arrived by now.
            try await Task.sleep(for: .seconds(3))
            XCTAssertTrue(session.signedIn, "round \(round): sent back to sign-in")
            XCTAssertNil(session.takeEndedNotice())
            try await wait(for: "round \(round): the gateway on the new session") { session.connection == .connected }
            // Connected is not said of a loop that has ended: a message goes
            // out through it and comes back with the instance's sequence.
            let body = "after the swap, round \(round)"
            try await session.sendMessage(channelID: channel, body: body)
            try await wait(for: "round \(round): the message to come back") {
                session.messages[channel]?.contains { $0.body == body && $0.seq > 0 } == true
            }
        }
        await session.signOut()
        let left = try await otherSessions(instance, token: token)
        XCTAssertEqual(left, [], "signed out, the instance still has a session of the phone")
    }

    /// One call to the instance's API as JSON, beside the client under test.
    private func call(_ instance: String, _ method: String, _ path: String, token: String? = nil, body: [String: String]? = nil) async throws -> (Data, Int) {
        var request = URLRequest(url: URL(string: "\(instance)/api/v1\(path)")!)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "accept")
        if let token { request.setValue("Bearer \(token)", forHTTPHeaderField: "authorization") }
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "content-type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        return (data, (response as? HTTPURLResponse)?.statusCode ?? 0)
    }

    /// A session of the test's own on the account, to look at the others with.
    private func signInBeside(_ instance: String, handle: String, password: String) async throws -> String {
        let pair = try JSONSerialization.jsonObject(with: try await call(instance, "POST", "/auth/sign-in/password", body: ["handleOrEmail": handle, "password": password]).0) as? [String: Any]
        return try XCTUnwrap(pair?["accessToken"] as? String)
    }

    /// The ids of the account's sessions other than the one asking.
    private func otherSessions(_ instance: String, token: String) async throws -> [String] {
        let list = try JSONSerialization.jsonObject(with: try await call(instance, "GET", "/sessions", token: token).0) as? [String: Any]
        return try (list?["sessions"] as? [[String: Any]] ?? []).filter { $0["current"] as? Bool != true }.map { try XCTUnwrap($0["sessionId"] as? String) }
    }

    @MainActor
    private func wait(for what: String, _ met: () -> Bool) async throws {
        for _ in 0..<150 where !met() { try await Task.sleep(for: .milliseconds(100)) }
        XCTAssertTrue(met(), "timed out waiting for \(what)")
    }
}
