fn playground_zoom(
    game: &crate::game::Game<'_>,
    zoom: &mut Option<crate::game::native_hooks::NativeZoom>,
) {
    /// A no-op zoom hook: another mod can set `zoom`, this one leaves it alone.
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;

    if !game.native_on(m::ID) || zoom.is_some() {
        return;
    }
    // To provide a zoom, build NativeZoom { factor, smooth, turn }.
}
