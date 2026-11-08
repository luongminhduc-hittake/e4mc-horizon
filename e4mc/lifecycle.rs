// The world closing: the relay closes its connection, which retires the domain,
// and the account becomes visible again.
fn e4mc_lifecycle(phase: crate::game::native_lifecycle::WorldPhase) {
    use crate::game::native_lifecycle::WorldPhase;
    if matches!(
        phase,
        WorldPhase::WorldCloseFinal | WorldPhase::WorldCloseFailed | WorldPhase::WorldOpenFailed
    ) {
        crate::game::e4mc::stop();
        crate::game::e4mc::end_hosting();
    }
}
