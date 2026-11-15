// The items Mekanism draws in the world (hook point `native_world_items`, API 44): what a bin holds.
fn on_world_items(game: &crate::game::Game<'_>, items: &mut Vec<crate::game::native_hooks::NativeWorldItem>) {
    crate::game::mekanism::world_items(game, items);
}
