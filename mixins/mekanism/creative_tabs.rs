// Mekanism's creative tabs (hook point `native_creative_tabs`, API 34): one per module, in the Java order.
fn on_creative_tabs(tabs: &mut Vec<crate::game::native_hooks::NativeCreativeTab>) {
    crate::game::mekanism::creative_tabs(tabs);
}
