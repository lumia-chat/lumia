# Icons

The 52 stroke icons used across Lumia, 40 lifted from the product design (`back` from the iOS boards, `member-remove` from the dark Quiet Split's member list, and the seven of the rail's menus from the Server folders board: `folder`, `folder-plus`, `folder-remove`, `arrow-up`, `arrow-down`, `trash`, `chevron-right`) and seven drawn after it: `reply`, the arrow of a quote reply; four for the device list and the encryption notice (`globe`, `monitor`, `phone`, `shield`), `members` for the button that shows a narrow window its side panel, and `settings`, the gear of a server's settings (the boards draw none); and `play`, over a video's poster and a video's or a recording's file row (specs/media.md 5; drawn for the desktop on 2026-11-15, no board has one, and the web's `icons.ts` does not carry it yet); and four for the desktop's own player of an attached video or recording (#21, drawn 2026-11-16 after `play`, no board has a player): `pause`, `volume`, `volume-off` and `full-screen`: 24 × 24 grid, round caps and joins, stroke 1.8–2.2 (lock 2.6; `more` is three filled dots). These files are drawn in #4C525E (`ink-quiet`, light theme) so they show as pictures; in code use the `Icon` component, which draws the same paths in `currentColor`.

- Navigation: `hash`, `lock`, `chevron-down`, `back`, `search`, `compose`, `plus`, `thread`, `mention`, `saved`, `members`
- Conversation: `voice`, `pin`, `more`, `reaction-add`, `emoji`, `send`, `file`, `download`, `play`, `member-joined`, `member-remove`
- The player: `pause`, `volume`, `volume-off`, `full-screen` (with `play`)
- Reactions: `thumbs-up`, `check-circle`, `heart`, `sparkle`
- Sign-in: `eye`, `eye-off`, `key`, `mail`
- Menus and cards: `user`, `preferences`, `settings`, `sign-out`, `check`, `close` (the account menu, the menu under a server's name and the profile card)
- The rail: `folder` (an open folder's head), `folder-plus`, `folder-remove`, `arrow-up`, `arrow-down`, `trash`, `chevron-right` (a server's and a folder's menus; the chevron marks "Move to folder" as opening another)

`sign-out` is not as the boards draw it. Theirs is an arrow going into a door, the picture every icon set calls log-in; this one is the same door mirrored with the arrow leaving (2026-11-12). The boards on the design canvas still carry the old one.
- Devices and encryption: `globe`, `monitor`, `phone`, `shield`
