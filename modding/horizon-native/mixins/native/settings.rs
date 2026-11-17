fn playground_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;
    if mod_id == m::ID {
        buttons.extend(n::settings(game).labels());
    }
}
