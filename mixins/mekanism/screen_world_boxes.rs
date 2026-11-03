// The boxes Mekanism draws in the world (hook point `native_world_boxes`, API 41): the Digital Miner's visuals.
fn on_world_boxes(game: &crate::game::Game<'_>, boxes: &mut Vec<crate::game::native_hooks::NativeWorldBox>) {
    crate::game::mekanism::world_boxes(game, boxes);
}
