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
        func call(_ method: String, _ path: String, token: String? = nil, body: [String: String]? = nil) async throws -> (Data, Int) {
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
        let pair = try JSONSerialization.jsonObject(with: try await call("POST", "/auth/sign-in/password", body: ["handleOrEmail": handle, "password": password]).0) as? [String: Any]
        let token = try XCTUnwrap(pair?["accessToken"] as? String)
        let list = try JSONSerialization.jsonObject(with: try await call("GET", "/sessions", token: token).0) as? [String: Any]
        let others = (list?["sessions"] as? [[String: Any]] ?? []).filter { $0["current"] as? Bool != true }
        XCTAssertEqual(others.count, 1)
        for other in others {
            let status = try await call("DELETE", "/sessions/\(try XCTUnwrap(other["sessionId"] as? String))", token: token).1
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

    @MainActor
    private func wait(for what: String, _ met: () -> Bool) async throws {
        for _ in 0..<150 where !met() { try await Task.sleep(for: .milliseconds(100)) }
        XCTAssertTrue(met(), "timed out waiting for \(what)")
    }
}
