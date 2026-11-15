// Options > Mods lists Mekanism with its keys (hook point `native_mods`, API 21): the player rebinds them there.
fn on_mods(game: &mut crate::game::Game<'_>, mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    crate::game::mekanism::list_mod(game, mods);
}
