// Ore and salt generation from the world's config (hook point `native_worldgen_entries`, API 66).
fn on_worldgen_entries(world_root: &std::path::Path, entries: &mut Vec<(String, String, String)>) {
    crate::game::mekanism::worldgen_entries(world_root, entries);
}
