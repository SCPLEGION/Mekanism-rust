//! The `gear` area of the mod (see lib.rs). Hook bodies live here; lib.rs only delegates.
//! Meka-Tool, MekaSuit, modules, jetpacks, weapons, tooltips, keys, HUD and commands.
//!
//! On the fork every gear item went through global hooks that asked the item's name; here each gear item gets a
//! [`GearPart`] in `register`, and the game asks it only for its own item (docs/horizon-api-migration.md).
use mekanism_core::{gear as core_gear, modules};
use mod_api::{AttackEntityEvent, LevelRandomHost, LivingDamagePreEvent, Flow, Hand, Host, HostSide, ItemStack, PlayerBreakSpeedEvent, ClientHost, ItemBarEvent, ItemDecorationsEvent, ItemPart, Registrar, TooltipEvent, InteractionResult, Text, UseItemEvent};
use mod_api::HostBlockStates;
use text::{color, line, translate_colored};

pub(crate) mod data;
mod jetpack;
mod mekasuit;
pub(crate) mod text;

/// State of this area, held in `Mod`. Items carry their own state in `custom_data`, so there is none yet.
#[derive(Default)]
pub(crate) struct State {
    /// Extra damage the next hit on an entity takes (set when the attack starts, taken when its damage is worked out).
    pending: std::sync::Mutex<std::collections::HashMap<i32, f32>>,
    /// Server ticks since the last second's work (the MekaSuit's units run once a second).
    second: std::sync::atomic::AtomicU32,
}

const SCUBA_TANK: &str = "mekanism:scuba_tank";
const CANTEEN: &str = "mekanism:canteen";
const FLAMETHROWER: &str = mekanism_core::flamethrower::ITEM;
const JETPACKS: [&str; 2] = ["mekanism:jetpack", "mekanism:jetpack_armored"];

/// Items whose bar and tooltip are a chemical tank, not energy.
const TANK_ITEMS: [&str; 5] = [JETPACKS[0], JETPACKS[1], SCUBA_TANK, CANTEEN, FLAMETHROWER];

/// Every item of the gear area gets a [`GearPart`].
pub(crate) fn register(r: &mut dyn Registrar) {
    crate::world::net::channel(r, jetpack::CHANNEL, Some(jetpack::server_receive), None);
    let mut ids = core_gear::energy_item_ids();
    ids.extend(TANK_ITEMS.map(str::to_owned));
    ids.sort();
    ids.dedup();
    for id in ids {
        let key = r.item_key(&id);
        // The part knows its item: the game asks it for that item only.
        r.item_part(key, GearPart { id: Box::leak(id.into_boxed_str()) });
    }
}

/// What every piece of Mekanism gear shares: bars, tooltips, and (in later stages) use, tick and wear.
struct GearPart {
    id: &'static str,
}

fn width(amount: u64, capacity: u64) -> i32 {
    ((13.0 * amount as f64 / capacity.max(1) as f64).round() as i32).clamp(0, 13)
}

/// Bar colour of a chemical (`Chemical.getTint`); grey for one the gear never holds.
pub(crate) fn chemical_color(id: &str) -> u32 {
    match id {
        "mekanism:hydrogen" => 0xFFFFFF,
        "mekanism:oxygen" => 0x6CE2FF,
        "mekanism:nutritional_paste" => 0xEB6CA3,
        _ => 0xAAAAAA,
    }
}

#[mod_api::horizon_part]
impl ItemPart for GearPart {
    /// `ItemEnergyBarDecorator` (0x3CFE9A) for an item holding energy; a tank item (jetpack, scuba tank,
    /// flamethrower, canteen) shows its tank in the chemical's colour instead.
    fn item_bar(&self, _: &dyn ClientHost, event: &mut ItemBarEvent<'_>) {
        let stack = event.stack;
        if stack.count() != 1 {
            return;
        }
        if TANK_ITEMS.contains(&self.id) {
            let fuel = data::fuel(stack, self.id);
            if fuel > 0 {
                *event.bar = Some((width(fuel, data::tank_capacity(stack, self.id)), chemical_color(data::item_chemical(self.id))));
            }
        } else if let Some(e) = data::item_energy_of(stack, self.id) {
            let stored = data::stored_energy(stack);
            if stored > 0 {
                *event.bar = Some((width(stored, e.capacity), 0x3CFE9A));
            }
        }
    }

