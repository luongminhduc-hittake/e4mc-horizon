# Changelog

## 0.1.0 — 2026-11-08

First release.

- `/e4mc start` opens the LAN itself and asks the e4mc relay for a public domain;
  guests join from a plain Minecraft 26.3 client through the relay.
- Works three ways: `/e4mc start`, opening the LAN by hand, or both.
- **Offline mode while hosting**, so friends without a Microsoft account can join
  even when the host plays on one. Only hosting is affected.
- **Options > Mods row and settings page**: switches for the relay, offline mode and
  the chat announcement, applied and saved as you click.
- `/e4mc`, `/e4mc status`, `/e4mc stop`, `/e4mc domain`.
- Config at `config/e4mc.json`; the broker picks the nearest relay, with a fallback
  and support for a self-hosted `e4mc-quiclime`.
- Builds on Horizon `nightly-2026-11-07` and later without a re-release: it names the
  hook-points version it needs instead of pinning a commit.

Verified on Linux, Horizon `nightly-2026-11-07`: two machines playing through the
relay with the host on a Microsoft account and the guest on an offline client; the
offline override confirmed with a real login handshake (`Login Finished`, never an
`Encryption Request`); the settings page read and toggled through the game's own UI.

Not verified: Windows and macOS, more than two players, and sessions longer than a
few minutes.
