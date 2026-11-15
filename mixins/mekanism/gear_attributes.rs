// Attribute modifiers the worn gear gives (hook point `native_gear_attributes`, API 54): the MekaSuit's Hydrostatic
// Repulsor Unit (water movement efficiency) and Gyroscopic Stabilization Unit (submerged mining speed).
fn on_gear_attributes(game: &mut crate::game::Game<'_>, attributes: &mut Vec<(String, String, f64, String)>) {
    crate::game::mekanism::gear_attributes(game, attributes);
}