    /// The second bar of a MekaSuit piece (`MekaSuitBarDecorator`): the tank of the piece, under the energy bar:
    /// the body armor's hydrogen with a Jetpack Unit, the helmet's nutritional paste with a Nutritional Injection Unit.
    fn item_decorations(&self, _: &dyn ClientHost, event: &mut ItemDecorationsEvent<'_>) {
        let stack = event.stack;
        if stack.count() != 1 {
            return;
        }
        let (amount, capacity, color) = if self.id == modules::BODYARMOR && data::enabled_modules(stack).contains_key(data::JETPACK_UNIT) {
            (data::fuel(stack, self.id), data::tank_capacity(stack, self.id), chemical_color(data::item_chemical(self.id)))
        } else if self.id == modules::HELMET && data::enabled_modules(stack).contains_key(data::NUTRITION_UNIT) {
            (data::suit_paste(stack), data::suit_paste_capacity(), chemical_color(mekanism_core::chemical::NUTRITIONAL_PASTE))
        } else {
            return;
        };
        let (x, y) = (event.x, event.y);
        event.draw.fill([x + 2.0, y + 12.0, 13.0, 2.0], 0xFF00_0000);
        event.draw.fill([x + 2.0, y + 12.0, width(amount, capacity) as f32, 1.0], 0xFF00_0000 | color);
    }

    /// Sneak + right click with the Atomic Disassembler cycles its mode (`ItemAtomicDisassembler.changeMode`, Mekanism's
    /// mode key on the fork), with the new mode on the action bar.
    fn use_item(&self, host: HostSide<'_>, event: &mut UseItemEvent<'_>) -> Option<InteractionResult> {
        if self.id == modules::MEKA_TOOL {
            return meka_tool_teleport(host, event);
        }
        if self.id != DISASSEMBLER {
            return None;
        }
        // The mode is stored on the server, which tells the client its new stack.
        let HostSide::Server(host) = host else {
            return None;
        };
        let player = host.player(event.player)?;
        if !player.is_secondary_use_active(&*host)? {
            return None;
        }
        let living = host.living(player.entity())?;
        let mut stack = event.stack.clone();
        let config = data::gear_config();
        let mode = data::disassembler_mode(&stack).next_enabled(&config);
        data::set_disassembler_mode(&mut stack, mode);
        living.set_hand_item(host, event.hand, Some(stack)).ok()?;
        // `MekanismLang.DISASSEMBLER_MODE_CHANGE.translate(INDIGO, mode, AQUA, efficiency)`.
        let line = Text::translate("disassembler.mekanism.mode_change", vec![text::colored(mode.label(), color::INDIGO), text::colored(mode.efficiency().to_string(), color::AQUA)]);
        let _ = player.action_bar(host, line);
        Some(InteractionResult::Success)
    }

    /// What the gear adds under its name (`Item.appendHoverText` with Mekanism's components), in the order the
    /// fork's `tooltip_body` ran: the disassembler's mode, the module list, then the stored energy.
    fn tooltip(&self, host: &dyn ClientHost, event: &mut TooltipEvent<'_>) {
        let stack = event.stack;
        if self.id == DISASSEMBLER {
            // `DisassemblerMode.addToTooltip`: the mode, then its efficiency, both in INDIGO.
            let mode = data::disassembler_mode(stack);
            event.lines.push(translate_colored("tooltip.mekanism.mode", color::INDIGO, color::INDIGO, &[mode.label()]));
            event.lines.push(translate_colored("disassembler.mekanism.efficiency", color::INDIGO, color::INDIGO, &[&mode.efficiency().to_string()]));
        }
        if let Some((chemical, amount)) = data::tank(stack) {
            event.lines.push(chemical_line(self.id, &chemical, amount));
        }
        if modules::is_container(self.id) {
            module_list(host, self.id, stack, event.lines);
        }
        let energy = data::stored_energy(stack);
        if energy > 0 {
            // `MekanismLang.STORED_ENERGY.translateColored(BRIGHT_GREEN, GRAY, EnergyDisplay)`.
            event.lines.push(translate_colored("tooltip.mekanism.stored_energy", color::BRIGHT_GREEN, color::GRAY, &[&mekanism_core::energy::format_fe(energy)]));
        }
    }
}

