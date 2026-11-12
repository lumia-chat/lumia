Lumia for Windows, a preview
============================

This is the Lumia desktop app built for Windows. It is a preview: it was
built on another system and nobody has yet run it on Windows. It may not
start. Please say what you find:
https://github.com/lumia-chat/lumia/issues

Run it
------

Unpack the folder anywhere and open Lumia.exe. It is not signed, so Windows
SmartScreen will warn before it runs the first time ("More info", then "Run
anyway").

What it does not do yet
-----------------------

- There is no installer, and it does not update itself. A new version is a
  new download.
- The lumia: link type is not registered, so signing in through the browser
  (a passkey, or your organization's sign-in) cannot return to the app. Sign
  in with a password, or link this computer from another device.
- It posts no system notifications.
- Closing the window quits it; there is no tray icon.
- It does not remember the window's size and place.

Your data is in %APPDATA%\chat.lumia.
