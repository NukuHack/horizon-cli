fn playground_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;

    if !pressed || !game.native_on(m::ID) {
        return;
    }
    let panel_key = game.native_mod_key(m::ID, m::KEYS[0][0], m::KEYS[0][2]);
    let overlay_key = game.native_mod_key(m::ID, m::KEYS[1][0], m::KEYS[1][2]);
    if key == panel_key {
        n::toggle_panel(game);
    } else if key == overlay_key {
        let mut s = n::settings(game);
        s.overlay = !s.overlay;
        for (k, v) in s.stored() {
            game.native_set_setting(m::ID, k, &v);
        }
        n::state(|st| st.settings = Some(s));
    }
}