const DISASSEMBLER: &str = "mekanism:atomic_disassembler";

/// "Hydrogen: 1,000 mB" (`ChemicalStack` tooltip of a chemical tank item).
fn chemical_line(item: &str, chemical: &str, amount: u64) -> (mod_api::Text, u32) {
    let canteen = item == CANTEEN;
    let name = pretty(chemical);
    let tint = if canteen { 0xD55ECB } else { 0xFFA160 };
    let text = text::colored(name, tint).append(mod_api::Text::literal(": ")).append(text::colored(grouped(amount), 0xCFCFCF)).append(mod_api::Text::literal(" mB"));
    (text, u32::MAX)
}

fn pretty(id: &str) -> String {
    id.rsplit(':')
        .next()
        .unwrap_or(id)
        .split('_')
        .map(|w| w.chars().next().map(|c| c.to_uppercase().collect::<String>() + &w[c.len_utf8()..]).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ")
}

fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The `module_container` appender: under the details key the installed modules (with `n/max` when stacked), else the hint.
fn module_list(host: &dyn ClientHost, item: &str, stack: &mod_api::ItemStack, lines: &mut Vec<(mod_api::Text, u32)>) {
    use mod_api::Text;
    if !details_key(host) {
        lines.push((Text::translate("tooltip.mekanism.hold_for_modules", vec![text::colored("Left Shift", color::INDIGO)]), text::line_color(color::GRAY)));
        return;
    }
    let installed = data::installed_modules(stack);
    if installed.is_empty() {
        lines.push(line("module.mekanism.installed.none", color::DARK_RED));
        return;
    }
    lines.push(line("module.mekanism.installed.list", color::BRIGHT_GREEN));
    for (id, n) in &installed {
        let name = Text::translate(format!("module.{}", id.replacen(':', ".", 1)), Vec::new());
        let line = if *n > 1 {
            let amount = Text::translate("generic.mekanism.fraction", vec![Text::literal(n.to_string()), Text::literal(modules::max_installed(item, id).to_string())]);
            Text::translate("generic.mekanism.transfer", vec![name, amount])
        } else {
            Text::translate("generic.mekanism.list", vec![name])
        };
        lines.push((line, u32::MAX));
    }
}

/// Whether the details key (Left Shift) is held.
fn details_key(host: &dyn ClientHost) -> bool {
    host.mapping("key.sneak").ok().flatten().is_some_and(|m| m.down)
}

/// `player_break_speed`: the Meka-Tool and the Atomic Disassembler mine at their own efficiency while they hold the
/// energy for the block, slower as it runs out, not at all when empty (`ItemMekaTool.getDestroySpeed`,
/// `ItemAtomicDisassembler.getDestroySpeed`).
pub(crate) fn player_break_speed(host: HostSide<'_>, event: &mut PlayerBreakSpeedEvent) -> Flow {
    let host: &dyn Host = match &host {
        HostSide::Server(h) => &**h,
        HostSide::Client(h) => &**h,
    };
    let Some((id, stack)) = held(host, event.player) else {
        return Flow::Continue;
    };
    if id != modules::MEKA_TOOL && id != DISASSEMBLER {
        return Flow::Continue;
    }
    let Some(hardness) = hardness_at(host, event.player, event.pos) else {
        return Flow::Continue;
    };
    if id == DISASSEMBLER {
        event.new_speed = core_gear::destroy_speed_with(data::gear_config().disassembler_energy_usage, data::disassembler_mode(&stack), data::stored_energy(&stack), hardness);
    } else {
        let installed = data::enabled_modules(&stack);
        let silk = installed.contains_key("mekanism:silk_touch_unit");
        let efficiency = modules::tool_efficiency(&installed, data::module_choice(&stack, "mekanism:excavation_escalation_unit", "excavation_mode"));
        event.new_speed = modules::tool_destroy_speed(efficiency, data::stored_energy(&stack), hardness, silk);
    }
    Flow::Continue
}

/// The registry name and stack of what `player` (an entity id) holds in the main hand.
fn held(host: &dyn Host, player: u32) -> Option<(String, ItemStack)> {
    let living = host.living(host.entity(player)?)?;
    let stack = living.hand_item(host, Hand::Main)?;
    let id = stack.item().id(host);
    id.starts_with("mekanism:").then_some((id, stack))
}

/// How hard the block at `pos` is to break (`BlockState.getDestroySpeed`), in the level of `player`.
fn hardness_at(host: &dyn Host, player: u32, pos: [i32; 3]) -> Option<f32> {
    let level = host.entity(player)?.level(host)?;
    host.block_states().destroy_speed(level.block_state(host, pos)?)
}

/// Right click with the Meka-Tool and its Teleportation Unit: to the top of the block looked at, up to the reach away, for
/// `teleport_energy` FE (`ItemMekaTool.use`, `ModuleTeleportationUnit.onItemUse`). Server-side; the client predicts nothing.
/// Gap: the 100 portal particles of `PacketPortalFX` are not sent yet.
fn meka_tool_teleport(host: HostSide<'_>, event: &mut UseItemEvent<'_>) -> Option<InteractionResult> {
    let HostSide::Server(host) = host else {
        return None;
    };
    let stack = event.stack;
    let installed = data::enabled_modules(stack);
    if !installed.contains_key("mekanism:teleportation_unit") {
        return None;
    }
    let player = host.player(event.player)?;
    let entity = player.entity();
    let eye = entity.eye_position(&*host)?;
    let dir = entity.view_vector(&*host)?;
    let reach = mekanism_core::config::gear_int("mekatool.maxTeleportReach", modules::TOOL_TELEPORT_REACH as u64) as f64;
    // `ModuleTeleportationUnit.requiresBlockTarget`: without it a miss teleports to the end of the reach.
    let requires_block = data::module_choice(stack, "mekanism:teleportation_unit", "teleportation_requires_block") != Some(0);
    let level = entity.level(&*host)?;
    let end = [0, 1, 2].map(|k| eye[k] + dir[k] * reach);
    let hit = level.clip(&*host, eye, end, mod_api::ClipBlock::Collider, mod_api::ClipFluid::None)?;
    let target = if !hit.miss {
        hit.block
    } else if !requires_block {
        end.map(|v| v.floor() as i32)
    } else {
        return None;
    };
    // `isValidDestinationBlock`: air or a liquid.
    let free = |dy: i32| {
        let at = [target[0], target[1] + dy, target[2]];
        level.block(&*host, at).map(|b| b.id(&*host)).is_some_and(|id| id.ends_with("air") || matches!(id.as_str(), "minecraft:water" | "minecraft:lava"))
    };
    if !free(1) || !free(2) {
        return None;
    }
    let at = entity.position(&*host)?;
    let distance_sq: f64 = (0..3).map(|k| (at[k] - f64::from(target[k])).powi(2)).sum();
    if distance_sq < 5.0 {
        return None;
    }
    let cost = modules::teleport_energy(distance_sq);
    let creative = player.is_creative(&*host).unwrap_or(false);
    let mut paid = stack.clone();
    if !creative {
        let stored = data::stored_energy(&paid);
        if stored < cost {
            return None;
        }
        data::set_energy(&mut paid, stored - cost);
        host.living(entity)?.set_hand_item(host, event.hand, Some(paid)).ok()?;
    }
    // `player.teleportTo(x + 0.5, y + 1.5, z + 0.5)` and the teleport sound.
    entity.teleport_to(host, [f64::from(target[0]) + 0.5, f64::from(target[1]) + 1.5, f64::from(target[2]) + 0.5], None);
    entity.play_sound(host, "minecraft:entity.player.teleport", 1.0, 1.0);
    Some(InteractionResult::Success)
}

/// `attack_entity`: the Meka-Tool's and the Atomic Disassembler's own damage, paid in energy outside creative
/// (`ItemMekaTool.adjustAttributes` with the Attack Amplification Unit, `ItemAtomicDisassembler.adjustAttributes` /
/// `postHurtEnemy`). The extra damage waits for the hit's damage to be worked out (`living_damage_pre`).
/// Gap: the attack strength (`0.2 + strength² * 0.8`) is not known here, so a hit counts as fully charged.
pub(crate) fn attack_entity(m: &crate::Mod, host: &mut dyn mod_api::ServerHost, event: &mut AttackEntityEvent<'_>) -> Flow {
    let Some(player) = host.players().acting_player() else {
        return Flow::Continue;
    };
    let entity = player.entity();
    let Some(living) = host.living(entity) else {
        return Flow::Continue;
    };
    let Some(tool) = living.hand_item(&*host, Hand::Main) else {
        return Flow::Continue;
    };
    let id = tool.item().id(&*host);
    if id != modules::MEKA_TOOL && id != DISASSEMBLER {
        return Flow::Continue;
    }
    use mekanism_core::config::gear_int;
    let creative = player.is_creative(&*host).unwrap_or(false);
    let stored = data::stored_energy(&tool);
    let (extra, cost) = if id == DISASSEMBLER {
        let cost = gear_int("atomic_disassembler.energyUsageWeapon", 2_000);
        let (min, max) = (gear_int("atomic_disassembler.minDamage", 4) as f64, gear_int("atomic_disassembler.maxDamage", 20) as f64);
        let have = if creative { u64::MAX } else { stored };
        let damage = if have >= cost || cost == 0 { max } else { min + (max - min) * (have as f64 / cost as f64) };
        (damage as f32, cost.min(stored))
    } else {
        let base = gear_int("mekatool.baseDamage", 4) as f32;
        let bonus = modules::attack_bonus(&data::enabled_modules(&tool), data::module_choice(&tool, "mekanism:attack_amplification_unit", "attack_damage"));
        let (extra, cost) = modules::attack(bonus, if creative { u64::MAX } else { stored });
        (base + extra.max(0.0), if extra > 0.0 { cost } else { 0 })
    };
    if !creative && cost > 0 {
        let mut paid = tool;
        data::set_energy(&mut paid, stored - cost.min(stored));
        let _ = living.set_hand_item(host, Hand::Main, Some(paid));
    }
    if extra > 0.0 {
        m.gear.pending.lock().unwrap().insert(event.entity, extra);
    }
    Flow::Continue
}

/// `living_damage_pre`: the hit takes the extra damage `attack_entity` set aside for its target.
pub(crate) fn living_damage_pre(m: &crate::Mod, _: &mut dyn LevelRandomHost, event: &mut LivingDamagePreEvent<'_>) {
    if let Some(extra) = m.gear.pending.lock().unwrap().remove(&event.entity) {
        let now = event.container.new_damage();
        event.container.set_new_damage(now + extra);
    }
}

/// `server_tick`: once a second, each player's MekaSuit works (`ModuleChargeDistributionUnit.tickServer`).
pub(crate) fn server_tick(m: &crate::Mod, host: &mut dyn mod_api::ServerHost, event: &mod_api::ServerTickEvent<'_>) {
    if !matches!(event.phase, mod_api::ServerTickPhase::ServerTickEnd { world_running: true }) {
        return;
    }
    if m.gear.second.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 20 != 0 {
        return;
    }
    for player in host.players().server_players() {
        mekasuit::distribute(host, player);
    }
}

/// `client_tick_post`: the window's player thrusts with a jetpack on.
pub(crate) fn client_tick_post(_: &crate::Mod, host: &mut dyn ClientHost) {
    jetpack::client_tick(host);
}
