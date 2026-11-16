// What a conversation plays (specs/media.md 5 and 6.6): an attached
// video in its row through AVKit's player with the system's controls and
// full screen, a recording in its row, and a link's embed in its card's
// place in a web view that holds the embed's iframe alone. One plays at a
// time; a video pauses when its row scrolls away, an embed (but a site's
// audio) goes back to its card, and leaving the conversation lets
// everything go (`PlaybackRules`, which the kit's tests cover). Nothing
// is fetched or loaded before a press.
import AVFoundation
import AVKit
import LumiaFFI
import LumiaKit
import SwiftUI
import UIKit
import WebKit

/// A conversation's players and its one embed. Each conversation screen
/// holds one and hands it to its rows through the environment.
@MainActor @Observable
final class MediaPlayback {
    private(set) var rules = PlaybackRules()
    /// An attachment being fetched, or an embed's address being made: its key.
    private(set) var loading: Set<String> = []
    /// What could not be played or shown, in words, by key.
    private(set) var failures: [String: String] = [:]
    /// The attached videos and recordings with a player, by attachment id.
    private(set) var players: [String: AVPlayer] = [:]
    /// Where a recording is as it plays, in milliseconds.
    private(set) var positions: [String: UInt32] = [:]
    /// The height a post that grows last reported, by key.
    private(set) var heights: [String: CGFloat] = [:]
    /// Embeds whose card opens the browser instead (a handle that did not resolve).
    private(set) var unavailable: Set<String> = []
    /// The embed shown, in its web view.
    private(set) var webView: WKWebView?
    /// A message to bring back into view: one pressed as an embed above it
    /// went back to its card, whose change of height moved it.
    private(set) var reveal: ScrollRequest?

    @ObservationIgnored private var files: [String: URL] = [:]
    @ObservationIgnored private var watchers: [String: [NSObjectProtocol]] = [:]
    @ObservationIgnored private var statusWatchers: [String: NSKeyValueObservation] = [:]
    @ObservationIgnored private var timeWatchers: [String: Any] = [:]
    @ObservationIgnored private var embedDelegate: EmbedDelegate?
    /// Bumped as the conversation is left, so a fetch that answers after it is dropped.
    @ObservationIgnored private var epoch = 0
    /// A video full screen in AVKit's own presentation: its row and the
    /// conversation going out of sight then is that presentation, not the
    /// person scrolling or leaving.
    @ObservationIgnored var fullScreen: String?

    func isPlaying(_ key: String) -> Bool { rules.playing == key }
    func shows(_ key: String) -> Bool { rules.embed?.key == key && webView != nil }

    // MARK: Attachments

    /// An attached video or recording pressed: fetched and played the first
    /// time, then played or paused. An attachment its server let go has no
    /// blobs and never plays.
    func press(_ a: AttachmentSummary, in messageID: String, session: LumiaSession) {
        guard !a.expired else { return }
        let key = a.attachmentId
        let slot = PlaybackSlot(key: key, kind: AttachmentKind(a.kind) == .audio ? .audio : .video)
        if let player = players[key] {
            if player.timeControlStatus == .paused {
                revealing(messageID, anchor: .center, if: apply(rules.start(slot)))
                player.play()
            } else {
                player.pause()
            }
            return
        }
        guard !loading.contains(key) else { return }
        loading.insert(key)
        failures[key] = nil
        let epoch = epoch
        Task {
            do {
                let file = try MediaFiles.write(try await session.attachmentData(a.main), for: a)
                guard epoch == self.epoch else { MediaFiles.remove(file); return }
                loading.remove(key)
                files[key] = file
                try? AVAudioSession.sharedInstance().setCategory(.playback)
                let player = AVPlayer(url: file)
                watch(player, slot)
                players[key] = player
                revealing(messageID, anchor: .center, if: apply(rules.start(slot)))
                player.play()
            } catch {
                guard epoch == self.epoch else { return }
                loading.remove(key)
                failures[key] = "This can’t be played here."
            }
        }
    }

