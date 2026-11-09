// `game::e4mc`: the relay session's life inside the game.
//
// One rule drives everything: **the session exists exactly while the world is
// actually published to the LAN**, and it is told the LAN port every tick. The
// relay gives a domain as soon as it is asked, so a session started before the
// LAN opens would hand out an address nothing answers.
//
// Two things ask for a session and both are honoured: the LAN opening on its own
// (Open to LAN, or `enabled` on), and `/e4mc start`. `/e4mc stop` closes it for
// as long as the LAN stays open; closing and reopening the LAN publishes again.

use crate::game::Game;
use crate::game::lan::LanSettings;
use crate::text::Text;
use e4mc_bridge::broker::Broker;
use e4mc_bridge::session::{Session, SessionState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the mod reads once and keeps.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    /// Open the relay whenever the world is published to the LAN.
    pub enabled: bool,
    /// Let friends without an account join: the LAN runs without Mojang
    /// authentication while this world is open, even when this computer plays
    /// on one. **Off by default** and only ever while `enabled` is on: an
    /// ordinary LAN host keeps authenticating its guests as it always did.
    /// Decided when the world opens, not while it runs.
    pub force_offline: bool,
    /// Ask e4mc's broker for the nearest relay instead of the fixed one.
    pub use_broker: bool,
    pub broker_url: String,
    pub relay_host: String,
    pub relay_port: u16,
    /// Say the domain in chat when it arrives.
    pub announce_in_chat: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            force_offline: false,
            use_broker: true,
            broker_url: "https://broker.e4mc.link/getBestRelay".into(),
            relay_host: "test.e4mc.link".into(),
            relay_port: 25575,
            announce_in_chat: true,
        }
    }
}

impl Config {
    fn broker(&self) -> Broker {
        Broker {
            use_broker: self.use_broker,
            url: self.broker_url.clone(),
            fallback_host: self.relay_host.clone(),
            fallback_port: self.relay_port,
        }
    }
}

const FILE: &str = "e4mc.json";

/// The version the game and the release both report.
pub const VERSION: &str = "0.1.2";

/// The config, read once and then kept here: the settings page changes it at
/// run time, so it cannot be a `OnceLock` of `&'static`.
static CONFIG: Mutex<Option<Config>> = Mutex::new(None);

/// A copy of the config. Cheap: a handful of fields.
pub fn config() -> Config {
    let mut slot = CONFIG.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(config) = slot.as_ref() {
        return config.clone();
    }
    let loaded = load();
    *slot = Some(loaded.clone());
    loaded
}

/// Reads the file, writing the defaults out on the first run.
fn load() -> Config {
    let Ok(dir) = Game::native_config_dir() else {
        return Config::default();
    };
    let path = dir.join(FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        // First run: write the defaults out, so there is something to edit.
        if let Ok(json) = serde_json::to_string_pretty(&Config::default()) {
            let _ = std::fs::write(&path, json);
        }
        return Config::default();
    };
    match serde_json::from_str(&text) {
        Ok(config) => config,
        Err(e) => {
            log::warn!("e4mc: {} doesn't read ({e}); using the defaults", path.display());
            Config::default()
        }
    }
}

/// Keeps `config` and writes it out. A write that fails leaves the running
/// game on the new value: the file is a record, not the authority.
pub fn save(config: &Config) {
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = Some(config.clone());
    let Ok(dir) = Game::native_config_dir() else {
        return;
    };
    match serde_json::to_string_pretty(config) {
        Ok(json) => {
            if let Err(e) = std::fs::write(dir.join(FILE), json) {
                log::warn!("e4mc: could not write config/e4mc.json: {e}");
            }
        }
        Err(e) => log::warn!("e4mc: could not encode the config: {e}"),
    }
}

/// "on"/"off", for a settings button's label.
pub fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

/// Whether this mod answers to `mod_id` (its settings page and its row).
pub fn mine(mod_id: &str) -> bool {
    mod_id == "e4mc"
}

