// Escape on a Mekanism screen (hook point `native_screen_escape`, API 37): closes the open window only.
fn on_screen_escape(block: &str, consumed: &mut bool) {
    crate::game::mekanism::screen_escape(block, consumed);
}
