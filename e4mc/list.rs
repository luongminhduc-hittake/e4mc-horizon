// e4mc's row in Options > Mods, where a player can find it and turn it off.
//
// The hook is dispatched once and every mod adds its own row, so this pushes
// e4mc's without asking whose turn it is.
fn e4mc_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    mods.push(crate::game::native_hooks::NativeMod {
        id: "e4mc".into(),
        title: format!("e4mc {}", crate::game::e4mc::VERSION),
        description: "Share this world over the e4mc relay: friends join without port forwarding."
            .into(),
        keys: Vec::new(),
    });
}
