// A click on e4mc's settings page: flip the switch, write the file, and let the
// change take effect without a restart.
//
// `force_offline` is re-applied at once: the flag `auth::current()` reads is
// live, so turning it on while hosting a world makes the next guest offline
// without closing anything.
fn e4mc_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    if mod_id != "e4mc" {
        return;
    }
    let _ = game;
    let mut config = crate::game::e4mc::config().clone();
    match button {
        0 => config.enabled = !config.enabled,
        1 => config.force_offline = !config.force_offline,
        2 => config.announce_in_chat = !config.announce_in_chat,
        _ => return,
    }
    crate::game::e4mc::save(&config);
    // A world being hosted right now follows the switch at once.
    crate::game::e4mc::apply_offline(config.force_offline);
}
