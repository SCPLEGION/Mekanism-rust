//! What Mekanism keeps in an item's `minecraft:custom_data`: energy, chemical tanks, installed modules and their
//! settings. Plain NBT, the same keys as the fork's runtime wrote, so saved items carry over.
use mekanism_core::{gear, module_config, modules};
use mod_api::{Compound, ItemStack, Tag};

pub(crate) const ENERGY_KEY: &str = "mekanism:energy";
pub(crate) const CHEMICAL_KEY: &str = "mekanism:chemical";
pub(crate) const MODULES_KEY: &str = "mekanism:modules";
pub(crate) const MODULE_CONFIG_KEY: &str = "mekanism:module_config";
pub(crate) const MODULE_TYPE_KEY: &str = "mekanism:module_type";
pub(crate) const JETPACK_MODE_KEY: &str = "mekanism:jetpack_mode";
const CUSTOM_DATA: &str = "minecraft:custom_data";

/// The item's `minecraft:custom_data`, when it has some.
/// `nbt.get(key)`: a compound's keys are Java strings, which a mod cannot name.
pub(crate) fn get<'a>(nbt: &'a Compound, key: &str) -> Option<&'a Tag> {
    nbt.iter().find(|(k, _)| k.to_string_lossy() == key).map(|(_, v)| v)
}

pub(crate) fn custom_data(stack: &ItemStack) -> Option<&Compound> {
    match get(stack.components()?.as_nbt(), CUSTOM_DATA)? {
        Tag::Compound(c) => Some(c),
        _ => None,
    }
}

pub(crate) fn get_long(nbt: &Compound, key: &str) -> u64 {
    match get(nbt, key) {
        Some(Tag::Long(v)) => (*v).max(0) as u64,
        Some(Tag::Int(v)) => (*v).max(0) as u64,
        _ => 0,
    }
}

pub(crate) fn get_string(nbt: &Compound, key: &str) -> Option<String> {
    match get(nbt, key)? {
        Tag::String(s) => s.to_string().ok(),
        _ => None,
    }
}

/// Sets `key` in the item's custom data, keeping the rest of it and the other components.
pub(crate) fn set_custom(stack: &mut ItemStack, key: &str, value: Option<Tag>) {
    let mut data = custom_data(stack).cloned().unwrap_or_default();
    match value {
        Some(v) => {
            data.insert(key.into(), v);
        }
        None => {
            data.retain(|k, _| k.to_string_lossy() != key);
        }
    }
    let mut patch = Compound::new();
    patch.insert(CUSTOM_DATA.into(), Tag::Compound(data));
    let _ = stack.apply_components(&patch);
}

pub(crate) fn stored_energy(stack: &ItemStack) -> u64 {
    custom_data(stack).map_or(0, |c| get_long(c, ENERGY_KEY))
}

pub(crate) fn set_energy(stack: &mut ItemStack, energy: u64) {
    set_custom(stack, ENERGY_KEY, Some(Tag::Long(energy.min(i64::MAX as u64) as i64)));
}

/// Every module the item holds, switched on or not.
pub(crate) fn installed_modules(stack: &ItemStack) -> modules::Installed {
    let mut out = modules::Installed::new();
    if let Some(Tag::Compound(c)) = custom_data(stack).and_then(|c| get(c, MODULES_KEY)) {
        for (k, v) in c.iter() {
            if let (Ok(id), Tag::Int(n)) = (k.to_string(), v)
                && *n > 0
            {
                out.insert(id, *n as u32);
            }
        }
    }
    out
}

