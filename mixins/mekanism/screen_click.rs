// A press on a Mekanism screen (hook point `native_screen_click`, API 20): the reactors' buttons.
fn on_screen_click(block: &str, at: [f32; 2], button: &mut Option<i32>) {
    crate::game::mekanism::screen_click(block, at, button);
}
