// A player's `/e4mc ...`.
fn e4mc_perform_command(
    game: &mut crate::game::Game<'_>,
    player: &crate::game::native_server::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    if *ran {
        return;
    }
    let words: Vec<&str> = command.split_whitespace().collect();
    if words.first() != Some(&"e4mc") {
        return;
    }
    *ran = crate::game::e4mc::command(game, player.uuid, &words);
}
