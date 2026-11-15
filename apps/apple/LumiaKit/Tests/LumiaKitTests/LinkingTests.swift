// Approving another device as values (identity spec 4.2): what pasted
// text is worth reading, a request read by the core and said as what it
// would add, a device already in the roster, the instance's refusals in
// words, and the code read out and typed back.
import Foundation
@testable import LumiaKit
import LumiaFFI
import XCTest

final class LinkingTests: XCTestCase {
    /// A link request as a new device shows it: `lumia.v1.LinkRequest`
    /// written field by field (1 device id, 2 its key, 3 the ephemeral key,
    /// 4 kind, 5 label, 6 web origin, 7 nonce), in standard base64.
    private func request(kind: UInt8, label: String, origin: String = "") -> String {
        func field(_ number: UInt8, _ bytes: [UInt8]) -> [UInt8] { [number << 3 | 2, UInt8(bytes.count)] + bytes }
        var bytes = field(1, Array(repeating: 7, count: 16))
        bytes += field(2, field(1, Array(repeating: 8, count: 32)))
        bytes += field(3, Array(repeating: 9, count: 32))
        bytes += [4 << 3, kind]
        bytes += field(5, Array(label.utf8))
        if !origin.isEmpty { bytes += field(6, Array(origin.utf8)) }
        bytes += field(7, Array(repeating: 1, count: 32))
        return Data(bytes).base64EncodedString()
    }

    private func summary(kind: String = "native", label: String = "", origin: String = "") -> LinkRequestSummary {
        LinkRequestSummary(deviceId: "07070707-0707-0707-0707-070707070707", kind: kind, label: label, webOrigin: origin)
    }

    private func device(_ id: String, revoked: Bool = false) -> DeviceSummary {
        DeviceSummary(deviceId: id, kind: "native", label: "", webOrigin: "", addedAtMicros: 0, thisDevice: false, revoked: revoked)
    }

    // MARK: Pasted text

    func testNothingPastedSaysNothing() {
        XCTAssertEqual(LinkApproval.read(""), .empty)
        XCTAssertEqual(LinkApproval.read("  \n "), .empty)
    }

    func testTextThatCannotBeARequestIsNotAskedAbout() {
        XCTAssertNil(LinkApproval.request(in: "hello"))
        XCTAssertNil(LinkApproval.request(in: "K7PQM2XD"), "a claim code is not a request")
        XCTAssertNil(LinkApproval.request(in: String(repeating: "ab-c", count: 40)), "not base64")
        XCTAssertNil(LinkApproval.request(in: String(repeating: "A", count: 65)), "not a whole number of base64 quads")
        XCTAssertEqual(LinkApproval.read("hello"), .notARequest)
    }

    func testBase64ThatIsNotARequestIsRefusedByTheCore() {
        let text = Data(Array(repeating: 0xFF, count: 120)).base64EncodedString()
        XCTAssertNotNil(LinkApproval.request(in: text), "worth asking about")
        XCTAssertEqual(LinkApproval.read(text), .notARequest)
    }

    func testARequestIsReadWrappedOrNot() {
        let text = request(kind: 1, label: "MacBook Pro")
        // Shared through a message it may come wrapped and indented.
        let wrapped = "  " + stride(from: 0, to: text.count, by: 40).map { i in
            String(text[text.index(text.startIndex, offsetBy: i)..<text.index(text.startIndex, offsetBy: min(i + 40, text.count))])
        }.joined(separator: "\n ") + "\n"
        XCTAssertEqual(LinkApproval.request(in: wrapped), text)
        guard case let .request(read, sent) = LinkApproval.read(wrapped) else { return XCTFail("not read as a request") }
        XCTAssertEqual(sent, text, "approves the request without its whitespace")
        XCTAssertEqual(read.kind, "native")
        XCTAssertEqual(read.label, "MacBook Pro")
        XCTAssertEqual(read.deviceId, "07070707-0707-0707-0707-070707070707")
        XCTAssertEqual(LinkApproval.adds(read), "A device named “MacBook Pro”")
    }

    func testABrowsersRequestSaysItsOrigin() {
        guard case let .request(read, _) = LinkApproval.read(request(kind: 2, label: "Safari", origin: "https://my.lumia.chat")) else {
            return XCTFail("not read as a request")
        }
        XCTAssertEqual(LinkApproval.adds(read), "A browser on https://my.lumia.chat")
    }

    // MARK: What it would add

    func testWhatARequestWouldAddInWords() {
        XCTAssertEqual(LinkApproval.adds(summary(label: "  Pixel 9 ")), "A device named “Pixel 9”")
        XCTAssertEqual(LinkApproval.adds(summary()), "A device with no name")
        XCTAssertEqual(LinkApproval.adds(summary(kind: "web", label: "Firefox", origin: "https://chat.example.org")), "A browser on https://chat.example.org")
        XCTAssertEqual(LinkApproval.adds(summary(kind: "web")), "A browser")
        XCTAssertEqual(LinkApproval.adds(summary(kind: "bot", label: "deploys")), "A bot named “deploys”")
        // A kind this phone does not know is a device.
        XCTAssertEqual(LinkApproval.adds(summary(kind: "unspecified", label: "x")), "A device named “x”")
    }

