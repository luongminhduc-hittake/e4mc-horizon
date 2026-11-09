// e4mc's settings page: the switches that matter.
//
// Labels are rebuilt each time the page opens, so they always show the values
// in the file rather than what the player last clicked.
fn e4mc_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id != "e4mc" {
        return;
    }
    let _ = game;
    let config = crate::game::e4mc::config();
    buttons.push(format!(
        "Open to the relay when the LAN opens: {}",
        crate::game::e4mc::on_off(config.enabled)
    ));
    buttons.push(format!(
        "No Mojang login while hosting (for friends without an account): {}",
        crate::game::e4mc::on_off(config.force_offline)
    ));
    buttons.push(format!(
        "Announce the address in chat: {}",
        crate::game::e4mc::on_off(config.announce_in_chat)
    ));
}
