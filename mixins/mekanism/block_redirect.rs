// A click or break aimed at one block acts on another (hook point `native_block_redirect`, API 43): a
// bounding block, the cell a multi-block machine fills beyond its own, answers for the machine that owns it.
fn on_block_redirect(game: &crate::game::Game<'_>, pos: [i32; 3], to: &mut Option<[i32; 3]>) {
    if to.is_none() {
        *to = crate::game::mekanism::bounding_owner(game, pos);
    }
}
