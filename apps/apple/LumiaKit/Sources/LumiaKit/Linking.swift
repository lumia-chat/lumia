// Approving another device from this phone (identity spec 4.2 steps 2 to
// 6), as values the tests cover: whether pasted text is worth reading as a
// link request at all, what a request would add as the screen says it
// (the core's `linkRequestSummary` reads it), whether that device is
// already among the identity's, the code as it is read out and heard, and
// what the instance refuses in words. The core builds, publishes and seals;
// these are the phone's words and choices around it.
import Foundation
import LumiaFFI

public enum LinkApproval {
    /// What the new device's Link screen says beside its request.
    public static let instruction = "On a device that has your private messages, open Devices, choose Approve a device and paste this request. Then type the code it shows here."
    /// Said under what a request would add, before Approve.
    public static let warning = "It will read your private messages from now on."
    /// Said under the code.
    public static let codeNote = "Type this code on the new device. It works once, within ten minutes."
    public static let notARequest = "That isn't a link request."
    public static let alreadyYours = "That device is already one of yours."
    public static let revoked = "That device was revoked. A revoked device can't be added again: start linking on it again, which makes a new request."
    /// The roster's limit (identity spec 3.3, `MAX_DEVICES` in the core).
    public static let tooMany = "Your identity has 20 devices, the most it can have. Revoke one you no longer use, then approve this one."
    public static let conflict = "Another of your devices changed your device list at the same moment. Approve again."

    /// What the field holds, as far as approving goes.
    public enum Reading: Equatable {
        /// Nothing pasted yet: nothing to say.
        case empty
        /// Not a link request; Approve stays off.
        case notARequest
        /// A request, with what it would add and the text to approve.
        case request(LinkRequestSummary, String)
    }

    /// The request in pasted text, whitespace taken out (a request shared
    /// through a message may arrive wrapped), or nil for text that cannot
    /// be one: a request is standard base64 of a few hundred characters,
    /// so anything else is not worth asking the core about.
    public static func request(in text: String) -> String? {
        let joined = String(text.unicodeScalars.filter { !CharacterSet.whitespacesAndNewlines.contains($0) })
        guard joined.count >= 64, joined.count <= 4096, joined.count % 4 == 0 else { return nil }
        let base64 = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=")
        guard joined.unicodeScalars.allSatisfy(base64.contains) else { return nil }
        return joined
    }

    /// What the field holds: empty, not a request, or a request the core read.
    public static func read(_ text: String) -> Reading {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return .empty }
        guard let request = request(in: text), let summary = try? linkRequestSummary(request: request) else { return .notARequest }
        return .request(summary, request)
    }

    /// What the request would add, in a sentence: "A browser on
    /// https://my.lumia.chat", "A device named “MacBook Pro”".
    public static func adds(_ summary: LinkRequestSummary) -> String {
        let label = summary.label.trimmingCharacters(in: .whitespacesAndNewlines)
        switch summary.kind {
        case "web":
            return summary.webOrigin.isEmpty ? "A browser" : "A browser on \(summary.webOrigin)"
        case "bot":
            return label.isEmpty ? "A bot with no name" : "A bot named “\(label)”"
        default:
            return label.isEmpty ? "A device with no name" : "A device named “\(label)”"
        }
    }

    /// Why a request cannot be approved here before anything is sent: the
    /// device is already in the roster this phone holds, active or
    /// revoked. Nil when it can be.
    public static func refusal(of summary: LinkRequestSummary, among devices: [DeviceSummary]) -> String? {
        guard let held = devices.first(where: { $0.deviceId == summary.deviceId }) else { return nil }
        return held.revoked ? revoked : alreadyYours
    }

    /// What approving failed with, in words: the instance's refusals of
    /// the new roster (protocol `IDENTITY_PROOF_INVALID` with its step's
    /// reason, `CONFLICT` once the core's own merge has been tried), else
    /// the message.
    public static func refusal(_ error: Error) -> String {
        guard case let .Api(code, _, details)? = error as? LumiaError else { return "Not approved: \(error.lumiaMessage)" }
        switch (code, details["reason"]) {
        case ("IDENTITY_PROOF_INVALID", "too_many_devices"): return tooMany
        case ("IDENTITY_PROOF_INVALID", "duplicate_device"): return alreadyYours
        case ("IDENTITY_PROOF_INVALID", "device_both_active_and_revoked"): return revoked
        case ("CONFLICT", _): return conflict
        case ("RATE_LIMITED", _): return "The instance is busy. Try again in a moment."
        default: return "Not approved: \(error.lumiaMessage)"
        }
    }

    /// The code in groups of four for reading out, "K7PQ M2XD".
    public static func spaced(_ code: String) -> String {
        let letters = Array(code.uppercased().filter { !$0.isWhitespace })
        return stride(from: 0, to: letters.count, by: 4).map { String(letters[$0..<min($0 + 4, letters.count)]) }.joined(separator: " ")
    }

    /// A code as typed on the new device, spaces and dashes taken out,
    /// since it is read out in groups: what is claimed.
    public static func typedCode(_ text: String) -> String {
        text.filter { !$0.isWhitespace && $0 != "-" }.uppercased()
    }
}

/// Linking by a prompt and six matching digits (identity spec 4.2.1), as
/// values the tests cover: both devices' stages from the core's strings,
/// the words each shows, which prompt shows and when to ask again. The
/// core opens, answers, reveals, approves and claims; these are the
/// phone's words and choices around it.
public enum LinkByPrompt {
    /// How often a screen with something under way asks again, beside
    /// every account event.
    public static let pollEvery: Duration = .seconds(2)
    /// How long "… is linked." and "Linking was stopped." stay before the prompt goes.
    public static let endingFor: Duration = .seconds(2)

