// A mob died (hook point `native_mob_died`, API 46): a Robit drops as its item.
fn on_mob_died(game: &mut crate::game::Game<'_>, kind: &str, at: [f64; 3], data: Option<&horizon_world::nbt::Compound>) {
    crate::game::mekanism::mob_died(game, kind, at, data);
}
