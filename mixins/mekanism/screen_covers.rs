// What Mekanism's tabs and windows cover on a machine's screen (hook point `native_screen_covers`,
// API 32): the slots under them are left alone.
fn on_screen_covers(block: &str, at: [f32; 2], covered: &mut bool) {
    crate::game::mekanism::screen_covers(block, at, covered);
}
