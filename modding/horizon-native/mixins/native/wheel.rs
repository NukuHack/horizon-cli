fn playground_wheel(
    game: &crate::game::Game<'_>,
    steps: f32,
    claimed: &mut bool,
) {
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;

/// `native_wheel` asks whether the mod takes the wheel. Setting `claimed`
/// stops the hotbar from also scrolling.
    if !game.native_on(m::ID) {
        return;
    }
    let mut s = n::settings(game);
    let next = (s.scale as f32 - steps).rem_euclid(4.0) as usize;
    if next != s.scale {
        s.scale = next;
        for (k, v) in s.stored() {
            game.native_set_setting(m::ID, k, &v);
        }
        n::state(|st| st.settings = Some(s));
        *claimed = true;
    }
}
