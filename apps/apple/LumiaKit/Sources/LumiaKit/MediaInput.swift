// What the platform knows of a file before the core makes an attachment
// of it (specs/media.md 3): a still picture decoded by ImageIO into
// upright sRGB RGBA8 (the only way a HEIC or HEIF gets in, since the core
// has no decoder for them), a video's poster frame, length and oriented
// size from AVFoundation, an audio file's length; and the other way, a
// picture someone sent decoded for drawing once the core says its header
// is within the bounds. The same bounds as the core's apply before
// ImageIO decodes anything.
import Accelerate
import AVFoundation
import CoreGraphics
import Foundation
import ImageIO
import LumiaFFI
import UniformTypeIdentifiers

public enum MediaInput {
    /// The core's decode bounds: a side and the pixels in all.
    public static let maxSide = 8192
    public static let maxPixels = 40_000_000
    /// The longest edge handed to the core: its display picture's.
    public static let longestEdge = 2560
    /// A video's poster: the core makes its thumbnail of at most 1,080 px from it.
    public static let posterEdge = 1280
    /// The longest edge a message's picture is decoded at: a thumbnail's
    /// own (specs/media.md 3.1), so one is drawn with every pixel it was
    /// sent with. It was 800 while thumbnails were 400, and stayed 800 when
    /// they became 1,080, so a phone drew them soft.
    public static let thumbnailEdge = 1080

    /// What kind of file the name and bytes say this is.
    public enum Nature: Equatable, Sendable {
        case still, gif, video, audio, other
    }

    public static func nature(name: String, type: UTType? = nil, data: Data? = nil) -> Nature {
        let type = type ?? UTType(filenameExtension: (name as NSString).pathExtension)
        if let data, let source = CGImageSourceCreateWithData(data as CFData, nil), let id = CGImageSourceGetType(source) as String? {
            let found = UTType(id)
            if found == .gif { return .gif }
            if found?.conforms(to: .image) == true { return .still }
        }
        guard let type else { return .other }
        if type.conforms(to: .gif) { return .gif }
        if type.conforms(to: .image) { return .still }
        if type.conforms(to: .movie) || type.conforms(to: .video) { return .video }
        if type.conforms(to: .audio) { return .audio }
        return .other
    }

    /// A picture's file as the core takes it: its bytes as chosen, with the
    /// pixels upright beside them for a still (a GIF is the core's to read,
    /// so its frames are kept). A picture over the bounds, or one ImageIO
    /// cannot read, goes without pixels, and the core sends it as a file
    /// when it cannot read it either.
    public static func picture(data: Data, name: String) -> AttachmentInput {
        let pixels = nature(name: name, data: data) == .still ? upright(data, longest: longestEdge) : nil
        return AttachmentInput(bytes: data, name: name, pixels: pixels)
    }

    /// The width and height a picture's header names, before any decoding.
    public static func size(of data: Data) -> (width: Int, height: Int)? {
        guard let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let w = props[kCGImagePropertyPixelWidth] as? Int, let h = props[kCGImagePropertyPixelHeight] as? Int else { return nil }
        return (w, h)
    }

    /// Within the core's bounds: at most 8,192 px a side and 40 million in all.
    public static func withinBounds(width: Int, height: Int) -> Bool {
        width > 0 && height > 0 && width <= maxSide && height <= maxSide && width * height <= maxPixels
    }

    /// A still picture decoded upright (its orientation applied), its
    /// longest edge at most `longest`, as sRGB RGBA8 without premultiplied
    /// alpha; nil over the bounds or when it does not decode.
    public static func upright(_ data: Data, longest: Int) -> Pixels? {
        guard let (w, h) = size(of: data), withinBounds(width: w, height: h),
              let image = decode(data, longest: min(longest, max(w, h))) else { return nil }
        return rgba(image)
    }