    // MARK: This phone, linking

    /// Where linking this phone stands.
    public enum Stage: Equatable, Sendable {
        /// Waiting for a device that has the identity to answer.
        case waiting
        /// Both screens show these digits, `482 913`.
        case compare(String)
        /// This phone holds the identity and is signed in by it.
        case linked
        /// "Not me", or "They don't match", on the other device.
        case declined
        /// Ten minutes went by, or the instance no longer has it.
        case expired

        /// The core's `LinkProgress`; a stage this phone does not know waits.
        public init(_ progress: LinkProgress) {
            switch progress.stage {
            case "compare": self = .compare(progress.digits)
            case "linked": self = .linked
            case "declined": self = .declined
            case "expired": self = .expired
            default: self = .waiting
            }
        }

        /// Still under way: asked again on a timer and on every account event.
        public var polls: Bool {
            switch self {
            case .waiting, .compare: true
            case .linked, .declined, .expired: false
            }
        }
    }

    /// What the link screen says at each stage; nothing once linked, which goes on into the app.
    public static func words(_ stage: Stage) -> String {
        switch stage {
        case .waiting: "Open Lumia on a device that has your private messages. It will ask about this phone."
        case .compare: "Check that your other device shows the same number, then approve it there."
        case .linked: ""
        case .declined: "It wasn't approved on your other device."
        case .expired: "That took too long."
        }
    }

    /// Why linking could not start, in words: the instance keeps at most
    /// five open for an account.
    public static func startRefusal(_ error: Error) -> String {
        if case let .Api(code, _, _)? = error as? LumiaError, code == "RATE_LIMITED" {
            return "Too many devices are waiting to be linked to your account. Try again in a few minutes."
        }
        return error.lumiaMessage
    }

    // MARK: A device of this identity, asked

    /// Where a prompt stands on this phone.
    public enum PromptStage: Equatable, Sendable {
        /// Nobody has answered: Continue or Not me.
        case new
        /// Answered here; the new device has yet to reveal.
        case waiting
        /// Both screens show these digits.
        case compare(String)
        /// The new device's secret did not match what it committed to:
        /// never approved, only "Not me".
        case refused

        /// The core's stage; one this phone does not know waits, which offers only "Not me".
        public init(_ prompt: LinkPrompt) {
            switch prompt.stage {
            case "new": self = .new
            case "compare": self = .compare(prompt.digits)
            case "refused": self = .refused
            default: self = .waiting
            }
        }
    }

    /// What the prompt sheet shows: the oldest prompt, or for a moment how
    /// the one answered here ended.
    public enum Sheet: Equatable, Sendable {
        case prompt(LinkPrompt)
        /// "Pixel 9 is linked."
        case linked(String)
        /// After "They don't match": "Linking was stopped."
        case stopped
    }

    /// How a prompt answered here ended, said before the sheet goes.
    public enum Ending: Equatable, Sendable {
        case linked(LinkRequestSummary)
        case stopped
    }

    /// The sheet's content: an ending first, then the oldest prompt (the
    /// instance lists them oldest first); nil for no sheet.
    public static func sheet(prompts: [LinkPrompt], ending: Ending?) -> Sheet? {
        switch ending {
        case let .linked(request): .linked("\(name(request, start: true)) is linked.")
        case .stopped: .stopped
        case nil: prompts.first.map(Sheet.prompt)
        }
    }

    /// Prompts are asked for again on a timer while any is open.
    public static func polls(_ prompts: [LinkPrompt]) -> Bool { !prompts.isEmpty }

    /// The device asking, as a sentence names it: its label, else "the
    /// browser" or "the new device".
    public static func name(_ request: LinkRequestSummary, start: Bool = false) -> String {
        let label = request.label.trimmingCharacters(in: .whitespacesAndNewlines)
        if !label.isEmpty { return label }
        let named = request.kind == "web" ? "the browser" : "the new device"
        return start ? named.prefix(1).uppercased() + named.dropFirst() : named
    }

    /// The prompt's title at each stage.
    public static func title(_ prompt: LinkPrompt) -> String {
        let request = prompt.request
        switch PromptStage(prompt) {
        case .new:
            guard request.kind == "web" else { return "\(name(request, start: true)) wants your private messages" }
            return request.webOrigin.isEmpty ? "A browser wants your private messages" : "A browser on \(request.webOrigin) wants your private messages"
        case .waiting: return "Waiting for \(name(request))…"
        case .compare: return "Does \(name(request)) show this number?"
        case .refused: return "This request can't be trusted. Don't approve it."
        }
    }

    /// The line under a new prompt's title.
    public static let onlyIfYours = "Only continue if you're linking a device of yours right now."
    /// After "They don't match".
    public static let stopped = "Linking was stopped."

    /// What answering, approving or declining failed with, in words.
    public static func refusal(_ error: Error) -> String {
        guard case let .Api(code, _, details)? = error as? LumiaError else { return error.lumiaMessage }
        switch (code, details["state"]) {
        case ("FAILED_PRECONDITION", "answered"): return "Another of your devices answered it, or it was declined."
        case ("NOT_FOUND", _): return "That request has ended."
        case ("IDENTITY_PROOF_INVALID", _), ("CONFLICT", _): return LinkApproval.refusal(error)
        default: return error.lumiaMessage
        }
    }
}
