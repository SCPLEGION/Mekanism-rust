// A Mekanism machine's screen layout (hook point `native_screen`, API 19): its slots where Mekanism puts
// them.
fn on_screen(block: &str, screen: &mut Option<crate::game::native_hooks::NativeScreen>) {
    crate::game::mekanism::screen(block, screen);
}
