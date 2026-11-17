//! Machine screens: layout, tabs, windows (extracted from the fork's runtime.rs, `Screens` section).
#![allow(unused, dead_code)]
use super::gfx::{Batch, Gfx};
use mekanism_core::bin::{self, Bin};
use mekanism_core::chemical::{self as chem, ChemicalTank};
use mekanism_core::entangloporter::{self as qe, Frequency};
use mekanism_core::redstone::RedstoneControl;
use mekanism_core::sides::{self as side_config, SideMode, Sides, Transmission};
use mekanism_core::tier::{self, BaseTier};
use mekanism_core::upgrades::{Upgrade, Upgrades};
use mekanism_core::{boiler, dynamic_tank, factory, fusion, isotopes, laser, modules, nuclear, plenisher, qio, seismic, sps, teleporter};
use mekanism_core::{chargepad, formulaic as fa, machine, oredictionificator as od, security as sec, stabilizer as st};
use mekanism_core::generator::HeatGenerator;
use mod_api::ItemStack;
use mekanism_core::frequency as freq;
use mekanism_core::evaporation as evap;
const FUSION_MAX_TEMP: u64 = 500_000_000;
const REACTION_TANK: u64 = 10_000;
const MINER_SLOTS: usize = 27;

const HEAT_GENERATOR: &str = "mekanismgenerators:heat_generator";
const FISSION_LOGIC: &str = "mekanismgenerators:fission_reactor_logic_adapter";
const FUSION_LOGIC: &str = "mekanismgenerators:fusion_reactor_logic_adapter";
const TURBINE_VALVE: &str = "mekanismgenerators:turbine_valve";
const TURBINE_ROTOR: &str = "mekanismgenerators:turbine_rotor";
const OBSIDIAN_TNT: &str = "mekanismadditions:obsidian_tnt";
const REACTION_CHAMBER: &str = "mekanism:pressurized_reaction_chamber";
const WASTE_BARREL: &str = "mekanism:radioactive_waste_barrel";
const CARDBOARD: &str = "mekanism:cardboard_box";
fn usage_of(_recipe: &str, default: u64) -> u64 { default }

/// The slots in a row of the chest menu a factory shows.
const FACTORY_ROW: usize = 9;
/// A factory's energy slot (`TileEntityFactory.energySlot`), after its two rows.
const FACTORY_ENERGY: usize = 2 * FACTORY_ROW;
/// The chemical slot of the item and chemical factories (`ChemicalInventorySlot.fillOrConvert`, at (7, 57)), or the
/// combiner factory's extra input; the sawing factory's secondary outputs start here instead.
const FACTORY_CHEM: usize = FACTORY_ENERGY + 1;

/// How many slots a factory has: its two rows, the energy slot, and its extra slot or secondary outputs.
fn factory_slot_count(block: &str) -> usize {
    match kind(block) {
        Some(Kind::Factory(n, "sawing", _)) => FACTORY_CHEM + n,
        Some(Kind::Factory(_, recipe, _)) if factory::has_extra_slot(recipe) => FACTORY_CHEM + 1,
        _ => FACTORY_CHEM,
    }
}

/// The advanced machine whose recipes and chemicals a chemical factory runs.
fn chem_factory(recipe: &str) -> Option<&'static chem::AdvancedMachine> {
    if recipe == "infusing" {
        return Some(&chem::INFUSING_FACTORY);
    }
    let block = match recipe {
        "compressing" => "mekanism:osmium_compressor",
        "purifying" => "mekanism:purification_chamber",
        "injecting" => "mekanism:chemical_injection_chamber",
        _ => return None,
    };
    chem::ADVANCED_MACHINES.iter().find(|m| m.block == block)
}

fn is_chem_factory(block: &str) -> bool {
    matches!(kind(block), Some(Kind::Factory(_, recipe, _)) if factory::takes_chemical(recipe))
}

