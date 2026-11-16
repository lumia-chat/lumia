// Inline video and audio, and embeds (specs/media.md 5 and 6.6), as the
// phone decides them: one playing at a time and what scrolling away and
// leaving do, an embed's size, the address it loads with Twitch's parent,
// the page that holds it, which navigations its web view lets through,
// and the words said of it.
import CoreGraphics
import Foundation
@testable import LumiaKit
import LumiaFFI
import XCTest

final class EmbedsTests: XCTestCase {
    private let video = PlaybackSlot(key: "v1", kind: .video)
    private let video2 = PlaybackSlot(key: "v2", kind: .video)
    private let audio = PlaybackSlot(key: "a1", kind: .audio)
    private let player = PlaybackSlot(key: "m1|https://youtu.be/x", kind: .embedPlayer)
    private let sound = PlaybackSlot(key: "m2|https://soundcloud.com/x/y", kind: .embedAudio)
    private let post = PlaybackSlot(key: "m3|https://x.com/a/status/1", kind: .embedPost)

    private func stops(_ effects: [(PlaybackSlot, PlaybackStop)]) -> [String: PlaybackStop] {
        Dictionary(uniqueKeysWithValues: effects.map { ($0.0.key, $0.1) })
    }

    // MARK: Playing

    func testStartingOnePausesAnyOtherAndPutsAnEmbedBack() {
        var rules = PlaybackRules()
        XCTAssertTrue(rules.start(video).isEmpty)
        XCTAssertEqual(rules.playing, "v1")
        XCTAssertEqual(stops(rules.start(audio)), ["v1": .pause], "a video playing pauses when a recording starts")
        XCTAssertTrue(rules.holds("v1"), "a paused video keeps its player and its place")
        XCTAssertEqual(rules.playing, "a1")
        XCTAssertEqual(stops(rules.start(player)), ["a1": .pause])
        XCTAssertEqual(rules.embed, player)
        XCTAssertEqual(stops(rules.start(post)), [player.key: .close], "one embed shown at a time")
        XCTAssertEqual(rules.embed, post)
        XCTAssertEqual(stops(rules.start(video2)), [post.key: .close], "starting a video puts the embed back to its card")
        XCTAssertNil(rules.embed)
        XCTAssertEqual(Set(rules.held.map(\.key)), ["v1", "a1", "v2"])
        XCTAssertTrue(rules.start(video).allSatisfy { $0.1 == .pause })
    }

    func testAPausedOneStartingAgainFromItsControlsPausesTheOther() {
        var rules = PlaybackRules()
        _ = rules.start(video)
        rules.paused("v1")
        XCTAssertNil(rules.playing)
        XCTAssertTrue(rules.start(video2).isEmpty, "nothing was playing")
        XCTAssertEqual(stops(rules.start(video)), ["v2": .pause])
    }

    func testScrollingAwayPausesAVideoAndPutsAPlayerOrPostBackButAudioPlaysOn() {
        var rules = PlaybackRules()
        _ = rules.start(video)
        XCTAssertEqual(rules.away("v1"), .pause)
        XCTAssertTrue(rules.holds("v1"))
        XCTAssertNil(rules.away("v1"), "already paused")
        _ = rules.start(audio)
        XCTAssertNil(rules.away("a1"), "audio plays on while scrolling")
        XCTAssertEqual(rules.playing, "a1")
        _ = rules.start(sound)
        XCTAssertNil(rules.away(sound.key), "a site's audio plays on too")
        XCTAssertEqual(rules.embed, sound)
        _ = rules.start(player)
        XCTAssertEqual(rules.away(player.key), .close)
        XCTAssertNil(rules.embed)
        _ = rules.start(post)
        XCTAssertEqual(rules.away(post.key), .close)
        XCTAssertNil(rules.away("nothing"), "nothing held")
    }

    func testLeavingClosesEverything() {
        var rules = PlaybackRules()
        _ = rules.start(video)
        _ = rules.start(audio)
        _ = rules.start(sound)
        XCTAssertEqual(Set(rules.leave().map(\.key)), ["v1", "a1", sound.key])
        XCTAssertTrue(rules.held.isEmpty)
        XCTAssertNil(rules.playing)
        rules = PlaybackRules()
        _ = rules.start(video)
        rules.close("v1")
        XCTAssertFalse(rules.holds("v1"))
        XCTAssertNil(rules.playing)
    }

