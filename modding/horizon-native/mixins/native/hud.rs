fn playground_hud(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    corners: &mut horizon_client::extras_hud::Corners,
) {
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;
    n::hud(game, batch, corners);
}
