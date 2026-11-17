fn playground_resource_pack(
    game: &mut crate::game::Game<'_>,
    p: &horizon_protocol::packets::configuration::ResourcePackPush,
) -> core::ops::ControlFlow<()> {
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;
    
    if !game.native_on(m::ID) {
        return core::ops::ControlFlow::Continue(());
    }
    if !n::settings(game).skip_packs {
        return core::ops::ControlFlow::Continue(());
    }
    // `Session::send_pack_status` handles both the configuration and play
    // phases, and grabs the same writer lock the game thread sends through.
    if let Some(remote) = &game.remote {
        remote.session.send_pack_status(p.id, 0); // 0 = SUCCESSFULLY_LOADED
    }
    /// Called at the HEAD of `Game::handle_pack_push`. If the mod is on and
    /// `skip_packs` is set, send `SUCCESSFULLY_LOADED` (action 0) for the pack
    /// and tell the mixin to skip the body — no prompt, no download, no SHA-1
    /// check. If the body is skipped, `ServerPacks::push` never runs, so no
    /// pack is ever staged into `downloads/` or reloaded.
    core::ops::ControlFlow::Break(())
}
