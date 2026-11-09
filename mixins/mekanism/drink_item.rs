// A mod's item that is drunk (hook point `native_drink_item`, API 72): the Canteen's drinking pose and use duration.
fn on_drink_item(game: &crate::game::Game<'_>, item: &str, ticks: &mut i32) {
    crate::game::mekanism::drink_item(game, item, ticks);
}