    /// Follows a player's own controls: one started there pauses the others,
    /// and a recording played to its end goes back to its start.
    private func watch(_ player: AVPlayer, _ slot: PlaybackSlot) {
        let key = slot.key
        statusWatchers[key] = player.observe(\.timeControlStatus, options: [.new]) { [weak self] player, _ in
            let status = player.timeControlStatus
            Task { @MainActor in self?.statusChanged(slot, status) }
        }
        guard slot.kind == .audio else { return }
        timeWatchers[key] = player.addPeriodicTimeObserver(forInterval: CMTime(value: 1, timescale: 2), queue: .main) { [weak self] time in
            let ms = time.seconds.isFinite ? UInt32(clamping: Int(time.seconds * 1000)) : 0
            Task { @MainActor in self?.positions[key] = ms }
        }
        if let item = player.currentItem {
            watchers[key] = [NotificationCenter.default.addObserver(forName: AVPlayerItem.didPlayToEndTimeNotification, object: item, queue: .main) { [weak player] _ in
                player?.seek(to: .zero)
            }]
        }
    }

    private func statusChanged(_ slot: PlaybackSlot, _ status: AVPlayer.TimeControlStatus) {
        guard players[slot.key] != nil else { return }
        switch status {
        case .playing:
            if rules.playing != slot.key { apply(rules.start(slot)) }
        case .paused:
            rules.paused(slot.key)
        default:
            break
        }
    }

    private func release(_ key: String) {
        guard let player = players[key] else { return }
        player.pause()
        statusWatchers[key]?.invalidate()
        statusWatchers[key] = nil
        if let token = timeWatchers.removeValue(forKey: key) { player.removeTimeObserver(token) }
        watchers.removeValue(forKey: key)?.forEach(NotificationCenter.default.removeObserver)
        players[key] = nil
        positions[key] = nil
        MediaFiles.remove(files.removeValue(forKey: key))
        rules.close(key)
    }

    /// Does what starting one does to the others; true when an embed went back to its card.
    @discardableResult
    private func apply(_ effects: [(PlaybackSlot, PlaybackStop)]) -> Bool {
        var closed = false
        for (slot, stop) in effects {
            if slot.kind.isEmbed {
                closeEmbed(slot.key)
                closed = true
            } else if stop == .pause {
                players[slot.key]?.pause()
            } else {
                release(slot.key)
            }
        }
        return closed
    }

    private func revealing(_ messageID: String, anchor: UnitPoint, if closed: Bool) {
        guard closed, !messageID.isEmpty else { return }
        reveal = ScrollRequest(id: messageID, anchor: anchor, n: (reveal?.n ?? 0) + 1)
    }

    // MARK: Scrolling away and leaving

    /// Its row went out of sight: a video pauses, an embed but a site's audio
    /// goes back to its card, and a recording plays on.
    func away(_ key: String) {
        guard fullScreen == nil else { return }
        switch rules.away(key) {
        case .pause?: players[key]?.pause()
        case .close?: closeEmbed(key)
        case nil: break
        }
    }

    /// The conversation was left or changed: everything stops and is let go.
    func leave() {
        guard fullScreen == nil else { return }
        epoch += 1
        for slot in rules.leave() {
            if slot.kind.isEmbed { closeEmbed(slot.key) } else { release(slot.key) }
        }
        loading = []
        failures = [:]
        heights = [:]
        unavailable = []
    }

    // MARK: Embeds

    /// A card with an embed pressed: its address made (a Bluesky handle
    /// resolved first), then the embed loaded in the card's place; any other
    /// embed goes back to its card and any video or recording pauses. A
    /// handle that does not resolve leaves the card, saying so, and its next
    /// press opens the browser.
    func show(_ embed: EmbedSummary, key: String, title: String, session: LumiaSession) {
        guard !loading.contains(key), !shows(key) else { return }
        loading.insert(key)
        failures[key] = nil
        let epoch = epoch
        Task {
            do {
                let address = try await session.embedAddress(embed)
                guard epoch == self.epoch else { return }
                loading.remove(key)
                let closed = apply(rules.start(PlaybackSlot(key: key, kind: PlaybackSlot.Kind(embed: embed.kind))))
                // The message the key names, its embed's top in view as it loads and grows.
                revealing(String(key.prefix { $0 != "|" }), anchor: .top, if: closed)
                load(address, key: key, title: title, scrolls: embed.kind == "post" && !embed.resizes && embed.fixedHeight > 0, instance: session.client.baseURL)
            } catch {
                guard epoch == self.epoch else { return }
                loading.remove(key)
                failures[key] = (error as? EmbedUnavailable)?.words ?? error.lumiaMessage
                unavailable.insert(key)
            }
        }
    }

