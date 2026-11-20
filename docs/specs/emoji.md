# Custom Emoji

Status: accepted, 2026-11-20 (GitHub #17; decided with Ryan the same day). This specification makes the custom emoji of section 6.6 of [architecture.md](../architecture.md) implementable. Wire shapes are in `proto/lumia/v1/chat.proto` (`CustomEmoji`, `EmojiRef`, `Message.emoji`, `ReactionCount.emoji_id`, `Server.emoji_limit`, `Permission.MANAGE_EMOJI`), `commands.proto` (`SendMessage.emoji`, `EditMessage.emoji`, `AddReaction.emoji_id`, `RemoveReaction.emoji_id`), `events.proto` (`MessageEdited.emoji`, `ReactionAdded.emoji_id`, `ReactionRemoved.emoji_id`, `EmojiUpdated`, `MembershipAdded.emoji`) and `api.proto` (`ListEmojiResponse`, `UpdateEmojiRequest`, `AccountSnapshot.emoji`). Where this document and the architecture disagree, the architecture wins and this document is fixed.

## 1. What was decided

- **A server has emoji of its own**, each a picture and a name, used as `:name:` in that server's messages and as reactions there.
- **Only in that server**: its channels and its server DMs. Not in another server, on this instance or another, and not in a personal conversation (Ryan, 2026-11-20). In a personal one the picture would have to be fetched from the server's instance while a private message is read, which tells that instance an address read something at that moment; and the picture is bytes another server's uploader chose, sent into a conversation that server does not govern.
- **Uploading, renaming and removing** need `MANAGE_EMOJI`, given by roles as every permission is; the owner and `ADMINISTRATOR` have it.
- **Pictures are made by the instance**, as avatars are (architecture 12): still ones as PNG, **animated ones as animated GIF** (Ryan, 2026-11-20: animated from the start), kept as blobs the instance seals and served by address to anyone.
- **Removing an emoji deletes its picture at once and leaves its name** (Ryan, 2026-11-20): messages that used it show `:name:` as text, and reactions with it stay, counted and drawn as `:name:`, but no one can add another.
- **How many a server may have is the instance's to say, per server** (Ryan, 2026-11-20: "so I can lock the number behind a premium plan"): a default for every server from the operator's environment, and a number for one server set by an operator's command. It is the first per-server allowance of architecture 16.

## 2. An emoji

```text
emoji_id     UUIDv7, minted by the instance
server_id    the server it belongs to
name         2 to 32 of a-z, 0-9 and _, the instance lowercasing what it is sent
image_url    a path under /api/v1/avatars/, as a server's icon_url is
animated     whether the picture is an animated GIF
width        the picture's pixels, at most 128 each way
height
created_by   the account that uploaded it
created_at
```

A name is one live emoji's alone in its server. A removed emoji's name is free again at once (section 7). Nothing else about a name is reserved: `:smile:` may be a server's picture, and Unicode's emoji have no `:names:` in Lumia's composer yet (section 9).

## 3. The picture

**Made by the instance** (architecture 12, "Avatars", whose reasoning holds whole: nothing of the upload is stored or served, and nobody who looks at the picture decodes bytes its uploader chose). The upload is the request's whole body, at most 8 MB, as an avatar's is (`Lumia.Media`). In `Lumia.Media`'s NIF, in this order:

1. **The format** is told from the first bytes: PNG (APNG included), JPEG, WebP or GIF, never from what the upload calls itself.
2. **Still or animated.** An upload is animated when it is a GIF, an APNG or a WebP of more than one frame. A JPEG, or any of the others with one frame, is still.
3. **Bounds before decoding.** A still picture is held to the avatar's bounds (8,192 a side, 25 million pixels). An animated one's canvas is held to 1,024 a side, and its frames are counted while they are decoded: past 200 frames, or past 64 million pixels decoded across frames, it is refused (`too_large`). Each frame decodes under the allocation limit avatars use.
4. **Colour** is converted to sRGB from an embedded profile, and a still picture is turned upright, as an avatar is.
5. **Fitted, never cropped.** The picture is scaled to fit within 128 by 128 pixels, keeping its shape, and never enlarged. Every frame of an animated one is scaled the same.
6. **Encoded here.** A still picture as PNG, alpha kept. An animated one as GIF, looping for ever, each frame's palette quantized from its own pixels, transparency kept where a pixel is less than half opaque, and each frame's delay kept, a delay under 20 ms made 100 ms (as browsers draw them). An animated result over 512 KiB is refused (`too_large`).

**Kept** as an avatar is: a blob the instance seals under a random content key wrapped by the instance master key, addressed by the keyed BLAKE3 of its ciphertext, of the kind `emoji`. **Served** by `GET /avatars/{address}`, which takes the kind `emoji` beside `avatar` and `icon`: public, without a session, cacheable for good, since the bytes under an address never change. Public is right for the same reason as a server's icon: an emoji is a picture its uploader put up to be seen by everyone in a server, and the address is 256 bits nobody computes without the master key. An instance without a blob store or a master key keeps no emoji and says so (`FAILED_PRECONDITION`, `details.state` `blobs`).

## 4. Managing them

| Method and path | Body → Response |
|---|---|
| `GET /servers/{server_id}/emoji` | → `ListEmojiResponse`: the server's live emoji, oldest first, and its limit. Any member, guests included (a guest reads messages that use them) |
| `POST /servers/{server_id}/emoji?name={name}` | a picture's bytes → `CustomEmoji` (201). `MANAGE_EMOJI` |
| `PATCH /servers/{server_id}/emoji/{emoji_id}` | `UpdateEmojiRequest` → `CustomEmoji`: a new name. `MANAGE_EMOJI` |
| `DELETE /servers/{server_id}/emoji/{emoji_id}` | → 204: removed (section 7). `MANAGE_EMOJI` |

Refusals: `PERMISSION_DENIED` without `MANAGE_EMOJI`; `INVALID_ARGUMENT` with `details.field` `name` for a name outside section 2's rule; `ALREADY_EXISTS` with `details.existing_id` for a name another live emoji of the server has; `INVALID_ARGUMENT` with `details.field` `picture` and `details.reason` (`too_large`, `unsupported`, `invalid`) as for an avatar; `RESOURCE_EXHAUSTED` with `details.state` `emoji` and `details.limit` when the server already has as many live emoji as its limit; `NOT_FOUND` for an emoji that is not one of the server's live ones.

Each change writes an audit row in its transaction (`emoji.create`, `emoji.rename`, `emoji.delete`, with `before` and `after`), and is told to every member as section 8 says. An upload is checked against the limit under the server's row lock, so two uploads at once cannot both take the last place.

## 5. The limit

- **The default** is `LUMIA_SERVER_EMOJI_MAX`, 50 unless set, for every server without a number of its own.
- **A server's own number** is set by the operator on the instance: `bin/lumia-admin emoji-limit <server_id> <n>`, and `bin/lumia-admin emoji-limit <server_id> default` to go back to the default. It is kept on the server (`servers.emoji_limit`, empty for the default), written to the audit log as `server.emoji_limit` with no actor and `override` set, and told to every member as `ServerUpdated`. Nothing a server's own admins can reach changes it. With plans (architecture 16) the plan's allowance takes this column's place, and the command stays as the operator's override.
- **`Server.emoji_limit`** carries the limit that applies, so a client says "12 of 50" without asking, and offers no upload at the limit.
- **Lowering a limit deletes nothing.** A server above its limit keeps every emoji it has; uploads are refused until it is below it again. No limit deletes what a server's own people chose to keep (architecture 16, as for files in 6.7).

The limit is on live emoji. A removed emoji counts for nothing.

## 6. In messages

**The text keeps `:name:`, and a list beside it says which emoji**, as mentions are kept (protocol 14): `EmojiRef { emoji_id, start, end }`, byte offsets into the body, half open, on `SendMessage`, `EditMessage` and `Message`. So a rename never rewrites a message, a removed emoji leaves its name readable, a client that knows no custom emoji shows what was typed, and a copy of the text is the text.

- **The core finds them.** At a send or an edit, every `:name:` in the body that names a live emoji of the channel's server, outside inline code and code blocks, becomes a ref. A shell sends the words; it never builds refs. A `:name:` that names nothing stays plain text.
- **The instance checks each ref**: it lies within the body on UTF-8 boundaries, refs do not overlap, and the text it covers is `:`, a name of section 2's shape and `:` (`INVALID_ARGUMENT`, `details.field` `emoji`, when any of that fails). A ref whose emoji is not a live emoji of the channel's server is **dropped**, its text kept as typed, not refused, so a send queued before a removal still goes and an edit of an old message is not held up by what was removed since. The covered name need not be the emoji's present one: a send made just before a rename is not refused for it. At most 200 refs a message; past that the rest are dropped.
- **In a personal conversation** there are no refs, and the core finds none: `:name:` is text there.
- **Drawing.** A ref whose emoji the client holds is drawn as its picture, at the height of an emoji in that text, its width by its shape, with `:name:` (the present name) as its tooltip and what assistive technology reads. A ref to an emoji the client does not hold (removed, or not yet known) is its text. An animated emoji plays, except where the system asks for reduced motion, where its first frame is drawn. A quote of a message is its text (`QuoteReply.text` has no refs), so a quoted emoji is its name.
- **The body's grammar is the core's** (`lumia_core::body`), which takes the refs and gives each shell an emoji span in place of the text, so no shell matches `:name:` itself.

## 7. Removed and renamed

**Removed.** The emoji's picture blob is deleted in the request, as a replaced avatar's is, and its row stays with `deleted_at` set, its name and id kept, so that reactions with it can still be named. It leaves `GET .../emoji`, `EmojiUpdated` and the snapshot at once. What follows from that:

- Messages keep their refs; every client draws them as their text, since no client holds the emoji.
- Reactions with it stay, counted as before. `ReactionCount.reaction` names it as `:name:`, by the name it had when it was removed; a client that does not hold the emoji draws that. Adding one is refused (`INVALID_ARGUMENT`, `details.field` `emoji_id`); taking one's own away works.
- A client drops a picture it cached for an emoji the server no longer lists.
- The picture stops being served at the removal, whether or not the store has let the blob go yet: a blob the store refuses to delete is retried later, never served again (`GET /avatars/{address}` serves an emoji's picture only while a live emoji names it).

No event is written into any channel for a removal: what a client holds of a message does not change, only what it can draw.

**Renamed.** The row's name changes, and `EmojiUpdated` tells the members. Messages are not rewritten: their text keeps the name they were written with, and they are drawn by id as the emoji's picture with its present name as the tooltip.

## 8. How clients learn of them

As a server's roles (authorization 6.4):

- `AccountSnapshot.emoji` carries the live emoji of every server the account is in, and `MembershipAdded.emoji` those of a server joined later.
- After an emoji is uploaded, renamed or removed, `Lumia.Workers.EmojiUpdated` appends `EmojiUpdated`, the server's live emoji as they are when it runs, all of them, to every member's account log, 500 members to a transaction. It is scheduled one second after the change, is one job for a burst of changes, runs one at a time with the server's other telling jobs, and reads under the server's row as they do. A job run twice tells twice, which costs nothing: the event says how things are. It also carries, in `removed`, the emoji removed in the last day, each with the name it had when it was removed: a rename and a removal made in one burst are one event, which would otherwise never tell the name the reactions left behind are to show (section 7). A client names the kept reactions of every emoji in either list by that name.
- A change of the server's limit is `ServerUpdated`, since the limit is on `Server`.

Each change is a log row for every member, as a role's is. Emoji change rarely; a server that uploads a batch is told once.

## 9. Reactions

A reaction is one Unicode emoji (`reaction`) or one of the server's emoji (`emoji_id`), never both: `AddReaction` and `RemoveReaction` carry one or the other (`INVALID_ARGUMENT`, `details.field` `reaction`, for both or neither). For a custom one the instance fills `reaction` in what it writes and answers with `:name:`, so `ReactionAdded`, `ReactionRemoved` and `ReactionCount` carry the emoji's id and its name, and a client that knows nothing of custom emoji shows a chip of the name.

- Adding needs `ADD_REACTIONS` as before, and an emoji that is a live emoji of the channel's server; the channel being a personal conversation is refused as any reaction command there already is.
- A custom reaction is one reaction per account, message and emoji, as a Unicode one is; the instance keeps it by the emoji's id, so a rename does not split the reactions with it in two.
- A client groups reactions by `emoji_id` where it is set and by `reaction` where it is not, and draws a held emoji's picture in the chip, `:name:` where it holds none.

## 10. On the clients

- **The picker** (architecture 6.6) shows, in a server's conversation, a section of the server's emoji headed by the server's name, after "Frequently used" and ahead of Unicode's groups; a search finds them by name. "Frequently used" holds a server's emoji only in that server. Choosing one in the composer inserts `:name:`; choosing one for a reaction reacts with it. The quick row of six stays Unicode's. In a personal conversation the section is not there.
- **The composer.** Typing `:` and two or more letters of a name lists the server's emoji that match, by the start of the name and then anywhere in it, each with its picture; choosing one completes `:name:` with a space after it. In a personal conversation nothing is listed.
- **Server settings** gain an **Emoji** section, shown to whoever holds `MANAGE_EMOJI`: the count against the limit ("12 of 50"), **Upload emoji** (a picture chosen from the system's picker, its name taken from the file's name, lowercased, with what the rule does not allow made `_` and cut to 32, editable before it is sent), and the list, each with its picture, `:name:`, who uploaded it, **Rename** in place, and **Remove**, which asks first: "Remove :name:? Messages that used it will show its name instead." At the limit, Upload is disabled and says the server has as many as it may.
- **What the core holds.** The live emoji of every server, from the snapshot and the account log, and the rules: whether this account may manage them, a name made from a file name, the matches for a typed prefix, the refs of a body. The shells fetch pictures by `image_url` through the image cache they already use for avatars.

## 11. Rejected

- *Any server's emoji anywhere* (Discord's paid choice), on one instance or across instances, and *emoji in personal conversations*. Section 1 has why; either can be added later without a change to what a message keeps, since a ref names an emoji by id.
- *Only `:name:` in the text, resolved when drawn.* A rename would change or break every message that used the name, and a removed name taken by a new emoji would draw the new picture in old messages.
- *Discord's `<:name:id>` in the text.* Ids in the body would show to any client that does not parse them, and copy as noise; Lumia already keeps mentions beside the text, not in it.
- *Removing a removed emoji's reactions.* A `ReactionRemoved` in every channel that used it, for a decision the members made by reacting.
- *Keeping a removed emoji's picture for the messages that used it.* Removing would not remove.
- *Still pictures only at first.* Proposed, and overruled by Ryan: animated emoji are much of what a server's emoji are for.
- *Animated WebP or APNG as what is kept.* No Rust-only encoder of animated WebP exists (libwebp is C, which architecture 3 would put in a Port worker), and Android's system decoder does not play APNG. GIF plays on every platform Lumia draws on, and at 128 pixels its palette costs little.
- *The client making the picture*, as it makes attachments. Every member would decode bytes the uploader chose, and the instance can make one small picture once.
- *The limit set by the server's admins*, or a column with no operator's command. It is an allowance, which a plan will grant (architecture 16), not a governance setting.

## 12. Not built, and later

Emoji per instance, offered in every server of it (architecture 6.6 keeps it as later; `server_id` stays a column so an instance's own can be told apart); `:names:` for Unicode's emoji in the composer; a server's emoji used outside it; an instance admin's HTTP endpoint for the limit, which comes with plans or an admin page; stickers; emoji in a server's or a channel's name; uploading several at once.

## 13. Tests

- The NIF: a still PNG with alpha kept as PNG within 128; a JPEG kept as PNG; a large picture fitted, a small one not enlarged; an animated GIF, an APNG and an animated WebP kept as animated GIF with their frame count and delays; one frame of an animated format kept still; 201 frames, a canvas of 1,025, and an animated result over 512 KiB refused; a declared size past the bounds refused before decoding.
- The instance: upload, rename and remove, each with and without `MANAGE_EMOJI`, by the owner, by `ADMINISTRATOR`; names refused and taken; the limit, the default and a server's own, the count of live emoji only, two uploads racing for the last place, a lowered limit keeping what is there; `emoji-limit` and its audit row and `ServerUpdated`; the picture served by address without a session and gone after removal; refs on send and edit checked, dropped when their emoji is removed or another server's, refused when malformed; a custom reaction added and removed, refused for a removed emoji and for one of another server, kept across a rename, its count named `:name:` after removal; `EmojiUpdated` to every member once for a burst, guests included; the snapshot and `MembershipAdded`.
- The core: refs found outside code and not inside it, at the right UTF-8 offsets; no refs in a personal conversation; the body's spans for refs held and not held; matches for a prefix; a name from a file name; reactions grouped by id; `EmojiUpdated` replacing what is held.
- Each shell: the picker's section and search, the composer's list, a message drawn with a held emoji, a removed one and an animated one under reduced motion, a custom reaction chip, and the settings section against a scratch instance.