    /// Decodes a picture with its orientation applied, at most `longest` on its longest edge.
    public static func decode(_ data: Data, longest: Int) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: longest,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }

    /// A picture's pixels as the core takes them: sRGB, RGBA8, row by row,
    /// alpha not premultiplied.
    public static func rgba(_ image: CGImage) -> Pixels? {
        let (w, h) = (image.width, image.height)
        guard w > 0, h > 0, let space = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
        var bytes = Data(count: w * h * 4)
        let drawn = bytes.withUnsafeMutableBytes { raw -> Bool in
            guard let context = CGContext(data: raw.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
            context.interpolationQuality = .high
            context.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
            // Only a picture with transparency needs its alpha taken back out.
            if image.alphaInfo != .none, image.alphaInfo != .noneSkipLast, image.alphaInfo != .noneSkipFirst {
                var buffer = vImage_Buffer(data: raw.baseAddress, height: vImagePixelCount(h), width: vImagePixelCount(w), rowBytes: w * 4)
                vImageUnpremultiplyData_RGBA8888(&buffer, &buffer, vImage_Flags(kvImageNoFlags))
            }
            return true
        }
        return drawn ? Pixels(rgba: bytes, width: UInt32(w), height: UInt32(h)) : nil
    }

    /// RGBA8 pixels (a placeholder, a poster) as an image to draw.
    public static func image(_ pixels: Pixels) -> CGImage? {
        let (w, h) = (Int(pixels.width), Int(pixels.height))
        guard w > 0, h > 0, pixels.rgba.count >= w * h * 4, let space = CGColorSpace(name: CGColorSpace.sRGB),
              let provider = CGDataProvider(data: pixels.rgba as CFData) else { return nil }
        return CGImage(width: w, height: h, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: w * 4, space: space, bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue), provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }

    /// A picture someone sent, decoded for drawing: only once the core has
    /// read its header and found it within the bounds, else nil (drawn as
    /// a file row).
    public static func drawable(_ data: Data, longest: Int = longestEdge) -> CGImage? {
        guard attachmentDrawable(bytes: data) else { return nil }
        return decode(data, longest: longest)
    }

    /// A small picture of a file for the composer's strip.
    public static func preview(_ data: Data) -> CGImage? {
        guard let (w, h) = size(of: data), withinBounds(width: w, height: h) else { return nil }
        return decode(data, longest: 160)
    }

    /// A video file as the core takes it: its bytes, its poster frame
    /// upright, its length and its size as it plays (rotation applied).
    /// Whatever AVFoundation cannot read is left out, and the core reads
    /// the container itself where it can.
    public static func video(url: URL, name: String) async throws -> (input: AttachmentInput, poster: CGImage?) {
        let data = try Data(contentsOf: url)
        let asset = AVURLAsset(url: url)
        var durationMs: UInt32?
        if let duration = try? await asset.load(.duration), duration.isNumeric {
            durationMs = UInt32(clamping: Int(duration.seconds * 1000))
        }
        var size = CGSize.zero
        if let track = try? await asset.loadTracks(withMediaType: .video).first,
           let natural = try? await track.load(.naturalSize), let transform = try? await track.load(.preferredTransform) {
            let turned = natural.applying(transform)
            size = CGSize(width: abs(turned.width), height: abs(turned.height))
        }
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: posterEdge, height: posterEdge)
        let poster = try? await generator.image(at: .zero).image
        let input = AttachmentInput(bytes: data, name: name, poster: poster.flatMap(rgba), durationMs: durationMs, videoWidth: UInt32(size.width.rounded()), videoHeight: UInt32(size.height.rounded()))
        return (input, poster)
    }

    /// An audio file as the core takes it: its bytes and its length.
    public static func audio(url: URL, name: String) async throws -> AttachmentInput {
        let data = try Data(contentsOf: url)
        var durationMs: UInt32?
        if let duration = try? await AVURLAsset(url: url).load(.duration), duration.isNumeric {
            durationMs = UInt32(clamping: Int(duration.seconds * 1000))
        }
        return AttachmentInput(bytes: data, name: name, durationMs: durationMs)
    }

    /// A file on disk as the core takes it, by what it is, and a small
    /// picture of it for the strip where there is one.
    public static func file(url: URL, name: String) async throws -> (input: AttachmentInput, preview: CGImage?) {
        switch nature(name: name) {
        case .video:
            let (input, poster) = try await video(url: url, name: name)
            return (input, poster)
        case .audio:
            return (try await audio(url: url, name: name), nil)
        case .still, .gif, .other:
            let data = try Data(contentsOf: url)
            return (picture(data: data, name: name), preview(data))
        }
    }

    /// A file's size on disk, read before its bytes are.
    public static func fileSize(_ url: URL) -> UInt64? {
        (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize).map { UInt64($0) }
    }

    /// Whether a file can go at all: anything but a still picture (which
    /// the core makes smaller) must fit under the instance's file cap.
    /// The words when it cannot.
    public static func refusal(size: UInt64, nature: Nature, limits: UploadLimits) -> String? {
        guard nature != .still, size > limits.fileMaxBytes else { return nil }
        return MediaWords.tooLarge(size, max: limits.fileMaxBytes)
    }
}
