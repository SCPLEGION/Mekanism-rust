// What each slot of a Mekanism machine's menu takes (hook point `native_slot_rules`, API 38): the energy
// slot only energy items, the output slots nothing.
fn on_slot_rules(block: &str, rules: &mut Vec<horizon_client::lan::menu::SlotRule>) {
    crate::game::mekanism::slot_rules(block, rules);
}
