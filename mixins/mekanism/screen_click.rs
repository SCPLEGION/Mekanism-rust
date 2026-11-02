// A press on a Mekanism screen (hook point `native_screen_click`, API 20): the reactors' buttons and
// the redstone control tab.
fn on_screen_click(block: &str, data: &[i32], at: [f32; 2], right: bool, button: &mut Option<i32>) {
    crate::game::mekanism::screen_click(block, data, at, right, button);
}
