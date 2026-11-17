fn playground_tick(game: &mut crate::game::Game<'_>) {
/// On a dedicated server or LAN host, `native_tick` runs every game tick.
/// Chatting every 100 ticks shows it fires on the server thread.
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;

    if game.native_window() || !game.native_on(m::ID) {
        return;
    }
    let n = n::state(|s| {
        s.ticks = s.ticks.wrapping_add(1);
        s.ticks
    });
    if n % 100 == 0 {
        game.native_chat(&format!("Playground tick {n}"));
    }
}
