// A projectile flew this tick on a server (hook point `native_projectile_moved`, API 83): it pops a balloon it met.
fn on_projectile_moved(game: &mut crate::game::Game<'_>, from: [f64; 3], to: [f64; 3], absorbed: &mut bool) {
    crate::game::mekanism::projectile_moved(game, from, to, absorbed);
}