    func testADeviceAlreadyInTheRosterIsNotApprovedAgain() {
        let id = summary().deviceId
        XCTAssertNil(LinkApproval.refusal(of: summary(), among: [device("11111111-1111-1111-1111-111111111111")]))
        XCTAssertEqual(LinkApproval.refusal(of: summary(), among: [device(id)]), LinkApproval.alreadyYours)
        XCTAssertEqual(LinkApproval.refusal(of: summary(), among: [device(id, revoked: true)]), LinkApproval.revoked)
    }

    // MARK: Refusals

    func testTheInstancesRefusalsInWords() {
        func api(_ code: String, _ details: [String: String] = [:]) -> Error { LumiaError.Api(code: code, detail: "refused", details: details) }
        XCTAssertEqual(LinkApproval.refusal(api("IDENTITY_PROOF_INVALID", ["step": "parse", "reason": "too_many_devices"])), LinkApproval.tooMany)
        XCTAssertEqual(LinkApproval.refusal(api("IDENTITY_PROOF_INVALID", ["step": "parse", "reason": "duplicate_device"])), LinkApproval.alreadyYours)
        XCTAssertEqual(LinkApproval.refusal(api("IDENTITY_PROOF_INVALID", ["reason": "device_both_active_and_revoked"])), LinkApproval.revoked)
        XCTAssertEqual(LinkApproval.refusal(api("CONFLICT", ["held": "AAAA"])), LinkApproval.conflict)
        XCTAssertEqual(LinkApproval.refusal(api("RATE_LIMITED")), "The instance is busy. Try again in a moment.")
        XCTAssertEqual(LinkApproval.refusal(api("IDENTITY_PROOF_INVALID", ["reason": "signature"])), "Not approved: refused")
        XCTAssertEqual(LinkApproval.refusal(LumiaError.Other(detail: "no network")), "Not approved: no network")
    }

    // MARK: The code

    func testTheCodeIsReadOutInFoursAndTypedBackWhole() {
        XCTAssertEqual(LinkApproval.spaced("K7PQM2XD"), "K7PQ M2XD")
        XCTAssertEqual(LinkApproval.spaced("k7pq m2xd"), "K7PQ M2XD")
        XCTAssertEqual(LinkApproval.spaced("ABCDE"), "ABCD E")
        XCTAssertEqual(LinkApproval.typedCode(" k7pq m2xd\n"), "K7PQM2XD")
        XCTAssertEqual(LinkApproval.typedCode("K7PQ-M2XD"), "K7PQM2XD")
    }

    // MARK: Linking by a prompt (identity spec 4.2.1)

    private func prompt(_ stage: String, digits: String = "", kind: String = "native", label: String = "Pixel 9", origin: String = "", id: String = "s1") -> LinkPrompt {
        LinkPrompt(sessionId: id, request: summary(kind: kind, label: label, origin: origin), stage: stage, digits: digits)
    }

    func testThisPhonesStagesAndWhenItAsksAgain() {
        typealias Stage = LinkByPrompt.Stage
        XCTAssertEqual(Stage(LinkProgress(stage: "waiting", digits: "")), .waiting)
        XCTAssertEqual(Stage(LinkProgress(stage: "compare", digits: "482 913")), .compare("482 913"))
        XCTAssertEqual(Stage(LinkProgress(stage: "linked", digits: "")), .linked)
        XCTAssertEqual(Stage(LinkProgress(stage: "declined", digits: "")), .declined)
        XCTAssertEqual(Stage(LinkProgress(stage: "expired", digits: "")), .expired)
        XCTAssertEqual(Stage(LinkProgress(stage: "something new", digits: "")), .waiting, "a stage not known here waits")
        XCTAssertTrue(Stage.waiting.polls)
        XCTAssertTrue(Stage.compare("482 913").polls)
        XCTAssertFalse(Stage.linked.polls, "linked goes on into the app")
        XCTAssertFalse(Stage.declined.polls)
        XCTAssertFalse(Stage.expired.polls)
        XCTAssertEqual(LinkByPrompt.pollEvery, .seconds(2))
    }

