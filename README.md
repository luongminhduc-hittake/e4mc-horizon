# e4mc for Horizon

> **An unofficial port.** This is not e4mc, and it is not made or endorsed by the e4mc
> project or its authors. It is a from-scratch Rust mod for
> [Horizon](https://github.com/horizon-rust/horizon) that speaks the e4mc relay's
> protocol, written so Horizon players can share a LAN world the way e4mc lets
> Minecraft players share one. Bug reports belong here, not with e4mc.

Open your Horizon LAN world to friends anywhere, **without port forwarding**. The
mod asks the e4mc relay for a public domain, you send that domain to your friends,
and they join from a plain Minecraft client — including friends on **offline
accounts** with no Microsoft login.

e4mc itself is by [vgskye and contributors](https://github.com/vgskye/e4mc-minecraft-architectury)
(MIT); the relay protocol follows [e4mc-quiclime](https://github.com/vgskye/e4mc-quiclime)
(MIT OR Apache-2.0). See [NOTICE](NOTICE) for the full credit.

## Install

In the Horizon launcher: **Mods → Add a native mod from GitHub**, and paste:

```
https://github.com/luongminhduc-hittake/e4mc-horizon
```

The launcher reads the release's `.hnmod`, builds it into the game, and asks before
running native code (mods run with your account's rights — read the source first,
it is all here).

Requires **Horizon `nightly-2026-11-07` or newer**: the relay client uses `quinn`
for QUIC, and that is the first official build whose `Cargo.lock` carries it. The
mod declares the hook-points version it needs rather than pinning a commit, so it
keeps building on later Nightlies and releases without a new download.

## Use

1. Open your world.
2. Type `/e4mc start`. The mod opens the LAN itself and asks the relay for an address.
3. Chat shows `e4mc: open — word-word.eu.e4mc.link` — click to copy.
4. Send it to your friends; they paste it into **Multiplayer → Direct Connection**.
5. `/e4mc stop` closes the address. Closing the world closes it too.

Opening the LAN by hand (Esc → Open to LAN) works the same: the mod notices and
publishes an address for it.

### In game: Options > Mods

e4mc has its own row in **Options > Mods** like any other mod, with a **Settings**
button next to it. Its page has three switches, applied and saved as you click:

- **Open to the relay when the LAN opens** (`enabled`)
- **Offline mode while hosting (no Mojang login)** (`force_offline`)
- **Announce the address in chat** (`announce_in_chat`)

The offline switch takes effect at once, even mid-session: turn it on while hosting
and the next guest joins without a Mojang login. Turning the whole row **off** stops
the mod starting a session; the config file below is the same setting, for edits
outside the game.

| Command | What it does |
| --- | --- |
| `/e4mc` / `/e4mc status` | whether it is open, the address, the LAN port |
| `/e4mc start` | open the LAN if needed, then get an address |
| `/e4mc stop` | close the address (the LAN stays open) |
| `/e4mc domain` | print the address |

## Config

`<game home>/config/e4mc.json`, written with defaults on first run:

```json
{
  "enabled": true,
  "force_offline": true,
  "use_broker": true,
  "broker_url": "https://broker.e4mc.link/getBestRelay",
  "relay_host": "test.e4mc.link",
  "relay_port": 25575,
  "announce_in_chat": true
}
```

**`force_offline`** is the one that matters. Horizon authenticates every LAN guest
with Mojang whenever the host plays on a Microsoft account, and a friend without an
account cannot answer that. With this on, the game plays as if no account were
signed in **while a world is hosted**, so the LAN runs in offline mode and anyone
can join. Joining an online-mode server from the title screen still uses your
account; only hosting is affected. It is **on by default** because it is the whole
point for the people this mod is for — set it to `false` if everyone you play with
owns the game and you want Mojang to authenticate them.

`use_broker` asks e4mc for the nearest relay; turn it off to use `relay_host` (for a
self-hosted [e4mc-quiclime](https://github.com/vgskye/e4mc-quiclime)).

## How it works

```
friend's Minecraft ──TCP──▶ e4mc relay ──QUIC (ALPN quiclime)──▶ this mod ──TCP──▶ LAN port
```

The relay hands out a domain and opens one QUIC stream per joining player; the mod
bridges each stream to `127.0.0.1:<LAN port>`. No game protocol is touched, so
vanilla 26.3 clients work and nothing is needed on the guest's side.

## Layout

```
e4mc/
  horizon-mixin.toml   the bundle: injections, modules, the bridge crate
  horizon-native.json  the release manifest (mod id, version, hook points)
  glue.rs              game::e4mc — config, start/stop, chat, the /e4mc command
  offline.rs           RETURN hook on auth::current: the LAN runs offline while hosting
  auth_flag.rs         auth::auth_suppress — the flag that hook reads
  hosted.rs            server_about_to_start: sets that flag before the LAN's mode is decided
  list.rs              the Options > Mods row
  settings.rs          the settings page's buttons
  settings_click.rs    a click on them: save and apply
  tick.rs              every tick: follow the LAN port, open/close to match
  lifecycle.rs         the world closing: end the session, show the account again
  register.rs          the /e4mc command tree
  command.rs           run /e4mc
  bridge/              plain Rust, no game types: broker, QUIC, framing, TCP bridge
    tests/live_relay.rs  end-to-end against the real e4mc relay
```

`bridge/` is testable on its own:

```sh
cd e4mc/bridge
cargo test                                             # 16 offline tests
cargo test --test live_relay -- --ignored --nocapture   # really talks to e4mc's relay
```

## What is verified, and what is not

Verified on Linux:

- 16 unit tests of the `bridge` crate (varint framing, broker, relay choice, a session
  that stops cleanly).
- The live test, against e4mc's real relay, including a 75-second idle before joining:
  the domain is issued and a Minecraft handshake travels through it into a local TCP
  server.
- **Two machines playing** through the relay with the host on a **Microsoft account**
  and the guest on an offline client: the guest logged in and joined.
- The offline override, with a real login handshake against a hosted LAN: the server
  answers compression then `Login Finished` (0x02), never an `Encryption Request`
  (0x01).

Not verified yet:

- More than two players (Horizon's LAN takes at most 8; that is the game's limit).
- Windows and macOS builds (only Linux was tested).
- Long sessions: the relay has a 30-connections-per-minute-per-IP limit and a
  keep-alive; hours of play are untested.

## Limits worth knowing

- Horizon's LAN takes at most **8 players**; that is the game's limit, not the mod's.
- The relay sees the game's traffic in the clear: it terminates the QUIC TLS. That is
  how a free relay works. Point `relay_host` at your own e4mc-quiclime if that
  matters.
- The relay is a free service run by one person, with no SLA. If e4mc changes its
  protocol or disappears, this mod breaks until it is updated.
- Guests need **Minecraft 26.3**, the same version as the host.
- A native mod runs with full access to the computer. That is Horizon's design, not a
  choice this mod makes.

## Licence

MIT — see [LICENSE](LICENSE). The MIT licence text is kept from e4mc/e4all; e4mc's
authorship of the original design is credited in [NOTICE](NOTICE).