/// What a Mekanism block entity is, from its block (and block entity) id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Machine(&'static str, u64),
    /// processes, recipe kind, energy per tick
    Factory(usize, &'static str, u64),
    HeatGenerator,
    /// peak FE a tick, storage
    Solar(u64, u64),
    Wind,
    EnergyCube(BaseTier),
    /// A universal cable and what it carries in a tick.
    Cable(u64),
    /// A thermodynamic conductor of a tier.
    Conductor(BaseTier),
    /// A pressurized tube and the mB it carries in a tick.
    Tube(u64),
    Oxidizer,
    /// A Pigment Extractor: an item into pigment, as the oxidizer makes gas.
    PigmentExtractor,
    /// A purification chamber or osmium compressor: an item and a chemical each tick.
    Advanced(&'static chem::AdvancedMachine),
    ChemicalInfuser,
    /// A Painting Machine: an item and pigment into a dyed item.
    Painting,
    /// A Pigment Mixer: two pigments into a third, as the chemical infuser does.
    PigmentMixer,
    /// A Thermal Evaporation Plant's controller; the tower's other blocks have no block entity.
    Evaporation,
    /// A Rotary Condensentrator, decondensentrating: a fluid into its chemical.
    Rotary,
    /// An Induction Matrix's port; the casing, cells and providers have no block entity.
    InductionPort,
    /// A logistical transporter and how many items it pulls at a time.
    Transporter(u32),
    /// A turbine rotor and the blades (0 to 2) it houses.
    TurbineRotor,
    /// An Obsidian TNT block: it lights itself from redstone.
    ObsidianTnt,
    Infuser,
    Separator,
    GasGenerator,
    ChemicalTank(BaseTier),
    /// The Radioactive Waste Barrel: radioactive chemicals only, decaying a little each second, emitting down when active.
    WasteBarrel,
    /// The Cardboard Box: a block (its state and block entity) packed inside.
    CardboardBox,
    DigitalMiner,
    FluidTank(BaseTier),
    /// The Chemical Dissolution Chamber: an item and a chemical each tick into a chemical.
    Dissolution,
    /// The Isotopic Centrifuge: a chemical into another.
    Centrifuge,
    /// A Fission Reactor's port; the first port of a formed reactor runs it.
    FissionPort,
    /// An Industrial Turbine's valve; the first valve runs it.
    TurbineValve,
    /// A Supercritical Phase Shifter's port; the first port runs it.
    SpsPort,
    /// A Dynamic Tank's valve; the first valve holds the tank.
    DynamicValve,
    /// The Antiprotonic Nucleosynthesizer: an item and antimatter into an item.
    Nucleosynthesizer,
    /// A Teleporter: sends players to another teleporter on its frequency.
    Teleporter,
    /// A Fusion Reactor's port; the first port of a formed reactor runs it.
    FusionPort,
    /// The Electric Pump: water (or heavy water) from below.
    Pump,
    /// The Fluidic Plenisher: fills the space below it with fluid.
    Plenisher,
    /// The Logistical Sorter: moves items from behind it to the front, by its filters.
    Sorter,
    /// The Oredictionificator: turns an item of a tag into the item its filter selects.
    Oredictionificator,
    /// The Nutritional Liquifier: food becomes nutritional paste.
    Liquifier,
    /// The Formulaic Assemblicator: crafts from its stock by an encoded formula.
    Assemblicator,
    /// The Dimensional Stabilizer: keeps a grid of chunks loaded for energy.
    Stabilizer,
    /// The Security Desk: its owner's trusted players and default mode, and the items it locks.
    SecurityDesk,
    /// The Modification Station: a module into an item in half a second.
    ModificationStation,
    /// The Chargepad: charges an item of the player standing on it.
    Chargepad,
    /// The Quantum Entangloporter: shares energy, fluid and chemicals across its frequency.
    Entangloporter,
    /// The Solar Neutron Activator: lithium into tritium in the sun.
    Activator,
    /// A Thermoelectric Boiler's valve; the first valve runs it.
    BoilerValve,
    /// The Resistive Heater: energy into heat for a boiler.
    Heater,
    /// The Chemical Washer: dirty slurry and water into clean slurry.
    Washer,
    /// The Chemical Crystallizer: a chemical into an item.
    Crystallizer,
    /// A mechanical pipe and the mB of fluid it carries in a tick.
    Pipe(u64),
    /// A bin: many of one item.
    Bin(BaseTier),
    /// A personal chest or barrel: six rows that open for their owner only.
    PersonalStorage,
    /// A QIO drive array, dashboard, importer or exporter.
    QioDriveArray,
    QioDashboard,
    QioImporter,
    QioExporter,
    QioRedstoneAdapter,
    /// A Fission or Fusion Reactor Logic Adapter: redstone output (and for the fission reactor, activation) from the reactor's state.
    FissionLogic,
    FusionLogic,
    /// A laser, laser amplifier or tractor beam: fires its energy along its facing.
    Laser,
    LaserAmplifier,
    TractorBeam,
    /// The Industrial Alarm: lit and turning while redstone powers it.
    Alarm,
    /// A Seismic Vibrator: lets a Seismic Reader see under its chunk while it has energy.
    Vibrator,
    /// The Bio-Generator: bioethanol (from bio fuel) into energy.
    BioGenerator,
    /// The Fuelwood Heater: furnace fuel into heat for a boiler.
    FuelwoodHeater,
    /// The Pressurized Reaction Chamber: an item, water and a chemical into an item and a chemical.
    Reaction,
}

fn kind(id: &str) -> Option<Kind> {
    if id == "mekanism:digital_miner" {
        return Some(Kind::DigitalMiner);
    }
    if let Some((recipe, usage)) = machine::machine(id) {
        return Some(Kind::Machine(recipe, usage_of(recipe, usage)));
    }
    if let Some((processes, recipe, usage)) = factory::factory(id) {
        return Some(Kind::Factory(processes, recipe, usage_of(recipe, usage)));
    }
    if id == HEAT_GENERATOR {
        return Some(Kind::HeatGenerator);
    }
    if let Some(t) = bin::tier_of(id) {
        return Some(Kind::Bin(t));
    }
    if bin::is_personal(id) {
        return Some(Kind::PersonalStorage);
    }
    if id == "mekanism:industrial_alarm" {
        return Some(Kind::Alarm);
    }
    if id == WASTE_BARREL {
        return Some(Kind::WasteBarrel);
    }
    if id == CARDBOARD {
        return Some(Kind::CardboardBox);
    }
    match id {
        qio::DRIVE_ARRAY => return Some(Kind::QioDriveArray),
        qio::DASHBOARD => return Some(Kind::QioDashboard),
        qio::IMPORTER => return Some(Kind::QioImporter),
        qio::EXPORTER => return Some(Kind::QioExporter),
        qio::REDSTONE_ADAPTER => return Some(Kind::QioRedstoneAdapter),
        FISSION_LOGIC => return Some(Kind::FissionLogic),
        FUSION_LOGIC => return Some(Kind::FusionLogic),
        laser::LASER => return Some(Kind::Laser),
        laser::AMPLIFIER => return Some(Kind::LaserAmplifier),
        laser::TRACTOR_BEAM => return Some(Kind::TractorBeam),
        seismic::VIBRATOR => return Some(Kind::Vibrator),
        _ => {}
    }
    use mekanism_core::generator as g;
    match id {
        "mekanismgenerators:solar_generator" => return Some(Kind::Solar(g::SOLAR_GENERATION, g::SOLAR_STORAGE)),
        "mekanismgenerators:advanced_solar_generator" => {
            return Some(Kind::Solar(g::ADVANCED_SOLAR_GENERATION, g::ADVANCED_SOLAR_STORAGE));
        }
        "mekanismgenerators:wind_generator" => return Some(Kind::Wind),
        "mekanismgenerators:gas_burning_generator" => return Some(Kind::GasGenerator),
        "mekanism:chemical_oxidizer" => return Some(Kind::Oxidizer),
        "mekanism:pigment_extractor" => return Some(Kind::PigmentExtractor),
        "mekanism:pigment_mixer" => return Some(Kind::PigmentMixer),
        "mekanism:painting_machine" => return Some(Kind::Painting),
        chem::LIQUIFIER => return Some(Kind::Liquifier),
        fa::BLOCK => return Some(Kind::Assemblicator),
        st::BLOCK => return Some(Kind::Stabilizer),
        sec::DESK => return Some(Kind::SecurityDesk),
        modules::STATION => return Some(Kind::ModificationStation),
        "mekanism:chemical_infuser" => return Some(Kind::ChemicalInfuser),
        "mekanism:antiprotonic_nucleosynthesizer" => return Some(Kind::Nucleosynthesizer),
        teleporter::TELEPORTER => return Some(Kind::Teleporter),
        "mekanism:thermal_evaporation_controller" => return Some(Kind::Evaporation),
        "mekanism:rotary_condensentrator" => return Some(Kind::Rotary),
        "mekanism:induction_port" => return Some(Kind::InductionPort),
        "mekanism:metallurgic_infuser" => return Some(Kind::Infuser),
        "mekanism:electrolytic_separator" => return Some(Kind::Separator),
        "mekanism:chemical_dissolution_chamber" => return Some(Kind::Dissolution),
        "mekanism:isotopic_centrifuge" => return Some(Kind::Centrifuge),
        "mekanismgenerators:fission_reactor_port" => return Some(Kind::FissionPort),
        TURBINE_VALVE => return Some(Kind::TurbineValve),
        TURBINE_ROTOR => return Some(Kind::TurbineRotor),
        OBSIDIAN_TNT => return Some(Kind::ObsidianTnt),
        fusion::PORT => return Some(Kind::FusionPort),
        sps::PORT => return Some(Kind::SpsPort),
        dynamic_tank::VALVE => return Some(Kind::DynamicValve),
        "mekanism:electric_pump" => return Some(Kind::Pump),
        "mekanism:fluidic_plenisher" => return Some(Kind::Plenisher),
        "mekanism:logistical_sorter" => return Some(Kind::Sorter),
        od::BLOCK => return Some(Kind::Oredictionificator),
        chargepad::CHARGEPAD => return Some(Kind::Chargepad),
        qe::ENTANGLOPORTER => return Some(Kind::Entangloporter),
        "mekanism:solar_neutron_activator" => return Some(Kind::Activator),
        boiler::VALVE => return Some(Kind::BoilerValve),
        boiler::HEATER => return Some(Kind::Heater),
        boiler::FUELWOOD_HEATER => return Some(Kind::FuelwoodHeater),
        REACTION_CHAMBER => return Some(Kind::Reaction),
        mekanism_core::generator::BIO_GENERATOR => return Some(Kind::BioGenerator),
        "mekanism:chemical_washer" => return Some(Kind::Washer),
        "mekanism:chemical_crystallizer" => return Some(Kind::Crystallizer),
        _ => {}
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_universal_cable")) {
        let tier = BaseTier::ALL.into_iter().find(|t| t.name() == tier)?;
        return tier::universal_cable(tier).map(Kind::Cable);
    }
    if let Some(m) = chem::ADVANCED_MACHINES.iter().find(|m| m.block == id) {
        return Some(Kind::Advanced(m));
    }
    if id == "mekanism:restrictive_transporter" || id == "mekanism:diversion_transporter" {
        return tier::transporter_pull(BaseTier::Basic).map(Kind::Transporter);
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_logistical_transporter")) {
        let tier = BaseTier::ALL.into_iter().find(|t| t.name() == tier)?;
        return tier::transporter_pull(tier).map(Kind::Transporter);
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_thermodynamic_conductor")) {
        let tier = BaseTier::ALL.into_iter().find(|t| t.name() == tier)?;
        return tier::conductor(tier).map(|_| Kind::Conductor(tier));
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_pressurized_tube")) {
        let tier = BaseTier::ALL.into_iter().find(|t| t.name() == tier)?;
        return tier::pressurized_tube(tier).map(Kind::Tube);
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_fluid_tank")) {
        return BaseTier::ALL.into_iter().find(|t| t.name() == tier).map(Kind::FluidTank);
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_mechanical_pipe")) {
        let tier = BaseTier::ALL.into_iter().find(|t| t.name() == tier)?;
        return tier::mechanical_pipe(tier).map(Kind::Pipe);
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_chemical_tank")) {
        return BaseTier::ALL.into_iter().find(|t| t.name() == tier).map(Kind::ChemicalTank);
    }
    let tier = id.strip_prefix("mekanism:")?.strip_suffix("_energy_cube")?;
    BaseTier::ALL
        .into_iter()
        .find(|t| t.name() == tier && *t != BaseTier::Creative)
        .map(Kind::EnergyCube)
}

const SPRITES: &str = "mekanism:gui/sprites/";
/// Mekanism's own GUI textures outside `sprites/` (the gauges' fluid textures the content build copies).
const SPRITES_ROOT: &str = "mekanism:gui/";

/// What a gauge fills with (`GuiTankGauge`): a fluid's still texture tinted with its colour (water's
/// blue; lava as it is), and a chemical's icon (`mek_liquid/liquid`, steam's own) tinted with its
/// chemical's colour; grey for anything unknown.
fn gauge_texture(chemical: Option<&(&str, &str, u32)>) -> (String, u32) {
    match chemical {
        Some(("minecraft:water", ..)) => ("block/water_still".into(), 0xFF3F_76E4),
        Some(("minecraft:lava", ..)) => ("block/lava_still".into(), 0xFFFF_FFFF),
        Some(("mekanism:steam", ..)) => (format!("{SPRITES_ROOT}gauge_fluid/steam"), 0xFFFF_FFFF),
        Some((_, _, rgb)) => (format!("{SPRITES_ROOT}gauge_fluid/liquid"), 0xFF00_0000 | rgb),
        None => (format!("{SPRITES_ROOT}gauge_fluid/liquid"), 0xFF8A_8A8A),
    }
}

/// A texture tiled 16 px at a time over `rect` from its bottom up, the top tile cut short
/// (`MekanismRenderer.renderTiledSprite`); a flat colour when the texture is missing.
fn fill_tiled(game: &Gfx<'_>, batch: &mut Batch, texture: &str, rect: [f32; 4], argb: u32) {
    let [x, y, w, h] = rect;
    let mut drawn = true;
    let mut dy = 0.0;
    while dy < h {
        let th = (h - dy).min(16.0);
        let mut dx = 0.0;
        while dx < w {
            let tw = (w - dx).min(16.0);
            drawn &= game.native_sprite_tinted(batch, texture, [0.0, 16.0 - th, tw, th], [x + dx, y + h - dy - th, tw, th], argb);
            dx += 16.0;
        }
        dy += 16.0;
    }
    if !drawn {
        game.native_fill(batch, rect, argb);
    }
}
/// Where the data slots after the furnace's four begin: stored energy, capacity (4 pieces each), the
/// speed and energy upgrades, and the energy used a tick (4 pieces).
const DATA_STORED: usize = 4;
const DATA_CAPACITY: usize = 8;
const DATA_SPEED: usize = 12;
const DATA_ENERGY_UPGRADES: usize = 13;
const DATA_USAGE: usize = 14;

/// A 64-bit value as four 15-bit data slots, low first.
fn wide(v: u64) -> [i32; 4] {
    let v = v.min((1 << 60) - 1);
    [0, 1, 2, 3].map(|k| ((v >> (15 * k)) & 0x7FFF) as i32)
}

fn read_wide(data: &[i32], at: usize) -> u64 {
    (0..4).fold(0, |acc, k| acc | ((data.get(at + k).copied().unwrap_or(0) as u64 & 0x7FFF) << (15 * k)))
}

/// Where the gauges' data slots begin; each gauge takes its amount and capacity (4 pieces each) and
/// the index of its chemical in `CHEMICALS`.
const DATA_GAUGES: usize = 18;
const GAUGE_SLOTS: usize = 9;

/// The chemicals and fluids a gauge can show, with Mekanism's colours (`MekanismChemicals`'s tints,
/// the fluids' own); anything else shows grey.
const CHEMICALS: &[(&str, &str, u32)] = &[
    ("minecraft:water", "Water", 0x3F76E4),
    ("minecraft:lava", "Lava", 0xFF6A00),
    ("mekanism:brine", "Brine", 0xFEEF9C),
    ("mekanism:heavy_water", "Heavy Water", 0x2B4F84),
    ("mekanism:hydrogen", "Hydrogen", 0xFFFFFF),
    ("mekanism:oxygen", "Oxygen", 0x6CE2FF),
    ("mekanism:steam", "Steam", 0xC4C4C4),
    ("mekanism:nutritional_paste", "Nutritional Paste", 0xEB6CA3),
    ("mekanism:chlorine", "Chlorine", 0xCFE800),
    ("mekanism:sodium", "Sodium", 0xE9FEF4),
    ("mekanism:superheated_sodium", "Superheated Sodium", 0xD19469),
    ("mekanism:hydrogen_chloride", "Hydrogen Chloride", 0xA8F1E9),
    ("mekanism:sulfur_dioxide", "Sulfur Dioxide", 0xA99D90),
    ("mekanism:sulfur_trioxide", "Sulfur Trioxide", 0xCE6C6C),
    ("mekanism:sulfuric_acid", "Sulfuric Acid", 0x82802B),
    ("mekanism:ethene", "Ethylene", 0xEACCF9),
    ("mekanism:lithium", "Lithium", 0xEBA400),
    ("mekanism:uranium_oxide", "Uranium Oxide", 0xE1F573),
    ("mekanism:uranium_hexafluoride", "Uranium Hexafluoride", 0x809960),
    ("mekanism:fissile_fuel", "Fissile Fuel", 0x2E332F),
    ("mekanism:nuclear_waste", "Nuclear Waste", 0x4F412A),
    ("mekanism:plutonium", "Plutonium", 0x1F919C),
    ("mekanism:polonium", "Polonium", 0x1B9E7B),
    ("mekanism:antimatter", "Antimatter", 0xA464B3),
    ("mekanismgenerators:deuterium", "Deuterium", 0xFF3232),
    ("mekanismgenerators:tritium", "Tritium", 0x64FF70),
    ("mekanismgenerators:fusion_fuel", "D-T Fuel", 0x7E007D),
        ("mekanism:aqua", "Aqua", 0x30FFF9),
        ("mekanism:black", "Black", 0x404040),
        ("mekanism:blue", "Blue", 0x366BD0),
        ("mekanism:brown", "Brown", 0xA17649),
        ("mekanism:cyan", "Cyan", 0x00F3D0),
        ("mekanism:dark_red", "Dark Red", 0xC9071F),
        ("mekanism:gray", "Gray", 0x7A7A7A),
        ("mekanism:green", "Green", 0x59C15F),
        ("mekanism:light_blue", "Light Blue", 0x559EFF),
        ("mekanism:light_gray", "Light Gray", 0xCFCFCF),
        ("mekanism:lime", "Lime", 0x75FF89),
        ("mekanism:magenta", "Magenta", 0xD55ECB),
        ("mekanism:orange", "Orange", 0xFFA160),
        ("mekanism:pink", "Pink", 0xFFBCC4),
        ("mekanism:purple", "Purple", 0xA460D9),
        ("mekanism:red", "Red", 0xFF383C),
        ("mekanism:white", "White", 0xFFFFFF),
        ("mekanism:yellow", "Yellow", 0xFFDD4F),
];

fn chemical_index(id: Option<&str>) -> i32 {
    id.and_then(|id| CHEMICALS.iter().position(|c| c.0 == id)).map_or(-1, |i| i as i32)
}

/// A gauge on a screen: its tank's key in the block entity, the tank's capacity, where it is and how
/// tall (`GaugeType.STANDARD` 18×60, `SMALL` 18×30, `WIDE` 66×50).
#[derive(Clone, Copy)]
struct Gauge {
    key: &'static str,
    capacity: u64,
    at: [f32; 2],
    size: [f32; 2],
}

const STANDARD: [f32; 2] = [18.0, 60.0];
const SMALL: [f32; 2] = [18.0, 30.0];
/// `GaugeType.MEDIUM`: 32 x 58 inside its frame.
const MEDIUM: [f32; 2] = [34.0, 60.0];
const WIDE: [f32; 2] = [66.0, 50.0];
/// `GaugeType.SMALL_MED`: 16 x 46 inside the frame.
const SMALL_MED: [f32; 2] = [18.0, 48.0];

/// The power bar a screen has: vertical (6×54) or horizontal (54×6), at its Gui class's position.
#[derive(Clone, Copy)]
enum PowerBar {
    Vertical([f32; 2]),
    /// A vertical bar of its own height (`GuiVerticalPowerBar` with a height).
    Tall([f32; 2], f32),
    Horizontal([f32; 2]),
    /// The energy cube's wide energy gauge.
    Gauge([f32; 2]),
    /// A small-medium energy gauge (`GuiEnergyGauge` with `GaugeType.SMALL_MED`).
    SmallGauge([f32; 2]),
    /// An energy gauge of its own size (`GuiEnergyGauge` with a width and a height).
    Sized([f32; 2], [f32; 2]),
}

/// Everything a Mekanism screen draws for a block: its slots (None hides one the block does not use),
/// slot sprites, gauges, power bar and progress bar (`LARGE_RIGHT`, 48×8, or the machine's `BAR`).
struct Layout {
    slots: [Option<([f32; 2], &'static str)>; 3],
    gauges: Vec<Gauge>,
    power: Option<PowerBar>,
    progress: Option<([f32; 2], &'static str, [f32; 2])>,
}

fn layout(block: &str) -> Option<Layout> {
    let gauge = |key, capacity, x, y, size| Gauge { key, capacity, at: [x, y], size };
    let large_right = |x, y| Some(([x, y], "progress/large_right", [48.0, 8.0]));
    Some(match kind(block)? {
        // GuiFactory: its slots and arrows are drawn by `factory_slots`; the power bar sits at the right.
        Kind::Factory(n, recipe, _) => Layout {
            slots: [None, None, None],
            // The item and chemical factories' `GuiChemicalBar` (7, 76), 138 wide (172 in the ultimate), 4 high.
            gauges: if factory::takes_chemical(recipe) {
                vec![gauge("chemical", factory::chemical_per_process(recipe) * n as u64, 7.0, 76.0, [if n == 9 { 172.0 } else { 138.0 }, 4.0])]
            } else {
                vec![]
            },
            // `GuiVerticalPowerBar(imageWidth - 12, 16, sawing ? 73 : 52)`.
            power: Some(PowerBar::Tall([screen_width(block) - 12.0, 16.0], if matches!(kind(block), Some(Kind::Factory(_, "sawing", _))) { 73.0 } else { 52.0 })),
            progress: None,
        },
        // The frequency screens (`draw_frequency_screen`): the teleporter's power bar.
        Kind::Teleporter => Layout { slots: [None, None, None], gauges: vec![], power: Some(PowerBar::Vertical([158.0, 26.0])), progress: None },
        Kind::Entangloporter => Layout { slots: [None, None, None], gauges: vec![], power: None, progress: None },
        // TileEntityOredictionificator and GuiOredictionificator.
        Kind::Oredictionificator => Layout {
            slots: [Some(([56.0, 115.0], "slot/normal")), None, Some(([164.0, 115.0], "slot/output"))],
            gauges: vec![],
            power: None,
            progress: large_right(94.0, 119.0),
        },
        // TileEntityElectricMachine and GuiElectricMachine.
        // GuiPrecisionSawmill: the input and energy slots 8 px further left, the progress bar at (78, 38).
        Kind::Machine("sawing", _) => Layout {
            slots: [Some(([56.0, 17.0], "slot/normal")), Some(([56.0, 53.0], "slot/power")), Some(([116.0, 35.0], "slot/output"))],
            gauges: vec![],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: Some(([78.0, 38.0], "progress/bar", [25.0, 9.0])),
        },
        // GuiElectricMachine's power bar is at y 16; the combiner's (GuiCombiner) at 15.
        Kind::Machine(recipe, _) => Layout {
            slots: [
                Some(([64.0, 17.0], "slot/normal")),
                Some(([64.0, 53.0], if recipe == "combining" { "slot/normal" } else { "slot/power" })),
                Some(([116.0, 35.0], "slot/output")),
            ],
            gauges: vec![],
            power: Some(PowerBar::Vertical([164.0, if recipe == "combining" { 15.0 } else { 16.0 }])),
            progress: Some(([86.0, 38.0], "progress/bar", [25.0, 9.0])),
        },
        // TileEntityChemicalOxidizer and GuiChemicalOxidizer.
        Kind::Oxidizer => Layout {
            slots: [Some(([26.0, 36.0], "slot/normal")), Some(([152.0, 14.0], "slot/power")), None],
            gauges: vec![gauge("output", chem::OXIDIZER_TANK, 131.0, 13.0, STANDARD)],
            power: Some(PowerBar::Horizontal([115.0, 75.0])),
            progress: large_right(64.0, 40.0),
        },
        // TileEntityPigmentExtractor and GuiPigmentExtractor: the oxidizer's screen.
        Kind::PigmentExtractor => Layout {
            slots: [Some(([26.0, 36.0], "slot/normal")), Some(([152.0, 14.0], "slot/power")), None],
            gauges: vec![gauge("output", chem::PIGMENT_EXTRACTOR_TANK, 131.0, 13.0, STANDARD)],
            power: Some(PowerBar::Horizontal([115.0, 75.0])),
            progress: large_right(64.0, 40.0),
        },
        // TileEntityDimensionalStabilizer and GuiDimensionalStabilizer: the energy slot and the power bar.
        Kind::Stabilizer => Layout {
            slots: [Some(([143.0, 35.0], "slot/power")), None, None],
            gauges: vec![],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: None,
        },
        // TileEntityModificationStation and GuiModificationStation.
        Kind::ModificationStation => Layout {
            slots: [Some(([35.0, 118.0], "slot/normal")), Some(([125.0, 118.0], "slot/normal")), Some(([151.0, 21.0], "slot/power"))],
            gauges: vec![],
            power: Some(PowerBar::Vertical([156.0, 40.0])),
            progress: Some(([65.0, 123.0], "progress/large_right", [48.0, 8.0])),
        },
        // TileEntitySecurityDesk and GuiSecurityDesk: the unlock slot above the lock slot, on their holder.
        Kind::SecurityDesk => Layout {
            slots: [Some(([146.0, 18.0], "slot/inner_holder_slot")), Some(([146.0, 97.0], "slot/inner_holder_slot")), None],
            gauges: vec![],
            power: None,
            progress: None,
        },
        // TileEntityFormulaicAssemblicator and GuiFormulaicAssemblicator: the formula slot and the first two stock
        // slots; the rest are `extra_slots`.
        Kind::Assemblicator => Layout {
            slots: [Some(([6.0, 26.0], "slot/normal")), Some(([8.0, 98.0], "slot/normal")), Some(([26.0, 98.0], "slot/normal"))],
            gauges: vec![],
            power: Some(PowerBar::Vertical([159.0, 15.0])),
            progress: Some(([86.0, 43.0], "progress/tall_right", [22.0, 15.0])),
        },
        // TileEntityNutritionalLiquifier and GuiNutritionalLiquifier.
        Kind::Liquifier => Layout {
            slots: [Some(([26.0, 36.0], "slot/normal")), Some(([155.0, 5.0], "slot/power")), Some(([110.0, 36.0], "slot/output"))],
            gauges: vec![gauge("output", chem::LIQUIFIER_TANK, 133.0, 13.0, STANDARD)],
            power: Some(PowerBar::Horizontal([115.0, 75.0])),
            progress: large_right(54.0, 40.0),
        },
        // TileEntityChemicalDissolutionChamber and its screen.
        Kind::Dissolution => Layout {
            slots: [Some(([28.0, 36.0], "slot/normal")), Some(([152.0, 14.0], "slot/power")), None],
            gauges: vec![
                gauge("input", nuclear::DISSOLUTION_TANK, 7.0, 4.0, STANDARD),
                gauge("output", nuclear::DISSOLUTION_TANK, 131.0, 13.0, STANDARD),
            ],
            power: Some(PowerBar::Horizontal([115.0, 75.0])),
            progress: large_right(64.0, 40.0),
        },
        // TileEntityChemicalCrystallizer and its screen.
        Kind::Crystallizer => Layout {
            slots: [None, Some(([152.0, 5.0], "slot/power")), Some(([129.0, 57.0], "slot/output"))],
            gauges: vec![gauge("input", nuclear::CRYSTALLIZER_TANK, 7.0, 4.0, STANDARD)],
            power: Some(PowerBar::Vertical([157.0, 23.0])),
            progress: large_right(53.0, 61.0),
        },
        // TileEntityElectrolyticSeparator and its screen; the empty buckets come out below the full one.
        Kind::Separator => Layout {
            slots: [Some(([26.0, 35.0], "slot/normal")), Some(([143.0, 35.0], "slot/power")), None],
            gauges: vec![
                gauge("fluid", chem::SEPARATOR_FLUID, 5.0, 10.0, STANDARD),
                gauge("left", chem::SEPARATOR_GAS, 58.0, 18.0, SMALL),
                gauge("right", chem::SEPARATOR_GAS, 100.0, 18.0, SMALL),
            ],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            // `GuiProgress(tile::getActive, ProgressType.BI, 80, 30)`.
            progress: Some(([80.0, 30.0], "progress/bidirectional", [16.0, 6.0])),
        },
        // TileEntityEnergyCube: an item there charges from the cube on the right (our slot 0) and
        // gives the cube its energy on the left (our slot 1).
        Kind::EnergyCube(_) => Layout {
            slots: [Some(([143.0, 35.0], "slot/power")), Some(([17.0, 35.0], "slot/power")), None],
            gauges: vec![],
            power: Some(PowerBar::Gauge([55.0, 18.0])),
            progress: None,
        },
        // TileEntityHeatGenerator and its screen: fuel (17, 35), charge slot (143, 35), the lava gauge.
        Kind::HeatGenerator => Layout {
            slots: [Some(([143.0, 35.0], "slot/power")), Some(([17.0, 35.0], "slot/normal")), None],
            gauges: vec![gauge("lava", u64::from(mekanism_core::generator::TANK_CAPACITY), 55.0, 18.0, WIDE)],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: None,
        },
        // The bio-generator's fuel slot and bioethanol gauge (GuiBioGenerator).
        Kind::BioGenerator => Layout {
            slots: [Some(([143.0, 35.0], "slot/power")), Some(([17.0, 35.0], "slot/normal")), None],
            gauges: vec![gauge("fuel", mekanism_core::generator::BIO_TANK, 7.0, 15.0, [4.0, 52.0])],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: None,
        },
        // The gas-burning generator's fuel gauge (GuiGasGenerator).
        Kind::GasGenerator => Layout {
            slots: [Some(([143.0, 35.0], "slot/power")), Some(([17.0, 35.0], "slot/normal")), None],
            gauges: vec![gauge("fuel", chem::GAS_GENERATOR_TANK, 55.0, 18.0, WIDE)],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: None,
        },
        // The wind and advanced solar generators: a charge slot and the power bar.
        Kind::Wind | Kind::Solar(..) => Layout {
            slots: [Some(([143.0, 35.0], "slot/power")), None, None],
            gauges: vec![],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: None,
        },
        // TileEntityPaintingMachine and GuiPaintingMachine: pigment slot (6, 56), item (45, 35), output (116, 35).
        Kind::Painting => Layout {
            slots: [Some(([45.0, 35.0], "slot/normal")), Some(([6.0, 56.0], "slot/normal")), Some(([116.0, 35.0], "slot/output"))],
            gauges: vec![gauge("pigment", chem::PAINTING_TANK, 25.0, 13.0, STANDARD)],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: large_right(64.0, 39.0),
        },
        // TileEntityMetallurgicInfuser and its screen: the infuse item (17, 35), input (51, 43), output
        // (109, 43), the infusion bar (7, 15).
        Kind::Infuser => Layout {
            slots: [
                Some(([51.0, 43.0], "slot/normal")),
                Some(([17.0, 35.0], "slot/normal")),
                Some(([109.0, 43.0], "slot/output")),
            ],
            gauges: vec![gauge("infuse", chem::INFUSER_TANK, 7.0, 15.0, [4.0, 52.0])],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: Some(([72.0, 47.0], "progress/right", [32.0, 8.0])),
        },
        // TileEntityAdvancedElectricMachine and its screen: the chemical bar (68, 36) between the input
        // and the chemical item.
        Kind::Advanced(_) => Layout {
            slots: [
                Some(([64.0, 17.0], "slot/normal")),
                Some(([64.0, 53.0], "slot/normal")),
                Some(([116.0, 35.0], "slot/output")),
            ],
            gauges: vec![gauge("chemical", chem::ADVANCED_TANK, 68.0, 36.0, [6.0, 12.0])],
            power: Some(PowerBar::Vertical([164.0, 16.0])),
            progress: Some(([86.0, 38.0], "progress/bar", [25.0, 9.0])),
        },
        // TileEntityRotaryCondensentrator and its screen: buckets on the fluid side (155, 25) and
        // (155, 56), the chemical gauge (25, 13) and the fluid gauge (133, 13).
        Kind::Rotary => Layout {
            slots: [Some(([155.0, 25.0], "slot/normal")), Some(([155.0, 5.0], "slot/power")), Some(([155.0, 56.0], "slot/output"))],
            gauges: vec![
                gauge("chemical", chem::ROTARY_TANK, 25.0, 13.0, STANDARD),
                gauge("fluid", chem::ROTARY_TANK, 133.0, 13.0, STANDARD),
            ],
            power: Some(PowerBar::Horizontal([115.0, 75.0])),
            progress: None,
        },
        // TileEntityThermalEvaporationController and its screen: water buckets in at (28, 20), an empty
        // bucket for brine at (152, 20) and the buckets out at (152, 51); the input and output gauges.
        // The water gauge's capacity follows the tower's height (screen_data).
        Kind::Evaporation => Layout {
            slots: [
                Some(([28.0, 20.0], "slot/normal")),
                Some(([152.0, 20.0], "slot/normal")),
                Some(([152.0, 51.0], "slot/output")),
            ],
            gauges: vec![
                gauge("water", evap::input_capacity(0), 6.0, 13.0, STANDARD),
                gauge("brine", evap::OUTPUT_TANK, 172.0, 13.0, STANDARD),
            ],
            power: None,
            progress: None,
        },
        // A tank's one big gauge.
        Kind::ChemicalTank(t) => Layout {
            // `drainSlot` (16, 16) and `fillSlot` (16, 48).
            slots: [Some(([16.0, 16.0], "slot/output")), Some(([16.0, 48.0], "slot/normal")), None],
            // `GuiChemicalBar`: a horizontal bar of 116 x 10 (drawn by `draw_tank_screen`).
            gauges: vec![gauge("chemical", chem::chemical_tank(t).0, 42.0, 16.0, [116.0, 10.0])],
            power: None,
            progress: None,
        },
        Kind::FluidTank(t) => Layout {
            slots: [Some(([146.0, 19.0], "slot/normal")), Some(([146.0, 51.0], "slot/output")), None],
            // `GuiFluidGauge(.., GaugeType.WIDE, this, 48, 18)`.
            gauges: vec![gauge("fluid", tier::fluid_tank(t).0, 48.0, 18.0, WIDE)],
            power: None,
            progress: None,
        },
        // The multiblocks show their master's tanks (capacities from its size, in screen_data), their
        // state as text and, for the reactors, two buttons.
        Kind::FissionPort => Layout {
            slots: [None; 3],
            gauges: vec![
                gauge("fuel", 0, 25.0, 13.0, STANDARD),
                gauge("water", 0, 6.0, 13.0, STANDARD),
                gauge("steam", 0, 152.0, 13.0, STANDARD),
                gauge("waste", 0, 171.0, 13.0, STANDARD),
                // The numbers the screens show, packed (never drawn as gauges): see `multiblock_data`.
                gauge("stats_a", 1 << 58, 0.0, 0.0, SMALL),
                gauge("stats_b", 1 << 58, 0.0, 0.0, SMALL),
                gauge("stats_c", 1 << 58, 0.0, 0.0, SMALL),
            ],
            power: None,
            progress: None,
        },
        Kind::FusionPort => Layout {
            slots: [None; 3],
            gauges: vec![
                gauge("deuterium", fusion::FUEL_CAPACITY, 7.0, 13.0, STANDARD),
                gauge("tritium", fusion::FUEL_CAPACITY, 27.0, 13.0, STANDARD),
                gauge("water", 0, 131.0, 13.0, STANDARD),
                gauge("steam", 0, 151.0, 13.0, STANDARD),
                // The D-T fuel, and the plasma's and the case's temperatures in kelvin (a `GuiNumberGauge`).
                gauge("fuel", fusion::FUEL_CAPACITY, 84.0, 50.0, STANDARD),
                gauge("plasma", FUSION_MAX_TEMP, 12.0, 50.0, STANDARD),
                gauge("case", FUSION_MAX_TEMP, 66.0, 50.0, STANDARD),
                // Packed numbers for the screens (never drawn): the heat given to water in the amount and to
                // the air in the capacity (thousandths of a kelvin), the larger of the injection rate and the
                // last burn.
                gauge("stats_a", 1 << 58, 0.0, 0.0, SMALL),
                gauge("stats_b", 1 << 58, 0.0, 0.0, SMALL),
            ],
            power: None,
            progress: None,
        },
        Kind::BoilerValve => Layout {
            slots: [None; 3],
            // `GuiThermoelectricBoiler`: the heated coolant and water tanks on the left, the steam and cooled
            // coolant tanks on the right.
            gauges: vec![
                gauge("heated_coolant", 0, 6.0, 13.0, STANDARD),
                gauge("water", 0, 26.0, 13.0, STANDARD),
                gauge("steam", 0, 174.0, 13.0, STANDARD),
                gauge("cooled_coolant", 0, 194.0, 13.0, STANDARD),
            ],
            power: None,
            progress: None,
        },
        // `GuiFissionReactorLogicAdapter` / `GuiFusionReactorLogicAdapter`: no slots, gauges or bars of their own.
        Kind::FissionLogic | Kind::FusionLogic => Layout { slots: [None; 3], gauges: vec![], power: None, progress: None },
        Kind::TurbineValve => Layout {
            slots: [None; 3],
            // `GuiIndustrialTurbine`: a medium steam gauge, the energy bar at the right edge.
            // The two "stats" gauges carry the counts the statistics screen shows (never drawn as gauges).
            gauges: vec![
                gauge("steam", 0, 6.0, 13.0, MEDIUM),
                gauge("stats_a", 1 << 58, 0.0, 0.0, SMALL),
                gauge("stats_b", 1 << 58, 0.0, 0.0, SMALL),
                gauge("stats_c", 1 << 58, 0.0, 0.0, SMALL),
            ],
            power: Some(PowerBar::Vertical([178.0, 16.0])),
            progress: None,
        },
        // GuiDynamicTank: one gauge for the fluid or chemical.
        // GuiAntiprotonicNucleosynthesizer, on the advanced machines' panel: the item, the antimatter
        // pellets below it, the output, and the antimatter gauge on the left.
        // GuiPRC: the item (54, 40), energy (141, 22) and output (116, 40) slots; the fluid, chemical and
        // output gauges.
        Kind::Reaction => Layout {
            slots: [
                Some(([54.0, 40.0], "slot/normal")),
                Some(([141.0, 22.0], "slot/power")),
                Some(([116.0, 40.0], "slot/output")),
            ],
            gauges: vec![
                gauge("fluid", REACTION_TANK, 5.0, 15.0, STANDARD),
                gauge("input", REACTION_TANK, 28.0, 15.0, STANDARD),
                gauge("output", REACTION_TANK, 140.0, 45.0, SMALL),
            ],
            power: Some(PowerBar::Vertical([163.0, 21.0])),
            progress: Some(([77.0, 43.0], "progress/bar", [25.0, 9.0])),
        },
        Kind::Nucleosynthesizer => Layout {
            slots: [
                Some(([26.0, 40.0], "slot/normal")),
                Some(([6.0, 69.0], "slot/normal")),
                Some(([152.0, 40.0], "slot/output")),
            ],
            gauges: vec![gauge("chemical", chem::NUCLEOSYNTHESIZER_TANK, 5.0, 18.0, SMALL_MED)],
            power: Some(PowerBar::SmallGauge([172.0, 18.0])),
            progress: None,
        },
        // GuiDynamicTank: the two container slots in their holder, the merged tank's medium gauge.
        Kind::DynamicValve => Layout {
            slots: [Some(([146.0, 21.0], "slot/inner_holder_slot")), Some(([146.0, 51.0], "slot/inner_holder_slot")), None],
            gauges: vec![gauge("fluid", 0, 7.0, 16.0, [34.0, 56.0])],
            power: None,
            progress: None,
        },
        // GuiSPS: the polonium gauge on the left, the antimatter gauge on the right.
        Kind::SpsPort => Layout {
            slots: [None; 3],
            gauges: vec![
                gauge("input", sps::input_capacity(), 7.0, 17.0, STANDARD),
                gauge("output", sps::output_capacity(), 151.0, 17.0, STANDARD),
                // `lastProcessed` in thousandths of a mB (never drawn).
                gauge("stats_a", 1 << 58, 0.0, 0.0, SMALL),
            ],
            power: None,
            progress: None,
        },
        // GuiSeismicVibrator: the vertical power bar (164, 15) and the energy slot (143, 35).
        Kind::Vibrator => Layout {
            slots: [Some(([143.0, 35.0], "slot/power")), None, None],
            gauges: vec![],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: None,
        },
        // `GuiInductionMatrix`: the medium energy gauge, and the two slots in the element holder (the stats
        // gauges carry the last input and output and the make-up of the matrix; they are never drawn).
        Kind::InductionPort => Layout {
            slots: [Some(([146.0, 21.0], "slot/inner_holder_slot")), Some(([146.0, 51.0], "slot/inner_holder_slot")), None],
            gauges: vec![gauge("stats_a", 1 << 58, 0.0, 0.0, SMALL), gauge("stats_b", 1 << 58, 0.0, 0.0, SMALL)],
            power: Some(PowerBar::Sized([7.0, 16.0], [34.0, 56.0])),
            progress: None,
        },
        Kind::Laser => Layout {
            slots: [None; 3],
            gauges: vec![],
            power: Some(PowerBar::Gauge([55.0, 18.0])),
            progress: None,
        },
        // GuiLaserTractorBeam: no elements but its 27 slots (`dynamicSlots`) and the security tab.
        Kind::TractorBeam | Kind::PersonalStorage => Layout { slots: [None; 3], gauges: vec![], power: None, progress: None },
        // GuiLaserAmplifier: the standard energy gauge at (6, 10); the thresholds ride in a never-drawn gauge
        // (the lower one in the amount, the upper one in the capacity).
        Kind::LaserAmplifier => Layout {
            slots: [None; 3],
            gauges: vec![gauge("stats_a", 1 << 58, 0.0, 0.0, SMALL)],
            power: Some(PowerBar::Sized([6.0, 10.0], STANDARD)),
            progress: None,
        },
        // TileEntityDigitalMiner and its screen: the energy slot (152, 20) and the power bar.
        // GuiSolarNeutronActivator: no energy, lit while the sun lets it work.
        Kind::Activator => Layout {
            slots: [None, Some(([5.0, 56.0], "slot/normal")), Some(([155.0, 56.0], "slot/normal"))],
            gauges: vec![gauge("input", isotopes::ACTIVATOR_TANK, 25.0, 13.0, STANDARD), gauge("output", isotopes::ACTIVATOR_TANK, 133.0, 13.0, STANDARD)],
            power: None,
            progress: None,
        },
        // GuiIsotopicCentrifuge.
        Kind::Centrifuge => Layout {
            slots: [Some(([155.0, 14.0], "slot/power")), Some(([5.0, 56.0], "slot/normal")), Some(([155.0, 56.0], "slot/normal"))],
            gauges: vec![gauge("input", nuclear::CENTRIFUGE_TANK, 25.0, 13.0, STANDARD), gauge("output", nuclear::CENTRIFUGE_TANK, 133.0, 13.0, STANDARD)],
            power: Some(PowerBar::Horizontal([115.0, 75.0])),
            progress: None,
        },
        // GuiChemicalInfuser: the left and right inputs, the output in the middle, 9 px higher.
        Kind::ChemicalInfuser => Layout {
            slots: [Some(([154.0, 14.0], "slot/power")), Some(([6.0, 56.0], "slot/normal")), Some(([154.0, 56.0], "slot/normal"))],
            gauges: vec![
                gauge("left", chem::CHEMICAL_INFUSER_TANK, 25.0, 13.0, STANDARD),
                gauge("output", chem::CHEMICAL_INFUSER_TANK, 79.0, 4.0, STANDARD),
                gauge("right", chem::CHEMICAL_INFUSER_TANK, 133.0, 13.0, STANDARD),
            ],
            power: Some(PowerBar::Horizontal([115.0, 75.0])),
            progress: None,
        },
        // GuiPigmentMixer: the chemical infuser's screen with 1,000 mB inputs and a 2,000 mB output.
        Kind::PigmentMixer => Layout {
            slots: [Some(([154.0, 14.0], "slot/power")), Some(([6.0, 56.0], "slot/normal")), Some(([154.0, 56.0], "slot/normal"))],
            gauges: vec![
                gauge("left", chem::PIGMENT_MIXER_TANK, 25.0, 13.0, STANDARD),
                gauge("output", chem::PIGMENT_MIXER_TANK * 2, 79.0, 4.0, STANDARD),
                gauge("right", chem::PIGMENT_MIXER_TANK, 133.0, 13.0, STANDARD),
            ],
            power: Some(PowerBar::Horizontal([115.0, 75.0])),
            progress: None,
        },
        // GuiChemicalWasher: water, dirty slurry, clean slurry.
        Kind::Washer => Layout {
            slots: [Some(([152.0, 14.0], "slot/power")), Some(([152.0, 56.0], "slot/normal")), None],
            gauges: vec![
                gauge("fluid", nuclear::WASHER_TANK, 7.0, 13.0, STANDARD),
                gauge("input", nuclear::WASHER_TANK, 28.0, 13.0, STANDARD),
                gauge("output", nuclear::WASHER_TANK, 131.0, 13.0, STANDARD),
            ],
            power: Some(PowerBar::Horizontal([115.0, 75.0])),
            progress: None,
        },
        // GuiElectricPump and GuiFluidicPlenisher: the tank on the left, a screen in the middle.
        Kind::Pump => Layout {
            slots: [Some(([28.0, 20.0], "slot/normal")), Some(([28.0, 51.0], "slot/output")), Some(([143.0, 35.0], "slot/power"))],
            gauges: vec![gauge("fluid", isotopes::PUMP_TANK, 6.0, 13.0, STANDARD)],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: None,
        },
        Kind::Plenisher => Layout {
            slots: [Some(([28.0, 20.0], "slot/normal")), Some(([28.0, 51.0], "slot/output")), Some(([143.0, 35.0], "slot/power"))],
            gauges: vec![gauge("fluid", plenisher::PLENISHER_TANK, 6.0, 13.0, STANDARD)],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: None,
        },
        // GuiResistiveHeater: the screen with its temperature and usage, the power bar on the right.
        Kind::Heater => Layout { slots: [Some(([15.0, 35.0], "slot/power")), None, None], gauges: vec![], power: Some(PowerBar::Vertical([164.0, 15.0])), progress: None },
        // GuiFuelwoodHeater: its fuel slot, the screen, the flame.
        Kind::FuelwoodHeater => Layout { slots: [Some(([15.0, 29.0], "slot/normal")), None, None], gauges: vec![], power: None, progress: None },
        Kind::DigitalMiner => Layout {
            slots: [None, Some(([152.0, 20.0], "slot/power")), None],
            gauges: vec![],
            power: Some(PowerBar::Vertical([157.0, 39.0])),
            progress: None,
        },
        _ => return None,
    })
}

/// The energy a block's screen shows the capacity of.

fn screen_height(block: &str) -> f32 {
    match kind(block) {
        // `DEFAULT_IMAGE_HEIGHT + 11` for the secondary resource bar.
        Some(Kind::Factory(_, recipe, _)) if factory::takes_chemical(recipe) => 177.0,
        // `DEFAULT_IMAGE_HEIGHT + 21` for the secondary output row.
        Some(Kind::Factory(_, "sawing", _)) => 187.0,
        Some(Kind::Nucleosynthesizer) => 193.0,
        // `DEFAULT_IMAGE_HEIGHT + 5`.
        Some(Kind::Reaction) => 171.0,
        // `DEFAULT_IMAGE_HEIGHT + 16`.
        Some(Kind::SpsPort) => 182.0,
        // `DEFAULT_IMAGE_HEIGHT + 89`.
        Some(Kind::FissionPort) => 255.0,
        // `DEFAULT_IMAGE_HEIGHT + 76`.
        Some(Kind::DigitalMiner) => 242.0,
        // `DEFAULT_IMAGE_HEIGHT + 56`.
        Some(Kind::PersonalStorage) => 222.0,
        // `DEFAULT_IMAGE_HEIGHT + 64`.
        Some(Kind::Assemblicator) => 230.0,
        // `DEFAULT_IMAGE_HEIGHT + 64`.
        Some(Kind::Oredictionificator | Kind::SecurityDesk | Kind::ModificationStation) => 230.0,
        _ => 166.0,
    }
}

/// The item slots past the three of `Layout` (a hopper menu's fourth): the energy slot of the machines
/// that have more than an input, an output and one more slot (`EnergyInventorySlot.fillOrConvert`).
fn extra_slots(block: &str) -> Vec<([f32; 2], &'static str)> {
    match kind(block) {
        Some(Kind::Machine("combining", _)) | Some(Kind::Advanced(_)) => vec![([39.0, 35.0], "slot/power")],
        // The sawmill's secondary output slot (`TileEntityPrecisionSawmill`).
        Some(Kind::Machine("sawing", _)) => vec![([132.0, 35.0], "slot/output")],
        Some(Kind::Infuser) => vec![([143.0, 35.0], "slot/power")],
        Some(Kind::Painting) => vec![([144.0, 35.0], "slot/power")],
        Some(Kind::Nucleosynthesizer) => vec![([173.0, 69.0], "slot/power")],
        // The bucket slot under the water slot (`outputInputSlot`).
        Some(Kind::Evaporation) => vec![([28.0, 51.0], "slot/output")],
        // `FusionReactorMultiblockData.reactorSlot`: the hohlraum that starts the reaction.
        Some(Kind::FusionPort) => vec![([85.0, 39.0], "slot/normal")],
        // `ChemicalInventorySlot`s: the output's drain slot, the input's fill slot (`TileEntityChemical*`).
        Some(Kind::Oxidizer | Kind::PigmentExtractor) => vec![([152.0, 55.0], "slot/normal")],
        // `FluidInventorySlot.drain` (the bucket to fill) and the filled container's output slot.
        Some(Kind::Liquifier) => vec![([155.0, 25.0], "slot/normal"), ([155.0, 56.0], "slot/output")],
        Some(Kind::Assemblicator) => (2..fa::INPUTS)
            .map(|i| ([8.0 + 18.0 * (i % 9) as f32, 98.0 + 18.0 * (i / 9) as f32], "slot/normal"))
            .chain((0..fa::GRID).map(|i| ([26.0 + 18.0 * (i % 3) as f32, 17.0 + 18.0 * (i / 3) as f32], "slot/normal")))
            .chain((0..fa::OUTPUTS).map(|i| ([116.0 + 18.0 * (i % 2) as f32, 17.0 + 18.0 * (i / 2) as f32], "slot/output")))
            .chain([([152.0, 76.0], "slot/power")])
            .collect(),
        Some(Kind::Dissolution) => vec![([8.0, 65.0], "slot/normal"), ([152.0, 55.0], "slot/normal")],
        Some(Kind::Crystallizer) => vec![([8.0, 65.0], "slot/normal")],
        Some(Kind::ChemicalInfuser | Kind::PigmentMixer) => vec![([80.0, 65.0], "slot/normal")],
        Some(Kind::Separator) => vec![([59.0, 52.0], "slot/normal"), ([101.0, 52.0], "slot/normal")],
        // The chemical side's drain slot and the output it moves a full item to.
        Some(Kind::Rotary) => vec![([5.0, 25.0], "slot/normal"), ([5.0, 56.0], "slot/output")],
        // The washer's fluid slots on the holder at the right: the bucket to fill the water tank and its empty one.
        Some(Kind::Washer) => vec![([180.0, 71.0], "slot/normal"), ([180.0, 102.0], "slot/output")],
        // The miner's 27 internal slots (`BasicInventorySlot.at(8 + 18x, 92 + 18y)`).
        Some(Kind::DigitalMiner) => (0..MINER_SLOTS).map(|i| ([8.0 + 18.0 * (i % 9) as f32, 92.0 + 18.0 * (i / 9) as f32], "slot/normal")).collect(),
        _ => vec![],
    }
}

/// A factory screen's width: 34 wider for the ultimate tier's nine processes.
fn screen_width(block: &str) -> f32 {
    match kind(block) {
        Some(Kind::Factory(9, ..)) => 210.0,
        // `DEFAULT_IMAGE_WIDTH + 20`.
        Some(Kind::Nucleosynthesizer | Kind::Evaporation) => 196.0,
        // `DEFAULT_IMAGE_WIDTH + 42`.
        Some(Kind::BoilerValve) => 218.0,
        // `DEFAULT_IMAGE_WIDTH + 14`.
        Some(Kind::TurbineValve) => 190.0,
        // `DEFAULT_IMAGE_WIDTH + 19`.
        Some(Kind::FissionPort) => 195.0,
        // `DEFAULT_IMAGE_WIDTH + 10`.
        Some(Kind::FusionPort) => 186.0,
        // `DEFAULT_IMAGE_WIDTH + 20`.
        Some(Kind::FissionLogic | Kind::FusionLogic) => 196.0,
        // `DEFAULT_IMAGE_WIDTH + 60`.
        Some(Kind::Oredictionificator) => 236.0,
        _ => 176.0,
    }
}

/// Each process's input and output slot (`TileEntityItemToItemFactory.addSlots`).
fn factory_slots(n: usize) -> Vec<([f32; 2], [f32; 2])> {
    (0..n).map(|i| factory::slot_x(n, i) as f32).map(|x| ([x, 13.0], [x, 57.0])).collect()
}

/// A factory's slot frames and the arrows between each process's two slots.
fn draw_factory_slots(game: &Gfx<'_>, batch: &mut Batch, _block: &str, n: usize, l: &Layout, data: &[i32], origin: [f32; 2]) {
    let [ox, oy] = origin;
    let first = redstone_slot(l) + 5;
    sprite(game, batch, "slot/power", [0.0, 0.0, 18.0, 18.0], [ox + 6.0, oy + 12.0, 18.0, 18.0]);
    let kind_of = kind(_block);
    if matches!(kind_of, Some(Kind::Factory(_, recipe, _)) if factory::has_extra_slot(recipe)) {
        sprite(game, batch, "slot/extra", [0.0, 0.0, 18.0, 18.0], [ox + 6.0, oy + 56.0, 18.0, 18.0]);
    }
    let sawing = matches!(kind_of, Some(Kind::Factory(_, "sawing", _)));
    for (i, (input, output)) in factory_slots(n).into_iter().enumerate() {
        sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + input[0] - 1.0, oy + input[1] - 1.0, 18.0, 18.0]);
        sprite(game, batch, "slot/output", [0.0, 0.0, 18.0, 18.0], [ox + output[0] - 1.0, oy + output[1] - 1.0, 18.0, 18.0]);
        if sawing {
            sprite(game, batch, "slot/output", [0.0, 0.0, 18.0, 18.0], [ox + output[0] - 1.0, oy + output[1] + 20.0 - 1.0, 18.0, 18.0]);
        }
        let (x, y) = (ox + input[0] + 4.0, oy + 33.0);
        sprite(game, batch, "progress/down/empty", [0.0, 0.0, 8.0, 20.0], [x, y, 8.0, 20.0]);
        let h = (data.get(first + i).copied().unwrap_or(0) & 31).clamp(0, 20) as f32;
        // `GuiSlot.warning` on the process's input and output.
        let warn = data.get(first + i).copied().unwrap_or(0) >> 5;
        if warn & 2 != 0 {
            sprite(game, batch, "slot/warning", [0.0, 0.0, 18.0, 18.0], [ox + input[0] - 1.0, oy + input[1] - 1.0, 18.0, 18.0]);
        }
        // Bit 16 is the sawing factory's secondary output without room: that slot warns, not the main one.
        if warn & 4 != 0 && warn & 16 == 0 {
            sprite(game, batch, "slot/warning", [0.0, 0.0, 18.0, 18.0], [ox + output[0] - 1.0, oy + output[1] - 1.0, 18.0, 18.0]);
        }
        if warn & 16 != 0 {
            sprite(game, batch, "slot/warning", [0.0, 0.0, 18.0, 18.0], [ox + output[0] - 1.0, oy + output[1] + 20.0 - 1.0, 18.0, 18.0]);
        }
        if h > 0.0 {
            sprite(game, batch, "progress/down/full", [0.0, 0.0, 8.0, h], [x, y, 8.0, h]);
        }
    }
}

/// Where a screen's redstone data slot is (after its gauges).
fn redstone_slot(l: &Layout) -> usize {
    DATA_GAUGES + l.gauges.len() * GAUGE_SLOTS
}

/// The redstone mode and the signal now and last tick, from the screen's data.
fn redstone_of(l: &Layout, data: &[i32]) -> (RedstoneControl, bool, bool) {
    let v = data.get(redstone_slot(l)).copied().unwrap_or(0).max(0) as u32;
    (RedstoneControl::from_ordinal(v & 3), v & 4 != 0, v & 8 != 0)
}

/// Whether the block's redstone control lets it run this tick (`TileEntityMekanism.isRedstoneActivated`);

const TAB_UPGRADE: u8 = 1;
const TAB_REDSTONE: u8 = 2;
const TAB_SECURITY: u8 = 4;
/// The side and transporter configuration tabs.
const TAB_SIDES: u8 = 8;
/// The container edit mode tab (`GuiContainerEditModeTab`) of the fluid tank and the dynamic tank.
const TAB_EDIT_MODE: u8 = 16;

/// Which tabs a block's screen has: machines have them all; generators no side configuration;
/// energy cubes and chemical tanks no upgrades (`without(AttributeUpgradeSupport)`); fluid tanks no
/// redstone either; multiblocks none.
fn tabs(block: &str) -> u8 {
    let all = TAB_UPGRADE | TAB_REDSTONE | TAB_SECURITY | TAB_SIDES;
    match kind(block) {
        Some(
            Kind::Machine(..)
            | Kind::Factory(..)
            | Kind::Oxidizer
            | Kind::PigmentExtractor
            | Kind::PigmentMixer
            | Kind::Liquifier
            | Kind::Assemblicator
            | Kind::Dissolution
            | Kind::Crystallizer
            | Kind::Separator
            | Kind::Infuser
            | Kind::Painting
            | Kind::Advanced(_)
            | Kind::Rotary
            | Kind::Reaction
            | Kind::Nucleosynthesizer
            | Kind::ChemicalInfuser
            | Kind::Washer
            | Kind::Centrifuge
            | Kind::ModificationStation,
        ) => all,
        Some(
            Kind::HeatGenerator | Kind::BioGenerator | Kind::GasGenerator | Kind::Wind | Kind::Solar(..) | Kind::DigitalMiner | Kind::Pump | Kind::Plenisher | Kind::Stabilizer,
        ) => {
            TAB_UPGRADE | TAB_REDSTONE | TAB_SECURITY
        }
        Some(Kind::EnergyCube(_) | Kind::ChemicalTank(_)) => TAB_REDSTONE | TAB_SECURITY | TAB_SIDES,
        Some(Kind::Vibrator | Kind::Heater | Kind::FuelwoodHeater | Kind::LaserAmplifier) => TAB_REDSTONE | TAB_SECURITY,
        Some(Kind::Oredictionificator) => TAB_REDSTONE | TAB_SECURITY | TAB_SIDES,
        Some(Kind::TractorBeam | Kind::PersonalStorage) => TAB_SECURITY,
        Some(Kind::FissionLogic | Kind::FusionLogic) => TAB_REDSTONE,
        Some(Kind::FluidTank(_)) => TAB_SECURITY | TAB_EDIT_MODE,
        Some(Kind::DynamicValve) => TAB_EDIT_MODE,
        _ => 0,
    }
}

/// A tab on a screen: where its holder is, its icon, its colour (`SpecialColors`; white for the
/// tabs that register none) and what it is.
struct Tab {
    at: [f32; 2],
    icon: &'static str,
    argb: u32,
    what: TabKind,
}

#[derive(Clone, Copy, PartialEq)]
enum TabKind {
    Sides,
    Transporter,
    Upgrade,
    Security,
    Redstone,
    /// A factory's auto-sort toggle (`GuiSortingTab`).
    Sort,
    /// The Digital Miner's visuals toggle (`GuiVisualsTab`).
    Visuals,
    /// The container edit mode (both, fill, empty).
    EditMode,
    /// The Laser Amplifier's redstone output mode (`GuiAmplifierTab`).
    Amplifier,
}

fn screen_tabs(block: &str, l: &Layout, data: &[i32]) -> Vec<Tab> {
    let t = tabs(block);
    let right = screen_width(block);
    let mut out = vec![];
    if t & TAB_SIDES != 0 {
        out.push(Tab { at: [-26.0, 6.0], icon: "button/configuration", argb: 0xFFFF_FFFF, what: TabKind::Sides });
        out.push(Tab { at: [-26.0, 34.0], icon: "button/transporter_config", argb: 0xFFFF_FFFF, what: TabKind::Transporter });
    }
    if matches!(kind(block), Some(Kind::Factory(..))) {
        out.push(Tab { at: [-26.0, 62.0], icon: "button/sorting", argb: 0xFFFF_FFFF, what: TabKind::Sort });
    }
    if matches!(kind(block), Some(Kind::DigitalMiner | Kind::Stabilizer)) {
        out.push(Tab { at: [-26.0, 6.0], icon: "button/visuals", argb: 0xFFFF_FFFF, what: TabKind::Visuals });
    }
    // `GuiContainerEditModeTab` (imageWidth, 138): its icon follows the mode.
    if t & TAB_EDIT_MODE != 0 {
        let icon = ["container_edit_mode/both", "container_edit_mode/fill", "container_edit_mode/empty"][edit_mode(l, data)];
        out.push(Tab { at: [right, 138.0], icon, argb: 0xFF36_6BD0, what: TabKind::EditMode });
    }
    // `GuiAmplifierTab` (imageWidth, 109): the icon follows the redstone output mode.
    if kind(block) == Some(Kind::LaserAmplifier) {
        let icon = ["amplifier/off", "amplifier/entity", "amplifier/contents"][amplifier_mode(data)];
        out.push(Tab { at: [right, 109.0], icon, argb: 0xFFFF_FFFF, what: TabKind::Amplifier });
    }
    if t & TAB_UPGRADE != 0 {
        out.push(Tab { at: [right, 6.0], icon: "button/upgrade", argb: 0xFFFF_FFFF, what: TabKind::Upgrade });
    }
    if t & TAB_SECURITY != 0 {
        out.push(Tab { at: [right, 34.0], icon: sec_tab_icon(), argb: 0xFFFF_A160, what: TabKind::Security });
    }
    if t & TAB_REDSTONE != 0 {
        let icon = redstone_of(l, data).0.sprite();
        out.push(Tab { at: [right, 137.0], icon, argb: 0xFFC9_071F, what: TabKind::Redstone });
    }
    out
}

/// The container edit mode (0 both, 1 fill, 2 empty), kept beside the redstone data.
fn edit_mode(l: &Layout, data: &[i32]) -> usize {
    (data.get(redstone_slot(l)).copied().unwrap_or(0).max(0) as usize >> 4) % 3
}

/// The part of a tab that takes clicks and shows its tooltip: the button inside the holder's border.
fn tab_button(tab: &Tab) -> [f32; 4] {
    [tab.at[0] + 4.0, tab.at[1] + 4.0, 18.0, 18.0]
}

/// A nine-slice sprite of `size` with a `border` px frame, tinted, drawn over `dst`.
fn nine_slice_tinted(game: &Gfx<'_>, batch: &mut Batch, name: &str, size: [f32; 2], border: f32, dst: [f32; 4], argb: u32) {
    let [x, y, w, h] = dst;
    // As vanilla's `GuiSpriteScaling.NineSlice`: a border wider than half the area is cut to half of it.
    let (bx, by) = (border.min((w / 2.0).floor()), border.min((h / 2.0).floor()));
    let cols = [(0.0, bx, x, bx), (border, size[0] - 2.0 * border, x + bx, w - 2.0 * bx), (size[0] - bx, bx, x + w - bx, bx)];
    let rows = [(0.0, by, y, by), (border, size[1] - 2.0 * border, y + by, h - 2.0 * by), (size[1] - by, by, y + h - by, by)];
    for (su, sw, dx, dw) in cols {
        for (sv, sh, dy, dh) in rows {
            if dw > 0.0 && dh > 0.0 {
                game.native_sprite_tinted(batch, &format!("{SPRITES}{name}"), [su, sv, sw, sh], [dx, dy, dw, dh], argb);
            }
        }
    }
}

/// Draws a screen's tabs (`GuiSideHolder.draw`, `GuiInsetElement.drawBackground`): the holder, the
/// button (`button`, or `button_focus` under the cursor) one pixel in from the panel's side, its icon;
/// a redstone tab whose machine waits for a signal shows its warning stripes over the holder.
fn draw_tabs(game: &Gfx<'_>, batch: &mut Batch, block: &str, l: &Layout, data: &[i32], origin: [f32; 2]) {
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    let (control, powered, last) = redstone_of(l, data);
    for tab in screen_tabs(block, l, data) {
        let left = tab.at[0] < 0.0;
        let holder = if left { "holder_left" } else { "holder_right" };
        let [x, y] = [ox + tab.at[0], oy + tab.at[1]];
        nine_slice_tinted(game, batch, holder, [26.0, 9.0], 4.0, [x, y, 26.0, if tab.what == TabKind::Sort { 35.0 } else { 26.0 }], tab.argb);
        // The warning is a 16 px tiled sprite multiplied over the holder; the stripes tinted with the
        // tab's colour come close to that blend.
        if tab.what == TabKind::Redstone && control.warns(powered, last) {
            for (dx, w) in [(0.0, 16.0), (16.0, 10.0)] {
                for (dy, h) in [(0.0, 16.0), (16.0, 10.0)] {
                    let name = format!("{SPRITES}warning");
                    game.native_sprite_tinted(batch, &name, [0.0, 0.0, w, h], [x + dx, y + dy, w, h], tab.argb);
                }
            }
        }
        let [bx, by, bw, bh] = tab_button(&tab);
        let bx = ox + bx + if left { 1.0 } else { -1.0 };
        let hovered = inside([cursor[0] - ox, cursor[1] - oy], tab_button(&tab));
        let button = if hovered { "button_focus" } else { "button" };
        nine_slice_tinted(game, batch, button, [200.0, 20.0], 2.0, [bx, oy + by, bw, bh], 0xFFFF_FFFF);
        sprite(game, batch, tab.icon, [0.0, 0.0, 18.0, 18.0], [bx, oy + by, bw, bh]);
        if tab.what == TabKind::Sort {
            let word = if data.get(redstone_slot(l) + 5 + 9).copied().unwrap_or(0) != 0 { "on" } else { "off" };
            let tw = game.native_text_width(word) * 0.75;
            game.native_text_scaled(batch, word, [x + (26.0 - tw) / 2.0 + if left { 1.0 } else { -1.0 }, y + 24.0], 0x40_4040, 0.75);
        }
    }
}

/// A tab's tooltip (`MekanismLang.SIDE_CONFIG`, `TRANSPORTER_CONFIG`, `UPGRADES`, the security tab's
/// mode and owner, the redstone mode).
fn tab_tooltip(block: &str, l: &Layout, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) -> bool {
    let Some(tab) = screen_tabs(block, l, data).into_iter().find(|t| inside(at, tab_button(t))) else {
        return false;
    };
    match tab.what {
        TabKind::Sides => lines.push("Side Config".into()),
        TabKind::Transporter => lines.push("Transporter Config".into()),
        TabKind::Upgrade => lines.push("Upgrades".into()),
        TabKind::Security => sec_tab_tooltip(lines),
        TabKind::Redstone => lines.push(redstone_of(l, data).0.name().into()),
        TabKind::Sort => lines.push("Auto-Sort".into()),
        TabKind::EditMode => lines.push(["Both", "Fill", "Empty"][edit_mode(l, data)].into()),
        TabKind::Amplifier => lines.push(format!("Redstone Output: {}", ["Off", "Entity Detection", "Energy Contents"][amplifier_mode(data)])),
        TabKind::Visuals => {
            let (shown, too_big) = (miner_visuals_on(data), kind(block) == Some(Kind::DigitalMiner) && miner_view(block, data).radius > 64);
            lines.push(format!("Visuals: {}", if shown { "on" } else { "off" }));
            if too_big {
                lines.push("\u{a7}cArea too large to display visuals.".into());
            }
        }
    }
    true
}

// --- Windows ----------------------------------------------------------------------------------------
//
// Mekanism's windows over a screen (`GuiWindow`), opened from its tabs; drawn over the slots
// (`native_screen_overlay`, API 32). Which one is open is the client's own state.

/// The side configuration window (`GuiSideConfiguration`) and the transmission type it shows (an
/// index into `transmissions`).
#[derive(Clone, Copy, PartialEq)]
enum Window {
    Sides(usize),
    /// The upgrade window and the registry index of the upgrade selected in its list.
    Upgrades(Option<usize>),
    /// The transporter configuration window (`GuiTransporterConfig`).
    Transporter,
}

thread_local! {
    static WINDOW: std::cell::RefCell<Option<(String, Window)>> = const { std::cell::RefCell::new(None) };
}

/// The window open on `block`'s screen.
fn window(block: &str) -> Option<Window> {
    WINDOW.with_borrow(|w| w.as_ref().filter(|(b, _)| b == block).map(|(_, w)| *w))
}

fn set_window(block: &str, w: Option<Window>) {
    WINDOW.with_borrow_mut(|open| *open = w.map(|w| (block.to_owned(), w)));
}

/// The types a block's side configuration has, in its config's order (`AttributeSideConfig`).
fn transmissions(block: &str) -> &'static [Transmission] {
    match kind(block) {
        Some(Kind::Machine(..) | Kind::Factory(..) | Kind::EnergyCube(_) | Kind::ModificationStation) => &[Transmission::Item, Transmission::Energy],
        Some(Kind::ChemicalTank(_)) => &[Transmission::Item, Transmission::Chemical],
        // The orders of `MekanismBlockTypes.withSideConfig`.
        Some(Kind::Reaction) => &[Transmission::Item, Transmission::Chemical, Transmission::Fluid, Transmission::Energy],
        Some(Kind::Washer | Kind::Rotary) => &[Transmission::Chemical, Transmission::Fluid, Transmission::Item, Transmission::Energy],
        Some(Kind::Separator) => &[Transmission::Fluid, Transmission::Chemical, Transmission::Item, Transmission::Energy],
        Some(Kind::Liquifier) => &[Transmission::Item, Transmission::Fluid, Transmission::Energy],
        Some(Kind::Entangloporter) => &[Transmission::Item, Transmission::Fluid, Transmission::Chemical, Transmission::Energy, Transmission::Heat],
        _ => &[Transmission::Item, Transmission::Chemical, Transmission::Energy],
    }
}

/// `TransmissionType`'s name, icon (`transmission/*`) and tab colour (`SpecialColors.TAB_*_CONFIG`).
fn transmission_look(t: Transmission) -> (&'static str, &'static str, u32) {
    match t {
        Transmission::Item => ("Items", "transmission/items", 0xFFCF_CFCF),
        Transmission::Chemical => ("Chemicals", "transmission/chemicals", 0xFFFF_DD4F),
        Transmission::Energy => ("Energy", "transmission/energy", 0xFF59_C15F),
        Transmission::Fluid => ("Fluids", "transmission/fluids", 0xFF36_6BD0),
        Transmission::Heat => ("Heat", "transmission/heat", 0xFFFF_A160),
    }
}

/// Whether a type can be ejected (`ConfigInfo.canEject`): a machine's energy only comes in.
fn can_eject(block: &str, t: Transmission) -> bool {
    match t {
        Transmission::Energy => matches!(kind(block), Some(Kind::EnergyCube(_))),
        // The entangloporter's heat only conducts (`heatConfig.setCanEject(false)`).
        Transmission::Heat => false,
        _ => true,
    }
}

/// The side configuration window: where it opens on the screen (centred, 15 down), and its parts in
/// its own coordinates (`GuiSideConfiguration`'s constructor).
const SIDE_WINDOW: [f32; 4] = [10.0, 15.0, 156.0, 135.0];
const WINDOW_CLOSE: [f32; 4] = [6.0, 6.0, 8.0, 8.0];
const WINDOW_EJECT: [f32; 4] = [136.0, 6.0, 14.0, 14.0];
const WINDOW_CLEAR: [f32; 4] = [136.0, 95.0, 14.0, 14.0];
const WINDOW_SCREEN: [f32; 4] = [38.0, 25.0, 80.0, 12.0];
/// The face buttons (22×22) by relative side: front, left, right, back, top, bottom.
const SIDE_BUTTONS: [[f32; 2]; 6] = [[67.0, 69.0], [44.0, 69.0], [90.0, 69.0], [44.0, 92.0], [67.0, 46.0], [67.0, 92.0]];

/// The menu buttons the window sends: a face (200 + 16 × type + 2 × side, + 1 for a right click),
/// auto-eject (280 + type), and the clear button (285 + 2 × type, + 1 for a right click).
const BUTTON_SIDE: i32 = 200;
const BUTTON_EJECT: i32 = 280;
const BUTTON_CLEAR: i32 = 285;

/// A rectangle of the window in the screen's coordinates.
fn in_window(r: [f32; 4]) -> [f32; 4] {
    [SIDE_WINDOW[0] + r[0], SIDE_WINDOW[1] + r[1], r[2], r[3]]
}

/// The window's type tabs (`GuiConfigTypeTab`): on its left, 28 px apart; the one it shows is hidden.
fn window_type_tabs(block: &str, shown: usize) -> Vec<(usize, [f32; 4])> {
    transmissions(block)
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != shown && *i < 5)
        .map(|(i, _)| (i, in_window([-26.0, 2.0 + 28.0 * i as f32, 26.0, 26.0])))
        .collect()
}

