//! Module settings (`ModuleConfig`, `ModuleData.defaultConfigs`): every module can be switched off, and some
//! have options. The Module Tweaker edits them; an item keeps them next to the modules it holds.

/// What a module's option holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bool(bool),
    /// A choice among `options` (in order), starting at `default`; a bounded one offers only the first
    /// `installed + extra` of them (`ModuleEnumConfig.createBounded`).
    Enum { options: &'static [&'static str], default: usize, extra: Option<usize> },
}

#[derive(Clone, Copy, Debug)]
pub struct Option_ {
    /// The option's name (`module.mekanism.<key>`).
    pub key: &'static str,
    /// The words the tweaker shows for it.
    pub label: &'static str,
    pub kind: Kind,
}

/// How a module is built (`ModuleData.ModuleDataBuilder`).
#[derive(Clone, Copy, Debug)]
pub struct Data {
    pub id: &'static str,
    /// `noDisable`: no Enabled option.
    pub no_disable: bool,
    pub disabled_by_default: bool,
    pub handles_mode_change: bool,
    pub mode_change_disabled_by_default: bool,
    pub renders_hud: bool,
    /// `exclusive(...)`: it conflicts with others.
    pub exclusive: bool,
    pub options: &'static [Option_],
}

const fn b(key: &'static str, label: &'static str, default: bool) -> Option_ {
    Option_ { key, label, kind: Kind::Bool(default) }
}

const fn e(key: &'static str, label: &'static str, options: &'static [&'static str], default: usize, extra: Option<usize>) -> Option_ {
    Option_ { key, label, kind: Kind::Enum { options, default, extra } }
}

const fn m(id: &'static str) -> Data {
    Data { id, no_disable: false, disabled_by_default: false, handles_mode_change: false, mode_change_disabled_by_default: false, renders_hud: false, exclusive: false, options: &[] }
}

const SPEED: &[&str] = &["Off", "Slow", "Normal", "Fast", "Super Fast", "Extreme"];
const DAMAGE: &[&str] = &["0", "4", "8", "16", "24", "32"];
const FARM: &[&str] = &["0", "1", "3", "5", "7"];
const BLAST: &[&str] = &["Off", "Low", "Medium", "High", "Extreme"];
const RANGE: &[&str] = &["0", "2", "4", "6", "8"];
const JETPACK: &[&str] = &["Regular", "Hover", "Vector", "Disabled"];
const THRUST: &[&str] = &["0.5x", "1x", "2x", "3x", "4x"];
const SPRINT: &[&str] = &["0", "0.05", "0.1", "0.25", "0.5"];
const JUMP: &[&str] = &["0", "0.5", "1", "3", "5"];
const STEP: &[&str] = &["0", "0.5", "1", "1.5", "2"];
const ATTRACT: &[&str] = &["0", "1", "3", "5", "10"];

pub const MODULES: &[Data] = &[
    Data { no_disable: true, ..m("mekanism:energy_unit") },
    Data { no_disable: true, ..m("mekanism:color_modulation_unit") },
    m("mekanism:laser_dissipation_unit"),
    m("mekanism:radiation_shielding_unit"),
    Data { handles_mode_change: true, renders_hud: true, options: &[e("excavation_mode", "Excavation Speed", SPEED, 2, Some(2))], ..m("mekanism:excavation_escalation_unit") },
    Data { renders_hud: true, options: &[e("attack_damage", "Bonus Attack Damage", DAMAGE, 2, Some(2))], ..m("mekanism:attack_amplification_unit") },
    Data { exclusive: true, options: &[e("farming_radius", "Farming Radius", FARM, 1, Some(1))], ..m("mekanism:farming_unit") },
    Data { exclusive: true, ..m("mekanism:shearing_unit") },
    Data { exclusive: true, ..m("mekanism:silk_touch_unit") },
    Data { exclusive: true, ..m("mekanism:fortune_unit") },
    Data { handles_mode_change: true, renders_hud: true, options: &[e("blast_radius", "Blast Radius", BLAST, 1, Some(1))], ..m("mekanism:blasting_unit") },
    Data {
        handles_mode_change: true,
        renders_hud: true,
        options: &[b("extended_mode", "Extended Mode", false), e("mining_range", "Excavation Range", RANGE, 1, Some(1))],
        ..m("mekanism:vein_mining_unit")
    },
    Data { exclusive: true, options: &[b("teleportation_requires_block", "Requires Block Target", true)], ..m("mekanism:teleportation_unit") },
    Data { options: &[b("breathing.held", "Fill Held", true)], ..m("mekanism:electrolytic_breathing_unit") },
    Data {
        options: &[b("purification.beneficial", "Remove Beneficial", false), b("purification.neutral", "Remove Neutral", false), b("purification.harmful", "Remove Harmful", true)],
        ..m("mekanism:inhalation_purification_unit")
    },
    Data { disabled_by_default: true, handles_mode_change: true, renders_hud: true, ..m("mekanism:vision_enhancement_unit") },
    Data { renders_hud: true, ..m("mekanism:nutritional_injection_unit") },
    Data { renders_hud: true, ..m("mekanism:dosimeter_unit") },
    Data { renders_hud: true, ..m("mekanism:geiger_unit") },
    Data {
        handles_mode_change: true,
        renders_hud: true,
        exclusive: true,
        options: &[e("jetpack_mode", "Jetpack Mode", JETPACK, 0, None), e("jetpack_mult", "Thrust Multiplier", THRUST, 1, Some(1)), e("jetpack_mult.hover", "Hover Thrust Multiplier", THRUST, 1, Some(1))],
        ..m("mekanism:jetpack_unit")
    },
    Data { options: &[b("charge_suit", "Charge Suit", true), b("charge_inventory", "Charge Inventory", false)], ..m("mekanism:charge_distribution_unit") },
    Data { handles_mode_change: true, renders_hud: true, exclusive: true, options: &[e("speed_boost", "Speed Boost", SPRINT, 1, None)], ..m("mekanism:gravitational_modulating_unit") },
    Data { handles_mode_change: true, mode_change_disabled_by_default: true, ..m("mekanism:elytra_unit") },
    Data { handles_mode_change: true, options: &[e("sprint_boost", "Sprint Boost", SPRINT, 1, Some(1))], ..m("mekanism:locomotive_boosting_unit") },
    m("mekanism:gyroscopic_stabilization_unit"),
    m("mekanism:hydrostatic_repulsor_unit"),
    m("mekanism:motorized_servo_unit"),
    Data {
        options: &[e("jump_boost", "Jump Boost", JUMP, 1, Some(1)), e("step_assist", "Step Assist", STEP, 1, Some(1))],
        ..m("mekanism:hydraulic_propulsion_unit")
    },
    Data { handles_mode_change: true, options: &[e("range", "Range", ATTRACT, 1, Some(1))], ..m("mekanism:magnetic_attraction_unit") },
    m("mekanism:frost_walker_unit"),
    m("mekanism:soul_surfer_unit"),
    m("mekanismgenerators:solar_recharging_unit"),
    m("mekanismgenerators:geothermal_generator_unit"),
];

