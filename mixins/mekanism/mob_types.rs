// The mob types load (hook point `native_mob_types`, API 46): the Robit.
fn on_mob_types(types: &mut std::collections::HashMap<String, horizon_client::mobs::MobType>) {
    crate::game::mekanism::mob_types(types);
}