    func testAnEmbedsKindFromTheCore() {
        XCTAssertEqual(PlaybackSlot.Kind(embed: "player"), .embedPlayer)
        XCTAssertEqual(PlaybackSlot.Kind(embed: "audio"), .embedAudio)
        XCTAssertEqual(PlaybackSlot.Kind(embed: "post"), .embedPost)
        XCTAssertFalse(PlaybackSlot.Kind.video.isEmbed)
    }

    // MARK: Sizes

    private func embed(_ link: String) throws -> EmbedSummary { try XCTUnwrap(embedFor(url: link), link) }

    func testAPlayerIsTheColumnsWidthAtItsShapeAndAtMost400Tall() throws {
        let youtube = try embed("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        XCTAssertEqual(EmbedLayout.size(youtube, column: 320), CGSize(width: 320, height: 180))
        XCTAssertEqual(EmbedLayout.size(youtube, column: 800), CGSize(width: 711, height: 400), "at most 400 tall")
        let short = try embed("https://youtube.com/shorts/dQw4w9WgXcQ")
        XCTAssertEqual(EmbedLayout.size(short, column: 360), CGSize(width: 225, height: 400), "upright, held to 400 tall")
        XCTAssertEqual(EmbedLayout.size(youtube, column: 0), .zero)
    }

    func testAudioIsTheColumnsWidthAndTheSitesHeight() throws {
        let track = try embed("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC")
        XCTAssertEqual(track.kind, "audio")
        XCTAssertEqual(EmbedLayout.size(track, column: 343), CGSize(width: 343, height: 152))
        let sound = try embed("https://soundcloud.com/forss/flickermood")
        XCTAssertEqual(EmbedLayout.size(sound, column: 343), CGSize(width: 343, height: 166))
    }

    func testAPostIsAtMost550WideAndGrowsToWhatItReports() throws {
        let x = try embed("https://x.com/jack/status/20")
        XCTAssertTrue(x.resizes)
        XCTAssertEqual(EmbedLayout.size(x, column: 343), CGSize(width: 343, height: 250), "its starting height")
        XCTAssertEqual(EmbedLayout.size(x, column: 700), CGSize(width: 550, height: 250))
        XCTAssertEqual(EmbedLayout.size(x, column: 343, reported: 612.4), CGSize(width: 343, height: 613))
        XCTAssertEqual(EmbedLayout.size(x, column: 343, reported: 5), CGSize(width: 343, height: 80))
        XCTAssertEqual(EmbedLayout.size(x, column: 343, reported: 1e9), CGSize(width: 343, height: 2000))
        XCTAssertEqual(EmbedLayout.size(x, column: 343, reported: .nan), CGSize(width: 343, height: 250))
        let instagram = try embed("https://www.instagram.com/p/CxYz123abcD/")
        XCTAssertFalse(instagram.resizes)
        XCTAssertEqual(EmbedLayout.size(instagram, column: 343, reported: 900), CGSize(width: 343, height: 640), "a fixed post never grows")
    }

    // MARK: The page

    func testTwitchNamesTheInstancesHostAsItsParent() throws {
        let instance = try XCTUnwrap(URL(string: "https://chat.acme.dev:8443/app"))
        let clip = try embed("https://clips.twitch.tv/AwkwardHelplessSalamander")
        XCTAssertTrue(clip.needsParent)
        XCTAssertEqual(EmbedPage.address(clip.url, needsParent: true, instance: instance), clip.url + "&parent=chat.acme.dev")
        let youtube = try embed("https://youtu.be/dQw4w9WgXcQ")
        XCTAssertEqual(EmbedPage.address(youtube.url, needsParent: youtube.needsParent, instance: instance), youtube.url)
        XCTAssertEqual(EmbedPage.address("https://player.twitch.tv/embed", needsParent: true, instance: instance), "https://player.twitch.tv/embed?parent=chat.acme.dev")
        XCTAssertNil(EmbedPage.address("", needsParent: false, instance: instance), "a Bluesky post not yet resolved")
        XCTAssertEqual(EmbedPage.base(instance)?.absoluteString, "https://chat.acme.dev:8443/")
    }

    func testThePageHoldsOnlyTheEmbedAndTakesHeightsFromItAlone() throws {
        let page = EmbedPage.html(src: "https://platform.twitter.com/embed/Tweet.html?id=20&dnt=true", title: "Jack \"on\" <X>")
        XCTAssertTrue(page.contains(#"src="https://platform.twitter.com/embed/Tweet.html?id=20&amp;dnt=true""#))
        XCTAssertTrue(page.contains(#"title="Jack &quot;on&quot; &lt;X&gt;""#), "the title escaped")
        XCTAssertTrue(page.contains(#"var origin = "https://platform.twitter.com";"#))
        XCTAssertTrue(page.contains("event.source !== frame.contentWindow || event.origin !== origin"), "heights only from the embed's own window and origin")
        XCTAssertTrue(page.contains("messageHandlers.lumiaEmbed.postMessage"))
        XCTAssertTrue(page.contains(#"allow="autoplay; encrypted-media; fullscreen; picture-in-picture""#))
        XCTAssertEqual(page.components(separatedBy: "<iframe").count, 2, "one iframe")
        XCTAssertTrue(page.contains(#"scrolling="no""#), "a post that grows is drawn whole, so a drag over it scrolls the conversation")
        XCTAssertFalse(EmbedPage.html(src: "https://www.instagram.com/p/CxYz123abcD/embed", title: "", scrolls: true).contains("scrolling="), "a post of a fixed height scrolls inside")
        let literal = EmbedPage.json("</script>\"\\")
        XCTAssertFalse(literal.contains("<") || literal.contains(">"), "nothing in it ends the script")
        XCTAssertTrue(literal.hasPrefix("\"") && literal.hasSuffix("\"") && literal.contains("/script"))
        XCTAssertTrue(literal.contains(#"\"\\""#), "its quote and backslash escaped")
    }

    func testTheWebViewLoadsOnlyThePageAndTheEmbedsOrigins() throws {
        let origins = Set(embedOrigins())
        XCTAssertTrue(origins.contains("https://www.youtube-nocookie.com"))
        let page = URL(string: "https://chat.acme.dev/")
        let embed = URL(string: "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?autoplay=1")
        let away = try XCTUnwrap(URL(string: "https://www.youtube.com/watch?v=dQw4w9WgXcQ"))
        XCTAssertEqual(EmbedNavigation.decide(url: page, mainFrame: true, wrapperLoaded: false, userActivated: false, origins: origins), .allow, "the page itself")
        XCTAssertEqual(EmbedNavigation.decide(url: away, mainFrame: true, wrapperLoaded: true, userActivated: true, origins: origins), .openOutside(away), "nothing replaces the page; a link opens in the browser")
        XCTAssertEqual(EmbedNavigation.decide(url: URL(string: "lumia://x"), mainFrame: true, wrapperLoaded: true, userActivated: true, origins: origins), .cancel)
        XCTAssertEqual(EmbedNavigation.decide(url: embed, mainFrame: false, wrapperLoaded: true, userActivated: false, origins: origins), .allow)
        XCTAssertEqual(EmbedNavigation.decide(url: URL(string: "about:blank"), mainFrame: false, wrapperLoaded: true, userActivated: false, origins: origins), .allow)
        XCTAssertEqual(EmbedNavigation.decide(url: away, mainFrame: false, wrapperLoaded: true, userActivated: true, origins: origins), .openOutside(away), "a link followed in the embed opens in the browser")
        XCTAssertEqual(EmbedNavigation.decide(url: URL(string: "https://ads.example.com/frame"), mainFrame: false, wrapperLoaded: true, userActivated: false, origins: origins), .cancel, "a frame from elsewhere is not loaded")
        XCTAssertEqual(EmbedNavigation.decide(url: nil, mainFrame: false, wrapperLoaded: true, userActivated: false, origins: origins), .cancel)
    }

    // MARK: Words

    func testThePressIsNamedByTheCoresLabelAndTheCardsTitle() throws {
        let youtube = try embed("https://youtu.be/dQw4w9WgXcQ")
        XCTAssertEqual(EmbedWords.press(youtube, title: "Never Gonna Give You Up"), "Play video from YouTube, Never Gonna Give You Up")
        let x = try embed("https://x.com/jack/status/20")
        XCTAssertEqual(EmbedWords.press(x, title: " "), "Show post from X")
        XCTAssertEqual(EmbedWords.button(x), "Show post")
        XCTAssertEqual(EmbedWords.button(youtube), "Play")
        XCTAssertEqual(EmbedWords.title(x, cardTitle: ""), "X")
        XCTAssertEqual(EmbedWords.title(youtube, cardTitle: "Song"), "Song")
        XCTAssertEqual(EmbedWords.notFound(handle: "ada.bsky.social"), "Bluesky couldn’t find @ada.bsky.social. The card opens it in the browser.")
        XCTAssertEqual(MediaWords.position(ms: 12_000, of: 90_000), "0:12 / 1:30")
        XCTAssertEqual(MediaWords.position(ms: 95_000, of: 90_000), "1:30 / 1:30")
        XCTAssertEqual(MediaWords.position(ms: 12_000, of: 0), "0:12")
    }
}
