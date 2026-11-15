// A mod's item that is a bow (hook point `native_bow_item`, API 66): the Electric Bow.
fn on_bow_item(game: &crate::game::Game<'_>, item: &str, bow: &mut bool) {
    crate::game::mekanism::bow_item(game, item, bow);
}