/// Whether this mod should act at all: the relay runs, and it may suppress
/// authentication for the world about to open.
pub fn wanted() -> bool {
    config().enabled
}

/// Starts hosting: the LAN's online mode is decided while the world opens, long
/// before the first tick, so the flag has to be set here, from the
/// `server_about_to_start` hook that runs just before it.
///
/// Setting it later than this has no effect on the LAN: `OnlineMode::lan()` has
/// already read `auth::current()`, and the `NetConfig` it built is what every
/// login goes through. So the switch in the settings page decides the **next**
/// world, not this one.
pub fn begin_hosting() {
    let force = config().force_offline;
    if force {
        log::info!("e4mc: the LAN runs in offline mode (config/e4mc.json, force_offline)");
    }
    crate::auth::auth_suppress::set_suppress(force);
}

/// Stops hosting: the account is visible again, so joining an online-mode
/// server from the title screen works as usual.
pub fn end_hosting() {
    crate::auth::auth_suppress::set_suppress(false);
}

/// The one session, if any: the game thread starts and stops it.
static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// The LAN port the session bridges to, updated every tick.
static PORT: Mutex<Option<u16>> = Mutex::new(None);
/// The last domain told to chat, so it is said once.
static ANNOUNCED: Mutex<Option<String>> = Mutex::new(None);
/// Set by `/e4mc stop`: the player closed the address while the LAN stays open,
/// so the tick must not dial the relay again until they ask (`/e4mc start`) or
/// the LAN is closed and opened once more.
static CLOSED: AtomicBool = AtomicBool::new(false);

/// The port supplier handed to the session: it reads the shared cell, so a LAN
/// port that appears after the session started is picked up.
fn port_source() -> Arc<dyn Fn() -> Option<u16> + Send + Sync> {
    Arc::new(|| *PORT.lock().unwrap_or_else(|e| e.into_inner()))
}

fn set_port(port: Option<u16>) {
    *PORT.lock().unwrap_or_else(|e| e.into_inner()) = port;
}

fn running() -> bool {
    SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|session| session.domain().is_some())
}

/// Starts the relay for the world, opening the LAN first when it isn't open.
/// `Err` when the LAN can't open or the relay refuses.
pub fn start(game: &mut Game<'_>) -> Result<String, String> {
    let config = config();
    CLOSED.store(false, Ordering::Relaxed);
    if game.lan_port().is_none() {
        open_lan(game)?;
    }
    let port = game.lan_port().ok_or_else(|| "e4mc: the world isn't shared".to_string())?;
    set_port(Some(port));

    {
        let mut slot = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = slot.as_ref()
            && let Some(domain) = existing.domain()
        {
            return Ok(domain);
        }
        if let Some(old) = slot.take() {
            old.stop();
        }
        slot.replace(Session::start(config.broker(), port_source()));
    }

    // `/e4mc start` is an explicit request, so it may wait a bounded time.
    let session = SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|session| session.wait_for_domain(Duration::from_secs(20)));
    match session {
        Some(Ok(domain)) => {
            if config.announce_in_chat {
                announce(game, &domain);
            }
            Ok(domain)
        }
        Some(Err(e)) => {
            stop();
            Err(e)
        }
        None => Err("e4mc: no session".into()),
    }
}

/// Opens the world to the LAN the way the game's own screen does.
fn open_lan(game: &mut Game<'_>) -> Result<(), String> {
    let settings = LanSettings {
        port: crate::lan::available_port(),
        guest_commands: false,
        force_game_mode: false,
    };
    game.lan_open(settings)
        .map_err(|e| format!("e4mc: could not open the LAN: {e}"))
}

