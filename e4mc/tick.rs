// The end of a server tick: keep the session's LAN port current, and follow the
// world being shared or closed.
//
// The offline switch is decided earlier than this: `server_about_to_start` sets
// it before `integrated_open` reads the LAN's online mode. Nothing here can
// change it for a world already open.
fn e4mc_tick(
    game: &mut crate::game::Game<'_>,
    phase: crate::game::native_lifecycle::ServerTickPhase,
) {
    if matches!(
        phase,
        crate::game::native_lifecycle::ServerTickPhase::ServerTickEnd { .. }
    ) {
        crate::game::e4mc::tick(game);
    }
}
