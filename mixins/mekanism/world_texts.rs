// The text Mekanism draws in the world (hook point `native_world_texts`, API 44): a bin's count.
fn on_world_texts(game: &crate::game::Game<'_>, texts: &mut Vec<crate::game::native_hooks::NativeWorldText>) {
    crate::game::mekanism::world_texts(game, texts);
}
