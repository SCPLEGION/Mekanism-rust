// A mob the mod draws itself (hook point `native_mob_hidden`, API 48): a Robit.
fn on_mob_hidden(game: &crate::game::Game<'_>, id: u32, kind: &str, hidden: &mut bool) {
    let _ = kind;
    crate::game::mekanism::mob_hidden(game, id, hidden);
}
