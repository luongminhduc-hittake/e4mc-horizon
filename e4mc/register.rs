// `/e4mc` in every player's command tree.
fn e4mc_register_commands(commands: &mut Vec<crate::game::native_server::NativeCommand>) {
    commands.extend(crate::game::e4mc::commands());
}