    /// Back to its card, by its Close or from elsewhere.
    func close(_ key: String) {
        rules.close(key)
        closeEmbed(key)
    }

    fileprivate func reported(_ key: String, height: CGFloat) {
        guard shows(key), height.isFinite, height > 0 else { return }
        heights[key] = height
    }

    private func closeEmbed(_ key: String) {
        guard let delegate = embedDelegate, delegate.key == key else { return }
        if let webView {
            webView.pauseAllMediaPlayback(completionHandler: nil)
            webView.stopLoading()
            webView.configuration.userContentController.removeScriptMessageHandler(forName: EmbedPage.handler)
            webView.removeFromSuperview()
        }
        webView = nil
        embedDelegate = nil
        heights[key] = nil
    }

    /// A web view holding only the embed: nothing kept between loads, media
    /// playing inline and on the press that loaded it, the page's base the
    /// instance's origin, and only the embeds' origins loaded.
    private func load(_ address: String, key: String, title: String, scrolls: Bool, instance: URL) {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        config.allowsInlineMediaPlayback = true
        config.mediaTypesRequiringUserActionForPlayback = []
        // Some sites draw a lesser embed for a web view they do not take for Safari.
        config.applicationNameForUserAgent = "Version/18.0 Mobile/15E148 Safari/604.1"
        let delegate = EmbedDelegate(key: key, origins: Set(embedOrigins()), playback: self)
        config.userContentController.add(delegate, name: EmbedPage.handler)
        let web = WKWebView(frame: .zero, configuration: config)
        web.navigationDelegate = delegate
        web.uiDelegate = delegate
        web.isOpaque = false
        web.backgroundColor = .clear
        web.scrollView.backgroundColor = .clear
        web.scrollView.isScrollEnabled = false
        web.scrollView.bounces = false
        web.accessibilityLabel = title
        #if DEBUG
            if #available(iOS 16.4, *) { web.isInspectable = true }
        #endif
        embedDelegate = delegate
        webView = web
        web.loadHTMLString(EmbedPage.html(src: address, title: title, scrolls: scrolls), baseURL: EmbedPage.base(instance))
    }
}

/// An embed's web view's rules: the page first, then only the embeds'
/// origins in its frames; a link followed elsewhere, or one opening a new
/// window, opens in the browser. Heights come only from the page's own
/// script, which takes them only from the embed's window.
private final class EmbedDelegate: NSObject, WKNavigationDelegate, WKUIDelegate, WKScriptMessageHandler {
    let key: String
    let origins: Set<String>
    weak var playback: MediaPlayback?
    private var loaded = false

    init(key: String, origins: Set<String>, playback: MediaPlayback) {
        self.key = key; self.origins = origins; self.playback = playback
    }

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction) async -> WKNavigationActionPolicy {
        // No frame is a new window, which is the main frame's to refuse.
        let main = action.targetFrame?.isMainFrame ?? true
        let decision = EmbedNavigation.decide(url: action.request.url, mainFrame: main, wrapperLoaded: loaded, userActivated: action.navigationType == .linkActivated, origins: origins)
        switch decision {
        case .allow:
            if main { loaded = true }
            return .allow
        case .cancel:
            return .cancel
        case let .openOutside(url):
            UIApplication.shared.open(url, options: [:], completionHandler: nil)
            return .cancel
        }
    }

    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for action: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        if let url = action.request.url, ["http", "https"].contains(url.scheme?.lowercased() ?? "") { UIApplication.shared.open(url, options: [:], completionHandler: nil) }
        return nil
    }

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        // The page's own script, never a frame inside it.
        guard message.frameInfo.isMainFrame, let height = message.body as? NSNumber else { return }
        let key = key
        Task { @MainActor [weak playback] in playback?.reported(key, height: CGFloat(height.doubleValue)) }
    }
}

// MARK: Views

