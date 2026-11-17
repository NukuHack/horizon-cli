fn playground_event(
    game: &mut crate::game::Game<'_>,
    mod_id: &str,
    event: &crate::game::native_hooks::NativePanelEvent,
) {
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;
    if mod_id == m::ID {
        if !game.native_on(m::ID) {
            return;
        }
        let Some(event) = n::convert_event(game, event) else {
            return;
        };
        let creative = game.native_playing() && game.creative;
        let Some(outs) = n::state(|s| s.panel.as_mut().map(|p| p.handle(event, creative))) else {
            return;
        };
        for out in outs {
            n::apply(game, out);
        }
    }
}
