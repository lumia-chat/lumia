Lumia for Linux, a preview
==========================

This is the Lumia desktop app built for Linux. It is a preview: it was built
and tested without a screen, and nobody has yet used it on a Linux desktop.
Expect rough edges, and please say what you find:
https://github.com/lumia-chat/lumia/issues

Run it
------

    ./lumia

It needs a Vulkan driver (it does not fall back to OpenGL), and these
libraries, which most desktops already have: libvulkan, libxkbcommon, and
libwayland-client (on Wayland) or libX11 (on X11). Text in Chinese, Japanese and Korean and emoji are drawn with the
system's fonts; without Noto CJK and Noto Color Emoji (or others like them)
they show as empty boxes.

Install it
----------

The .deb beside this archive installs the same program with a launcher entry
and the lumia: link type, which signing in through the browser needs:

    sudo apt install ./Lumia-*-linux-*.deb

From this archive, by hand:

    install -Dm755 lumia ~/.local/bin/lumia
    install -Dm644 chat.lumia.desktop ~/.local/share/applications/chat.lumia.desktop
    install -Dm644 chat.lumia.png ~/.local/share/icons/hicolor/256x256/apps/chat.lumia.png
    update-desktop-database ~/.local/share/applications

What it does not do yet
-----------------------

- It does not update itself. A new version is a new download.
- It posts no system notifications.
- Closing the window quits it; there is no tray icon.
- It does not remember the window's size and place.

Your data is in $XDG_DATA_HOME/chat.lumia (~/.local/share/chat.lumia).