/// The faces of type index `t` in relative-side order and whether its auto-eject is off, from the
/// screen's data (after the redstone slot).
fn window_sides(l: &Layout, data: &[i32], t: Transmission) -> (u32, bool) {
    let at = redstone_slot(l) + 1;
    let packed = data.get(at + t as usize).copied().unwrap_or(0).max(0) as u32;
    let no_eject = data.get(at + Transmission::ALL.len()).copied().unwrap_or(0).max(0) as u32 & (1 << t as u32) != 0;
    (packed, no_eject)
}

/// The clear button's target (`GuiSideConfiguration.getTargetType`): when every face has the same
/// mode, the next (or previous) one; else None.
fn clear_target(packed: u32, forward: bool) -> SideMode {
    let first = Sides::relative_get(packed, 0);
    if (1..6).all(|s| Sides::relative_get(packed, s) == first) {
        if forward { first.window_next() } else { first.window_previous() }
    } else {
        SideMode::Off
    }
}

/// `BasicColorButton.getButtonBlitColor`: the colour a little less saturated and brighter.
fn button_tint(rgb: u32) -> u32 {
    let [_, r, g, b] = rgb.to_be_bytes().map(|c| c as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    let sat = if max == 0.0 { 0.0 } else { d / max };
    let (sat, v) = ((sat - 0.1).max(0.0), (max + 0.1).min(1.0));
    let c = v * sat;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    0xFF00_0000 | to(r) << 16 | to(g) << 8 | to(b)
}

/// A Mekanism button (`MekanismButton`): `button`, `button_focus` under the cursor or
/// `button_inactive`, tinted, with an icon of `icon_size` px over it.
#[allow(clippy::too_many_arguments)]
fn draw_button(game: &Gfx<'_>, batch: &mut Batch, r: [f32; 4], hovered: bool, active: bool, argb: u32, icon: Option<(&str, f32)>) {
    let base = match (active, hovered) {
        (false, _) => "button_inactive",
        (true, true) => "button_focus",
        (true, false) => "button",
    };
    nine_slice_tinted(game, batch, base, [200.0, 20.0], 2.0, r, argb);
    if let Some((icon, size)) = icon {
        sprite(game, batch, icon, [0.0, 0.0, size, size], r);
    }
}

/// Text centred in a width.
fn centred(game: &Gfx<'_>, batch: &mut Batch, text: &str, x: f32, width: f32, y: f32, rgb: u32) {
    let w = game.native_text_width(text);
    game.native_text(batch, text, [x + (width - w) / 2.0, y], rgb, false);
}

fn nine_slice(game: &Gfx<'_>, batch: &mut Batch, name: &str, size: f32, border: f32, dst: [f32; 4]) {
    let [x, y, w, h] = dst;
    let (b, inner) = (border, size - 2.0 * border);
    let cols = [(0.0, b, x, b), (b, inner, x + b, w - 2.0 * b), (size - b, b, x + w - b, b)];
    let rows = [(0.0, b, y, b), (b, inner, y + b, h - 2.0 * b), (size - b, b, y + h - b, b)];
    for (su, sw, dx, dw) in cols {
        for (sv, sh, dy, dh) in rows {
            if dw > 0.0 && dh > 0.0 {
                game.native_sprite(batch, &format!("{SPRITES}{name}"), [su, sv, sw, sh], [dx, dy, dw, dh]);
            }
        }
    }
}

fn sprite(game: &Gfx<'_>, batch: &mut Batch, name: &str, src: [f32; 4], dst: [f32; 4]) {
    game.native_sprite(batch, &format!("{SPRITES}{name}"), src, dst);
}

/// The rectangle a power bar covers.
fn power_rect(p: PowerBar) -> [f32; 4] {
    match p {
        PowerBar::Vertical([x, y]) => [x, y, 6.0, 54.0],
        PowerBar::Tall([x, y], h) => [x, y, 6.0, h],
        PowerBar::Horizontal([x, y]) => [x, y, 54.0, 6.0],
        PowerBar::Gauge([x, y]) => [x, y, WIDE[0], WIDE[1]],
        PowerBar::SmallGauge([x, y]) => [x, y, SMALL_MED[0], SMALL_MED[1]],
        PowerBar::Sized([x, y], [w, h]) => [x, y, w, h],
    }
}

/// What the miner's screen reads from its data (`miner_flags` and the count after it).
struct MinerView {
    state: u32,
    running: bool,
    eject: bool,
    pull: bool,
    silk: bool,
    low_power: bool,
    to_mine: u32,
    radius: i32,
}

fn miner_view(block: &str, data: &[i32]) -> MinerView {
    let at = layout(block).map_or(0, |l| redstone_slot(&l));
    let v = data.get(at).copied().unwrap_or(0).max(0) as u32 >> 4;
    let n = |i: usize| data.get(at + i).copied().unwrap_or(0).max(0) as u32;
    MinerView {
        state: v >> 4 & 3,
        running: v >> 6 & 1 == 1,
        eject: v >> 7 & 1 == 1,
        pull: v >> 8 & 1 == 1,
        silk: v >> 9 & 1 == 1,
        low_power: v >> 10 & 1 == 1,
        to_mine: n(1) | n(2) << 15,
        radius: n(3) as i32,
    }
}

fn inside(at: [f32; 2], rect: [f32; 4]) -> bool {
    at[0] >= rect[0] && at[0] < rect[0] + rect[2] && at[1] >= rect[1] && at[1] < rect[1] + rect[3]
}

fn amplifier_mode(data: &[i32]) -> usize {
    data.get(3).copied().unwrap_or(0).max(0) as usize % 3
}


/// What the open screen shows of its block's security.
#[derive(Clone, Default)]
struct SecView {
    /// The owner's name, if the block has one.
    name: Option<String>,
    /// Whether the viewer is the owner.
    mine: bool,
    mode: u8,
    overridden: bool,
    /// The desk's frequency: its mode, override and trusted names; None while offline.
    freq: Option<(u8, bool, Vec<String>)>,
}

thread_local! {
    static SEC_VIEW: std::cell::RefCell<SecView> = std::cell::RefCell::new(SecView::default());
    static SEC_UI: std::cell::RefCell<SecUi> = std::cell::RefCell::new(SecUi::default());
}

#[derive(Default)]
struct SecUi {
    pos: [i32; 3],
    text: freq::TextBox,
    selected: Option<usize>,
    scroll: usize,
}

fn sec_tab_icon() -> &'static str {
    match SEC_VIEW.with_borrow(|v| v.mode) {
        1 => "security/private",
        2 => "security/protected",
        _ => "security/public",
    }
}

