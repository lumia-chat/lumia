Lumia for Linux, a preview
==========================

Lumia desktop app for Linux, preview. Please report issues here:
https://github.com/lumia-chat/lumia/issues

Run it
------

    ./lumia

It needs a Vulkan driver (it does not fall back to OpenGL), and libvulkan,
libxkbcommon, and libwayland-client (on Wayland) or libX11 (on X11), which
most desktops already have. Text in Chinese, Japanese and Korean is drawn with
the system's fonts; without Noto CJK or another like it, it shows as boxes.

Install it by hand
------------------

    install -Dm755 lumia ~/.local/bin/lumia
    install -Dm644 chat.lumia.desktop ~/.local/share/applications/chat.lumia.desktop
    install -Dm644 chat.lumia.png ~/.local/share/icons/hicolor/256x256/apps/chat.lumia.png
    update-desktop-database ~/.local/share/applications

The launcher entry gives Lumia the lumia: link type, which signing in through
the browser needs.

Updates
-------

This folder does not update itself: a new version is a new download. To have
Lumia kept up to date, install it from https://packages.lumia.chat (Debian,
Ubuntu, Fedora and other dnf systems), or use the AppImage from the release,
which updates itself.

Your data is in $XDG_DATA_HOME/chat.lumia (~/.local/share/chat.lumia).
