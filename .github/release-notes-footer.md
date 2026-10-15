## Installing unsigned builds

These builds are **ad-hoc signed, not notarized** (no Apple Developer ID),
so Gatekeeper will block the first open. This is expected — the app is built
in the open from this repo's source. To open it anyway:

1. Right-click (Control-click) the app → **Open**, then click **Open**, or
2. Try to open it, then go to **System Settings → Privacy & Security** and
   click **Open Anyway**.

macOS remembers the exception, so this is a one-time step per install.
Apple's own walkthrough:
[Safely open apps on your Mac](https://support.apple.com/en-us/102445)
