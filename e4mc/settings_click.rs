// A click on e4mc's settings page: flip the switch and write the file.
//
// Nothing is applied to a world that is already open, on purpose. The LAN's
// online mode is decided once, while the world loads (`OnlineMode::lan()` reads
// `auth::current()`), and every later login goes through that `NetConfig`. A
// switch flipped mid-session therefore changes the file and the next world, not
// this one — which is also what the running LAN is actually doing.
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
}
