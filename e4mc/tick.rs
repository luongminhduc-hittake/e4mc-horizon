// The end of a server tick: keep the session's LAN port current, and follow the
// world being shared or closed.
//
// Hosting starts earlier than this: `server_about_to_start` sets the offline
// flag before `integrated_open` reads the LAN's online mode. By the first tick
// the world is already hosting, so this only records that and follows the LAN.
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