/// The stored value of a module option, or its default (`ModuleConfigItem`).
pub(crate) fn module_setting(stack: &ItemStack, module: &str, s: &module_config::Setting, installed: u32) -> u32 {
    let stored = match custom_data(stack).and_then(|c| get(c, MODULE_CONFIG_KEY)) {
        Some(Tag::Compound(all)) => match get(all, module) {
            Some(Tag::Compound(m)) => match get(m, s.key) {
                Some(Tag::Int(v)) => Some(v.to_string()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    };
    module_config::parse(s.kind, stored.as_deref(), installed)
}

/// The value of `module`'s option `key` on `stack` (an index for choices, 0 or 1 for switches); None without the module.
pub(crate) fn module_choice(stack: &ItemStack, module: &str, key: &str) -> Option<usize> {
    let installed = installed_modules(stack).get(module).copied()?;
    module_config::settings(module, installed).into_iter().find(|s| s.key == key).map(|s| module_setting(stack, module, &s, installed) as usize)
}

/// Whether `module` is switched on (the `enabled` option; modules that can't be disabled always are).
pub(crate) fn module_enabled(stack: &ItemStack, module: &str) -> bool {
    let n = installed_modules(stack).get(module).copied().unwrap_or(0);
    match module_config::settings(module, n).first() {
        Some(s) if s.key == "enabled" => module_setting(stack, module, s, n) == 1,
        _ => n > 0,
    }
}

/// Installed modules that are switched on.
pub(crate) fn enabled_modules(stack: &ItemStack) -> modules::Installed {
    let mut all = installed_modules(stack);
    all.retain(|id, _| module_enabled(stack, id));
    all
}

/// The module a module item stands for (`mekanism:module_type`).
pub(crate) fn module_type(stack: &ItemStack, item_id: &str) -> Option<String> {
    if item_id != modules::MODULE_ITEM {
        return None;
    }
    get_string(custom_data(stack)?, MODULE_TYPE_KEY)
}

/// An item's energy limits; the Meka-Tool's and the MekaSuit's grow with their energy units.
pub(crate) fn item_energy_of(stack: &ItemStack, item_id: &str) -> Option<gear::ItemEnergy> {
    let mut e = gear::item_energy(item_id)?;
    if modules::is_container(item_id) {
        (e.capacity, e.rate) = modules::energy_from(&installed_modules(stack), e.capacity, e.rate);
    }
    Some(e)
}

// --- Chemical tanks in items ---------------------------------------------------------------------------------

pub(crate) const SUIT_PASTE_KEY: &str = "mekanism:suit_nutritional_paste";
pub(crate) const NUTRITION_UNIT: &str = "mekanism:nutritional_injection_unit";
pub(crate) const JETPACK_UNIT: &str = "mekanism:jetpack_unit";
const CANTEEN_CAPACITY: u64 = 64_000;
const SCUBA_CAPACITY: u64 = 24_000;
const SUIT_PASTE_CAPACITY: u64 = 128_000;

/// The chemical an item with a chemical tank holds: a scuba tank oxygen, a canteen paste, the rest hydrogen.
pub(crate) fn item_chemical(id: &str) -> &'static str {
    match id {
        "mekanism:canteen" => mekanism_core::chemical::NUTRITIONAL_PASTE,
        "mekanism:scuba_tank" => "mekanism:oxygen",
        _ => "mekanism:hydrogen",
    }
}

/// What the item's tank holds at most (`GearConfig`), 0 for a MekaSuit body armor without a Jetpack Unit.
pub(crate) fn tank_capacity(stack: &ItemStack, id: &str) -> u64 {
    use mekanism_core::config::gear_int;
    match id {
        "mekanism:canteen" => gear_int("canteen.maxStorage", CANTEEN_CAPACITY),
        "mekanism:scuba_tank" => gear_int("scuba_tank.capacity", SCUBA_CAPACITY),
        mekanism_core::flamethrower::ITEM => gear_int("flamethrower.capacity", mekanism_core::flamethrower::CAPACITY),
        modules::BODYARMOR => {
            // `ItemMekaSuitArmor`: the tank holds `mekaSuitJetpackMaxStorage` per installed Jetpack Unit, none without one.
            let count = u64::from(installed_modules(stack).get(JETPACK_UNIT).copied().unwrap_or(0));
            gear_int("mekasuit.jetpackMaxStorage", modules::SUIT_JETPACK_CAPACITY) * count
        }
        _ => gear_int("jetpack.capacity", gear::JETPACK_CAPACITY),
    }
}

/// The chemical of the item's tank and how much of it, when it holds some.
pub(crate) fn tank(stack: &ItemStack) -> Option<(String, u64)> {
    let Tag::Compound(c) = get(custom_data(stack)?, CHEMICAL_KEY)? else {
        return None;
    };
    let id = get_string(c, "id")?;
    let amount = get_long(c, "amount");
    (amount > 0).then_some((id, amount))
}

/// The hydrogen (or oxygen, paste) a tank item holds.
pub(crate) fn fuel(stack: &ItemStack, id: &str) -> u64 {
    tank(stack).filter(|(chemical, _)| chemical == item_chemical(id)).map_or(0, |(_, amount)| amount.min(tank_capacity(stack, id)))
}

pub(crate) fn set_fuel(stack: &mut ItemStack, id: &str, amount: u64) {
    let value = (amount > 0).then(|| {
        let mut c = Compound::new();
        c.insert("id".into(), Tag::String(item_chemical(id).into()));
        c.insert("amount".into(), Tag::Long(amount.min(i64::MAX as u64) as i64));
        Tag::Compound(c)
    });
    set_custom(stack, CHEMICAL_KEY, value);
}

pub(crate) fn suit_paste(stack: &ItemStack) -> u64 {
    custom_data(stack).map_or(0, |c| get_long(c, SUIT_PASTE_KEY))
}

pub(crate) fn suit_paste_capacity() -> u64 {
    mekanism_core::config::gear_int("mekasuit.nutritionalMaxStorage", SUIT_PASTE_CAPACITY)
}

/// A jetpack, or MekaSuit body armor with a Jetpack Unit (`ModuleJetpackUnit`).
pub(crate) fn is_jetpack(stack: &ItemStack, id: &str) -> bool {
    matches!(id, "mekanism:jetpack" | "mekanism:jetpack_armored") || (id == modules::BODYARMOR && enabled_modules(stack).contains_key(JETPACK_UNIT))
}

// --- Atomic Disassembler ---------------------------------------------------------------------------------------

pub(crate) const MODE_KEY: &str = "mekanism:disassembler_mode";

/// The mode as stored; a mode the config switched off reads back as normal (`DisassemblerMode.CODEC`).
pub(crate) fn disassembler_mode(stack: &ItemStack) -> gear::DisassemblerMode {
    let mode = custom_data(stack).and_then(|c| get_string(c, MODE_KEY)).as_deref().and_then(gear::DisassemblerMode::from_name).unwrap_or_default();
    if mode.enabled(&gear_config()) { mode } else { gear::DisassemblerMode::Normal }
}

/// `GearConfig` of the running world.
pub(crate) fn gear_config() -> mekanism_core::config::GearConfig {
    mekanism_core::config::GearConfig::default()
}

/// Stores the disassembler's mode, keeping the rest of the item's data.
pub(crate) fn set_disassembler_mode(stack: &mut ItemStack, mode: gear::DisassemblerMode) {
    set_custom(stack, MODE_KEY, Some(Tag::String(mode.name().into())));
}
