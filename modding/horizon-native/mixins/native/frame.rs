fn playground_frame(game: &mut crate::game::Game<'_>) {
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;
    /// `native_frame` is called every frame. `native_hunger` and `native_player`
    /// return `Option`, so `?` is not available here; use `if let`.
    // native_player() has x/y/z (not health); game.player has the full data.
    let Some(np) = game.native_player() else {
        return;
    };
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let pos = [np.x, np.y, np.z];
    let health = p.health.health();
    let food = p.health.food.food;
    let fps = game.native_frame_rate();
    let target = game
        .eye_position()
        .and_then(|eye| {
            let dir = crate::game::view_vector(game.yaw, game.pitch);
            game.clip_ray(eye, dir, 5.0)
        })
        .map(|hit| {
            let facts = game
                .res
                .facts()
                .get(game.world.block(hit.pos[0], hit.pos[1], hit.pos[2]));
            facts.name.to_string()
        });
    n::state(|s| {
        s.snapshot = Some(n::Snapshot {
            pos,
            health,
            food,
            fps,
            target,
        })
    });
}
