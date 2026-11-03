// A block became another, whatever did it (hook point `native_block_removed`, API 43): a multi-block
// machine that is gone takes its bounding blocks with it (`AttributeHasBounding.removeBoundingBlocks`).
fn on_block_removed(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    old: &str,
    old_properties: &std::collections::BTreeMap<String, String>,
    new: &str,
) {
    let _ = new;
    crate::game::mekanism::clear_bounding_blocks(game, pos, old, old_properties);
}
