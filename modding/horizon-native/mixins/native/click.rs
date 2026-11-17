fn playground_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;
    if mod_id == m::ID {
        let mut s = n::settings(game);
        if !s.click(button) {
            return;
        }
        for (key, value) in s.stored() {
            game.native_set_setting(m::ID, key, &value);
        }
        n::state(|st| st.settings = Some(s));
    }
}
