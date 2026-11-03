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
}
