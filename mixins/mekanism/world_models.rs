// The models Mekanism draws in the world (hook point `native_world_models`, API 43): wind generator rotors.
fn on_world_models(game: &crate::game::Game<'_>, models: &mut Vec<crate::game::native_hooks::NativeWorldModel>) {
    crate::game::mekanism::world_models(game, models);
}
