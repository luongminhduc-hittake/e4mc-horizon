// The LAN's online mode. Horizon sends every guest to Mojang's session service
// whenever the host plays on a Microsoft account, which a friend without an
// account can't answer. When the player has asked for it (`force_offline`, off
// by default) the account is hidden here while e4mc is on, so
// `OnlineMode::lan()` answers `None` and the LAN runs in offline mode: guests
// join with the name their game uses, as in a vanilla world hosted by an
// offline player.
//
// Only while a world is hosted, and only when e4mc is on: joining an online-mode
// server from the title screen still uses the account. The host's own player
// comes in over the memory connection, which `conn.rs` keeps out of online mode
// anyway.
fn e4mc_offline(value: Option<Arc<AuthSession>>) -> Option<Arc<AuthSession>> {
    let suppressed = crate::auth::auth_suppress::suppressed();
    log::debug!("e4mc: auth::current suppressed={suppressed}");
    if suppressed { None } else { value }
}