/// `OwnerDisplay`: "Owner: name", or "No Owner" in red.
fn owner_line(view: &SecView, coloured: bool) -> String {
    match &view.name {
        None => "\u{a7}cNo Owner".to_owned(),
        Some(n) if coloured => format!("\u{a7}8Owner: {}{n}", if view.mine { "\u{a7}a" } else { "\u{a7}c" }),
        Some(n) => format!("\u{a7}8Owner: {n}"),
    }
}

/// The security tab's tooltip (`GuiSecurityTab.updateTooltip`).
fn sec_tab_tooltip(lines: &mut Vec<String>) {
    let view = SEC_VIEW.with_borrow(Clone::clone);
    lines.push(format!("\u{a7}7Security: {}", sec::Mode::from_ordinal(i64::from(view.mode)).name()));
    lines.push(owner_line(&view, true));
    if view.overridden {
        lines.push("\u{a7}c(Overridden)".to_owned());
    }
}

thread_local! {
    /// The miners whose region this player shows (`clientRendering`).
    static MINER_VISUALS: std::cell::RefCell<std::collections::HashSet<[i32; 3]>> = std::cell::RefCell::new(Default::default());
    /// The miner the open screen belongs to, for the tooltip, which gets no game.
    static MINER_OPEN: std::cell::Cell<[i32; 3]> = const { std::cell::Cell::new([0; 3]) };
}

fn miner_visuals_on(_data: &[i32]) -> bool {
    MINER_VISUALS.with_borrow(|v| v.contains(&MINER_OPEN.get()))
}