/// Ends the session; the relay forgets the domain once the connection closes.
pub fn stop() {
    if let Some(session) = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take() {
        session.stop();
    }
    set_port(None);
    *ANNOUNCED.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// `/e4mc stop`: closes the address and keeps it closed, so the tick does not
/// dial the relay again for the LAN that is still open.
pub fn stop_and_hold() {
    CLOSED.store(true, Ordering::Relaxed);
    stop();
}

pub fn status() -> (Config, SessionState) {
    let state = SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(Session::state)
        .unwrap_or(SessionState::Stopped);
    (config(), state)
}

/// The domain of the running session, if it has one.
pub fn domain() -> Option<String> {
    SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(Session::domain)
}

/// Called at the end of every server tick: follows the LAN, and starts or stops
/// the session to match. Never waits for the network.
pub fn tick(game: &mut Game<'_>) {
    let shared = game.lan_port();
    set_port(shared);
    let running = running();
    let config = config();

    // The player closed the address (`/e4mc stop`) but the LAN is still open:
    // leave it closed. Closing the LAN clears this, so opening it again publishes.
    if shared.is_none() {
        CLOSED.store(false, Ordering::Relaxed);
    }
    // The world stopped being shared (or the mod was switched off): close.
    if running && (shared.is_none() || !config.enabled) {
        stop();
        return;
    }
    // The world is shared and the relay isn't up: dial it, without waiting.
    if !running && shared.is_some() && config.enabled && !CLOSED.load(Ordering::Relaxed) {
        let mut slot = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            slot.replace(Session::start(config.broker(), port_source()));
        }
        return;
    }
    // A domain that arrived between two ticks gets its line now.
    let domain = domain().filter(|_| config.announce_in_chat);
    if let Some(domain) = domain {
        announce(game, &domain);
    }
}

/// The domain in chat, clickable to copy, once per domain.
fn announce(game: &mut Game<'_>, domain: &str) {
    let mut told = ANNOUNCED.lock().unwrap_or_else(|e| e.into_inner());
    if told.as_deref() == Some(domain) {
        return;
    }
    let line = Text::literal("e4mc: ")
        .append(Text::copyable(domain))
        .append(Text::literal("  (click to copy)"));
    game.native_broadcast(line);
    *told = Some(domain.to_string());
}

/// The usages `/e4mc` answers.
pub fn commands() -> Vec<crate::game::native_server::NativeCommand> {
    vec![
        crate::game::native_server::NativeCommand::new("e4mc"),
        crate::game::native_server::NativeCommand::new("e4mc status"),
        crate::game::native_server::NativeCommand::new("e4mc start"),
        crate::game::native_server::NativeCommand::new("e4mc stop"),
        crate::game::native_server::NativeCommand::new("e4mc domain"),
    ]
}

/// Runs one `/e4mc ...`, answering the player who typed it.
pub fn command(game: &mut Game<'_>, player: u128, words: &[&str]) -> bool {
    let (config, state) = status();
    let message = match words.get(1).copied() {
        None | Some("status") => match &state {
            SessionState::Started { domain } => format!(
                "e4mc: open — {domain} (LAN port {})",
                game.lan_port().map_or_else(|| "?".into(), |p| p.to_string())
            ),
            SessionState::Starting => "e4mc: asking the relay for an address…".to_string(),
            SessionState::Stopped => {
                if !config.enabled {
                    "e4mc: turned off in config/e4mc.json".to_string()
                } else if CLOSED.load(Ordering::Relaxed) {
                    "e4mc: closed. /e4mc start gets an address again.".to_string()
                } else {
                    "e4mc: closed. /e4mc start opens the LAN and gets an address.".to_string()
                }
            }
        },
        Some("start") => match start(game) {
            Ok(domain) => format!("e4mc: open — {domain}"),
            Err(e) => e,
        },
        Some("stop") => {
            stop_and_hold();
            "e4mc: address closed (the LAN stays open).".to_string()
        }
        Some("domain") => match domain() {
            Some(domain) => domain,
            None => "e4mc: no address yet.".to_string(),
        },
        Some(other) => format!("e4mc: unknown subcommand {other:?}"),
    };
    game.native_tell(player, Text::literal(message));
    true
}
