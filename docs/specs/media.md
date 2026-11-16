# Attachments and Link Cards

Status: accepted, 2026-11-15 (settled in a grill with Ryan the same day); amended 2026-11-16 for inline video and embeds (issue #21, decided with Ryan that day). This specification makes the attachment half of section 12 of [architecture.md](../architecture.md) and the link cards of section 6 implementable. Wire shapes are in `proto/lumia/v1/chat.proto` (`Attachment`, `BlobRef`, `Card`), `commands.proto` (`SendMessage`, `EditMessage`, `SendEncrypted`), `events.proto` (`MessageCardsSet`, `AttachmentsExpired`), `api.proto` (uploads, `UnfurlRequest`, `ServerStorage`) and `e2ee.proto` (`DmAttachment`, `DmCard`). Where this document and the architecture disagree, the architecture wins and this document is fixed.

## 1. Model

```text
Blob        one encrypted object in the blob store, uploaded by a client, named by an instance-minted id
BlobRef     a blob's id, its stored size, and what opens it: key, nonce, BLAKE3 of the plaintext
Attachment  one thing a message carries: a kind, a name, a media type, dimensions, a placeholder,
            alt text, a main BlobRef and an optional thumbnail BlobRef
Card        a link's preview: URL, title, description, site name, colour, optional image
```

Three rules hold everywhere:

- **The sharer's client does all attachment work.** Stripping metadata, resizing, thumbnails, the placeholder and encryption happen in `lumia-core` on the sharer's device, for server content and personal DMs alike. The instance never decodes an attachment's bytes; it never sees them in the clear. Avatars and server icons are not attachments and stay as built (architecture 12, "Avatars").
- **Every blob is client-encrypted.** A random 32-byte key and a 24-byte nonce per blob, XChaCha20-Poly1305 over the whole plaintext, the BLAKE3-256 of the plaintext checked after decryption. The blob store, and the bucket's operator, hold only ciphertext.
- **An attachment is never a link.** It is a structured record on the message. Every fetch of an attachment blob is access-checked against the message that carries it (section 4.3). A card's image is the one exception, served by its unguessable address (section 6.4).

## 2. Kinds and caps

| Kind | What | Main blob | Thumbnail blob | Cap on the main blob |
|---|---|---|---|---|
| `IMAGE` | A still picture, processed (section 3.1) | JPEG or PNG made by the core | JPEG or PNG, ≤ 1,080 px | `LUMIA_UPLOAD_IMAGE_MAX_BYTES`, default 2 MiB |
| `GIF` | An animated GIF, as chosen | the GIF | first frame, ≤ 1,080 px | `LUMIA_UPLOAD_FILE_MAX_BYTES`, default 10 MiB |
| `VIDEO` | MP4, MOV (metadata stripped) or WebM (as chosen) | the video | poster frame when the platform made one | file cap |
| `AUDIO` | MP3, M4A, AAC, OGG, Opus, WAV, FLAC | the audio | none | file cap |
| `FILE` | Anything else, and any picture sent "as a file" | the file, metadata stripped where section 3.4 says | none | file cap |

A thumbnail blob is at most 512 KiB. A message carries at most 10 attachments. The caps are instance-wide until entitlements exist (architecture 16); they then come from the plan of whoever pays, and the two variables become the instance's ceiling. The instance enforces a blob's size against the cap of the role it was created for (section 4.1) and, at send, that each attachment's blobs were created for the roles its kind needs. It cannot check that a blob holds what its kind says, since it never sees the bytes; a client that lies about a kind still cannot store more than the file cap.

## 3. Processing in the client

`lumia-core` owns this as pure functions over bytes (no I/O, so it runs the same on every platform and in wasm). A shell hands it the file's bytes, its name and, where the platform made them, decoded pixels or a poster frame; it hands back the blobs to encrypt and the attachment's metadata.

Decoding is bounded exactly as the instance bounds an avatar: the format from the bytes (never the name or the declared type), the declared dimensions checked before decoding (at most 8,192 px a side and 40 million in all), the decode under an allocation limit. A file that fails any of these is sent as a `FILE`.

### 3.1 Still images

Formats decoded by the core: JPEG, PNG, WebP, and a GIF with one frame. HEIC and HEIF have no Rust decoder; a shell whose platform decodes them (iOS, Android, macOS, Safari) hands the core the decoded pixels and the orientation, and a shell that cannot sends the file as a `FILE`.

- **Display**: the orientation applied, every other piece of metadata dropped (EXIF, XMP, IPTC, comments, ICC beyond sRGB conversion), the longest edge at most 2,560 px. Encoded as PNG when it has transparency and the PNG fits under the image cap; otherwise JPEG, starting at quality 85 and stepping down by 10 to 55, then the longest edge down by a quarter at a time, until it fits. A picture that cannot fit at 640 px is refused in words.
- **Thumbnail**: the display image with its longest edge at most 1,080 px (a picture across a phone's column on a display of three pixels a point), JPEG at 85 (PNG when transparent), stepping down in quality and then size to fit the instance's thumbnail cap (`UploadLimits.thumbnail_max_bytes`, 512 KiB).
- **Placeholder**: the ThumbHash of a 100 px version, carried inline in the attachment (about 25 bytes).
- **Dimensions**: the display image's width and height, after orientation.

No original is kept. WebP is never produced: its lossy encoder is C, and the core builds for wasm without one.

### 3.2 GIFs

Kept byte for byte apart from metadata: comment extensions and application extensions other than `NETSCAPE2.0` (the loop count) are dropped, which removes XMP. The first frame becomes the thumbnail and the placeholder; the logical screen size is the dimensions. A GIF of one frame is an `IMAGE`.

### 3.3 Video

Never re-encoded. MP4 and MOV are rewritten in place: every `udta` and `meta` box (location, device, software) becomes a `free` box of the same size with its contents zeroed, and the creation and modification times in `mvhd`, `tkhd` and `mdhd` are zeroed. Box sizes do not change, so no chunk offset moves. WebM is uploaded as chosen. The dimensions and duration come from the container where the core can read them (`tkhd` for MP4 and MOV), and from the shell otherwise.

The poster frame comes from the platform: AVFoundation on Apple, `MediaMetadataRetriever` on Android, a `<video>` element drawn to a canvas on the web. The desktop shell on Windows and Linux makes none yet. A poster becomes the thumbnail blob and the placeholder, processed as a still image; without one, the placeholder is a play symbol on the theme's surface.

### 3.4 Files

A picture sent "as a file" keeps its pixels and loses its metadata losslessly: JPEG drops every APP segment but the ICC profile (APP2) and a minimal EXIF holding only the orientation; PNG drops `eXIf`, `tEXt`, `iTXt`, `zTXt` and `tIME`; WebP drops its `EXIF` and `XMP ` chunks and clears their flags; HEIC and HEIF zero the payloads of their `Exif` and `mime` (XMP) items in place; MP4 and MOV as in 3.3. Any other file goes as chosen: its own metadata (a PDF's author) is its owner's choice. The name is the file's name, at most 255 bytes of UTF-8.

## 4. Storage

### 4.1 Uploading

```text
client                          instance                          blob store
  | POST /uploads {channel, role,  |                                  |
  |   size}                       -> check ATTACH_FILES, the cap       |
  |                                | row: blob id, uploader, channel,  |
  |                                |   role, size, state=pending       |
  | <- {blob_id, upload_url,       |                                  |
  |     headers, expires_at}       |                                  |
  | PUT upload_url (exact length) -------------------------------------> |
  | POST /uploads/{id}/complete   -> HEAD the object: size matches?     |
  | <- 204                         | state=stored                      |
  | send the message naming it    -> state=referenced                  |
```

- **Roles**: `DISPLAY` (an `IMAGE`'s main blob, image cap), `THUMBNAIL` (512 KiB), `FILE` (every other main blob, file cap). The size is the ciphertext's: the plaintext plus the 16-byte tag.
- **The upload URL** is a presigned S3 `PUT` for `att/<blob id>`, valid for 15 minutes, with `content-length` among its signed headers so the store refuses any other length. For a `local:` or `fs:` store it is the instance's own `PUT /api/v1/blobs/upload/{token}`, where the token is a signed, expiring statement of the blob id and size, needing no session. Clients treat both the same: `PUT` the ciphertext with the headers the response names.
- **Complete** checks the stored object's length against the declared size and deletes the object and the row on a mismatch.
- **Orphans**: a blob neither referenced nor deleted 24 hours after it was created is removed, object and row, by an Oban job.
- **Removal**, by any path: the row is marked first, which stops the blob being served or named, then the object is deleted, then the row. A row whose object the store did not let go stays marked, and the hourly job asks again.
- The bucket needs a CORS rule allowing `PUT` and `GET` from the web client's origin, with the `content-length` and `content-type` headers. The deployment spec says so.

### 4.2 Sending

**Server content.** `SendMessage.attachments` carries each attachment whole: kind, name, media type, dimensions, duration, placeholder, alt text, and the `BlobRef`s with their keys. The instance checks that each blob is the caller's, `stored`, unreferenced, in the same channel, of the right role for its kind, and within the 10-attachment limit; refuses with `INVALID_ARGUMENT` otherwise. It keeps the attachments with the message, keys included, as it keeps the message's text (server content is readable by the instance, architecture hard rule 5; the event's stored payload is the bytes delivered, delivery spec 4), and marks the blobs referenced. The keys reach only those who can read the message. The blob store alone opens nothing.

**Personal DMs.** Uploads name the conversation (`PERSONAL_*` channel). The attachment metadata and keys travel in `DmSend.attachments` inside the MLS message. `SendEncrypted.blob_ids` lists the blobs in the clear so the instance marks them referenced and the orphan job leaves them; blob ids, sizes and their count are what the instance could already see.

### 4.3 Fetching

`GET /api/v1/blobs/{blob_id}` with the session answers `302` to a presigned `GET` valid for 60 seconds (S3), with `Cache-Control: no-store` so no redirect outlives a delete, or the ciphertext itself (`local:`, `fs:`), with `Cache-Control: private, max-age=300`. The caller must be able to read the message that references the blob: for server content, `VIEW_CHANNEL` and `READ_HISTORY` on its channel and the message not deleted; for a personal DM, membership of the conversation. A blob not referenced yet is fetchable only by its uploader. Anything else is `NOT_FOUND`. The client decrypts, checks the hash, and keeps the plaintext in its own cache.

### 4.4 Removal

- **Server content**: an edit may remove attachments (`EditMessage.remove_attachment_ids`) but not add them. A delete tombstones the message and its attachments stop being served at once; their blobs go when the message is purged (after the instance's delete window, 30 days by default, architecture 8.5), by the same job that serves retention, account erasure and "also delete my messages". Moderators remove media by deleting the message. A server's settings can also expire its attachments by age or to make room (4.5).
- **Personal DMs**: the sender's client, on a `DmDelete` or a `DmEdit` that drops attachments, calls `DELETE /api/v1/blobs/{blob_id}` (the uploader only; the object goes at once). Blobs also go with their conversation when it is gone for every participant, and with their uploader's account on erasure.

### 4.5 A server's storage

Decided with Ryan 2026-11-15 (architecture 6.7, "A server's files").

- **What counts.** Every blob made for one of the server's channels, server DMs included, by its stored length, from the moment its slot is made (`POST /uploads`) until it is marked for removal: blobs uploading, stored, named by a message, and those of deleted messages still in their delete window. The instance keeps the sum on the server's row, changed in the transaction that makes a slot or marks blobs, so the limit is checked without listing the bucket and two uploads at once cannot both pass it. Link cards' pictures and the server's picture are the instance's and do not count. Personal conversations count against nobody yet (architecture 16 says the sender; not built).
- **The limit** is `LUMIA_SERVER_STORAGE_MAX_BYTES`, the same for every server, with none when it is unset. A per-server allowance comes with plans (architecture 16).
- **Usage**: `GET /servers/{id}/storage` answers `ServerStorage {used_bytes, max_bytes}` (`max_bytes` 0 for no limit) to whoever holds `MANAGE_SERVER`. The server's settings show it: "1.2 GB of 5 GB used", or "1.2 GB used".
- **When the server is full** (`Server.storage_full`, changed as the other settings are, architecture 6.7):
  - `REFUSE` (the default): a slot that would take the server past its limit is refused with `RESOURCE_EXHAUSTED`, `details.state` `storage` and `details.quota` the limit in bytes. Clients say "This server is out of space for files" on the file.
  - `DELETE_OLDEST`: the instance first expires the attachments of the server's oldest messages, across its channels, until the new blob fits, then makes the slot. What blobs that are not yet in a message hold cannot be freed this way, so a blob that still does not fit, or one larger than the limit, is refused as above.
- **How long attachments are kept** (`Server.attachment_retention_days`, 0 for ever, the default): a daily job expires the attachments of every message older than that. It is apart from message retention: a server can keep its text for ever and its files for 90 days. Message retention still takes a message's attachments with it.
- **Expiring** is done per channel, for every message below a `seq`, so one event says it: the messages' blobs are marked and removed as in 4.1; each such message's attachments are rewritten in place with `expired` set and their `BlobRef`s gone (name, kind, media type, dimensions and placeholder stay; the message's text and cards are untouched), in the messages and in the stored events that carry them, so history served later says so too; and `AttachmentsExpired {before_seq}` is appended to the channel. A client marks every attachment below `before_seq` expired in its store, stops fetching it, and draws it as a file row with the name and "No longer available". What a client already holds in its own cache may stay there: this frees the instance's storage and is not a deletion promise (a message deleted is, 4.4).
- **Personal DMs** are untouched by both settings: no server governs them.

**Rejected.** *Counting by listing the bucket, with a prefix per server.* S3 and R2 can neither total a prefix nor delete one in a call, so it would be a listing per check; the rows already know each blob's server and length, and they are what finds an object, so an object removed from the bucket by prefix would leave rows naming nothing. *Letting the limit decide what goes.* A server whose admins chose `REFUSE` never loses a file to the limit (architecture 16: no entitlement overrides governance).

## 5. Showing attachments

- **Images**: drawn at their dimensions scaled to the column (at most 400 px tall), the placeholder until the thumbnail arrives, the display blob fetched when opened. Clicking opens a full-screen viewer with zoom and Save.
- **GIFs**: animate in place unless the system asks for reduced motion, in which case the first frame is shown with a GIF badge and plays on click.
- **Video**: the poster (or the play placeholder) with a play button. Pressed, it plays in the message row with the platform's controls and a full-screen button: a `<video>` element on the web, AVPlayer on iOS, Media3 on Android, Rune's video element in the desktop shell on macOS and Windows. Linux, where Rune has none, opens it in the system's player. Never autoplays; starting one pauses any other, and it pauses when its row scrolls away or the conversation changes.
- **Audio**: an inline player with controls. It plays on until another starts or the conversation is left.
- **Files**: a row with an icon for the kind, the name, the size and a download control.
- **Alt text**: an image's alt text is its accessibility label and is shown in the viewer.
- **Display settings** (plan step 9, "Display settings"): **Images** `link` draws every attachment as a file row that opens the viewer when clicked, and a video's or a recording's row plays it on a press (a recording under its row, as it always does); **Embeds** `link` draws no cards and no embeds (6.6). The controls appear on the Preferences page now that they act.
- **Receiving is decoding untrusted bytes.** Nobody re-encodes on the instance, so every client decodes what a sharer chose: the same bounds as 3 apply before decoding, and a picture that fails them is drawn as a file row.
- **Notifications** stay text: "Priya sent a photo", "sent a file".
- **Search**: a server attachment's name is indexed with its message; on-device search does the same for personal DMs.
- **Composing**: a button, paste, and drag and drop add files to a strip above the composer, each with its preview, a ×, and (for images) an alt text field. Processing and upload start when a file is added; the message sends when they finish, and a failure is said in words on the file.

## 6. Link cards

### 6.1 Which links

Up to 3 per message, in order of appearance: `http` and `https` URLs in the text, never inside an inline code span or a code block, never written as `<https://…>` (which means "no card"). The core's message parser decides this, the same on every client.

### 6.2 What a card holds

From the page's `<head>`: `og:title`, `og:description`, `og:site_name`, `og:image` (with `og:image:width` and `height`), `theme-color`; failing Open Graph, `twitter:title`, `twitter:description`, `twitter:image`; failing those, `<title>` and `<meta name="description">`. A title is at most 300 characters and a description at most 1,000, trimmed with an ellipsis. A page with no title gets no card. An `og:video` or `og:type` of `video.*` marks the card as a video, drawn with a play symbol over its image. Pressing a card whose link has an embed (6.6) loads the embed in its place; any other card opens the URL in the browser, video or not. oEmbed comes later.

### 6.3 The fetcher

The same rules in Elixir (the instance) and Rust (`lumia-client`, native targets only):

- `http` and `https` only, ports 80 and 443, at most 5 redirects, each hop held to every rule here.
- The host is resolved first and the connection made to the address resolved; any address that is private, loopback, link-local, unique-local, multicast, unspecified, CGNAT (100.64/10), or the cloud metadata address is refused, IPv4 and IPv6 alike, IPv4-mapped IPv6 included. In the client this keeps a link from reading the sharer's own router.
- 5 s for the whole fetch: resolving the name, the page and its picture share one deadline. At most 1 MiB of HTML, reading stopped once the head has ended as the card's tokenizer reads it (a `</head>` or `<body>` outside comments and scripts); only `text/html` and `application/xhtml+xml` are parsed. The image at most 5 MiB.
- A link as people write it: a character a URL cannot hold as written (a space, `|`, a letter outside ASCII in the path) is percent-encoded before the URL is parsed, as browsers do.
- User agent `LumiaBot/<version> (+https://lumia.chat/bot)`; no cookies; `Accept-Language` from the sharer's locale on the client and `en` on the instance.
- The image is fetched by the same rules, decoded under the same bounds as 3, and kept as a thumbnail (longest edge 800 px, JPEG or PNG). Receivers never contact the site until they open the link.

### 6.4 Server content: the instance builds the card

The instance fetches pages only when its operator turned link cards on (`LUMIA_LINK_CARDS=true`; architecture hard rule 3). Until then `GET /instance` says `link_cards_enabled: false`, `POST /unfurl` answers no card, and messages are sent with none; nothing else changes.

- **Preview**: while composing, every client (native and web) asks `POST /api/v1/unfurl {channel_id, url}` once the typing pauses for 500 ms, and draws the card above the composer with a ×. The instance needs `SEND_MESSAGES` in the channel, limits each account to 30 fetches a minute (counted from when a fetch starts, so requests made at once cannot pass it; a page fetched for a sent message counts against its author, and one past the limit goes without a card), and keeps each result in its cache for an hour, keyed by URL (`link_cards`: the card and its image's address; a page that gave no card is cached as none, for 10 minutes).
- **Send**: `SendMessage.card_urls` names the links whose cards were kept (a subset of 6.1's). The instance checks each appears in the body, is `http(s)`, and that there are at most 3. It appends `MessageCreated` with the cards it has cached; for any it has not, it fetches after the commit and appends `MessageCardsSet` (the message's cards in full) when they arrive. Sending never waits for a fetch.
- **Edit**: `EditMessage.cards` (when set) names the links whose cards stay; a link no longer in the body loses its card; a new one in the set is fetched as at send. The author removes a card this way.
- **The card's image** is a blob made and sealed by the instance (`Lumia.Blobs`, through the avatar pipeline in `native/lumia_media`), served by its address at `GET /api/v1/cards/{address}` without a session, as an avatar is: it is a copy of a public page's picture, and its address is 64 hex digits nobody can compute. A card image no message references and older than the cache is deleted by the orphan job.
- Cards are inside the message's payload, so retention and purge drop them with it. A deleted message holds its cards' pictures until its delete window passes (as its attachments, 4.4), then lets them go.

Because the instance builds them, server cards cannot be forged by a member, and sites see the instance's address, never a member's.

### 6.5 Personal DMs: the sharer's client builds the card

The instance must not learn what is linked in a personal conversation. On native clients the core fetches the page (6.3), builds the card, encrypts its image as a thumbnail blob uploaded like an attachment's (role `THUMBNAIL`), and sends it in `DmSend.cards` inside the MLS message. `DmEdit.cards`, when set, replaces them. The web client cannot read other sites (CORS) and does not use the instance here, so a link sent from the web in a personal DM has no card.

A modified client can forge a personal DM card. Receivers bound what a forgery can do:

- a card is drawn only if its URL appears in the message text;
- the site shown is the host the receiver parses from that URL, never the card's `site_name`;
- opening the card opens the URL from the text.

A forged card can mislead about a page's title and picture, never about where it goes.

### 6.6 Embeds

Decided with Ryan 2026-11-16 (issue #21). A link to one of the sites below plays or shows inside the conversation, in its card's place, once someone presses the card.

- **Sites.** Players: YouTube (Shorts and `youtu.be` too), Vimeo, Twitch (past broadcasts and clips), TikTok, and Facebook's videos. Audio: SoundCloud and Spotify. Posts: X (twitter.com too), Bluesky, Facebook, Instagram (posts and reels) and Reddit. Each is the site's own iframe embed, which needs no API key. Mastodon is not one: its posts' addresses cannot be told from any other site's. There is no generic player for a page's `og:video`: it would load whatever player a page names.
- **The embed comes from the link, not the card.** `lumia_core::embeds::embed_for` reads a link the text holds against one pattern per site and builds the embed's address from the parts it checked (an id of the site's length and alphabet, a number that is a number), so nothing odd in a link reaches an embed's address. A link no pattern names exactly (a profile, a search, a comment, a short link only the site can follow) has no embed; its card opens the browser as before. `proto/vectors/embeds/cases.json` holds the cases every client agrees on. Nothing is stored and the protocol does not change: a forged card cannot swap in another player, since the card is never read for one.
- **Nothing loads from a site until a press.** The card is drawn as 6.2 has it, with a play button over its picture for a player or audio and a "Show post" button for a post; pressing it loads the embed in the card's place. Receivers still never contact a site until they choose to (6.3), in personal conversations as in channels. YouTube is loaded from its privacy-enhanced domain (`youtube-nocookie.com`), X's with `dnt=true`. The press is the play: a player's address asks it to start (`autoplay=1`, SoundCloud's `auto_play=true`), and the host lets it (`allow="autoplay"`).
- **Bluesky.** An embed names the account by its DID. A link that does too (`bsky.app/profile/did:plc:…/post/…`) has its address at once; one that names a handle (`bsky.app/profile/<handle>/post/…`) is resolved on the press, by the client, from Bluesky's public API (`com.atproto.identity.resolveHandle` at `public.api.bsky.app`, which answers without a session and with CORS headers), never through the instance; the DID it gives is checked (`did:plc:` and 24 base32 letters, or `did:web:` and a host) before it goes into the address (`bluesky_embed_url`). A handle that does not resolve leaves the card, which then opens the browser.
- **Twitch** refuses an embed that does not name the page holding it: the client adds `&parent=` and its own host to the address (the instance's, on the web and the phones).
- **Size.** Players at the column's width at most, at their shape, and at most 400 tall as pictures are: 16:9, and 9:16 for YouTube's Shorts and TikTok. Audio at the column's width and the height of the site's own embed code: SoundCloud 166 for a track and 450 for a playlist; Spotify 152 for a track or an episode and 352 for an album, a playlist, a show or an artist. Posts at most 550 wide: X, Bluesky and Reddit start 250 tall and grow to the height their embed reports by `postMessage`, taken only from that embed's own window and read in each site's own form by `lumia_core::embeds::reported_height` (held to 50–5000; `proto/vectors/embeds/heights.json`); Facebook's posts are 500 tall and Instagram's 640, and what does not fit scrolls inside. TikTok is a player rather than a post of a fixed height: the embed TikTok documents and its oEmbed gives (`tiktok.com/player/v1`) is an upright video, where the older post embed (`embed/v2`) is as tall as its caption.
- **Hosting.** The web draws an iframe sandboxed to `allow-scripts allow-same-origin allow-popups allow-popups-to-escape-sandbox allow-presentation` (scripts, the player's own origin rather than none, its links opening in a new tab, and full screen), with `allow="autoplay; encrypted-media; fullscreen; picture-in-picture"` and `referrerpolicy="strict-origin-when-cross-origin"`; the instance's Content-Security-Policy for the web app names the embeds' origins (`lumia_core::embeds::EMBED_ORIGINS`) in `frame-src`. The phones draw a web view (WKWebView, Android WebView) whose page is the embed's iframe alone, with the instance's origin as its base so that a site refusing an embed with no referrer (YouTube) plays; it loads only those origins, and any other link opens in the browser.
- **The desktop has no web view**, so a card with an embed still opens the browser there; only attached videos play inline (5).
- **Leaving it.** A video stops when its row scrolls away or the conversation changes: an embed goes back to its card, since no site's player can be paused from outside it without that site's own script. Audio (SoundCloud, Spotify) plays on until another starts or the conversation is left. A post stays shown when it is scrolled past and when an attachment starts; it is being read, not played. Pressing a card puts back any other embed (one is shown at a time) and pauses any attached video or audio; starting an attachment puts back an embed that plays (a player or audio). These are `lumia_core::playback`'s rules, with the cases every client agrees on in `proto/vectors/playback/cases.json`.
- **Screen readers.** The press is a button named by the core's label and the card's title after a comma: "Play video from YouTube, <title>", "Play audio from SoundCloud, <title>", "Show post from X". The iframe's title is the card's title, or the site's name when there is none.
- **Display settings.** Embeds `link` draws no card and so no embed; the link in the text opens the browser.

## 7. Not in v1

Embeds for sites not in 6.6 (Mastodon among them) and for a page's `og:video`, embeds in the desktop shell (it has no web view), inline video on Linux's desktop, oEmbed, spoilers, text and code file previews, image editing or cropping before sending, re-encoding video, an instance moderation action that removes one attachment, per-plan caps and allowances, a limit on what personal conversations store, cards in personal DMs sent from the web.
