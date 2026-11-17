//! Jetpacks and the MekaSuit's Jetpack Unit (`CommonPlayerTickHandler`, `IJetpackItem.handleJetpackMotion`).
//! The window's player thrusts itself and tells the server, which spends the hydrogen.
use mekanism_core::{module_config, modules};
use mod_api::{ClientHost, EquipmentSlot, ItemStack, PlayerRef, ServerHost};

use super::data;
use crate::world::net;

pub(super) const CHANNEL: &str = "mekanism:jetpack";
const JETPACKS: [&str; 2] = ["mekanism:jetpack", "mekanism:jetpack_armored"];
const THRUST_MULTIPLIERS: [f32; 5] = [0.5, 1.0, 2.0, 3.0, 4.0];
const UNIT: &str = data::JETPACK_UNIT;

/// The item worn on the chest, when it is a jetpack: its registry name and stack.
fn worn(host: &dyn mod_api::Host, living: mod_api::LivingRef) -> Option<(String, ItemStack)> {
    let stack = living.equipment(host, EquipmentSlot::Chest)?;
    let id = stack.item().id(host);
    data::is_jetpack(&stack, &id).then_some((id, stack))
}

/// The jetpack's mode (0 normal, 1 hover, 2 vector, 3 disabled) and thrust multiplier.
fn settings(id: &str, stack: &ItemStack) -> (u32, f32) {
    if JETPACKS.contains(&id) {
        return (custom_mode(stack).min(3) as u32, 1.0);
    }
    if !data::module_enabled(stack, UNIT) {
        return (3, 1.0);
    }
    let installed = data::installed_modules(stack).get(UNIT).copied().unwrap_or(1);
    let setting = |key: &str| module_config::settings(UNIT, installed).into_iter().find(|s| s.key == key).map(|s| data::module_setting(stack, UNIT, &s, installed));
    let mode = setting("jetpack_mode").unwrap_or(0).min(3);
    let mult = setting(if mode == 1 { "jetpack_mult.hover" } else { "jetpack_mult" }).unwrap_or(1) as usize;
    (mode, THRUST_MULTIPLIERS[mult.min(4)])
}

fn custom_mode(stack: &ItemStack) -> u64 {
    data::custom_data(stack).map_or(0, |c| data::get_long(c, data::JETPACK_MODE_KEY))
}

/// `Entity.calculateViewVector`.
fn view_vector(pitch: f64, yaw: f64) -> [f64; 3] {
    let (p, y) = (pitch.to_radians(), yaw.to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}

/// The window's tick: holding jump with a fuelled jetpack on, outside creative flight, thrusts in its mode and stops the
/// fall, and tells the server, which spends the hydrogen.
pub(super) fn client_tick(host: &mut dyn ClientHost) {
    if !host.playing() {
        return;
    }
    let Some(player) = host.local_player() else {
        return;
    };
    let entity = player.entity();
    let Some(living) = host.living(entity) else {
        return;
    };
    let Some((id, chest)) = worn(&*host, living) else {
        return;
    };
    let fuel = data::fuel(&chest, &id);
    if fuel == 0 {
        return;
    }
    let (mut mode, mult) = settings(&id, &chest);
    let down = |host: &dyn ClientHost, key: &str| host.mapping(key).ok().flatten().is_some_and(|m| m.down);
    let ascending = down(&*host, "key.jump");
    let descending = down(&*host, "key.sneak");
    let (Some(rotation), Some(motion)) = (entity.rotation(&*host), entity.velocity(&*host)) else {
        return;
    };
    let (yaw, pitch) = (f64::from(rotation[1]), f64::from(rotation[0]));
    // Creative flight and the ground are not the jetpack's.
    let flying = player.abilities(&*host).is_some_and(|a| a.flying);
    let grounded = entity.on_ground(&*host).unwrap_or(false) || living.is_sleeping(&*host).unwrap_or(false) || flying;
    let active = match mode {
        3 => false,
        1 => (ascending && !descending) || !grounded,
        _ => ascending,
    };
    if !active {
        return;
    }
    // `ItemMekaSuitArmor.useJetpackFuel`: `ceil(multiplier)` mB for that much thrust (the item spends one).
    let amount = if JETPACKS.contains(&id.as_str()) { 1 } else { (mult.ceil() as u64).max(1) }.min(fuel);
    let thrust = 0.15 * amount as f64;
    // `IJetpackItem.handleJetpackMotion`.
    if mode == 2 && descending {
        mode = 0;
    }
    let coefficient = |y: f64| (-y).exp().min(1.0);
    let gliding = living.is_gliding(&*host).unwrap_or(false);
    let mut delta = [0.0f64; 3];
    let mut reset = true;
    if (mode == 0 || mode == 2) && gliding {
        let forward = view_vector(pitch, yaw);
        for k in 0..3 {
            let drag = (forward[k] * 1.5 - motion[k]) * 0.5;
            delta[k] = forward[k] * thrust + drag;
        }
        reset = false;
    } else if mode == 0 {
        delta = [0.08 * motion[0], thrust * coefficient(motion[1]), 0.08 * motion[2]];
    } else if mode == 2 {
        let up = view_vector(pitch + 90.0, yaw);
        delta = [up[0] * thrust, up[1] * thrust * coefficient(motion[1]), up[2] * thrust];
    } else {
        // Hover: the vertical speed is held, so the push is the difference to the speed wanted.
        let y = motion[1];
        let wanted = if ascending == descending {
            if y > 0.0 {
                (y - thrust).max(0.0)
            } else if y < 0.0 && !grounded {
                (y + thrust).min(0.0)
            } else {
                y
            }
        } else if ascending {
            (y + thrust).min(2.0 * thrust)
        } else if !grounded {
            (y - thrust).max(-2.0 * thrust)
        } else {
            y
        };
        delta[1] = wanted - y;
    }
    host.push_player(delta.map(|v| v as f32));
    net::to_server(host, CHANNEL, &[amount as u8, u8::from(reset)]);
}

/// A client's thrust: the server spends the hydrogen in the jetpack that player wears.
pub(super) fn server_receive(host: &mut dyn ServerHost, player: PlayerRef, data_bytes: &[u8]) {
    let Some(living) = host.living(player.entity()) else {
        return;
    };
    let Some((id, mut chest)) = worn(&*host, living) else {
        return;
    };
    let fuel = data::fuel(&chest, &id);
    if fuel == 0 {
        return;
    }
    let amount = u64::from(data_bytes.first().copied().unwrap_or(1).max(1)).min(fuel);
    data::set_fuel(&mut chest, &id, fuel - amount);
    let _ = living.set_equipment(host, EquipmentSlot::Chest, Some(chest));
    let _ = modules::SUIT_JETPACK_CAPACITY;
}