pub fn data(id: &str) -> Option<&'static Data> {
    MODULES.iter().find(|d| d.id == id)
}

/// One setting a module shows, in the order the tweaker lists them (`ModuleData.defaultConfigs`): enabled,
/// the mode-key and HUD switches, then its own options.
#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
}

/// The settings of `module` with `installed` of it in the item (the hydrostatic repulsor's swim boost only
/// appears at four, `BOOST_STACKS`).
pub fn settings(module: &str, installed: u32) -> Vec<Setting> {
    let Some(d) = data(module) else {
        return vec![];
    };
    let mut out = vec![];
    if !d.no_disable {
        out.push(Setting { key: "enabled", label: "Enabled", kind: Kind::Bool(!d.disabled_by_default) });
    }
    if d.handles_mode_change {
        out.push(Setting { key: "handle_mode_change", label: "Handle Mode Key", kind: Kind::Bool(!d.mode_change_disabled_by_default) });
    }
    if d.renders_hud {
        out.push(Setting { key: "render_hud", label: "Show in HUD", kind: Kind::Bool(true) });
    }
    for o in d.options {
        out.push(Setting { key: o.key, label: o.label, kind: o.kind });
    }
    if module == "mekanism:hydrostatic_repulsor_unit" && installed >= 4 {
        out.push(Setting { key: "swim_boost", label: "Swim Boost", kind: Kind::Bool(true) });
    }
    out
}

/// The choices an enum setting offers with `installed` of the module (`createBounded`).
pub fn choices(kind: Kind, installed: u32) -> usize {
    match kind {
        Kind::Enum { options, extra: Some(extra), .. } => options.len().min(installed as usize + extra),
        Kind::Enum { options, .. } => options.len(),
        Kind::Bool(_) => 2,
    }
}

/// A stored value as text: "true"/"false" or the option's index. Reads the default for what is missing or
/// out of range.
pub fn parse(kind: Kind, stored: Option<&str>, installed: u32) -> u32 {
    match kind {
        Kind::Bool(d) => u32::from(stored.map_or(d, |s| s == "1")),
        Kind::Enum { default, .. } => {
            let n = choices(kind, installed);
            stored.and_then(|s| s.parse::<usize>().ok()).filter(|v| *v < n).unwrap_or(default.min(n.saturating_sub(1))) as u32
        }
    }
}

/// The value as stored, or None when it is the default (so an unconfigured item stays clean).
pub fn encode(kind: Kind, value: u32) -> Option<String> {
    match kind {
        Kind::Bool(d) => (value != u32::from(d)).then(|| (if value == 1 { "1" } else { "0" }).to_owned()),
        Kind::Enum { default, .. } => (value as usize != default).then(|| value.to_string()),
    }
}

/// What an option shows for `value`.
pub fn value_text(kind: Kind, value: u32) -> String {
    match kind {
        Kind::Bool(_) => (if value == 1 { "True" } else { "False" }).to_owned(),
        Kind::Enum { options, .. } => options.get(value as usize).copied().unwrap_or("").to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_follow_the_module_data() {
        let s = settings("mekanism:jetpack_unit", 2);
        let keys: Vec<_> = s.iter().map(|x| x.key).collect();
        assert_eq!(keys, ["enabled", "handle_mode_change", "render_hud", "jetpack_mode", "jetpack_mult", "jetpack_mult.hover"]);
        assert!(settings("mekanism:energy_unit", 3).is_empty());
        assert_eq!(settings("mekanism:vision_enhancement_unit", 1)[0].kind, Kind::Bool(false));
        assert_eq!(settings("mekanism:hydrostatic_repulsor_unit", 3).len(), 1);
        assert_eq!(settings("mekanism:hydrostatic_repulsor_unit", 4).len(), 2);
    }

    #[test]
    fn enums_are_bounded_by_the_installed_count() {
        let k = settings("mekanism:farming_unit", 1)[1].kind;
        assert_eq!(choices(k, 1), 2);
        assert_eq!(choices(k, 4), 5);
        // A stored value past the bound falls back to the default.
        assert_eq!(parse(k, Some("4"), 1), 1);
        assert_eq!(parse(k, Some("2"), 3), 2);
        assert_eq!(encode(k, 1), None);
        assert_eq!(encode(k, 3).as_deref(), Some("3"));
    }
}
