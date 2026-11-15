// The quads Mekanism draws in the world (hook point `native_world_quads`, API 45): the SPS's bolts and billboards.
fn on_world_quads(game: &crate::game::Game<'_>, quads: &mut Vec<crate::game::native_hooks::NativeWorldQuad>) {
    crate::game::mekanism::world_quads(game, quads);
}
