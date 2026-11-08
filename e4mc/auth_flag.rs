// The switch `auth::current` reads: while a world is being hosted with
// `force_offline` on, the game plays as if no account were signed in, so the
// LAN runs in offline mode and friends without an account can join.
//
// It lives here, in the render crate, because that is where `auth` is; the
// game crate sets it from `game::e4mc`.

use std::sync::atomic::{AtomicBool, Ordering};

static SUPPRESS: AtomicBool = AtomicBool::new(false);

/// Called by `game::e4mc` when a world opens or closes.
pub fn set_suppress(suppress: bool) {
    SUPPRESS.store(suppress, Ordering::Relaxed);
}

/// Whether the account is hidden from the LAN right now.
pub fn suppressed() -> bool {
    SUPPRESS.load(Ordering::Relaxed)
}
