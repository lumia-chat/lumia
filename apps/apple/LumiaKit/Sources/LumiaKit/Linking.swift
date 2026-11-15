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
