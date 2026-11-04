// A right click on a mob (hook point `native_use_mob`, API 46): a Robit opens its screen.
fn on_use_mob(game: &mut crate::game::Game<'_>, id: u32, claimed: &mut bool) {
    crate::game::mekanism::use_mob(game, id, claimed);
}
