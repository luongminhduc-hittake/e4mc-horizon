# Changelog

## 0.1.2 — 2026-11-09

- **`/e4mc stop` now keeps the address closed.** Before, the next server tick saw a
  LAN still open and dialled the relay again, so the address came back a moment
  later with a new domain and the command did nothing. It stays closed until
  `/e4mc start`, or until the LAN is closed and opened again.
- `/e4mc status` tells the two states apart ("closed" vs "turned off in
  config/e4mc.json").

Verified on Linux, Horizon `nightly-2026-11-07`: `/e4mc start` publishes and opens
the LAN in offline mode when `force_offline` is on; `/e4mc stop` closes it and it
stays closed across ticks with the LAN still open; Open to LAN on a world with
`force_offline` off answers an `Encryption Request`.

## 0.1.1 — 2026-11-09

Fixes from the registry review of 0.1.0: offline hosting was on by default, and it
applied to worlds this mod had nothing to do with.

- **`force_offline` now defaults to `false`.** A LAN world authenticates its guests
  as it always did; turning offline hosting on is a choice, made for the worlds you
  actually share.
- **It only applies when e4mc is on.** `server_about_to_start` checks `enabled`, so a
  world opened with the mod switched off is an ordinary authenticated world. Before,
  the switch's position was ignored and the account was hidden anyway.
- **The settings page says when a switch lands.** Horizon fixes a LAN's online mode
  while the world loads and never re-reads it, so the offline switch applies to the
  next world, not the one already open. The README said the opposite and has been
  corrected.
- The settings label reads *No Mojang login while hosting (for friends without an
  account)*, which says what it does rather than naming the mechanism.

No change to the relay, the command, or the config format.

Verified on Linux, Horizon `nightly-2026-11-07`:

- With `force_offline` off, a hosted LAN answers a real login with an `Encryption
  Request` (0x01) and authenticates the guest: the account is not hidden.
- With it on, the same handshake gets `Login Finished` (0x02) and never an
  `Encryption Request`.
- With `enabled` off, opening a world leaves `auth::current()` answering the account,
  whether or not `force_offline` is on.
- The settings page toggles the three switches and writes `config/e4mc.json`.

Not verified: Windows and macOS, more than two players, and sessions longer than a
few minutes.

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
