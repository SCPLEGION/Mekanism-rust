// Whether a Mekanism screen's text box has focus (hook point `native_screen_typing`, API 33): the
// frequency selector's name box.
fn on_screen_typing(block: &str, typing: &mut bool) {
    crate::game::mekanism::screen_typing(block, typing);
}
