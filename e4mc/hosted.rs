// Before the server loads its worlds (`ServerAboutToStartEvent`), and before
// `integrated_open` decides the LAN's online mode: this is the one moment the
// offline override can be set in time for the world being opened.
//
// Only when this mod is on. A world opened with e4mc turned off is an ordinary
// world, and it keeps authenticating its guests.
fn e4mc_server_about_to_start(dedicated: bool) {
    if !dedicated && crate::game::e4mc::wanted() {
        crate::game::e4mc::begin_hosting();
    }
}