/// An attached video playing in its row: AVKit's player with the system's
/// controls and its full-screen button. A long press on it is still the
/// message's actions.
struct InlineVideo: UIViewControllerRepresentable {
    let player: AVPlayer
    let key: String
    let playback: MediaPlayback
    let hold: (() -> Void)?

    func makeCoordinator() -> Coordinator { Coordinator(key: key, playback: playback) }

    func makeUIViewController(context: Context) -> AVPlayerViewController {
        let controller = AVPlayerViewController()
        controller.player = player
        controller.allowsPictureInPicturePlayback = false
        controller.entersFullScreenWhenPlaybackBegins = false
        controller.exitsFullScreenWhenPlaybackEnds = true
        controller.updatesNowPlayingInfoCenter = false
        controller.videoGravity = .resizeAspect
        controller.delegate = context.coordinator
        controller.view.backgroundColor = .black
        controller.view.accessibilityIdentifier = "inline-video"
        let press = UILongPressGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.pressed(_:)))
        press.minimumPressDuration = MediaPress.hold
        press.cancelsTouchesInView = false
        press.delegate = context.coordinator
        controller.view.addGestureRecognizer(press)
        context.coordinator.hold = hold
        return controller
    }

    func updateUIViewController(_ controller: AVPlayerViewController, context: Context) {
        if controller.player !== player { controller.player = player }
        context.coordinator.hold = hold
    }

    final class Coordinator: NSObject, AVPlayerViewControllerDelegate, UIGestureRecognizerDelegate {
        let key: String
        weak var playback: MediaPlayback?
        var hold: (() -> Void)?

        init(key: String, playback: MediaPlayback) {
            self.key = key; self.playback = playback
        }

        @objc func pressed(_ press: UILongPressGestureRecognizer) {
            if press.state == .began { hold?() }
        }

        func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }

        func playerViewController(_ controller: AVPlayerViewController, willBeginFullScreenPresentationWithAnimationCoordinator coordinator: UIViewControllerTransitionCoordinator) {
            playback?.fullScreen = key
        }

        func playerViewController(_ controller: AVPlayerViewController, willEndFullScreenPresentationWithAnimationCoordinator coordinator: UIViewControllerTransitionCoordinator) {
            coordinator.animate(alongsideTransition: nil) { [weak self] context in
                if !context.isCancelled { self?.playback?.fullScreen = nil }
            }
        }
    }
}

/// The shown embed's web view, in its card's place. The web view is the
/// playback's, so a site's audio plays on while its row is out of sight.
struct EmbedHost: UIViewRepresentable {
    let web: WKWebView

    func makeUIView(context: Context) -> UIView {
        let view = UIView()
        view.clipsToBounds = true
        view.backgroundColor = .clear
        adopt(into: view)
        return view
    }

    func updateUIView(_ view: UIView, context: Context) {
        if web.superview !== view { adopt(into: view) }
    }

    static func dismantleUIView(_ view: UIView, coordinator: ()) {
        view.subviews.forEach { $0.removeFromSuperview() }
    }

    private func adopt(into view: UIView) {
        view.subviews.forEach { $0.removeFromSuperview() }
        web.frame = view.bounds
        web.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(web)
    }
}

/// An embed's bar over it and the embed under it, at the embed's size in
/// the column (`EmbedLayout`), the bar as wide.
struct EmbedBox: Layout {
    let embed: EmbedSummary
    let reported: CGFloat?

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let size = EmbedLayout.size(embed, column: proposal.width ?? 320, reported: reported)
        let bar = subviews.first?.sizeThatFits(ProposedViewSize(width: size.width, height: nil)).height ?? 0
        return CGSize(width: size.width, height: bar + size.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard subviews.count == 2 else { return }
        let size = EmbedLayout.size(embed, column: bounds.width, reported: reported)
        let bar = subviews[0].sizeThatFits(ProposedViewSize(width: size.width, height: nil)).height
        subviews[0].place(at: bounds.origin, proposal: ProposedViewSize(width: size.width, height: bar))
        subviews[1].place(at: CGPoint(x: bounds.minX, y: bounds.minY + bar), proposal: ProposedViewSize(size))
    }
}
