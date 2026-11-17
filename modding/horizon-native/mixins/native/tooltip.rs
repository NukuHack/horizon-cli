fn playground_tooltip(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    stack: &horizon_client::items::ItemStack,
    x: f32,
    y: f32,
) -> core::ops::ControlFlow<()> {
/// `draw_stack_tooltip` is a cancellable HEAD. Returning `Continue` lets
/// Horizon draw its own tooltip; returning `Break(())` suppresses it.
/// Here we only append a line and let Horizon finish.
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;
    
    if !game.native_on(m::ID) || !stack.item.ends_with("shulker_box") {
        return core::ops::ControlFlow::Continue(());
    }
    // `native_tooltip` (API 12) is the stable way to edit lines; this HEAD
    // hook is shown because it also lets you cancel the whole tooltip.
    core::ops::ControlFlow::Continue(())
}
