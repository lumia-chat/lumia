// Inline video and audio, and embeds (specs/media.md 5 and 6.6), as the
// phone decides them: one playing at a time by the core's rules (every
// case of `proto/vectors/playback/cases.json` carried out), the later of
// two presses playing, what scrolling away and leaving do, an embed's
// size, the address it loads with Twitch's parent, the page that holds it
// and what its script hands the core for a height (the page's own script
// run, over `proto/vectors/embeds/heights.json`), which navigations its
// web view lets through, and the words said of it.
import CoreGraphics
import Foundation
import JavaScriptCore
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

    /// A file of the cases every client agrees on, `proto/vectors/<path>`.
    private func vectors(_ path: String) throws -> [String: Any] {
        var root = URL(fileURLWithPath: #filePath)
        for _ in 0..<6 { root.deleteLastPathComponent() }
        let data = try Data(contentsOf: root.appendingPathComponent("proto/vectors/\(path)"))
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    // MARK: Playing

    func testACardPressedStopsEverythingElseAndAnAttachmentLeavesAPostShown() {
        var held = PlaybackHeld()
        XCTAssertTrue(held.start(video).isEmpty)
        XCTAssertEqual(held.playing, "v1")
        XCTAssertEqual(stops(held.start(audio)), ["v1": .pause], "a video playing pauses when a recording starts")
        XCTAssertTrue(held.holds("v1"), "a paused video keeps its player and its place")
        XCTAssertEqual(held.playing, "a1")
        XCTAssertEqual(stops(held.start(player)), ["a1": .pause])
        XCTAssertEqual(held.embed, player)
        XCTAssertEqual(stops(held.start(post)), [player.key: .close], "one embed shown at a time")
        XCTAssertEqual(held.embed, post)
        XCTAssertTrue(held.start(video2).isEmpty, "a post stays shown when an attachment starts: it is read, not played")
        XCTAssertEqual(held.embed, post)
        XCTAssertEqual(held.playing, "v2")
        XCTAssertEqual(stops(held.start(sound)), [post.key: .close, "v2": .pause], "a card pressed puts the post back and pauses the video")
        XCTAssertEqual(held.embed, sound)
        XCTAssertEqual(stops(held.start(video)), [sound.key: .close], "an attachment started puts back an embed that plays")
        XCTAssertNil(held.embed)
        XCTAssertEqual(Set(held.held.map(\.key)), ["v1", "a1", "v2"])
        XCTAssertTrue(held.start(video2).allSatisfy { $0.1 == .pause })
    }

    func testAPausedOneStartingAgainFromItsControlsPausesTheOther() {
        var held = PlaybackHeld()
        _ = held.start(video)
        held.paused("v1")
        XCTAssertNil(held.playing)
        XCTAssertTrue(held.start(video2).isEmpty, "nothing was playing")
        XCTAssertEqual(stops(held.start(video)), ["v2": .pause])
    }

    func testScrollingAwayPausesAVideoAndPutsAPlayerBackButAudioPlaysOnAndAPostStays() {
        var held = PlaybackHeld()
        _ = held.start(video)
        XCTAssertEqual(held.away("v1"), .pause)
        XCTAssertTrue(held.holds("v1"))
        XCTAssertNil(held.away("v1"), "already paused")
        _ = held.start(audio)
        XCTAssertNil(held.away("a1"), "audio plays on while scrolling")
        XCTAssertEqual(held.playing, "a1")
        _ = held.start(sound)
        XCTAssertNil(held.away(sound.key), "a site's audio plays on too")
        XCTAssertEqual(held.embed, sound)
        _ = held.start(player)
        XCTAssertEqual(held.away(player.key), .close)
        XCTAssertNil(held.embed)
        _ = held.start(post)
        XCTAssertNil(held.away(post.key), "a post stays shown when it is scrolled past")
        XCTAssertEqual(held.embed, post)
        XCTAssertNil(held.away("nothing"), "nothing held")
    }

    /// The core's rules, carried out: every case the clients agree on.
    func testWhatStartingOneStopsAndWhatStopsOffScreenAreTheCoresInEveryCase() throws {
        func slot(_ key: String, _ value: Any?) throws -> PlaybackSlot {
            let playing = try XCTUnwrap(value as? [String: Any])
            let embed = try XCTUnwrap(playing["embed"] as? Bool)
            switch (embed, try XCTUnwrap(playing["kind"] as? String)) {
            case (false, "video"): return PlaybackSlot(key: key, kind: .video)
            case (false, "audio"): return PlaybackSlot(key: key, kind: .audio)
            case (true, "video"): return PlaybackSlot(key: key, kind: .embedPlayer)
            case (true, "audio"): return PlaybackSlot(key: key, kind: .embedAudio)
            case (true, "post"): return PlaybackSlot(key: key, kind: .embedPost)
            case let (embed, kind):
                XCTFail("nothing the phone plays: \(embed) \(kind)")
                return PlaybackSlot(key: key, kind: .video)
            }
        }
        let cases = try vectors("playback/cases.json")
        let starts = try XCTUnwrap(cases["starts"] as? [[String: Any]])
        XCTAssertFalse(starts.isEmpty)
        for c in starts {
            let why = c["why"] as? String ?? ""
            let starting = try slot("starting", c["starting"]), other = try slot("other", c["other"])
            let expected = try XCTUnwrap(c["stops"] as? Bool)
            var held = PlaybackHeld()
            _ = held.start(other)
            let effects = stops(held.start(starting))
            XCTAssertEqual(effects["other"], expected ? (other.kind.isEmbed ? .close : .pause) : nil, why)
            XCTAssertEqual(held.holds("other"), !(expected && other.kind.isEmbed), "an embed that stops goes back to its card; an attachment keeps its player: \(why)")
            XCTAssertEqual(held.playing, "starting", why)
        }
        let offScreen = try XCTUnwrap(cases["off_screen"] as? [[String: Any]])
        XCTAssertFalse(offScreen.isEmpty)
        for c in offScreen {
            let why = c["why"] as? String ?? ""
            let playing = try slot("playing", c["playing"])
            let expected = try XCTUnwrap(c["stops"] as? Bool)
            var held = PlaybackHeld()
            _ = held.start(playing)
            XCTAssertEqual(held.away("playing"), expected ? (playing.kind.isEmbed ? .close : .pause) : nil, why)
            XCTAssertEqual(held.holds("playing"), !(expected && playing.kind.isEmbed), why)
        }
    }

    func testOfTwoPressesTheLaterOnePlaysWhicheverIsFetchedFirst() {
        var held = PlaybackHeld()
        held.want("v1") // a large video pressed
        held.want("a1") // then a small recording
        XCTAssertTrue(held.arrived("a1"), "the last press starts when its file is here")
        _ = held.start(audio)
        XCTAssertFalse(held.arrived("v1"), "the video's file came after a later press: it does not take over")
        XCTAssertEqual(held.playing, "a1")
        XCTAssertFalse(held.holds("v1"))

        // An embed's address made after an attachment was started, by a press or its own controls.
        held.want(post.key)
        _ = held.start(video2)
        XCTAssertNil(held.wanted)
        XCTAssertFalse(held.arrived(post.key), "the card stays a card")

        // Pressed again while it is on its way, it is the last press again, and starts once.
        held.want("v1")
        held.want(player.key)
        held.want("v1")
        XCTAssertFalse(held.arrived(player.key))
        XCTAssertTrue(held.arrived("v1"))
        XCTAssertFalse(held.arrived("v1"))

        // Pausing another meanwhile is not a press for something else.
        held.want("v1")
        held.paused("v2")
        XCTAssertTrue(held.arrived("v1"))

        held.want("v1")
        held.failed(player.key)
        XCTAssertEqual(held.wanted, "v1", "another's failure changes nothing")
        held.failed("v1")
        XCTAssertNil(held.wanted)
        held.want("v1")
        _ = held.leave()
        XCTAssertFalse(held.arrived("v1"), "nothing starts after the conversation was left")
    }

    func testLeavingClosesEverything() {
        var held = PlaybackHeld()
        _ = held.start(video)
        _ = held.start(audio)
        _ = held.start(sound)
        XCTAssertEqual(Set(held.leave().map(\.key)), ["v1", "a1", sound.key])
        XCTAssertTrue(held.held.isEmpty)
        XCTAssertNil(held.playing)
        held = PlaybackHeld()
        _ = held.start(video)
        held.close("v1")
        XCTAssertFalse(held.holds("v1"))
        XCTAssertNil(held.playing)
    }

    func testAnEmbedsKindFromTheCore() {
        XCTAssertEqual(PlaybackSlot.Kind(embed: "player"), .embedPlayer)
        XCTAssertEqual(PlaybackSlot.Kind(embed: "audio"), .embedAudio)
        XCTAssertEqual(PlaybackSlot.Kind(embed: "post"), .embedPost)
        XCTAssertFalse(PlaybackSlot.Kind.video.isEmbed)
        XCTAssertEqual(player.summary.kind, "video", "an embed's player is a video to the core's rules")
        XCTAssertTrue(player.summary.embed)
        XCTAssertFalse(audio.summary.embed)
    }

    // MARK: Sizes

    private func embed(_ link: String) throws -> EmbedSummary { try XCTUnwrap(embedFor(url: link), link) }

    func testACardsEmbedIsAskedOfTheCoreOnceALink() throws {
        var asked: [String] = []
        let lookup = EmbedLookup { asked.append($0); return embedFor(url: $0) }
        let link = "https://www.youtube.com/watch?v=dQw4w9WgXcQ", plain = "https://www.rust-lang.org/"
        for _ in 0..<5 {
            XCTAssertEqual(lookup.embed(for: link)?.provider, "youtube")
            XCTAssertNil(lookup.embed(for: plain), "a link with no embed is remembered as one too")
        }
        XCTAssertEqual(asked, [link, plain], "however often its card is drawn")
        XCTAssertEqual(EmbedLookup().embed(for: link)?.kind, "player", "the core's by default")
    }

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

    func testAPostIsAtMost550WideAndGrowsToWhatTheCoreReadOfItsReport() throws {
        let x = try embed("https://x.com/jack/status/20")
        XCTAssertTrue(x.resizes)
        XCTAssertEqual(EmbedLayout.size(x, column: 343), CGSize(width: 343, height: 250), "its starting height")
        XCTAssertEqual(EmbedLayout.size(x, column: 700), CGSize(width: 550, height: 250))
        XCTAssertEqual(EmbedLayout.size(x, column: 343, reported: 613), CGSize(width: 343, height: 613))
        XCTAssertEqual(EmbedLayout.size(x, column: 343, reported: 3200), CGSize(width: 343, height: 3200), "a long post whole: the bounds are the core's, not the phone's")
        let report = #"{"twttr.embed":{"method":"twttr.private.resize","params":[{"height":1e9}]}}"#
        XCTAssertEqual(EmbedLayout.size(x, column: 343, reported: reportedEmbedHeight(provider: x.provider, message: report)), CGSize(width: 343, height: 5000), "held by the core")
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

    func testThePageHoldsOnlyTheEmbedAndListensOnlyForAPostThatGrows() throws {
        let src = "https://platform.twitter.com/embed/Tweet.html?id=20&dnt=true"
        let page = EmbedPage.html(src: src, title: "Jack \"on\" <X>", reports: true)
        XCTAssertTrue(page.contains(#"src="https://platform.twitter.com/embed/Tweet.html?id=20&amp;dnt=true""#))
        XCTAssertTrue(page.contains(#"title="Jack &quot;on&quot; &lt;X&gt;""#), "the title escaped")
        XCTAssertTrue(page.contains(#"var origin = "https://platform.twitter.com";"#))
        XCTAssertTrue(page.contains("<script>\n\(EmbedPage.listener(src: src))\n</script>"), "the script the next test runs")
        XCTAssertTrue(page.contains("messageHandlers.lumiaEmbed.postMessage"))
        XCTAssertFalse(page.contains("twttr") || page.contains("resize") || page.contains("JSON.parse"), "no site's form is read in the page: that is the core's")
        XCTAssertTrue(page.contains(#"allow="autoplay; encrypted-media; fullscreen; picture-in-picture""#))
        XCTAssertEqual(page.components(separatedBy: "<iframe").count, 2, "one iframe")
        XCTAssertTrue(page.contains(#"scrolling="no""#), "a post that grows is drawn whole, so a drag over it scrolls the conversation")
        let fixed = EmbedPage.html(src: "https://www.instagram.com/p/CxYz123abcD/embed", title: "", scrolls: true)
        XCTAssertFalse(fixed.contains("scrolling="), "a post of a fixed height scrolls inside")
        XCTAssertFalse(fixed.contains("<script"), "and one that does not grow is not listened to")
        let literal = EmbedPage.json("</script>\"\\")
        XCTAssertFalse(literal.contains("<") || literal.contains(">"), "nothing in it ends the script")
        XCTAssertTrue(literal.hasPrefix("\"") && literal.hasSuffix("\"") && literal.contains("/script"))
        XCTAssertTrue(literal.contains(#"\"\\""#), "its quote and backslash escaped")
    }

    /// The page's script, run: a stand-in for the page's window and its
    /// iframe, what `deliver` posts to it, and what it handed the app.
    private func runPage(src: String) throws -> JSContext {
        let context = try XCTUnwrap(JSContext())
        context.exceptionHandler = { _, error in XCTFail("the page's script threw: \(error?.toString() ?? "")") }
        context.evaluateScript("""
        var handed = [];
        var listeners = [];
        var frame = { contentWindow: {} };
        var elsewhere = {};
        var document = { getElementById: function (id) { return id === "embed" ? frame : null; } };
        var window = {
          addEventListener: function (type, listener) { if (type === "message") listeners.push(listener); },
          webkit: { messageHandlers: { \(EmbedPage.handler): { postMessage: function (message) { handed.push(message); } } } }
        };
        function deliver(data, fromTheEmbed, origin) {
          listeners.forEach(function (listener) { listener({ data: data, source: fromTheEmbed ? frame.contentWindow : elsewhere, origin: origin }); });
        }
        """)
        context.evaluateScript(EmbedPage.listener(src: src))
        XCTAssertEqual(context.evaluateScript("listeners.length")?.toInt32(), 1)
        return context
    }

    private func handed(_ context: JSContext) -> [String] {
        (context.evaluateScript("handed.splice(0)")?.toArray() ?? []).compactMap { $0 as? String }
    }

    func testThePagePassesOnWhatTheEmbedPostsAsTextAndTheCoreReadsTheHeight() throws {
        // Every case the clients agree on, posted as a string, through the page to the core.
        let cases = try XCTUnwrap(try vectors("embeds/heights.json")["vectors"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        let origin = "https://embed.example"
        let context = try runPage(src: "\(origin)/embed/1")
        let deliver = try XCTUnwrap(context.objectForKeyedSubscript("deliver"))
        for c in cases {
            let provider = try XCTUnwrap(c["provider"] as? String), message = try XCTUnwrap(c["message"] as? String)
            deliver.call(withArguments: [message, true, origin])
            let text = handed(context)
            XCTAssertEqual(text, [message], "a string is passed on as it was posted")
            XCTAssertEqual(reportedEmbedHeight(provider: provider, message: text.first ?? ""), (c["expect"] as? NSNumber)?.uint32Value, c["why"] as? String ?? "")
        }

        // An object is passed on as its JSON, in each site's form.
        func posted(_ object: String) -> String? {
            context.evaluateScript("deliver(\(object), true, \(EmbedPage.json(origin)))")
            return handed(context).first
        }
        let x = try XCTUnwrap(posted(#"{ "twttr.embed": { method: "twttr.private.resize", params: [{ height: 612.4, width: 550 }] } }"#))
        XCTAssertEqual(reportedEmbedHeight(provider: "x", message: x), 613)
        let xText = try XCTUnwrap(posted(#"{ "twttr.embed": JSON.stringify({ method: "twttr.private.resize", params: [{ height: 700 }] }) }"#))
        XCTAssertEqual(reportedEmbedHeight(provider: "x", message: xText), 700, "X's object holding the JSON as text")
        let bluesky = try XCTUnwrap(posted(#"{ height: 400.2, id: "1" }"#))
        XCTAssertEqual(reportedEmbedHeight(provider: "bluesky", message: bluesky), 401)
        XCTAssertNil(reportedEmbedHeight(provider: "x", message: bluesky), "any site's {height} is not X's")
        let reddit = try XCTUnwrap(posted(#"{ type: "resize.embed", data: 300 }"#))
        XCTAssertEqual(reportedEmbedHeight(provider: "reddit", message: reddit), 300)
        let long = try XCTUnwrap(posted(#"{ height: 3200 }"#))
        XCTAssertEqual(reportedEmbedHeight(provider: "bluesky", message: long), 3200, "a long post is drawn whole")
    }

    func testThePagePassesOnNothingFromAnotherWindowOrOrigin() throws {
        let origin = "https://platform.twitter.com"
        let context = try runPage(src: "\(origin)/embed/Tweet.html?id=20")
        let deliver = try XCTUnwrap(context.objectForKeyedSubscript("deliver"))
        let report = #"{"height":400}"#
        deliver.call(withArguments: [report, false, origin])
        XCTAssertEqual(handed(context), [], "another window, the embed's origin")
        deliver.call(withArguments: [report, true, "https://evil.example"])
        XCTAssertEqual(handed(context), [], "the embed's window, gone to another origin")
        deliver.call(withArguments: [report, true, origin])
        XCTAssertEqual(handed(context), [report])
        // What has no JSON is dropped, and nothing throws.
        context.evaluateScript("var loop = {}; loop.self = loop; deliver(loop, true, \(EmbedPage.json(origin))); deliver(undefined, true, \(EmbedPage.json(origin)));")
        XCTAssertEqual(handed(context), [])
        context.evaluateScript("deliver(42, true, \(EmbedPage.json(origin)))")
        XCTAssertEqual(handed(context), ["42"])
        XCTAssertNil(reportedEmbedHeight(provider: "x", message: "42"))
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

    func testABlueskyLookupSaysWhetherThePostWasNotFoundOrBlueskyNotReached() async throws {
        let (did, rkey) = ("did:plc:z72i7hdynmk6r22z27h6tvur", "3k2la3rsg5w2y")
        let address = try await blueskyAddress(handle: "ada.bsky.social", rkey: rkey) { handle in
            XCTAssertEqual(handle, "ada.bsky.social")
            return did
        }
        XCTAssertEqual(address, blueskyEmbedUrl(did: did, rkey: rkey))
        func unavailable(_ lookup: (String) async throws -> String?) async -> EmbedUnavailable? {
            do {
                _ = try await blueskyAddress(handle: "ada.bsky.social", rkey: rkey, lookup: lookup)
                return nil
            } catch {
                return error as? EmbedUnavailable
            }
        }
        // No account for the handle, or a DID no embed takes: the card opens the browser from then on.
        let notFound = EmbedUnavailable(words: "Bluesky couldn’t find @ada.bsky.social. The card opens it in the browser.", again: false)
        var found = await unavailable { _ in nil }
        XCTAssertEqual(found, notFound)
        found = await unavailable { _ in "did:plc:short" }
        XCTAssertEqual(found, notFound)
        // No answer from Bluesky is not word about the post: the next press asks again.
        found = await unavailable { _ in throw URLError(.timedOut) }
        XCTAssertEqual(found, EmbedUnavailable(words: "Couldn’t reach Bluesky. Try again.", again: true))
    }

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
        XCTAssertEqual(EmbedWords.unreached, "Couldn’t reach Bluesky. Try again.")
        XCTAssertEqual(MediaWords.position(ms: 12_000, of: 90_000), "0:12 / 1:30")
        XCTAssertEqual(MediaWords.position(ms: 95_000, of: 90_000), "1:30 / 1:30")
        XCTAssertEqual(MediaWords.position(ms: 12_000, of: 0), "0:12")
    }
}
