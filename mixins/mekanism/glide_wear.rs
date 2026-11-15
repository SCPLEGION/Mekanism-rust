// The wear of gliding (hook point `native_glide_wear`, API 54): the Elytra Unit pays for it in energy.
fn on_glide_wear(game: &mut crate::game::Game<'_>, wear: u32, claimed: &mut bool) {
    crate::game::mekanism::glide_wear(game, wear, claimed);
}
