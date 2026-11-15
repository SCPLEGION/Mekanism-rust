// What the slots of the machine at a position take (hook point `native_slot_rules_at`, API 77): the
// Formulaic Assemblicator's, which follow its own formula, stock control and auto mode.
fn on_slot_rules_at(pos: [i32; 3], block: &str, rules: &mut Vec<horizon_client::lan::menu::SlotRule>) {
    crate::game::mekanism::slot_rules_at(pos, block, rules);
}
