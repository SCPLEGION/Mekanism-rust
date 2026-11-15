// The biomes are loaded (hook point `native_biome_spawns`, API 84): Mekanism Additions' babies spawn where their parents do.
fn on_biome_spawns(biomes: &mut horizon_client::biome::Biomes) {
    crate::game::mekanism::biome_spawns(biomes);
}
