// Mekanism's game logic, ported from the Java sources in this repository (src/main, src/api).
//
// This crate has no Horizon dependency: horizon-mixins stages it as a workspace member of the game
// (`[[crate]]` in horizon-mixin.toml) and the bundle's helpers translate between the game's types
// and these. Everything here is plain data and functions, so `cargo test` runs it without a game.

pub mod bin;
pub mod boiler;
pub mod chemical;
pub mod dynamic_tank;
pub mod energy;
pub mod evaporation;
pub mod factory;
pub mod fission;
pub mod fusion;
pub mod gear;
pub mod generator;
pub mod heat;
pub mod isotopes;
pub mod laser;
pub mod machine;
pub mod matrix;
pub mod miner;
pub mod modules;
pub mod nuclear;
pub mod qio;
pub mod recipe_table;
pub mod recipes;
pub mod seismic;
pub mod sides;
pub mod slurry_table;
pub mod sps;
pub mod teleporter;
pub mod tier;
pub mod turbine;
pub mod upgrades;

/// The four modules of Mekanism and their namespaces, as the Java build registers them.
pub const NAMESPACES: [&str; 4] = [
    "mekanism",
    "mekanismgenerators",
    "mekanismadditions",
    "mekanismtools",
];

/// The Mekanism version this port follows (gradle.properties `mod_version`).
pub const MEKANISM_VERSION: &str = "10.8.0";

/// The line the game logs once a world opens with the mod built in.
pub fn loaded_message(world: &str) -> String {
    format!("[mekanism] Mekanism {MEKANISM_VERSION} for Horizon is active in world {world}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_loaded_message_names_the_world() {
        assert_eq!(
            super::loaded_message("New World"),
            "[mekanism] Mekanism 10.8.0 for Horizon is active in world New World"
        );
    }
}