    func testTheLinkScreensWords() {
        XCTAssertEqual(LinkByPrompt.words(.waiting), "Open Lumia on a device that has your private messages. It will ask about this phone.")
        XCTAssertEqual(LinkByPrompt.words(.compare("482 913")), "Check that your other device shows the same number, then approve it there.")
        XCTAssertEqual(LinkByPrompt.words(.declined), "It wasn't approved on your other device.")
        XCTAssertEqual(LinkByPrompt.words(.expired), "That took too long.")
        XCTAssertEqual(LinkByPrompt.words(.linked), "")
        XCTAssertEqual(LinkByPrompt.startRefusal(LumiaError.Api(code: "RATE_LIMITED", detail: "slow down", details: [:])),
                       "Too many devices are waiting to be linked to your account. Try again in a few minutes.")
        XCTAssertEqual(LinkByPrompt.startRefusal(LumiaError.Other(detail: "no network")), "no network")
    }

    func testAPromptsStages() {
        XCTAssertEqual(LinkByPrompt.PromptStage(prompt("new")), .new)
        XCTAssertEqual(LinkByPrompt.PromptStage(prompt("waiting")), .waiting)
        XCTAssertEqual(LinkByPrompt.PromptStage(prompt("compare", digits: "482 913")), .compare("482 913"))
        XCTAssertEqual(LinkByPrompt.PromptStage(prompt("refused")), .refused)
        XCTAssertEqual(LinkByPrompt.PromptStage(prompt("later")), .waiting, "a stage not known here offers only Not me")
    }

    func testAPromptsTitleByStageAndDevice() {
        XCTAssertEqual(LinkByPrompt.title(prompt("new")), "Pixel 9 wants your private messages")
        XCTAssertEqual(LinkByPrompt.title(prompt("new", label: "  ")), "The new device wants your private messages")
        XCTAssertEqual(LinkByPrompt.title(prompt("new", kind: "web", label: "Firefox", origin: "https://my.lumia.chat")),
                       "A browser on https://my.lumia.chat wants your private messages", "a browser by its origin, never its label")
        XCTAssertEqual(LinkByPrompt.title(prompt("new", kind: "web", label: "")), "A browser wants your private messages")
        XCTAssertEqual(LinkByPrompt.title(prompt("waiting")), "Waiting for Pixel 9…")
        XCTAssertEqual(LinkByPrompt.title(prompt("waiting", kind: "web", label: "")), "Waiting for the browser…")
        XCTAssertEqual(LinkByPrompt.title(prompt("compare", digits: "482 913")), "Does Pixel 9 show this number?")
        XCTAssertEqual(LinkByPrompt.title(prompt("compare", digits: "482 913", label: "")), "Does the new device show this number?")
        XCTAssertEqual(LinkByPrompt.title(prompt("refused")), "This request can't be trusted. Don't approve it.")
    }

    func testWhichPromptShowsOldestFirstAndHowOneEnds() {
        let older = prompt("new", id: "a")
        let newer = prompt("compare", digits: "111 222", label: "iPad", id: "b")
        XCTAssertNil(LinkByPrompt.sheet(prompts: [], ending: nil), "no sheet")
        XCTAssertEqual(LinkByPrompt.sheet(prompts: [older, newer], ending: nil), .prompt(older), "one at a time, as the instance lists them")
        XCTAssertEqual(LinkByPrompt.sheet(prompts: [newer], ending: nil), .prompt(newer), "the next once the first is gone")
        XCTAssertEqual(LinkByPrompt.sheet(prompts: [newer], ending: .linked(older.request)), .linked("Pixel 9 is linked."), "the ending before the next")
        XCTAssertEqual(LinkByPrompt.sheet(prompts: [], ending: .linked(summary(label: ""))), .linked("The new device is linked."))
        XCTAssertEqual(LinkByPrompt.sheet(prompts: [], ending: .stopped), .stopped)
        XCTAssertEqual(LinkByPrompt.stopped, "Linking was stopped.")
        XCTAssertTrue(LinkByPrompt.polls([older]))
        XCTAssertFalse(LinkByPrompt.polls([]))
    }

    func testAPromptsRefusalsInWords() {
        func api(_ code: String, _ details: [String: String] = [:]) -> Error { LumiaError.Api(code: code, detail: "refused", details: details) }
        XCTAssertEqual(LinkByPrompt.refusal(api("FAILED_PRECONDITION", ["state": "answered"])), "Another of your devices answered it, or it was declined.")
        XCTAssertEqual(LinkByPrompt.refusal(api("FAILED_PRECONDITION", ["state": "declined"])), "It was declined on another of your devices.")
        XCTAssertEqual(LinkByPrompt.refusal(api("NOT_FOUND")), "That request has ended.")
        XCTAssertEqual(LinkByPrompt.refusal(api("IDENTITY_PROOF_INVALID", ["reason": "too_many_devices"])), LinkApproval.tooMany)
        XCTAssertEqual(LinkByPrompt.refusal(api("CONFLICT")), LinkApproval.conflict)
        XCTAssertEqual(LinkByPrompt.refusal(LumiaError.Other(detail: "this request cannot be approved: start again on the new device")),
                       "this request cannot be approved: start again on the new device")
    }
}
