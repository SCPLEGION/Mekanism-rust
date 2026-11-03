// Mekanism's glue inside Horizon's game crate (`crate::game::mekanism`). Game logic lives in the
// `mekanism_core` crate; this module only translates between the game's types and its own: block entity
// NBT, item slots, tags, recipes and neighbours.

use super::native_lifecycle::WorldPhase;
use super::Game;
use horizon_client::containers::{ModBlockEntity, ModContainer};
use horizon_client::items::ItemStack;
use horizon_world::nbt::{Compound, JavaString, Tag};
use mekanism_core::bounding;
use mekanism_core::chemical::{self as chem, ChemicalTank};
use mekanism_core::energy::{Action, EnergyContainer};
use mekanism_core::evaporation as evap;
use mekanism_core::gear;
use mekanism_core::nuclear;
use mekanism_core::redstone::RedstoneControl;
use mekanism_core::qio;
use mekanism_core::laser;
use mekanism_core::seismic;
use mekanism_core::chargepad;
use mekanism_core::entangloporter::{self as qe, Frequency};
use mekanism_core::sorter::{self, Sorter};
use mekanism_core::oredictionificator as od;
use mekanism_core::plenisher::{self, Plenisher, Spot};
use mekanism_core::fission;
use mekanism_core::frequency as freq;
use mekanism_core::fusion;
use mekanism_core::bin::{self, Bin};
use mekanism_core::boiler;
use mekanism_core::isotopes;
use mekanism_core::miner;
use mekanism_core::turbine;
use mekanism_core::dynamic_tank;
use mekanism_core::sides::{self as side_config, SideMode, Sides, Transmission};
use mekanism_core::sps;
use mekanism_core::teleporter;
use mekanism_core::factory::{self, Factory};
use mekanism_core::generator::HeatGenerator;
use mekanism_core::machine::{self, ElectricMachine, Operation, Slot};
use mekanism_core::modules;
use mekanism_core::tier::{self, BaseTier};
use mekanism_core::upgrades::{Upgrade, Upgrades};
use std::sync::Arc;

/// The mod id players turn on and off in Options > Mods.
const MOD_ID: &str = "mekanism";
const HEAT_GENERATOR: &str = "mekanismgenerators:heat_generator";
/// The slots in a row of the chest menu a factory shows.
const FACTORY_ROW: usize = 9;
/// A factory's energy slot (`TileEntityFactory.energySlot`), after its two rows.
const FACTORY_ENERGY: usize = 2 * FACTORY_ROW;

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
    /// A pressurized tube and the mB it carries in a tick.
    Tube(u64),
    Oxidizer,
    /// A purification chamber or osmium compressor: an item and a chemical each tick.
    Advanced(&'static chem::AdvancedMachine),
    ChemicalInfuser,
    /// A Thermal Evaporation Plant's controller; the tower's other blocks have no block entity.
    Evaporation,
    /// A Rotary Condensentrator, decondensentrating: a fluid into its chemical.
    Rotary,
    /// An Induction Matrix's port; the casing, cells and providers have no block entity.
    InductionPort,
    /// A logistical transporter and how many items it pulls at a time.
    Transporter(u32),
    Infuser,
    Separator,
    GasGenerator,
    ChemicalTank(BaseTier),
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
    /// A laser, laser amplifier or tractor beam: fires its energy along its facing.
    Laser,
    LaserAmplifier,
    TractorBeam,
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
        return Some(Kind::Machine(recipe, usage));
    }
    if let Some((processes, recipe, usage)) = factory::factory(id) {
        return Some(Kind::Factory(processes, recipe, usage));
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
    match id {
        qio::DRIVE_ARRAY => return Some(Kind::QioDriveArray),
        qio::DASHBOARD => return Some(Kind::QioDashboard),
        qio::IMPORTER => return Some(Kind::QioImporter),
        qio::EXPORTER => return Some(Kind::QioExporter),
        qio::REDSTONE_ADAPTER => return Some(Kind::QioRedstoneAdapter),
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
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_logistical_transporter")) {
        let tier = BaseTier::ALL.into_iter().find(|t| t.name() == tier)?;
        return tier::transporter_pull(tier).map(Kind::Transporter);
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

pub(super) fn world_phase(phase: WorldPhase) {
    if phase == WorldPhase::WorldOpenReady {
        eprintln!("{}", mekanism_core::loaded_message("(server)"));
    }
}

pub(super) fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if entity.is_none() && kind(block).is_some() {
        *entity = Some((block.to_owned(), Compound::new()));
    }
}

pub(super) fn block_entity_ticks(block: &str, ticks: &mut bool) {
    if kind(block).is_some() {
        *ticks = true;
    }
}

/// Three slots shown in the furnace menu: 0 the input, 1 the energy or fuel slot, 2 the output; hoppers
/// fill the input from above and the middle slot from the sides, and take from below, as a furnace's.
pub(super) fn block_entity_container(block: &str, container: &mut Option<ModContainer>) {
    if container.is_some() {
        return;
    }
    // A factory: its inputs on the top row of a two-row chest menu, each process's output below it.
    if let Some(Kind::Factory(n, ..)) = kind(block) {
        let inputs: Vec<usize> = (0..n).collect();
        let outputs: Vec<usize> = (FACTORY_ROW..FACTORY_ROW + n).collect();
        *container = Some(ModContainer {
            slots: FACTORY_ENERGY + 1,
            faces: [outputs.clone(), inputs.clone(), inputs.clone(), inputs.clone(), inputs.clone(), inputs.clone()],
            inputs,
            outputs,
            menu: Some("minecraft:generic_9x3".into()),
        });
        return;
    }
    // A bin: hoppers fill its input slot from above and the sides and take from its output slot below;
    // it has no menu (its contents show on a right click).
    if let Some(Kind::Bin(_)) = kind(block) {
        *container = Some(ModContainer {
            slots: 2,
            faces: [vec![1], vec![0], vec![0], vec![0], vec![0], vec![0]],
            inputs: vec![0],
            outputs: vec![1],
            menu: None,
        });
        return;
    }
    // Personal storage: six rows that no hopper reaches.
    if let Some(Kind::PersonalStorage) = kind(block) {
        *container = Some(ModContainer {
            slots: bin::PERSONAL_SLOTS,
            faces: Default::default(),
            inputs: vec![],
            outputs: vec![],
            menu: Some("minecraft:generic_9x6".into()),
        });
        return;
    }
    // QIO: the drive array's two rows (its first twelve slots hold drives), the dashboard's six rows,
    // and an importer's slot that hoppers fill.
    let qio_menu = match kind(block) {
        Some(Kind::QioDriveArray) => Some((18, "minecraft:generic_9x2")),
        Some(Kind::QioDashboard) => Some((qio::DASHBOARD_SLOTS, "minecraft:generic_9x6")),
        _ => None,
    };
    if let Some((slots, menu)) = qio_menu {
        *container = Some(ModContainer { slots, faces: Default::default(), inputs: vec![], outputs: vec![], menu: Some(menu.into()) });
        return;
    }
    // The tractor beam's three rows: what its beam breaks, which hoppers take from any face.
    if let Some(Kind::TractorBeam) = kind(block) {
        let all: Vec<usize> = (0..laser::TRACTOR_SLOTS).collect();
        *container = Some(ModContainer {
            slots: laser::TRACTOR_SLOTS,
            faces: std::array::from_fn(|_| all.clone()),
            inputs: vec![],
            outputs: all,
            menu: Some("minecraft:generic_9x3".into()),
        });
        return;
    }
    // The Digital Miner: its 27 internal slots after the three of the layout (slot 1 takes energy items);
    // hoppers take what it mined from any face.
    if let Some(Kind::DigitalMiner) = kind(block) {
        let main: Vec<usize> = (MINER_FIRST..MINER_FIRST + MINER_SLOTS).collect();
        let faces: Vec<usize> = std::iter::once(1).chain(main.iter().copied()).collect();
        *container = Some(ModContainer {
            slots: MINER_FIRST + MINER_SLOTS,
            faces: std::array::from_fn(|_| faces.clone()),
            inputs: vec![1],
            outputs: main,
            menu: Some("minecraft:generic_9x4".into()),
        });
        return;
    }
    if let Some(Kind::QioImporter) = kind(block) {
        *container = Some(ModContainer { slots: 3, faces: std::array::from_fn(|_| vec![0]), inputs: vec![0], outputs: vec![], menu: Some("minecraft:furnace".into()) });
        return;
    }
    // The machines whose only slot is the energy slot (`EnergyInventorySlot`), which automation fills.
    if matches!(kind(block), Some(Kind::Centrifuge | Kind::ChemicalInfuser | Kind::Washer)) {
        // Their energy slot and chemical slots: a hopper's five slots, which the furnace's roles would spoil.
        let used: Vec<usize> = (0..3).filter(|&i| layout(block).is_some_and(|l| l.slots[i].is_some())).chain(3..3 + extra_slots(block).len()).collect();
        *container = Some(ModContainer {
            slots: 5,
            faces: std::array::from_fn(|_| used.clone()),
            inputs: used,
            outputs: vec![],
            menu: Some("minecraft:hopper".into()),
        });
        return;
    }
    // The input in the first slot (the furnace menu's), the result out of the last.
    if kind(block) == Some(Kind::Oredictionificator) {
        *container = Some(ModContainer {
            slots: 3,
            faces: [vec![2], vec![0], vec![0], vec![0], vec![0], vec![0]],
            inputs: vec![0],
            outputs: vec![2],
            menu: Some("minecraft:furnace".into()),
        });
        return;
    }
    if matches!(kind(block), Some(Kind::Heater | Kind::Vibrator)) {
        *container = Some(ModContainer {
            slots: 3,
            faces: std::array::from_fn(|_| vec![0]),
            inputs: vec![0],
            outputs: vec![],
            menu: Some("minecraft:furnace".into()),
        });
        return;
    }
    let (inputs, outputs) = match kind(block) {
        Some(Kind::Machine(..)) => (vec![0, 1], vec![2]),
        // The heat generator burns from the middle slot; generators charge an item in the first.
        Some(Kind::HeatGenerator | Kind::BioGenerator) => (vec![0, 1], vec![]),
        // The fuelwood heater burns from the first slot.
        Some(Kind::FuelwoodHeater) => (vec![0], vec![]),
        Some(Kind::Wind | Kind::GasGenerator) => (vec![0], vec![]),
        // The plain solar generator is an iron block with no menu.
        Some(Kind::Solar(..)) if block.ends_with("advanced_solar_generator") => (vec![0], vec![]),
        // The oxidizer's item goes in the first slot; the separator takes water buckets there and gives
        // the empty buckets back in the last.
        Some(Kind::Oxidizer | Kind::Dissolution) => (vec![0, 1], vec![]),
        // The crystallizer's crystals come out of the last slot.
        Some(Kind::Crystallizer) => (vec![1], vec![2]),
        Some(Kind::Separator) => (vec![0, 1], vec![2]),
        // An advanced machine's middle slot takes items that turn into its chemical (flint → oxygen).
        Some(Kind::Advanced(_)) => (vec![0, 1], vec![2]),
        // The nucleosynthesizer's item in the first slot, antimatter pellets in the middle one.
        Some(Kind::Nucleosynthesizer) => (vec![0, 1], vec![2]),
        // The reaction chamber's item in the first slot, its energy slot in the middle.
        Some(Kind::Reaction) => (vec![0, 1], vec![2]),
        // Water buckets in the first slot (empty buckets out of the last), and an empty bucket in the
        // middle one fills with brine.
        Some(Kind::Evaporation) => (vec![0, 1], vec![2, 3]),
        // Buckets in the first slot; the pump's and the plenisher's energy slot is the last.
        Some(Kind::Pump | Kind::Plenisher) => (vec![0, 2], vec![1]),
        Some(Kind::Rotary) => (vec![0, 1], vec![2]),
        // The item to infuse in the first slot, the infuse item (coal, redstone...) in the middle one.
        Some(Kind::Infuser) => (vec![0, 1], vec![2]),
        // An energy cube's charge slot (top) and discharge slot (bottom).
        Some(Kind::EnergyCube(_)) => (vec![0, 1], vec![]),
        // The fluid tank's bucket slot and the output slot under it.
        Some(Kind::FluidTank(_)) => (vec![0], vec![1]),
        // Tanks, the multiblocks' ports and the machines below open a screen with no slots of their own.
        Some(
            Kind::ChemicalTank(_)
            | Kind::FissionPort
            | Kind::TurbineValve
            | Kind::FusionPort
            | Kind::BoilerValve
            | Kind::SpsPort
            | Kind::DynamicValve
            | Kind::InductionPort
            | Kind::Laser
            | Kind::LaserAmplifier
            | Kind::Sorter
            | Kind::Chargepad
            | Kind::Entangloporter
            | Kind::Teleporter
            | Kind::QioRedstoneAdapter
            | Kind::QioExporter
            // Their bucket slots are not ported; the screen shows their tanks.
            | Kind::Activator,
        ) => {
            *container = Some(ModContainer {
                slots: 3,
                faces: Default::default(),
                inputs: vec![],
                outputs: vec![],
                menu: Some("minecraft:furnace".into()),
            });
            return;
        }
        _ => return,
    };
    // The machines with a fourth slot, and those whose slots the furnace's roles would spoil (a bucket
    // output between two inputs), open a hopper's five-slot menu.
    if !extra_slots(block).is_empty() || matches!(kind(block), Some(Kind::Pump | Kind::Plenisher | Kind::FluidTank(_))) {
        let mut inputs = inputs;
        let mut outputs = outputs;
        let mut side = vec![1];
        if !matches!(kind(block), Some(Kind::Pump | Kind::Plenisher | Kind::FluidTank(_) | Kind::Evaporation)) {
            for (i, (_, sprite)) in extra_slots(block).iter().enumerate() {
                if *sprite == "slot/output" {
                    outputs.push(3 + i);
                } else {
                    inputs.push(3 + i);
                    side.push(3 + i);
                }
            }
        }
        *container = Some(ModContainer {
            slots: 5,
            faces: [vec![2], vec![0], side.clone(), side.clone(), side.clone(), side],
            inputs,
            outputs,
            menu: Some("minecraft:hopper".into()),
        });
        return;
    }
    *container = Some(ModContainer {
        slots: 3,
        faces: [vec![2], vec![0], vec![1], vec![1], vec![1], vec![1]],
        inputs,
        outputs,
        menu: Some("minecraft:furnace".into()),
    });
}

pub(super) fn block_entity_tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if !game.native_on(MOD_ID) {
        return;
    }
    let Some(k) = kind(&entity.id) else {
        return;
    };
    // A type whose auto-eject is off gives out through no face (the window's eject button): as if
    // every face were Input (0b10 each), which `gives_out` reads.
    let no_eject = get_int(&entity.nbt, "no_eject");
    let ticking = Transmission::ALL.map(|t| if no_eject & (1 << t as u32) != 0 { Sides(0xAAA) } else { sides_of(&entity.nbt, t) });
    TICKING.set(Some((pos, ticking)));
    let facing = game.native_block_property(pos, "facing").and_then(|f| direction(&f)).map_or(2, face_index);
    // A machine its redstone control holds back does nothing this tick and goes dark.
    let allowed = tabs(&entity.id) & TAB_REDSTONE == 0 || redstone_allows(game, pos, entity);
    discharge_energy_slot(entity);
    let active = if !allowed {
        Some(false)
    } else {
        match k {
            Kind::Machine(recipe, usage) => tick_machine(game, entity, recipe, usage),
            Kind::Factory(n, recipe, usage) => tick_factory(game, entity, n, recipe, usage),
            Kind::HeatGenerator => tick_generator(game, pos, entity),
            Kind::Solar(peak, storage) => tick_solar(game, pos, entity, peak, storage),
            Kind::Wind => tick_wind(game, pos, entity),
            Kind::EnergyCube(t) => {
                tick_cube(game, pos, entity, t);
                None
            }
            Kind::Cable(rate) => {
                tick_cable(game, pos, entity, rate);
                None
            }
            Kind::Oxidizer => tick_oxidizer(game, pos, entity),
            Kind::Dissolution => tick_dissolution(game, pos, entity),
            Kind::Centrifuge => tick_centrifuge(game, pos, entity),
            Kind::FissionPort => {
                tick_fission_port(game, pos, entity);
                None
            }
            Kind::TurbineValve => tick_turbine_valve(game, pos, entity),
            Kind::FusionPort => {
                tick_fusion_port(game, pos, entity);
                None
            }
            Kind::SpsPort => {
                tick_sps_port(game, pos, entity);
                None
            }
            Kind::DynamicValve => {
                tick_dynamic_valve(game, pos, entity);
                None
            }
            Kind::Pump => tick_pump(game, pos, entity),
            Kind::Entangloporter => tick_entangloporter(game, pos, entity),
            Kind::Plenisher => tick_plenisher(game, pos, entity),
            Kind::Sorter => {
                tick_sorter(game, pos, entity);
                None
            }
            Kind::Activator => tick_activator(game, pos, entity),
            Kind::Oredictionificator => tick_oredictionificator(game, entity),
            Kind::BoilerValve => tick_boiler_valve(game, pos, entity),
            Kind::Heater => tick_heater(entity),
            Kind::FuelwoodHeater => tick_fuelwood_heater(game, entity),
            Kind::BioGenerator => tick_bio_generator(game, pos, entity),
            Kind::Reaction => tick_reaction(game, pos, entity),
            Kind::Washer => tick_washer(game, pos, entity),
            Kind::Crystallizer => tick_crystallizer(entity),
            Kind::Advanced(m) => tick_advanced(game, entity, m),
            Kind::Nucleosynthesizer => tick_nucleosynthesizer(game, entity),
            Kind::Teleporter => {
                tick_teleporter(game, pos, entity);
                None
            }
            Kind::ChemicalInfuser => tick_chemical_infuser(game, pos, entity),
            Kind::Evaporation => tick_evaporation(game, pos, entity),
            Kind::Rotary => tick_rotary(game, pos, entity),
            Kind::Transporter(pull) => {
                tick_transporter(game, pos, entity, pull);
                None
            }
            Kind::InductionPort => {
                tick_induction_port(game, pos, entity);
                None
            }
            Kind::Infuser => tick_infuser(game, entity),
            Kind::Separator => tick_separator(game, pos, entity),
            Kind::GasGenerator => tick_gas_generator(game, pos, entity),
            Kind::ChemicalTank(t) => {
                tick_chemical_tank(game, pos, entity, t);
                None
            }
            Kind::Tube(rate) => {
                tick_tube(game, pos, entity, rate);
                None
            }
            Kind::DigitalMiner => tick_miner(game, pos, entity),
            Kind::FluidTank(t) => {
                tick_fluid_tank(game, pos, entity, t);
                None
            }
            Kind::Pipe(rate) => {
                tick_pipe(game, pos, entity, rate);
                None
            }
            Kind::Bin(t) => {
                tick_bin(game, entity, t);
                None
            }
            Kind::PersonalStorage => None,
            Kind::QioDriveArray => {
                tick_drive_array(game, pos, entity);
                None
            }
            Kind::QioDashboard => {
                tick_dashboard(game, pos, entity);
                None
            }
            Kind::QioImporter => {
                tick_importer(game, pos, entity);
                None
            }
            Kind::QioExporter => {
                tick_exporter(game, pos, entity);
                None
            }
            Kind::QioRedstoneAdapter => {
                tick_redstone_adapter(game, pos, entity);
                None
            }
            Kind::Laser | Kind::LaserAmplifier | Kind::TractorBeam => tick_laser(game, pos, entity),
            Kind::Vibrator => tick_vibrator(game, pos, entity),
            // Lit for a second after it last charged something.
            Kind::Chargepad => Some((game.world.game_time.max(0) as u64).saturating_sub(get_long(&entity.nbt, "charged_at")) <= 20),
        }
    };
    screen_data(entity, facing);
    frequency_tick(game, pos, entity);
    adapter_sync(game, pos, entity);
    upgrade_sync(game, pos, entity);
    sorter_sync(game, pos, entity);
    od_sync(game, pos, entity);
    miner_sync(game, pos, entity);
    multiblock_data(game, pos, entity);
    if matches!(k, Kind::Cable(_) | Kind::Tube(_) | Kind::Transporter(_) | Kind::Pipe(_)) {
        show_connections(game, pos, entity);
    }
    if let Some(active) = active {
        let lit = if active { "true" } else { "false" };
        // Blocks without the furnace's `lit` (the solar generator's iron block) show no state.
        if game.native_block_property(pos, "lit").is_some_and(|now| now != lit) {
            game.native_set_block_property(pos, "lit", lit);
        }
    }
}

// --- Machines ---------------------------------------------------------------------------------------

/// The energy slot (`EnergyInventorySlot.fillOrConvert`) of the machines that take their energy from an
/// item in it: the item gives its energy to the machine's buffer before the machine runs.
fn discharge_energy_slot(entity: &mut ModBlockEntity) {
    let index = match kind(&entity.id) {
        Some(Kind::Oxidizer | Kind::Dissolution | Kind::Crystallizer | Kind::Separator | Kind::Rotary) => 1,
        Some(Kind::Machine("combining", _) | Kind::Advanced(_) | Kind::Infuser | Kind::Nucleosynthesizer) => 3,
        Some(Kind::Pump | Kind::Plenisher) => 2,
        Some(Kind::Centrifuge | Kind::ChemicalInfuser | Kind::Washer | Kind::Heater | Kind::Vibrator) => 0,
        _ => return,
    };
    let capacity = energy_capacity(entity);
    if capacity == 0 || energy_item(entity, index).is_none() {
        return;
    }
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    discharge_slot(entity, index, &mut energy);
    put_long(&mut entity.nbt, "energy", energy.stored);
}

fn tick_machine(game: &mut Game<'_>, entity: &mut ModBlockEntity, recipe: &str, usage: u64) -> Option<bool> {
    let mut m = ElectricMachine::with_upgrades(usage, upgrades(&entity.nbt));
    m.energy.stored = get_long(&entity.nbt, "energy").min(m.energy.capacity);
    m.progress = get_int(&entity.nbt, "progress");
    // The middle slot is the machine's energy slot (the combiner's holds its extra input instead): an
    // energy tablet or cube there runs the machine.
    if recipe != "combining" {
        discharge_slot(entity, 1, &mut m.energy);
    }
    let mut input = slot(entity.items.first().cloned().flatten());
    let mut middle = slot(entity.items.get(1).cloned().flatten());
    let mut output = slot(entity.items.get(2).cloned().flatten());
    // The sawmill's secondary output waits in the machine until the output slot takes it; the machine
    // stops meanwhile, as a full secondary slot stops it in Mekanism.
    let mut waiting = secondary(&entity.nbt);
    if let Some((item, count)) = waiting.clone() {
        let max = max_stack(game, &item);
        let fits = output.item.as_ref().is_none_or(|i| *i == item) && output.count + count <= max;
        if fits {
            output.item = Some(item);
            output.count += count;
            waiting = None;
        }
    }
    let (operation, max_stack, chance) = if waiting.is_some() {
        (None, 64, None)
    } else {
        find_operation(game, recipe, entity.items.first().cloned().flatten(), &middle)
    };
    let before = output.count;
    // The issues the warning tab lists (`RecipeError`): bit 0 energy, 1 input, 2 output room, 3 input without a recipe.
    let mut warn = 0;
    match &operation {
        None if input.item.is_some() && waiting.is_none() => warn |= 8,
        None => {}
        Some(op) => {
            let fits = match &output.item {
                None => op.output_count <= max_stack,
                Some(item) => *item == op.output && output.count + op.output_count <= max_stack,
            };
            if !fits {
                warn |= 4;
            } else if input.count < op.input_count || (recipe == "combining" && middle.count < op.extra_count) {
                warn |= 2;
            } else if m.energy.stored < m.usage {
                warn |= 1;
            }
        }
    }
    if waiting.is_some() {
        warn |= 4;
    }
    put_int(&mut entity.nbt, "warn", warn);
    let extra = (recipe == "combining").then_some(&mut middle);
    m.tick(&mut input, &mut output, extra, operation.as_ref(), max_stack);
    if output.count != before
        && let Some((item, count, p)) = chance
        && roll(entity) < p
    {
        waiting = Some((item, count));
    }
    set_slot(entity, 0, &input);
    if recipe == "combining" {
        set_slot(entity, 1, &middle);
    }
    set_slot(entity, 2, &output);
    put_long(&mut entity.nbt, "energy", m.energy.stored);
    put_int(&mut entity.nbt, "progress", m.progress);
    set_secondary(&mut entity.nbt, waiting.as_ref());
    // The furnace menu's bars: the flame shows stored energy, the arrow the progress. Data slots are
    // 16 bits, so energy goes as thousandths of the capacity.
    entity.data = vec![
        (m.energy.stored * 1000 / m.energy.capacity.max(1)) as i32,
        1000,
        m.progress as i32,
        m.ticks_required as i32,
    ];
    // Mekanism's screen reads the rest (`screen_tooltip`).
    let u = upgrades(&entity.nbt);
    entity.data.extend(wide(m.energy.stored));
    entity.data.extend(wide(m.energy.capacity));
    entity.data.extend([u.speed as i32, u.energy as i32]);
    entity.data.extend(wide(m.usage));
    Some(m.active)
}

fn tick_factory(game: &mut Game<'_>, entity: &mut ModBlockEntity, n: usize, recipe: &str, usage: u64) -> Option<bool> {
    let mut f = Factory::new(n, usage, upgrades(&entity.nbt));
    f.machine.energy.stored = get_long(&entity.nbt, "energy").min(f.machine.energy.capacity);
    discharge_slot(entity, FACTORY_ENERGY, &mut f.machine.energy);
    if let Some(Tag::IntArray(saved)) = entity.nbt.get(&JavaString::from("progress")) {
        for (p, v) in f.progress.iter_mut().zip(saved) {
            *p = (*v).max(0) as u32;
        }
    }
    let mut outputs: Vec<Slot> = (0..n)
        .map(|i| slot(entity.items.get(FACTORY_ROW + i).cloned().flatten()))
        .collect();
    if get_int(&entity.nbt, "sorting") & 1 != 0 && get_int(&entity.nbt, "sort_tick") % 20 == 0 {
        sort_factory_inputs(game, entity, n, recipe);
    }
    let sort_tick = get_int(&entity.nbt, "sort_tick");
    put_int(&mut entity.nbt, "sort_tick", (sort_tick + 1) % 20);
    let mut inputs: Vec<Slot> = (0..n).map(|i| slot(entity.items.get(i).cloned().flatten())).collect();
    let operations: Vec<Option<(Operation, u32)>> = (0..n)
        .map(|i| {
            let (op, max, _) = find_operation(game, recipe, entity.items.get(i).cloned().flatten(), &Slot::default());
            op.map(|o| (o, max))
        })
        .collect();
    // The issues of each process (`RecipeError`): 1 energy, 2 input, 4 output room, 8 input without a recipe.
    let warn: Vec<i32> = (0..n)
        .map(|i| match &operations[i] {
            None if inputs[i].item.is_some() => 8,
            None => 0,
            Some((op, max)) => {
                let fits = match &outputs[i].item {
                    None => op.output_count <= *max,
                    Some(item) => *item == op.output && outputs[i].count + op.output_count <= *max,
                };
                if !fits {
                    4
                } else if inputs[i].count < op.input_count {
                    2
                } else {
                    i32::from(f.machine.energy.stored < f.machine.usage)
                }
            }
        })
        .collect();
    entity.nbt.insert("warn".into(), Tag::IntArray(warn));
    f.tick(&mut inputs, &mut outputs, &operations);
    for i in 0..n {
        set_slot(entity, i, &inputs[i]);
        set_slot(entity, FACTORY_ROW + i, &outputs[i]);
    }
    put_long(&mut entity.nbt, "energy", f.machine.energy.stored);
    entity.nbt.insert(
        "progress".into(),
        Tag::IntArray(f.progress.iter().map(|p| (*p).min(i32::MAX as u32) as i32).collect()),
    );
    Some(f.active)
}

/// `TileEntityFactory.sortInventory`, once a second: each kind of input item is spread evenly over the
/// slots that hold it and the empty ones it can fill (`factory::sort_counts`; a slot needs the recipe's
/// input amount at least).
fn sort_factory_inputs(game: &Game<'_>, entity: &mut ModBlockEntity, n: usize, recipe: &str) {
    let key = |s: &ItemStack| format!("{:?}", ItemStack { count: 1, ..s.clone() });
    let mut kinds: Vec<(String, Vec<usize>, u64)> = vec![];
    let mut empties = vec![];
    for i in 0..n {
        match entity.items.get(i).cloned().flatten() {
            None => empties.push(i),
            Some(s) => {
                let k = key(&s);
                match kinds.iter_mut().find(|(o, ..)| *o == k) {
                    Some((_, slots, total)) => {
                        slots.push(i);
                        *total += u64::from(s.count);
                    }
                    None => kinds.push((k, vec![i], u64::from(s.count))),
                }
            }
        }
    }
    for (_, slots, total) in kinds {
        let Some(proto) = entity.items[slots[0]].clone() else {
            continue;
        };
        if u64::from(max_stack(game, &proto.item)) * (slots.len() as u64) < total {
            continue;
        }
        let spare = std::mem::take(&mut empties);
        // A slot needs the recipe's input amount to run it (`getRecipe(...).getInput().getNeededAmount`);
        // an item with no recipe is split as if one item were enough.
        let sample = ItemStack { count: total.min(u64::from(max_stack(game, &proto.item))) as u32, ..proto.clone() };
        let min = find_operation(game, recipe, Some(sample), &Slot::default()).0.map_or(1, |o| u64::from(o.input_count));
        let counts = factory::sort_counts(total, slots.len(), spare.len(), min);
        let mut targets = slots.clone();
        let mut left = spare;
        while targets.len() < counts.len() {
            targets.push(left.remove(0));
        }
        empties = left;
        for (&i, &c) in targets.iter().zip(&counts) {
            entity.items[i] = (c > 0).then(|| ItemStack { count: c as u32, ..proto.clone() });
        }
    }
}

fn max_stack(game: &Game<'_>, id: &str) -> u32 {
    world_items!(game).get(id).map_or(64, |i| i.max_stack)
}

/// A Precision Sawmill's secondary output still waiting for the output slot.
fn secondary(nbt: &Compound) -> Option<(String, u32)> {
    let Some(Tag::Compound(c)) = nbt.get(&JavaString::from("secondary")) else {
        return None;
    };
    let Some(Tag::String(id)) = c.get(&JavaString::from("id")) else {
        return None;
    };
    Some((id.to_string().ok()?, get_int(c, "count").max(1)))
}

fn set_secondary(nbt: &mut Compound, waiting: Option<&(String, u32)>) {
    match waiting {
        Some((id, count)) => {
            let mut c = Compound::new();
            c.insert("id".into(), Tag::String(id.as_str().into()));
            put_int(&mut c, "count", *count);
            nbt.insert("secondary".into(), Tag::Compound(c));
        }
        None => {
            nbt.remove(&JavaString::from("secondary"));
        }
    }
}

/// A number in [0, 1) for the sawmill's chance: a splitmix64 step kept in the block entity, so each
/// machine has its own sequence that survives saving.
fn roll(entity: &mut ModBlockEntity) -> f64 {
    let mut x = get_long(&entity.nbt, "seed") ^ 0x9E37_79B9_7F4A_7C15;
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    put_long(&mut entity.nbt, "seed", x & (i64::MAX as u64));
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// The upgrades installed in a machine (`upgrades` in its NBT: one count per upgrade name).
fn upgrades(nbt: &Compound) -> Upgrades {
    let Some(Tag::Compound(c)) = nbt.get(&JavaString::from("upgrades")) else {
        return Upgrades::default();
    };
    let mut u = Upgrades::default();
    for kind in Upgrade::ALL {
        u.install(kind, get_int(c, kind.name()));
    }
    u
}

fn set_upgrades(nbt: &mut Compound, u: &Upgrades) {
    let mut c = Compound::new();
    for kind in Upgrade::ALL {
        if u.get(kind) > 0 {
            put_int(&mut c, kind.name(), u.get(kind));
        }
    }
    nbt.insert("upgrades".into(), Tag::Compound(c));
}

/// The `mekanism:upgrade_type` in a stack's custom data (the carried recipes put it there).
fn upgrade_type_of(stack: &ItemStack) -> Option<String> {
    if &*stack.item != "mekanism:upgrade" {
        return None;
    }
    let (_, data) = stack.extra.as_ref()?.other.iter().find(|(k, _)| k == "minecraft:custom_data")?;
    let Tag::Compound(c) = data else {
        return None;
    };
    match c.get(&JavaString::from("mekanism:upgrade_type")) {
        Some(Tag::String(id)) => id.to_string().ok(),
        _ => None,
    }
}

fn upgrade_of(stack: &ItemStack) -> Option<Upgrade> {
    Upgrade::from_id(&upgrade_type_of(stack)?)
}

/// Right click on a Mekanism machine holding a speed or energy upgrade: one goes in (all that fit when
/// sneaking), as dropping it in the machine's upgrade tab does. True when the click was used.
pub(super) fn install_upgrade(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) {
        return false;
    }
    let selected = game.selected;
    let Some(Some(held)) = game.inventory.get(selected).cloned() else {
        return false;
    };
    let Some(kind) = upgrade_of(&held) else {
        return false;
    };
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    let want = if sneaking { held.count } else { 1 };
    let installed = game.native_with_mod_block_entity(pos, |m| {
        let target = self::kind(&m.id);
        if !matches!(target, Some(Kind::Machine(..) | Kind::Factory(..) | Kind::DigitalMiner | Kind::Pump | Kind::Plenisher))
            || (kind == Upgrade::Filter && target != Some(Kind::Pump))
        {
            return None;
        }
        let mut u = upgrades(&m.nbt);
        let n = u.install(kind, want);
        set_upgrades(&mut m.nbt, &u);
        Some((n, u.get(kind)))
    });
    let Some(Some((n, now))) = installed else {
        return false;
    };
    if n > 0 {
        let left = held.count - n;
        game.inventory[selected] = (left > 0).then(|| ItemStack { count: left, ..held });
    }
    if let Some(player) = game.native_acting_player() {
        let line = format!("{} upgrades: {now}/{}", kind.name(), kind.max());
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
    true
}

/// What the machine of kind `recipe` does with `stack` in its input (and `middle`, the Combiner's extra
/// input): the operation, the output's stack size, and the sawmill's secondary output with its chance.
#[allow(clippy::type_complexity)]
fn find_operation(
    game: &Game<'_>,
    recipe: &str,
    stack: Option<ItemStack>,
    middle: &Slot,
) -> (Option<Operation>, u32, Option<(String, u32, f64)>) {
    let Some(stack) = stack else {
        return (None, 64, None);
    };
    let items = world_items!(game);
    let max = |id: &str| items.get(id).map_or(64, |i| i.max_stack);
    let in_tag = |tag: &str, item: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
    let op = |input_count, extra_count, output: &str, output_count| Operation {
        input_count,
        extra_count,
        output: output.to_owned(),
        output_count,
    };
    match recipe {
        "smelting" => {
            let Some(cooking) = world_recipes!(game).cook("minecraft:smelting", &stack) else {
                return (None, 64, None);
            };
            let out = cooking.result.item.to_string();
            let m = max(&out);
            (Some(op(1, 0, &out, cooking.result.count)), m, None)
        }
        "sawing" => {
            let Some(r) = mekanism_core::recipes::find_sawing(&stack.item, stack.count, &in_tag) else {
                return (None, 64, None);
            };
            // A recipe with only a secondary output gives it every time here.
            let (main, chance) = match (r.main, r.secondary) {
                (Some(main), Some((id, n))) => (main, Some((id.to_owned(), n, r.secondary_chance))),
                (Some(main), None) => (main, None),
                (None, Some(sec)) => (sec, None),
                (None, None) => return (None, 64, None),
            };
            (Some(op(r.input_count, 0, main.0, main.1)), max(main.0), chance)
        }
        "combining" => {
            let Some(extra) = middle.item.as_deref() else {
                return (None, 64, None);
            };
            let Some(r) = mekanism_core::recipes::find_combining(
                (&stack.item, stack.count),
                (extra, middle.count),
                &in_tag,
            ) else {
                return (None, 64, None);
            };
            (Some(op(r.main_count, r.extra_count, r.output, r.output_count)), max(r.output), None)
        }
        _ => {
            let Some(r) = mekanism_core::recipes::find(
                mekanism_core::recipes::table(recipe),
                &stack.item,
                stack.count,
                &in_tag,
            ) else {
                return (None, 64, None);
            };
            (Some(op(r.input_count, 0, r.output, r.output_count)), max(r.output), None)
        }
    }
}

// --- Heat Generator ---------------------------------------------------------------------------------

fn tick_generator(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let mut g = HeatGenerator::new();
    g.energy.stored = get_long(&entity.nbt, "energy").min(g.energy.capacity);
    g.lava = get_int(&entity.nbt, "lava");
    if let Some(heat) = get_double(&entity.nbt, "heat") {
        g.heat.heat = heat;
    }
    // FluidFuelInventorySlot.fillOrBurn: one furnace fuel item at a time becomes lava, when it fits.
    if let Some(Some(fuel)) = entity.items.get(1).cloned() {
        let burn = game.res.providers().burn_time(world_items!(game), &fuel, false).max(0) as u32;
        if g.accepts_fuel(burn) {
            g.add_fuel(burn);
            let left = fuel.count - 1;
            entity.items[1] = if left > 0 {
                Some(ItemStack { count: left, ..fuel })
            } else if &*fuel.item == "minecraft:lava_bucket" {
                Some(stack_of("minecraft:bucket", 1))
            } else {
                None
            };
        }
    }
    // Lava from the mechanical pipes and fluid tanks beside it.
    let mut lava = ChemicalTank::new(u64::from(mekanism_core::generator::TANK_CAPACITY));
    lava.insert("minecraft:lava", u64::from(g.lava));
    pull_fluid(game, pos, &mut lava, "minecraft:lava");
    g.lava = lava.amount as u32;
    g.tick();
    // The first slot charges an item from the generator (Mekanism's energy slot).
    charge_slot(entity, 0, &mut g.energy);
    push_energy(game, pos, &mut g.energy, None, false);
    put_long(&mut entity.nbt, "energy", g.energy.stored);
    put_int(&mut entity.nbt, "lava", g.lava);
    entity.nbt.insert("heat".into(), Tag::Double(g.heat.heat.to_bits()));
    // The flame shows the lava left, the arrow the temperature above ambient (up to 200 K).
    let warm = (g.heat.temperature() - mekanism_core::heat::AMBIENT_TEMP).clamp(0.0, 200.0) as i32;
    entity.data = vec![g.lava as i32, mekanism_core::generator::TANK_CAPACITY as i32, warm, 200];
    Some(g.active)
}

// --- Solar and wind ---------------------------------------------------------------------------------

/// `Level.canSeeSky`: nothing above `pos` that blocks the sky up to the top of the level.
fn can_see_sky(game: &Game<'_>, pos: [i32; 3]) -> bool {
    let top = game.world.top_y();
    (pos[1] + 1..top).all(|y| {
        let f = game.res.facts().get(game.world.block(pos[0], y, pos[2]));
        !f.full_collision && f.fluid == horizon_client::blocks::Fluid::None
    })
}

fn overworld(game: &Game<'_>) -> bool {
    game.dim == super::Dim::Overworld
}

/// The solar generator's sun: `LightLevelUtils.canSeeSun` and `getSunBrightness` (the sky light
/// factor, with the weather), and `SolarCheck`'s biome multiplier.
fn tick_solar(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, peak: u64, storage: u64) -> Option<bool> {
    let mut energy = EnergyContainer::new(storage, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(storage);
    let produced = sun_production(game, pos, peak);
    energy.insert(produced, Action::Execute);
    charge_slot(entity, 0, &mut energy);
    push_energy(game, pos, &mut energy, None, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    entity.data = vec![(energy.stored * 1000 / storage.max(1)) as i32, 1000, produced as i32, peak as i32];
    Some(produced > 0)
}

/// What a solar panel at `pos` with `peak` makes this tick.
fn sun_production(game: &Game<'_>, pos: [i32; 3], peak: u64) -> u64 {
    let sees_sun = overworld(game) && game.sky_darken() < 4 && can_see_sky(game, pos);
    if sees_sun {
        let mut sky = game.res.environment().sample(game.world.day_time, None, None);
        let (rain, thunder) = game.world.weather.levels(1.0);
        horizon_client::environment::apply_weather(&mut sky, rain, thunder);
        let biome = game.world.chunks.get(&(pos[0] >> 4, pos[2] >> 4)).map(|c| {
            let b = &game.res.biomes().list[c.biome(pos[0], pos[1], pos[2]) as usize];
            (b.temperature, b.downfall, b.precipitation)
        });
        let (t, d, rains) = biome.unwrap_or((0.8, 0.4, true));
        let m = mekanism_core::generator::solar_biome_multiplier(t, d, rains);
        mekanism_core::generator::solar_production(peak, true, sky.sky_light_factor, m)
    } else {
        0
    }
}

/// The wind generator: its multiplier is worked out once a second, from the height of its top.
fn tick_wind(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    use mekanism_core::generator as g;
    let mut energy = EnergyContainer::new(g::WIND_STORAGE, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(g::WIND_STORAGE);
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    let mut multiplier = get_double(&entity.nbt, "multiplier").unwrap_or(0.0);
    if ticker == 0 {
        let top = [pos[0], pos[1] + 4, pos[2]];
        let fluid = game.res.facts().get(game.world.block(top[0], top[1], top[2])).fluid;
        multiplier = if fluid == horizon_client::blocks::Fluid::None && can_see_sky(game, top) {
            g::wind_multiplier(top[1], game.world.bottom_y(), game.world.top_y() - 1)
        } else {
            0.0
        };
        entity.nbt.insert("multiplier".into(), Tag::Double(multiplier.to_bits()));
    }
    let produced = if multiplier > 0.0 { g::wind_generation(multiplier) } else { 0 };
    energy.insert(produced, Action::Execute);
    charge_slot(entity, 0, &mut energy);
    push_energy(game, pos, &mut energy, None, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    entity.data = vec![(energy.stored * 1000 / g::WIND_STORAGE) as i32, 1000, produced as i32, g::WIND_GENERATION_MAX as i32];
    Some(produced > 0)
}

// --- Energy cubes -----------------------------------------------------------------------------------

fn tick_cube(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, tier: BaseTier) {
    let (capacity, rate) = tier::energy_cube(tier);
    let mut energy = EnergyContainer::new(capacity, rate);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    // The top slot charges the item in it from the cube; the bottom one empties its item into it.
    charge_slot(entity, 0, &mut energy);
    discharge_slot(entity, 1, &mut energy);
    // A cube's default side configuration: energy goes out of its front only.
    let front = game
        .native_block_property(pos, "facing")
        .and_then(|f| direction(&f));
    if let Some(front) = front {
        push_energy(game, pos, &mut energy, Some(front), false);
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    entity.data = vec![(energy.stored * 1000 / capacity.max(1)) as i32, 1000, 0, 0];
}

// --- Universal cables -------------------------------------------------------------------------------

/// A cable holds one tick of what it carries. It gives to the machines and cubes beside it first, then
/// lets its energy run on to emptier cables (`tier::cable_share`), so a line of cables carries energy
/// from generators to machines without it flowing back. Mekanism pools a whole cable network instead;
/// the throughput per cable is the same.
fn tick_cable(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, rate: u64) {
    let mut energy = EnergyContainer::new(rate, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(rate);
    if energy.stored > 0 {
        push_energy(game, pos, &mut energy, None, true);
    }
    for (_, d) in DIRECTIONS {
        if energy.stored == 0 {
            break;
        }
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let mine = energy.stored;
        let given = game.native_with_mod_block_entity(at, |m| {
            let Some(Kind::Cable(their_rate)) = kind(&m.id) else {
                return 0;
            };
            let theirs = get_long(&m.nbt, "energy");
            let give = tier::cable_share(mine, theirs).min(their_rate.saturating_sub(theirs));
            put_long(&mut m.nbt, "energy", theirs + give);
            give
        });
        if let Some(given) = given {
            energy.extract(given, Action::Execute);
        }
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
}

// --- Chemicals --------------------------------------------------------------------------------------

fn get_tank(nbt: &Compound, key: &str, capacity: u64) -> ChemicalTank {
    let mut t = ChemicalTank::new(capacity);
    if let Some(Tag::Compound(c)) = nbt.get(&JavaString::from(key))
        && let Some(Tag::String(id)) = c.get(&JavaString::from("id"))
        && let Ok(id) = id.to_string()
    {
        t.insert(&id, get_long(c, "amount"));
    }
    t
}

fn put_tank(nbt: &mut Compound, key: &str, t: &ChemicalTank) {
    match &t.chemical {
        Some(id) if t.amount > 0 => {
            let mut c = Compound::new();
            c.insert("id".into(), Tag::String(id.as_str().into()));
            put_long(&mut c, "amount", t.amount);
            nbt.insert(key.into(), Tag::Compound(c));
        }
        _ => {
            nbt.remove(&JavaString::from(key));
        }
    }
}

/// Where a neighbour keeps chemicals it accepts: its NBT key and capacity, for `chemical`. Tubes take
/// from machines and tanks but not from other tubes, which share by gradient instead (`tick_tube`).
/// `d` is the receiver's offset from the giver: a tank takes nothing into its bottom, its output side.
fn chemical_receiver(id: &str, nbt: &Compound, chemical: &str, from_tube: bool, d: [i32; 3]) -> Option<(&'static str, u64)> {
    let kind = kind(id)?;
    // Fluids (water, lava, brine) share the chemical code here and go into the blocks with a fluid
    // tank: mechanical pipes, fluid tanks (not into their bottom), the separator and the rotary.
    if FLUIDS.contains(&chemical) {
        return match kind {
            Kind::Pipe(rate) if !from_tube => Some(("fluid", rate)),
            Kind::FluidTank(t) if d != [0, 1, 0] => Some(("fluid", tier::fluid_tank(t).0)),
            Kind::Separator if chem::separates(chemical) => Some(("fluid", chem::SEPARATOR_FLUID)),
            Kind::Rotary if chem::rotary_chemical(chemical).is_some() => Some(("fluid", chem::ROTARY_TANK)),
            Kind::Washer if chemical == "minecraft:water" => Some(("fluid", nuclear::WASHER_TANK)),
            Kind::Reaction if chemical == "minecraft:water" => Some(("fluid", REACTION_TANK)),
            Kind::Entangloporter if qe_takes(nbt, "fluid", chemical) => Some(("fluid_in", qe::FLUID_BUFFER)),
            Kind::Plenisher
                if plenisher::placeable(chemical).is_some()
                    && get_tank(nbt, "fluid", plenisher::PLENISHER_TANK).accepts(chemical) =>
            {
                Some(("fluid", plenisher::PLENISHER_TANK))
            }
            Kind::DynamicValve if valve_takes(nbt, chemical) => Some(("buffer", dynamic_tank::VALVE_BUFFER)),
            // Brine crystallizes into salt.
            Kind::Crystallizer if nuclear::find_crystallizing(chemical).is_some() => Some(("input", nuclear::CRYSTALLIZER_TANK)),
            _ => None,
        };
    }
    match kind {
        Kind::Tube(rate) if !from_tube => Some(("chemical", rate)),
        Kind::ChemicalTank(t) if d != [0, 1, 0] => Some(("chemical", chem::chemical_tank(t).0)),
        Kind::Advanced(m) if m.chemicals.contains(&chemical) && get_tank(nbt, "chemical", chem::ADVANCED_TANK).accepts(chemical) => Some(("chemical", chem::ADVANCED_TANK)),
        // The chemical infuser fills whichever input tank holds this chemical, else an empty one.
        Kind::ChemicalInfuser if mekanism_core::recipes::infusing_input(chemical) => {
            let cap = chem::CHEMICAL_INFUSER_TANK;
            let (left, right) = (get_tank(nbt, "left", cap), get_tank(nbt, "right", cap));
            let holds = |t: &ChemicalTank| t.chemical.as_deref() == Some(chemical);
            let key = if holds(&left) || (!holds(&right) && left.amount == 0) {
                "left"
            } else if holds(&right) || right.amount == 0 {
                "right"
            } else {
                return None;
            };
            Some((key, cap))
        }
        Kind::GasGenerator if chem::fuel(chemical).is_some() => Some(("fuel", chem::GAS_GENERATOR_TANK)),
        Kind::Dissolution if nuclear::dissolves_with(chemical) => Some(("input", nuclear::DISSOLUTION_TANK)),
        Kind::Centrifuge if nuclear::centrifuging(chemical).is_some() => Some(("input", nuclear::CENTRIFUGE_TANK)),
        Kind::Washer if nuclear::find_washing(chemical).is_some() => Some(("input", nuclear::WASHER_TANK)),
        Kind::Crystallizer if nuclear::find_crystallizing(chemical).is_some() => Some(("input", nuclear::CRYSTALLIZER_TANK)),
        Kind::Activator if isotopes::activating(chemical).is_some() => Some(("input", isotopes::ACTIVATOR_TANK)),
        Kind::Nucleosynthesizer if chemical == chem::ANTIMATTER => Some(("chemical", chem::NUCLEOSYNTHESIZER_TANK)),
        Kind::Reaction if mekanism_core::recipes::reaction_input(chemical) && get_tank(nbt, "input", REACTION_TANK).accepts(chemical) => {
            Some(("input", REACTION_TANK))
        }
        Kind::DynamicValve if valve_takes(nbt, chemical) => Some(("buffer", dynamic_tank::VALVE_BUFFER)),
        Kind::Entangloporter if qe_takes(nbt, "chemical", chemical) => Some(("chemical_in", qe::CHEMICAL_BUFFER)),
        _ => None,
    }
}

thread_local! {
    /// The block entity being ticked and its side configuration. `push_energy` and `push_chemical` are
    /// called with the ticking block's containers taken out of its NBT, so they read its sides here.
    static TICKING: std::cell::Cell<Option<([i32; 3], [Sides; 3])>> = const { std::cell::Cell::new(None) };
}

/// The faces' modes for one transmission type.
fn sides_of(nbt: &Compound, t: Transmission) -> Sides {
    Sides(get_long(nbt, t.key()) as u32)
}

fn face_index(d: [i32; 3]) -> usize {
    DIRECTIONS.iter().position(|(_, v)| *v == d).unwrap_or(0)
}

/// Whether the block at `pos` may give out `t` through face `d`, by its side configuration.
fn gives_out(pos: [i32; 3], d: [i32; 3], t: Transmission) -> bool {
    match TICKING.get() {
        Some((at, sides)) if at == pos => sides[t as usize].get(face_index(d)).gives_out(),
        _ => true,
    }
}

/// Whether a receiver takes in `t` through its face `back` (the one facing the giver).
fn takes_in(nbt: &Compound, back: [i32; 3], t: Transmission) -> bool {
    sides_of(nbt, t).get(face_index(back)).takes_in()
}

/// The Configurator: a right click on a face of a Mekanism block cycles that face between Default,
/// None, Input and Output, and says so in chat.
pub(super) fn configure_side(game: &mut Game<'_>, pos: [i32; 3], face: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) {
        return false;
    }
    let holding = game.inventory.get(game.selected).cloned().flatten();
    let Some(held) = holding.filter(|h| &*h.item == CONFIGURATOR) else {
        return false;
    };
    let t = configurator_mode(&held);
    let mode = game.native_with_mod_block_entity(pos, |m| {
        kind(&m.id)?;
        let mut sides = sides_of(&m.nbt, t);
        let mode = sides.cycle(face_index(face));
        put_long(&mut m.nbt, t.key(), u64::from(sides.0));
        Some(mode)
    });
    let Some(Some(mode)) = mode else {
        return false;
    };
    let name = DIRECTIONS[face_index(face)].0;
    let line = format!("Side {name} ({}): {}", t.name(), mode.name());
    game.system_message(horizon_client::text::Text::literal(line));
    true
}

const CONFIGURATOR: &str = "mekanism:configurator";
/// The key in the Configurator's custom data that holds the type it configures (Mekanism's
/// `configurator_mode`).
const CONFIGURATOR_KEY: &str = "mekanism:configurator_mode";

fn configurator_mode(stack: &ItemStack) -> Transmission {
    let name = custom_data(stack).and_then(|c| match c.get(&JavaString::from(CONFIGURATOR_KEY)) {
        Some(Tag::String(s)) => s.to_string().ok(),
        _ => None,
    });
    name.as_deref().and_then(Transmission::from_name).unwrap_or_default()
}

/// Sets a string in an item's custom data, keeping the rest of it.
fn set_custom_string(stack: &mut ItemStack, key: &str, value: &str) {
    let extra = stack.extra.get_or_insert_with(Default::default);
    let at = extra.other.iter().position(|(k, d)| k == "minecraft:custom_data" && matches!(d, Tag::Compound(_)));
    let at = at.unwrap_or_else(|| {
        extra.other.retain(|(k, _)| k != "minecraft:custom_data");
        extra.other.push(("minecraft:custom_data".to_owned(), Tag::Compound(Compound::new())));
        extra.other.len() - 1
    });
    if let Tag::Compound(c) = &mut extra.other[at].1 {
        c.insert(key.into(), Tag::String(value.into()));
    }
}

const FLUIDS: [&str; 4] = ["minecraft:water", "minecraft:lava", "mekanism:brine", chem::HEAVY_WATER];

/// Pushes up to `rate` of `from`'s chemical into the tanks and generators beside `pos` (or only the one
/// at `only`) that accept it.
fn push_chemical(
    game: &mut Game<'_>,
    pos: [i32; 3],
    from: &mut ChemicalTank,
    only: Option<[i32; 3]>,
    rate: u64,
    from_tube: bool,
) {
    let mut budget = rate;
    for (_, d) in DIRECTIONS {
        if only.is_some_and(|o| o != d) || !gives_out(pos, d, Transmission::Chemical) {
            continue;
        }
        let Some(chemical) = from.chemical.clone() else {
            return;
        };
        let offer = from.amount.min(budget);
        if offer == 0 {
            return;
        }
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let taken = game.native_with_mod_block_entity(at, |m| {
            if !takes_in(&m.nbt, [-d[0], -d[1], -d[2]], Transmission::Chemical) {
                return 0;
            }
            let Some((key, capacity)) = chemical_receiver(&m.id, &m.nbt, &chemical, from_tube, d) else {
                return 0;
            };
            let mut to = get_tank(&m.nbt, key, capacity);
            let n = to.insert(&chemical, offer);
            put_tank(&mut m.nbt, key, &to);
            n
        });
        if let Some(n) = taken {
            from.extract(n);
            budget -= n;
        }
    }
}

/// The Chemical Oxidizer: an item becomes a chemical over 100 ticks at 15 FE a tick, pushed out to the
/// tanks beside it.
fn tick_oxidizer(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = chem::OXIDIZER_USAGE * u64::from(chem::OXIDIZER_TICKS);
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut tank = get_tank(&entity.nbt, "output", chem::OXIDIZER_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    drain_tank_into_item(entity, 3, &mut tank);
    let recipe = entity.items.first().cloned().flatten().and_then(|stack| {
        let items = world_items!(game);
        let in_tag = |tag: &str, item: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
        mekanism_core::recipes::find_oxidizing(&stack.item, stack.count, &in_tag).map(|r| (stack, *r))
    });
    let warn = match &recipe {
        None if entity.items.first().is_some_and(|s| s.is_some()) => 8,
        Some((_, r)) if !(tank.accepts(r.output) && tank.amount + r.amount <= tank.capacity) => 4,
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    match recipe {
        Some((stack, r)) if tank.accepts(r.output) && tank.amount + r.amount <= tank.capacity => {
            if energy.consume(chem::OXIDIZER_USAGE) {
                active = true;
                progress += 1;
                if progress >= chem::OXIDIZER_TICKS {
                    progress = 0;
                    tank.insert(r.output, r.amount);
                    let left = stack.count - r.input_count;
                    entity.items[0] = (left > 0).then(|| ItemStack { count: left, ..stack });
                }
            }
        }
        _ => progress = 0,
    }
    push_chemical(game, pos, &mut tank, None, u64::MAX, false);
    // `RecipeError`s (bit 1 input, 2 output room, 3 no recipe): an item without a recipe, or a tank without room.
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "output", &tank);
    entity.data = vec![
        (energy.stored * 1000 / capacity) as i32,
        1000,
        progress as i32,
        chem::OXIDIZER_TICKS as i32,
    ];
    Some(active)
}

/// The Chemical Dissolution Chamber: an item in the first slot and `per_tick` of the chemical piped in,
/// every tick for 100 ticks, make the output chemical, pushed out to the tanks beside it.
fn tick_dissolution(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = machine::STORAGE_MULTIPLIER * nuclear::DISSOLUTION_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut input = get_tank(&entity.nbt, "input", nuclear::DISSOLUTION_TANK);
    let mut output = get_tank(&entity.nbt, "output", nuclear::DISSOLUTION_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    fill_tank_from_item(entity, 3, &mut input, nuclear::dissolves_with);
    drain_tank_into_item(entity, 4, &mut output);
    let recipe = entity.items.first().cloned().flatten().and_then(|stack| {
        let items = world_items!(game);
        let in_tag = |tag: &str, item: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
        nuclear::find_dissolution(&stack.item, stack.count, &in_tag).map(|r| (stack, *r))
    });
    let warn = match &recipe {
        None if entity.items.first().is_some_and(|s| s.is_some()) => 8,
        Some((_, r)) => {
            u32::from(!(input.chemical.as_deref() == Some(r.chemical) && input.amount >= r.per_tick)) << 1
                | u32::from(!(output.accepts(r.output) && output.amount + r.amount <= output.capacity)) << 2
        }
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    match recipe {
        Some((stack, r))
            if input.chemical.as_deref() == Some(r.chemical)
                && input.amount >= r.per_tick
                && output.accepts(r.output)
                && output.amount + r.amount <= output.capacity =>
        {
            if energy.consume(nuclear::DISSOLUTION_USAGE) {
                active = true;
                input.extract(r.per_tick);
                progress += 1;
                if progress >= nuclear::DISSOLUTION_TICKS {
                    progress = 0;
                    output.insert(r.output, r.amount);
                    let left = stack.count - r.count;
                    entity.items[0] = (left > 0).then(|| ItemStack { count: left, ..stack });
                }
            }
        }
        Some(_) => {}
        None => progress = 0,
    }
    push_chemical(game, pos, &mut output, None, u64::MAX, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "input", &input);
    put_tank(&mut entity.nbt, "output", &output);
    entity.data = vec![
        (energy.stored * 1000 / capacity) as i32,
        1000,
        progress as i32,
        nuclear::DISSOLUTION_TICKS as i32,
    ];
    Some(active)
}

/// The Isotopic Centrifuge: each tick it turns its input (uranium hexafluoride, nuclear waste) into its
/// output and pushes that to the tanks beside it.
fn tick_centrifuge(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = machine::STORAGE_MULTIPLIER * nuclear::CENTRIFUGE_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut input = get_tank(&entity.nbt, "input", nuclear::CENTRIFUGE_TANK);
    let mut output = get_tank(&entity.nbt, "output", nuclear::CENTRIFUGE_TANK);
    let mut active = false;
    fill_tank_from_item(entity, 1, &mut input, |c| nuclear::centrifuging(c).is_some());
    drain_tank_into_item(entity, 2, &mut output);
    if let Some((need, made, amount)) = input.chemical.as_deref().and_then(nuclear::centrifuging)
        && input.amount >= need
        && output.accepts(made)
        && output.amount + amount <= output.capacity
        && energy.consume(nuclear::CENTRIFUGE_USAGE)
    {
        input.extract(need);
        output.insert(made, amount);
        active = true;
    }
    push_chemical(game, pos, &mut output, None, u64::MAX, false);
    let warn = match input.chemical.as_deref().and_then(nuclear::centrifuging) {
        None if input.chemical.is_some() => 8,
        Some((need, made, amount)) => {
            u32::from(input.amount < need) << 1 | u32::from(!(output.accepts(made) && output.amount + amount <= output.capacity)) << 2
        }
        None => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "input", &input);
    put_tank(&mut entity.nbt, "output", &output);
    entity.data = vec![
        (energy.stored * 1000 / capacity) as i32,
        1000,
        (input.amount * 1000 / nuclear::CENTRIFUGE_TANK) as i32,
        1000,
    ];
    Some(active)
}

/// The Chemical Washer: each tick, a dirty slurry piped in and water from pipes make the clean slurry,
/// pushed out to the tanks and tubes beside it.
fn tick_washer(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = machine::STORAGE_MULTIPLIER * nuclear::WASHER_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut input = get_tank(&entity.nbt, "input", nuclear::WASHER_TANK);
    let mut water = get_tank(&entity.nbt, "fluid", nuclear::WASHER_TANK);
    let mut output = get_tank(&entity.nbt, "output", nuclear::WASHER_TANK);
    let mut active = false;
    drain_tank_into_item(entity, 1, &mut output);
    // `FluidInventorySlot.fill`: a water bucket empties into the tank, the empty one goes to the last slot.
    fill_tank_from_bucket(game, entity, 3, Some(4), &mut water, |f| f == "minecraft:water");
    if let Some(r) = input.chemical.as_deref().and_then(nuclear::find_washing)
        && input.amount >= r.input_amount
        && water.chemical.as_deref() == Some("minecraft:water")
        && water.amount >= r.water
        && output.accepts(r.output)
        && output.amount + r.amount <= output.capacity
        && energy.consume(nuclear::WASHER_USAGE)
    {
        input.extract(r.input_amount);
        water.extract(r.water);
        output.insert(r.output, r.amount);
        active = true;
    }
    push_chemical(game, pos, &mut output, None, u64::MAX, false);
    let warn = match input.chemical.as_deref().map(nuclear::find_washing) {
        Some(None) => 8,
        Some(Some(r)) => {
            u32::from(input.amount < r.input_amount || water.chemical.as_deref() != Some("minecraft:water") || water.amount < r.water) << 1
                | u32::from(!(output.accepts(r.output) && output.amount + r.amount <= output.capacity)) << 2
        }
        None => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "input", &input);
    put_tank(&mut entity.nbt, "fluid", &water);
    put_tank(&mut entity.nbt, "output", &output);
    Some(active)
}

/// The Chemical Crystallizer: over 200 ticks, a clean slurry (or brine) piped in becomes its crystal (or
/// salt) in the last slot.
fn tick_crystallizer(entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = machine::STORAGE_MULTIPLIER * nuclear::CRYSTALLIZER_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut input = get_tank(&entity.nbt, "input", nuclear::CRYSTALLIZER_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    fill_tank_from_item(entity, 3, &mut input, |c| nuclear::find_crystallizing(c).is_some());
    let out = entity.items.get(2).cloned().flatten();
    match input.chemical.as_deref().and_then(nuclear::find_crystallizing) {
        Some(r)
            if input.amount >= r.amount
                && out.as_ref().is_none_or(|o| &*o.item == r.output && o.count + r.count <= 64) =>
        {
            if energy.consume(nuclear::CRYSTALLIZER_USAGE) {
                active = true;
                progress += 1;
                if progress >= nuclear::CRYSTALLIZER_TICKS {
                    progress = 0;
                    input.extract(r.amount);
                    let count = out.map_or(0, |o| o.count) + r.count;
                    entity.items[2] = Some(stack_of(r.output, count));
                }
            }
        }
        _ => progress = 0,
    }
    let room = entity.items.get(2).cloned().flatten();
    let warn = match input.chemical.as_deref().map(nuclear::find_crystallizing) {
        Some(None) => 8,
        Some(Some(r)) => {
            u32::from(input.amount < r.amount) << 1 | u32::from(!room.as_ref().is_none_or(|o| &*o.item == r.output && o.count + r.count <= 64)) << 2
        }
        None => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "input", &input);
    entity.data = vec![
        (energy.stored * 1000 / capacity) as i32,
        1000,
        progress as i32,
        nuclear::CRYSTALLIZER_TICKS as i32,
    ];
    Some(active)
}

/// An advanced electric machine: the Purification Chamber (ore + 1 mB oxygen a tick → 3 clumps) or the
/// Osmium Compressor (refined obsidian or glowstone dust + osmium → ingot), 200 ticks. Its chemical comes
/// from the blocks beside it, or from items in the middle slot (`chemical_conversion`: flint → oxygen,
/// osmium ingot → osmium).
fn tick_advanced(game: &mut Game<'_>, entity: &mut ModBlockEntity, m: &'static chem::AdvancedMachine) -> Option<bool> {
    let ticks = chem::ADVANCED_TICKS;
    let capacity = m.usage * u64::from(ticks);
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut tank = get_tank(&entity.nbt, "chemical", chem::ADVANCED_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    let items = world_items!(game);
    let in_tag = |tag: &str, item: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
    if let Some(Some(extra)) = entity.items.get(1).cloned()
        && let Some(c) = mekanism_core::recipes::find_conversion(&extra.item, &in_tag)
        && m.chemicals.contains(&c.output)
        && tank.accepts(c.output)
        && tank.amount + c.amount <= tank.capacity
    {
        tank.insert(c.output, c.amount);
        entity.items[1] = (extra.count > 1).then(|| ItemStack { count: extra.count - 1, ..extra });
    }
    let recipe = entity.items.first().cloned().flatten().and_then(|stack| {
        // With a chemical in the tank, only its recipes (the injection chamber takes two chemicals).
        let held = tank.chemical.as_deref();
        m.table
            .iter()
            .find(|r| {
                held.is_none_or(|c| c == r.chemical)
                    && stack.count >= r.input_count
                    && r.input.matches(&stack.item, &in_tag)
            })
            .map(|r| (stack, *r))
    });
    let out = entity.items.get(2).cloned().flatten();
    let warn = match (&recipe, entity.items.first().is_some_and(|s| s.is_some())) {
        (None, true) => 8,
        (Some((_, r)), _) => {
            let max = items.get(r.output).map_or(64, |i| i.max_stack);
            let fits = out.as_ref().is_none_or(|o| &*o.item == r.output && o.count + r.output_count <= max);
            u32::from(!(tank.chemical.as_deref() == Some(r.chemical) && tank.amount >= r.chemical_amount)) << 1 | u32::from(!fits) << 2
        }
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    match recipe {
        Some((stack, r)) => {
            let max = items.get(r.output).map_or(64, |i| i.max_stack);
            let fits = match &out {
                None => true,
                Some(o) => &*o.item == r.output && o.count + r.output_count <= max,
            };
            if fits
                && tank.chemical.as_deref() == Some(r.chemical)
                && tank.amount >= r.chemical_amount
                && energy.consume(m.usage)
            {
                active = true;
                tank.extract(r.chemical_amount);
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    let left = stack.count - r.input_count;
                    entity.items[0] = (left > 0).then(|| ItemStack { count: left, ..stack });
                    entity.items[2] = Some(match out {
                        Some(o) => ItemStack { count: o.count + r.output_count, ..o },
                        None => stack_of(r.output, r.output_count),
                    });
                }
            }
        }
        None => progress = 0,
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "chemical", &tank);
    entity.data = vec![
        (energy.stored * 1000 / capacity) as i32,
        1000,
        progress as i32,
        ticks as i32,
    ];
    Some(active)
}

/// The Antiprotonic Nucleosynthesizer (`TileEntityAntiprotonicNucleosynthesizer`): antimatter
/// pellets in the middle slot fill its tank (`chemical_conversion`), and an item with antimatter turns
/// into another over the recipe's duration, 15 FE a tick, the antimatter used when the operation
/// finishes.
fn tick_nucleosynthesizer(game: &mut Game<'_>, entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = machine::STORAGE_MULTIPLIER * chem::NUCLEOSYNTHESIZER_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut tank = get_tank(&entity.nbt, "chemical", chem::NUCLEOSYNTHESIZER_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut ticks = 0;
    let mut active = false;
    let items = world_items!(game);
    let in_tag = |tag: &str, item: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
    if let Some(Some(extra)) = entity.items.get(1).cloned()
        && let Some(c) = mekanism_core::recipes::find_conversion(&extra.item, &in_tag)
        && c.output == chem::ANTIMATTER
        && tank.accepts(c.output)
        && tank.amount + c.amount <= tank.capacity
    {
        tank.insert(c.output, c.amount);
        entity.items[1] = (extra.count > 1).then(|| ItemStack { count: extra.count - 1, ..extra });
    }
    let recipe = entity.items.first().cloned().flatten().and_then(|stack| {
        mekanism_core::recipe_table::NUCLEOSYNTHESIZING
            .iter()
            .find(|r| stack.count >= r.recipe.input_count && r.recipe.input.matches(&stack.item, &in_tag))
            .map(|r| (stack, *r))
    });
    let out = entity.items.get(2).cloned().flatten();
    match recipe {
        Some((stack, n)) => {
            let r = n.recipe;
            ticks = n.duration;
            let max = items.get(r.output).map_or(64, |i| i.max_stack);
            let fits = match &out {
                None => true,
                Some(o) => &*o.item == r.output && o.count + r.output_count <= max,
            };
            if fits
                && tank.chemical.as_deref() == Some(r.chemical)
                && tank.amount >= r.chemical_amount
                && energy.consume(chem::NUCLEOSYNTHESIZER_USAGE)
            {
                active = true;
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    tank.extract(r.chemical_amount);
                    let left = stack.count - r.input_count;
                    entity.items[0] = (left > 0).then(|| ItemStack { count: left, ..stack });
                    entity.items[2] = Some(match out {
                        Some(o) => ItemStack { count: o.count + r.output_count, ..o },
                        None => stack_of(r.output, r.output_count),
                    });
                }
            }
        }
        None => progress = 0,
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "chemical", &tank);
    entity.data = vec![(energy.stored * 1000 / capacity) as i32, 1000, progress as i32, ticks as i32];
    Some(active)
}

/// The Chemical Infuser: one operation a tick of two chemicals into a third (oxygen + sulfur dioxide →
/// sulfur trioxide), 15 FE a tick, pushed out to the blocks beside it. Each input tank takes one of the
/// two chemicals (`chemical_receiver`).
fn tick_chemical_infuser(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let cap = chem::CHEMICAL_INFUSER_TANK;
    let capacity = machine::STORAGE_MULTIPLIER * chem::CHEMICAL_INFUSER_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut left = get_tank(&entity.nbt, "left", cap);
    let mut right = get_tank(&entity.nbt, "right", cap);
    let mut out = get_tank(&entity.nbt, "output", cap);
    let mut active = false;
    fill_tank_from_item(entity, 1, &mut left, mekanism_core::recipes::infusing_input);
    fill_tank_from_item(entity, 2, &mut right, mekanism_core::recipes::infusing_input);
    drain_tank_into_item(entity, 3, &mut out);
    if let (Some(a), Some(b)) = (left.chemical.clone(), right.chemical.clone())
        && let Some((r, swapped)) = mekanism_core::recipes::find_chemical_infusing(&a, &b)
    {
        let (need_a, need_b) = if swapped { (r.right_amount, r.left_amount) } else { (r.left_amount, r.right_amount) };
        if left.amount >= need_a
            && right.amount >= need_b
            && out.accepts(r.output)
            && out.amount + r.amount <= out.capacity
            && energy.consume(chem::CHEMICAL_INFUSER_USAGE)
        {
            left.extract(need_a);
            right.extract(need_b);
            out.insert(r.output, r.amount);
            active = true;
        }
    }
    push_chemical(game, pos, &mut out, None, u64::MAX, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "left", &left);
    put_tank(&mut entity.nbt, "right", &right);
    put_tank(&mut entity.nbt, "output", &out);
    Some(active)
}

/// The Metallurgic Infuser: the middle slot's item (coal, redstone, diamond dust...) turns into infuse
/// chemical in a 1,000 mB tank (`chemical_conversion`), and an item with enough of it becomes the
/// recipe's output after 200 ticks at 5 FE a tick (`metallurgic_infusing`: iron + 10 carbon → enriched
/// iron).
fn tick_infuser(game: &mut Game<'_>, entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = chem::INFUSER_USAGE * u64::from(chem::INFUSER_TICKS);
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut tank = get_tank(&entity.nbt, "infuse", chem::INFUSER_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    let items = world_items!(game);
    let in_tag = |tag: &str, item: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
    // ChemicalInventorySlot: one item at a time, when all of its chemical fits.
    if let Some(Some(infuse)) = entity.items.get(1).cloned()
        && let Some(c) = mekanism_core::recipes::find_conversion(&infuse.item, &in_tag)
        && tank.accepts(c.output)
        && tank.amount + c.amount <= tank.capacity
    {
        tank.insert(c.output, c.amount);
        entity.items[1] = (infuse.count > 1).then(|| ItemStack { count: infuse.count - 1, ..infuse });
    }
    let recipe = match (entity.items.first().cloned().flatten(), tank.chemical.clone()) {
        (Some(stack), Some(chemical)) => {
            mekanism_core::recipes::find_infusing(&stack.item, stack.count, &chemical, &in_tag).map(|r| (stack, *r))
        }
        _ => None,
    };
    let out = entity.items.get(2).cloned().flatten();
    let warn = match (&recipe, entity.items.first().is_some_and(|s| s.is_some())) {
        (None, true) if tank.chemical.is_none() => 2,
        (None, true) => 8,
        (Some((_, r)), _) => {
            let max = items.get(r.output).map_or(64, |i| i.max_stack);
            let fits = out.as_ref().is_none_or(|o| &*o.item == r.output && o.count + r.output_count <= max);
            u32::from(tank.amount < r.chemical_amount) << 1 | u32::from(!fits) << 2
        }
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    match recipe {
        Some((stack, r)) if tank.amount >= r.chemical_amount => {
            let max = items.get(r.output).map_or(64, |i| i.max_stack);
            let fits = match &out {
                None => true,
                Some(o) => &*o.item == r.output && o.count + r.output_count <= max,
            };
            if fits && energy.consume(chem::INFUSER_USAGE) {
                active = true;
                progress += 1;
                if progress >= chem::INFUSER_TICKS {
                    progress = 0;
                    tank.extract(r.chemical_amount);
                    let left = stack.count - r.input_count;
                    entity.items[0] = (left > 0).then(|| ItemStack { count: left, ..stack });
                    entity.items[2] = Some(match out {
                        Some(o) => ItemStack { count: o.count + r.output_count, ..o },
                        None => stack_of(r.output, r.output_count),
                    });
                }
            }
        }
        _ => progress = 0,
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "infuse", &tank);
    // The flame shows the infuse tank, the arrow the progress.
    entity.data = vec![
        (tank.amount * 1000 / chem::INFUSER_TANK) as i32,
        1000,
        progress as i32,
        chem::INFUSER_TICKS as i32,
    ];
    Some(active)
}

/// The Electrolytic Separator: water or brine buckets (or brine from a Thermal Evaporation Plant) fill
/// its 24,000 mB tank, and each tick runs one `separating` operation at 4 FE: 2 mB water → 2 hydrogen +
/// 1 oxygen, or 10 mB brine → 1 sodium + 1 chlorine. The chemicals go out to the blocks beside it.
fn tick_separator(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = machine::STORAGE_MULTIPLIER * chem::SEPARATOR_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut fluid = get_tank(&entity.nbt, "fluid", chem::SEPARATOR_FLUID);
    // FluidInventorySlot.fill: a bucket empties into the tank when it fits and the bucket has
    // somewhere to go.
    // The empty bucket stays in the slot (there is no output slot for it).
    fill_tank_from_bucket(game, entity, 0, None, &mut fluid, |f| matches!(f, "minecraft:water" | "mekanism:brine"));
    let mut left = get_tank(&entity.nbt, "left", chem::SEPARATOR_GAS);
    let mut right = get_tank(&entity.nbt, "right", chem::SEPARATOR_GAS);
    drain_tank_into_item(entity, 3, &mut left);
    drain_tank_into_item(entity, 4, &mut right);
    let warn = if fluid.amount > 0 && (left.amount >= left.capacity || right.amount >= right.capacity) { 4 } else { 0 };
    put_int(&mut entity.nbt, "warn", warn);
    let active = chem::separate(&mut fluid, &mut left, &mut right, &mut energy);
    push_chemical(game, pos, &mut left, None, u64::MAX, false);
    push_chemical(game, pos, &mut right, None, u64::MAX, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "fluid", &fluid);
    put_tank(&mut entity.nbt, "left", &left);
    put_tank(&mut entity.nbt, "right", &right);
    // The flame shows the fluid, the arrow the left gas tank.
    entity.data = vec![
        (fluid.amount * 1000 / chem::SEPARATOR_FLUID) as i32,
        1000,
        (left.amount * 1000 / chem::SEPARATOR_GAS) as i32,
        1000,
    ];
    Some(active)
}

/// The fluid in a bucket item: vanilla's water or lava bucket, or a Mekanism `<fluid>_bucket`.
fn bucket_fluid(item: &str) -> Option<String> {
    match item {
        "minecraft:water_bucket" => return Some("minecraft:water".into()),
        "minecraft:lava_bucket" => return Some("minecraft:lava".into()),
        _ => {}
    }
    let fluid = item.strip_suffix("_bucket")?;
    fluid.starts_with("mekanism:").then(|| fluid.to_owned())
}

/// The Rotary Condensentrator turning fluid into chemical (`rotary` recipes: water → water vapour),
/// 1 mB a tick at 2 FE, from buckets in its first slot (the empty bucket comes out of the last). The
/// chemical goes out to the blocks beside it. Condensentrating (chemical → fluid) needs a mode switch
/// and fluid outputs, which come later.
fn tick_rotary(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = machine::STORAGE_MULTIPLIER * chem::ROTARY_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut fluid = get_tank(&entity.nbt, "fluid", chem::ROTARY_TANK);
    let mut gas = get_tank(&entity.nbt, "chemical", chem::ROTARY_TANK);
    if let Some(Some(bucket)) = entity.items.first().cloned()
        && let Some(f) = bucket_fluid(&bucket.item)
        && chem::rotary_chemical(&f).is_some()
        && fluid.accepts(&f)
        && fluid.amount + 1_000 <= fluid.capacity
        && give_back_bucket(entity)
    {
        fluid.insert(&f, 1_000);
        entity.items[0] = None;
    }
    // `ChemicalInventorySlot.rotary`: the gas drains into a tank item, which moves to the output slot once full.
    drain_tank_into_item(entity, 3, &mut gas);
    move_full_chemical_item(entity, 3, 4);
    let warn = match fluid.chemical.as_deref().filter(|_| fluid.amount > 0).map(chem::rotary_chemical) {
        Some(None) => 8,
        Some(Some(output)) if !(gas.accepts(output) && gas.amount < gas.capacity) => 4,
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    let mut active = false;
    if let Some(output) = fluid.chemical.as_deref().and_then(chem::rotary_chemical)
        && fluid.amount > 0
        && gas.accepts(output)
        && gas.amount < gas.capacity
        && energy.consume(chem::ROTARY_USAGE)
    {
        fluid.extract(1);
        gas.insert(output, 1);
        active = true;
    }
    push_chemical(game, pos, &mut gas, None, u64::MAX, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "fluid", &fluid);
    put_tank(&mut entity.nbt, "chemical", &gas);
    entity.data = vec![
        (fluid.amount * 1000 / chem::ROTARY_TANK) as i32,
        1000,
        (gas.amount * 1000 / chem::ROTARY_TANK) as i32,
        1000,
    ];
    Some(active)
}

/// Puts an empty bucket into the last slot, if it has room for one.
/// The bucket that holds `fluid`: the reverse of `bucket_fluid`.
fn fluid_bucket(fluid: &str) -> Option<String> {
    match fluid {
        "minecraft:water" => Some("minecraft:water_bucket".into()),
        "minecraft:lava" => Some("minecraft:lava_bucket".into()),
        f if f.starts_with("mekanism:") => Some(format!("{f}_bucket")),
        _ => None,
    }
}

fn give_back_bucket(entity: &mut ModBlockEntity) -> bool {
    let out = entity.items.get(2).cloned().flatten();
    match out {
        None => entity.items[2] = Some(stack_of("minecraft:bucket", 1)),
        Some(s) if &*s.item == "minecraft:bucket" && s.count < 16 => {
            entity.items[2] = Some(ItemStack { count: s.count + 1, ..s })
        }
        Some(_) => return false,
    }
    true
}

/// One of `item` into slot `index`, when it is empty or holds the same item with room.
fn put_item(game: &Game<'_>, entity: &mut ModBlockEntity, index: usize, item: &str) -> bool {
    match entity.items.get(index).cloned().flatten() {
        None => match entity.items.get_mut(index) {
            Some(slot) => *slot = Some(stack_of(item, 1)),
            None => return false,
        },
        Some(s) if &*s.item == item && s.count < max_stack(game, item) => entity.items[index] = Some(ItemStack { count: s.count + 1, ..s }),
        Some(_) => return false,
    }
    true
}

/// One item out of slot `index`.
fn take_one(entity: &mut ModBlockEntity, index: usize) {
    if let Some(Some(s)) = entity.items.get(index).cloned() {
        entity.items[index] = (s.count > 1).then(|| ItemStack { count: s.count - 1, ..s });
    }
}

/// `FluidInventorySlot.fill`: a full bucket in slot `from` empties into `tank` (1000 mB, when `accepts`
/// the fluid and it fits). The empty bucket goes to slot `out`, or stays in `from` without one.
fn fill_tank_from_bucket(game: &Game<'_>, entity: &mut ModBlockEntity, from: usize, out: Option<usize>, tank: &mut ChemicalTank, accepts: impl Fn(&str) -> bool) -> bool {
    let Some(Some(bucket)) = entity.items.get(from).cloned() else {
        return false;
    };
    let Some(fluid) = bucket_fluid(&bucket.item) else {
        return false;
    };
    if !accepts(&fluid) || !tank.accepts(&fluid) || tank.amount + 1_000 > tank.capacity {
        return false;
    }
    match out {
        Some(o) => {
            if !put_item(game, entity, o, "minecraft:bucket") {
                return false;
            }
            take_one(entity, from);
        }
        None if bucket.count == 1 => entity.items[from] = Some(stack_of("minecraft:bucket", 1)),
        None => return false,
    }
    tank.insert(&fluid, 1_000);
    true
}

/// `FluidInventorySlot.drain`: an empty bucket in slot `from` fills from `tank` (1000 mB). The full
/// bucket goes to slot `out`, or replaces a lone empty one in `from` without one.
fn drain_tank_into_bucket(game: &Game<'_>, entity: &mut ModBlockEntity, from: usize, out: Option<usize>, tank: &mut ChemicalTank) -> bool {
    let Some(Some(bucket)) = entity.items.get(from).cloned() else {
        return false;
    };
    if &*bucket.item != "minecraft:bucket" || tank.amount < 1_000 {
        return false;
    }
    let Some(full) = tank.chemical.as_deref().and_then(fluid_bucket) else {
        return false;
    };
    match out {
        Some(o) => {
            if !put_item(game, entity, o, &full) {
                return false;
            }
            take_one(entity, from);
        }
        None if bucket.count == 1 => entity.items[from] = Some(stack_of(&full, 1)),
        None => return false,
    }
    tank.extract(1_000);
    true
}

/// The capacity of a chemical tank item (`Capabilities.CHEMICAL.item()`): the tiers of chemical tank.
fn chemical_item_capacity(item: &str) -> Option<u64> {
    match kind(item)? {
        Kind::ChemicalTank(t) => Some(chem::chemical_tank(t).0),
        _ => None,
    }
}

/// `drainTankIntoSlot(outputSlot)`: a chemical tank item in slot `from` that is full goes to the empty slot `to`.
fn move_full_chemical_item(entity: &mut ModBlockEntity, from: usize, to: usize) {
    let Some(Some(stack)) = entity.items.get(from).cloned() else {
        return;
    };
    let Some(capacity) = chemical_item_capacity(&stack.item) else {
        return;
    };
    if entity.items.get(to).is_some_and(|s| s.is_none()) && item_tank(&stack, CHEMICAL_KEY, capacity).is_some_and(|t| t.amount >= capacity) {
        entity.items[to] = Some(stack);
        entity.items[from] = None;
    }
}

/// The ids of the items a chemical slot takes: the chemical tanks.
fn chemical_item_ids() -> Vec<String> {
    BaseTier::ALL.into_iter().map(|t| format!("mekanism:{}_chemical_tank", t.name())).collect()
}

/// Writes `tank` as the chemical an item holds (its custom data under `CHEMICAL_KEY`).
fn set_item_tank(stack: &mut ItemStack, tank: &ChemicalTank) {
    let mut c = Compound::new();
    put_tank(&mut c, "t", tank);
    let value = match c.get(&JavaString::from("t")) {
        Some(Tag::Compound(t)) => t.clone(),
        _ => Compound::new(),
    };
    set_custom_compound(stack, CHEMICAL_KEY, value);
}

/// `ChemicalInventorySlot.fill` (`fillTankFromSlot`): a chemical tank item in slot `from` gives what it holds
/// to `tank` while the tank takes it (`accepts`) and has room.
fn fill_tank_from_item(entity: &mut ModBlockEntity, from: usize, tank: &mut ChemicalTank, accepts: impl Fn(&str) -> bool) -> bool {
    let Some(Some(mut stack)) = entity.items.get(from).cloned() else {
        return false;
    };
    let Some(capacity) = chemical_item_capacity(&stack.item) else {
        return false;
    };
    let Some(mut held) = item_tank(&stack, CHEMICAL_KEY, capacity) else {
        return false;
    };
    let Some(chemical) = held.chemical.clone() else {
        return false;
    };
    if !accepts(&chemical) || !tank.accepts(&chemical) {
        return false;
    }
    let moved = held.amount.min(tank.capacity - tank.amount);
    if moved == 0 {
        return false;
    }
    tank.insert(&chemical, moved);
    held.extract(moved);
    set_item_tank(&mut stack, &held);
    entity.items[from] = Some(stack);
    true
}

/// `ChemicalInventorySlot.drain` (`drainTankIntoSlot`): `tank`'s chemical goes into a chemical tank item in
/// slot `from` that is empty or holds the same chemical, while it has room.
fn drain_tank_into_item(entity: &mut ModBlockEntity, from: usize, tank: &mut ChemicalTank) -> bool {
    let Some(Some(mut stack)) = entity.items.get(from).cloned() else {
        return false;
    };
    let (Some(capacity), Some(chemical)) = (chemical_item_capacity(&stack.item), tank.chemical.clone()) else {
        return false;
    };
    let mut held = item_tank(&stack, CHEMICAL_KEY, capacity).unwrap_or_else(|| ChemicalTank::new(capacity));
    if !held.accepts(&chemical) {
        return false;
    }
    let moved = tank.amount.min(held.capacity - held.amount);
    if moved == 0 {
        return false;
    }
    held.insert(&chemical, moved);
    tank.extract(moved);
    set_item_tank(&mut stack, &held);
    entity.items[from] = Some(stack);
    true
}

// --- Digital Miner ----------------------------------------------------------------------------------

/// The first of the Digital Miner's 27 internal slots (`TileEntityDigitalMiner`'s `inventory`); slot 1 is its
/// energy slot.
const MINER_FIRST: usize = 3;
const MINER_SLOTS: usize = 27;
/// The searcher's states (`ThreadMinerSearch.State`): not started, searching, finished.
const MINER_IDLE: u32 = 0;
const MINER_SEARCHING: u32 = 1;
const MINER_FINISHED: u32 = 2;
const MINER_CHANNEL: &str = "mekanism:miner";

fn get_i32(nbt: &Compound, key: &str, default: i32) -> i32 {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::Int(v)) => *v,
        _ => default,
    }
}

fn miner_region(nbt: &Compound) -> miner::Region {
    let d = miner::Region::default();
    miner::Region { radius: get_i32(nbt, "radius", d.radius), min_y: get_i32(nbt, "min_y", d.min_y), max_y: get_i32(nbt, "max_y", d.max_y) }
}

fn put_miner_region(nbt: &mut Compound, r: miner::Region) {
    for (k, v) in [("radius", r.radius), ("min_y", r.min_y), ("max_y", r.max_y)] {
        nbt.insert(k.into(), Tag::Int(v));
    }
}

fn miner_filters(nbt: &Compound) -> Vec<miner::Filter> {
    strings_of(nbt, "filters").iter().filter_map(|l| miner::Filter::load(l)).collect()
}

fn put_miner_filters(nbt: &mut Compound, filters: &[miner::Filter]) {
    let lines: Vec<String> = filters.iter().map(miner::Filter::save).collect();
    nbt.insert("filters".into(), string_list(&lines));
}

/// What the miner uses a tick: the upgrades' usage, times silk touch's multiplier and the region's cost.
fn miner_usage(nbt: &Compound) -> u64 {
    let base = upgrades(nbt).energy_per_tick(miner::ENERGY_USAGE);
    miner_region(nbt).energy_per_tick(base, get_int(nbt, "silk") == 1)
}

fn miner_found(nbt: &Compound) -> Vec<i32> {
    match nbt.get(&JavaString::from("found")) {
        Some(Tag::IntArray(v)) => v.clone(),
        _ => vec![],
    }
}

/// `TileEntityDigitalMiner.reset`: forgets the search and what it found; the miner stops.
fn miner_reset(nbt: &mut Compound) {
    for key in ["mstate", "cursor", "running", "delay", "to_mine"] {
        put_int(nbt, key, 0);
    }
    nbt.remove(&JavaString::from("found"));
    nbt.remove(&JavaString::from("missing"));
}

/// Puts `stack` into the miner's internal slots (a stack merges into the same item first); `None` when it
/// all fitted, else what is left.
fn miner_insert(game: &Game<'_>, items: &mut [Option<ItemStack>], mut stack: ItemStack) -> Option<ItemStack> {
    let max = max_stack(game, &stack.item);
    let same = |a: &ItemStack, b: &ItemStack| a.item == b.item && a.damage == b.damage && a.extra == b.extra;
    for slot in items.iter_mut().skip(MINER_FIRST).take(MINER_SLOTS) {
        if let Some(have) = slot
            && same(have, &stack)
            && have.count < max
        {
            let moved = (max - have.count).min(stack.count);
            have.count += moved;
            stack.count -= moved;
            if stack.count == 0 {
                return None;
            }
        }
    }
    for slot in items.iter_mut().skip(MINER_FIRST).take(MINER_SLOTS) {
        if slot.is_none() {
            let moved = max.min(stack.count);
            *slot = Some(ItemStack { count: moved, ..stack.clone() });
            stack.count -= moved;
            if stack.count == 0 {
                return None;
            }
        }
    }
    Some(stack)
}

/// The block ids the miner never breaks (`MekanismTags.Blocks.MINER_BLACKLIST`, `isAir`).
fn miner_skips(name: &str, hardness: f32) -> bool {
    hardness < 0.0 || matches!(name, "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air")
}

/// The back of the miner (`getOppositeDirection`) as a step.
fn miner_back(game: &Game<'_>, pos: [i32; 3]) -> [i32; 3] {
    let d = game.native_block_property(pos, "facing").and_then(|f| direction(&f)).unwrap_or([0, 0, 1]);
    [-d[0], 0, -d[2]]
}

/// The Digital Miner (`TileEntityDigitalMiner`): Start searches its region for the blocks its filters name
/// (everything else in inverse mode) and, once the search is finished, mines them from the highest index
/// every `ticksPerMine` (speed upgrades shorten it, energy upgrades cut the usage). What a block drops must
/// fit in its 27 slots, else it waits; a filter's replacement block is put where the block was. With
/// Auto-eject on, a stack goes to the inventory behind it every 10 ticks.
fn tick_miner(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let u = upgrades(&entity.nbt);
    let usage = miner_usage(&entity.nbt);
    let delay_length = u.ticks(miner::TICKS_PER_MINE).max(1);
    let region = miner_region(&entity.nbt);
    let mut energy = EnergyContainer::new(miner::STORAGE.max(usage), u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(energy.capacity);
    discharge_slot(entity, 1, &mut energy);
    let mut state = get_int(&entity.nbt, "mstate");
    let mut found = miner_found(&entity.nbt);
    // The searcher thread: a slice of the region a tick, from the top down.
    if state == MINER_SEARCHING {
        let filters = miner_filters(&entity.nbt);
        let inverse = get_int(&entity.nbt, "inverse") == 1;
        let item_tags = world_items!(game).item_tags.clone();
        let in_tag = |tag: &str, block: &str| item_tags.get(tag).is_some_and(|t| t.contains(block));
        let cursor = get_int(&entity.nbt, "cursor");
        let total = region.total_size();
        let end = cursor.saturating_add(miner::SCAN_LIMIT).min(total);
        for index in region.search_order(cursor, miner::SCAN_LIMIT) {
            let at = region.position(pos, index);
            if at == pos {
                continue;
            }
            let block = game.world.block(at[0], at[1], at[2]);
            let facts = game.res.facts().get(block);
            if miner_skips(&facts.name, facts.hardness) {
                continue;
            }
            let matched = miner::matching_filter(&filters, &facts.name, &in_tag).is_some();
            if inverse != matched {
                found.push(index as i32);
            }
        }
        put_int(&mut entity.nbt, "cursor", end);
        if end >= total {
            state = MINER_FINISHED;
            put_int(&mut entity.nbt, "mstate", state);
        }
        entity.nbt.insert("found".into(), Tag::IntArray(found.clone()));
    }
    let running = get_int(&entity.nbt, "running") == 1;
    let mut delay = get_int(&entity.nbt, "delay").min(delay_length);
    let mut active = false;
    if running && state == MINER_FINISHED {
        if found.is_empty() {
            put_int(&mut entity.nbt, "running", 0);
        } else if energy.consume(usage) {
            active = true;
            delay = delay.saturating_sub(1);
            if delay == 0 {
                mine_next(game, pos, entity, &mut found);
                entity.nbt.insert("found".into(), Tag::IntArray(found.clone()));
                delay = delay_length;
            }
        }
    }
    // Auto-eject (`doEject`): a stack to the inventory behind it (one block up and two back), every 10 ticks.
    if get_int(&entity.nbt, "eject") == 1 && game.ticks % 10 == 0 {
        let back = miner_back(game, pos);
        let target = [pos[0] + 2 * back[0], pos[1] + 1, pos[2] + 2 * back[2]];
        let face = horizon_core::geometry::Direction::ALL[face_index([-back[0], 0, -back[2]])];
        if let Some(i) = (MINER_FIRST..MINER_FIRST + MINER_SLOTS).find(|&i| entity.items.get(i).is_some_and(|s| s.is_some())) {
            let stack = entity.items[i].take().unwrap_or_else(|| unreachable!());
            entity.items[i] = game.native_insert_item(target, face, stack);
        }
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "delay", delay);
    put_int(&mut entity.nbt, "to_mine", found.len() as u32);
    put_int(&mut entity.nbt, "warn", u32::from(usage > energy.stored));
    entity.data = vec![
        (energy.stored * 1000 / energy.capacity.max(1)) as i32,
        1000,
        (delay_length - delay) as i32,
        delay_length as i32,
    ];
    Some(active)
}

/// Mines the highest block the search found, if its drops fit in the miner's slots and its replacement
/// (`setReplace`) can be had; otherwise it waits (and the missing block shows on the screen).
fn mine_next(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, found: &mut Vec<i32>) {
    let Some(&index) = found.last() else {
        return;
    };
    let region = miner_region(&entity.nbt);
    let at = region.position(pos, index as u32);
    let state = game.world.block(at[0], at[1], at[2]);
    let facts = game.res.facts().get(state);
    let filters = miner_filters(&entity.nbt);
    let inverse = get_int(&entity.nbt, "inverse") == 1;
    let item_tags = world_items!(game).item_tags.clone();
    let in_tag = |tag: &str, block: &str| item_tags.get(tag).is_some_and(|t| t.contains(block));
    let matched = miner::matching_filter(&filters, &facts.name, &in_tag);
    // The block changed since the search saw it: it is no longer a target.
    if miner_skips(&facts.name, facts.hardness) || inverse == matched.is_some() {
        found.pop();
        return;
    }
    let (target, requires) = match matched {
        Some(f) => (f.replace.clone(), f.requires_replacement),
        None => (get_string(&entity.nbt, "inv_replace").unwrap_or_default(), get_int(&entity.nbt, "inv_require") == 1),
    };
    let drops = if get_int(&entity.nbt, "silk") == 1 {
        vec![stack_of(&facts.name, 1)]
    } else {
        let registry = game.pack.assets.registry;
        match &game.world_tables {
            Some(t) => &t.items,
            None => game.res.items(),
        }
        .block_drops_with(registry, state, None, &[], &mut game.world.random)
    };
    let mut slots = entity.items.clone();
    slots.resize(MINER_FIRST + MINER_SLOTS, None);
    for stack in drops.into_iter().filter(|s| s.count > 0 && &*s.item != "minecraft:air") {
        // Nothing fits: it waits for room (`tryInsert` fails) and the block stays.
        if miner_insert(game, &mut slots, stack).is_some() {
            return;
        }
    }
    // The replacement comes from the miner's own slots first, then the inventory on top with Auto-pull.
    let mut replacement = false;
    if !target.is_empty() {
        if let Some(slot) = slots.iter_mut().skip(MINER_FIRST).take(MINER_SLOTS).find(|s| s.as_ref().is_some_and(|s| *s.item == *target)) {
            if let Some(have) = slot {
                have.count -= 1;
                if have.count == 0 {
                    *slot = None;
                }
            }
            replacement = true;
        } else if get_int(&entity.nbt, "pull") == 1 {
            let wanted = |s: &ItemStack| *s.item == *target;
            replacement = game.native_extract_item_where([pos[0], pos[1] + 2, pos[2]], horizon_core::geometry::Direction::Down, 1, &wanted).is_some();
        }
    }
    if !target.is_empty() && !replacement && requires {
        entity.nbt.insert("missing".into(), Tag::String(target.as_str().into()));
        return;
    }
    entity.nbt.remove(&JavaString::from("missing"));
    entity.items = slots;
    found.pop();
    let block = horizon_core::position::BlockPos::new(at[0], at[1], at[2]);
    game.with_level(|b, level, ticks| b.destroy_block(level, ticks, block, false));
    if replacement && let Some(new) = game.pack.assets.registry.default_state(&target) {
        game.with_level(|b, level, ticks| b.set_block(level, ticks, block, new, 3));
    }
}

/// What the miner's screens read: its settings and filters, and the block it misses, in the `client`
/// compound.
fn miner_sync(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if kind(&entity.id) != Some(Kind::DigitalMiner) {
        return;
    }
    let mut client = Compound::new();
    put_miner_region(&mut client, miner_region(&entity.nbt));
    client.insert("filters".into(), string_list(&strings_of(&entity.nbt, "filters")));
    for key in ["inverse", "inv_require"] {
        put_int(&mut client, key, get_int(&entity.nbt, key));
    }
    for key in ["inv_replace", "missing"] {
        if let Some(v) = get_string(&entity.nbt, key) {
            client.insert(key.into(), Tag::String(v.as_str().into()));
        }
    }
    put_client(game, pos, entity, &["radius", "min_y", "max_y", "filters", "inverse", "inv_require", "inv_replace", "missing"], client);
}

/// A player's press on the miner's config screen: `pos`, an op and its payload. Only within reach.
/// Ops: 0 radius, 1 min Y, 2 max Y (a number), 3 inverse, 4 inverse requires replacement, 5 inverse
/// replace target (an item id, empty for none), 6 new filter, 7 replace filter `value`, 8 delete, 9 up,
/// 10 down, 11 top, 12 bottom, 13 toggle enabled.
fn miner_payload(game: &mut Game<'_>, data: &[u8]) {
    let (Some(pos), Some(&op), Some(value), Some(text), Some(player)) = (data.get(..12), data.get(12), data.get(13..17), data.get(17..), game.native_acting_player()) else {
        return;
    };
    let int = |i: usize| i32::from_be_bytes([pos[i], pos[i + 1], pos[i + 2], pos[i + 3]]);
    let pos = [int(0), int(4), int(8)];
    let near = (0..3).map(|k| (player.position[k] - f64::from(pos[k]) - 0.5).powi(2)).sum::<f64>() <= 64.0;
    if !near || kind(block_name(game, pos)) != Some(Kind::DigitalMiner) {
        return;
    }
    let value = i32::from_be_bytes([value[0], value[1], value[2], value[3]]);
    let text = String::from_utf8_lossy(text).into_owned();
    game.native_with_mod_block_entity(pos, |m| {
        let region = miner_region(&m.nbt);
        let mut filters = miner_filters(&m.nbt);
        let index = usize::try_from(value).unwrap_or(usize::MAX);
        let flip = |nbt: &mut Compound, key: &str| put_int(nbt, key, u32::from(get_int(nbt, key) == 0));
        match op {
            0 => put_miner_region(&mut m.nbt, region.with_radius(value)),
            1 => put_miner_region(&mut m.nbt, region.with_min_y(value)),
            2 => put_miner_region(&mut m.nbt, region.with_max_y(value)),
            3 => flip(&mut m.nbt, "inverse"),
            4 => flip(&mut m.nbt, "inv_require"),
            5 => {
                if text.is_empty() {
                    m.nbt.remove(&JavaString::from("inv_replace"));
                } else {
                    m.nbt.insert("inv_replace".into(), Tag::String(text.as_str().into()));
                }
            }
            6 => {
                if let Some(f) = miner::Filter::load(&text).filter(|f| !f.name.is_empty() && !filters.contains(f)) {
                    filters.push(f);
                }
            }
            7 => {
                if let (Some(slot), Some(f)) = (filters.get_mut(index), miner::Filter::load(&text).filter(|f| !f.name.is_empty())) {
                    *slot = f;
                }
            }
            8 if index < filters.len() => {
                filters.remove(index);
            }
            9 | 11 if index > 0 && index < filters.len() => {
                let f = filters.remove(index);
                filters.insert(if op == 9 { index - 1 } else { 0 }, f);
            }
            10 | 12 if index + 1 < filters.len() => {
                let f = filters.remove(index);
                filters.insert(if op == 10 { index + 1 } else { filters.len() }, f);
            }
            13 => {
                if let Some(f) = filters.get_mut(index) {
                    f.enabled = !f.enabled;
                }
            }
            _ => {}
        }
        put_miner_filters(&mut m.nbt, &filters);
    });
}

// --- Transmitter shapes -----------------------------------------------------------------------------

/// The kind of thing a transmitter carries, from its block id: tiers of one kind join each other.
fn pipe_family(name: &str) -> Option<&'static str> {
    let n = name.strip_prefix("mekanism:")?;
    [
        ("_universal_cable", "cable"),
        ("_pressurized_tube", "tube"),
        ("_logistical_transporter", "transporter"),
        ("restrictive_transporter", "transporter"),
        ("diversion_transporter", "transporter"),
        ("_mechanical_pipe", "pipe"),
        ("_thermodynamic_conductor", "conductor"),
    ]
    .into_iter()
    .find_map(|(suffix, family)| n.ends_with(suffix).then_some(family))
}

/// Vanilla blocks with an inventory a transporter feeds and empties.
fn vanilla_container(name: &str) -> bool {
    let Some(n) = name.strip_prefix("minecraft:") else {
        return false;
    };
    matches!(n, "chest" | "trapped_chest" | "barrel" | "furnace" | "blast_furnace" | "smoker" | "hopper" | "dropper" | "dispenser" | "brewing_stand")
        || n.ends_with("shulker_box")
}

/// Whether a transmitter of `family` reaches into the Mekanism block `kind` (Mekanism's
/// `TransmitterType` and the capabilities its neighbour offers).
fn serves(family: &str, kind: Kind) -> bool {
    match family {
        "cable" => !matches!(
            kind,
            Kind::Cable(_)
                | Kind::Tube(_)
                | Kind::Transporter(_)
                | Kind::ChemicalTank(_)
                | Kind::Evaporation
                | Kind::FluidTank(_)
                | Kind::Pipe(_)
                | Kind::FissionPort
        ),
        "pipe" => matches!(
            kind,
            Kind::Reaction
                | Kind::BioGenerator
                | Kind::FluidTank(_)
                | Kind::Separator
                | Kind::Rotary
                | Kind::HeatGenerator
                | Kind::FissionPort
                | Kind::TurbineValve
                | Kind::FusionPort
                | Kind::Washer
                | Kind::Pump
                | Kind::Entangloporter
                | Kind::Plenisher
                | Kind::BoilerValve
                | Kind::DynamicValve
        ),
        "tube" => matches!(
            kind,
            Kind::Reaction
                | Kind::Entangloporter
                | Kind::Oxidizer
                | Kind::Advanced(_)
                | Kind::Nucleosynthesizer
                | Kind::ChemicalInfuser
                | Kind::Rotary
                | Kind::Separator
                | Kind::GasGenerator
                | Kind::ChemicalTank(_)
                | Kind::Dissolution
                | Kind::Centrifuge
                | Kind::FissionPort
                | Kind::TurbineValve
                | Kind::FusionPort
                | Kind::Washer
                | Kind::Crystallizer
                | Kind::Activator
                | Kind::BoilerValve
                | Kind::SpsPort
                | Kind::DynamicValve
        ),
        // The blocks with item slots (`block_entity_container`).
        "transporter" => matches!(
            kind,
            Kind::Machine(..)
                | Kind::Factory(..)
                | Kind::HeatGenerator
                | Kind::Oxidizer
                | Kind::Dissolution
                | Kind::Crystallizer
                | Kind::Separator
                | Kind::Advanced(_)
                | Kind::Evaporation
                | Kind::Rotary
                | Kind::Infuser
                | Kind::EnergyCube(_)
        ),
        _ => false,
    }
}

/// Twice a second a transmitter shows the sides it joins: its block's six direction properties (the
/// mushroom stem template's) are `false` toward a transmitter of its kind or a block it serves. Mekanism
/// renders the same from its connection cache.
fn show_connections(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "shape_ticker");
    put_int(&mut entity.nbt, "shape_ticker", (ticker + 1) % 10);
    if ticker != 0 {
        return;
    }
    let Some(family) = pipe_family(&entity.id).or_else(|| pipe_family(block_name(game, pos))) else {
        return;
    };
    for (side, d) in DIRECTIONS {
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let name = block_name(game, at).to_owned();
        let joined = pipe_family(&name) == Some(family)
            || (family == "transporter" && vanilla_container(&name))
            || (family == "pipe" && name == EVAPORATION_VALVE)
            || game
                .native_with_mod_block_entity(at, |m| kind(&m.id))
                .flatten()
                .is_some_and(|k| serves(family, k));
        game.native_set_block_property(pos, side, if joined { "false" } else { "true" });
    }
}

// --- Logistical transporters ------------------------------------------------------------------------

fn is_transporter(name: &str) -> bool {
    name.strip_prefix("mekanism:").is_some_and(|n| n.ends_with("_logistical_transporter"))
}

/// A logistical transporter, simplified: twice a second it pulls up to its tier's count from a container
/// beside it (a chest, a furnace's output, a machine's output when its face gives out), and sends it
/// at once along the connected transporters into the nearest container that takes it: a chest, a
/// furnace, a hopper or another machine's input, through the side it faces. What fits nowhere goes back.
/// Mekanism moves items visibly along the pipe at the tier's speed and sorts by colour and filters.
fn tick_transporter(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, pull: u32) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 10);
    if ticker != 0 {
        return;
    }
    for (_, d) in DIRECTIONS {
        let source = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let back = [-d[0], -d[1], -d[2]];
        // A Mekanism block's face must give out (side configuration); any container then gives what a
        // hopper there could take (API 24 `native_extract_item`).
        let closed = game
            .native_with_mod_block_entity(source, |m| !sides_of(&m.nbt, Transmission::Item).get(face_index(back)).gives_out())
            .unwrap_or(false);
        if closed || is_transporter(block_name(game, source)) {
            continue;
        }
        let face = horizon_core::geometry::Direction::ALL[face_index(back)];
        let Some(stack) = game.native_extract_item(source, face, pull) else {
            continue;
        };
        if let Some(left) = route_items(game, pos, source, stack) {
            // What fits nowhere goes back where it came from.
            if let Some(lost) = game.native_insert_item(source, face, left) {
                game.native_insert_item(source, horizon_core::geometry::Direction::Up, lost);
            }
        }
        return;
    }
}

fn load_sorter(nbt: &Compound) -> Sorter {
    let filters = match nbt.get(&JavaString::from("filters")) {
        Some(Tag::String(s)) => Sorter::parse_filters(&s.to_string().unwrap_or_default()),
        _ => Vec::new(),
    };
    Sorter {
        filters,
        auto_eject: get_int(nbt, "eject") == 1,
        single_item: get_int(nbt, "single") == 1,
        round_robin: get_int(nbt, "round_robin") == 1,
        disabled: strings_of(nbt, "disabled"),
        opts: match nbt.get(&JavaString::from("filter_opts")) {
            Some(Tag::String(o)) => Sorter::parse_opts(&o.to_string().unwrap_or_default()),
            _ => Default::default(),
        },
    }
}

fn save_sorter(nbt: &mut Compound, s: &Sorter) {
    nbt.insert("filters".into(), Tag::String(s.save_filters().as_str().into()));
    put_int(nbt, "eject", u32::from(s.auto_eject));
    put_int(nbt, "single", u32::from(s.single_item));
    put_int(nbt, "round_robin", u32::from(s.round_robin));
    nbt.insert("disabled".into(), string_list(&s.disabled));
    nbt.insert("filter_opts".into(), Tag::String(s.save_opts().as_str().into()));
}

/// The Logistical Sorter, every 10 ticks: each filter in turn takes its items from the inventory behind
/// the sorter, until one moves something; then, with auto-eject, any item. They go into the
/// transporters in front (or straight into an inventory there), and what fits nowhere goes back.
fn tick_sorter(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let delay = get_int(&entity.nbt, "delay").saturating_sub(1);
    put_int(&mut entity.nbt, "delay", delay);
    if delay > 0 {
        return;
    }
    put_int(&mut entity.nbt, "delay", sorter::DELAY);
    let Some(d) = game.native_block_property(pos, "facing").and_then(|f| direction(&f)) else {
        return;
    };
    let settings = load_sorter(&entity.nbt);
    let back = [pos[0] - d[0], pos[1] - d[1], pos[2] - d[2]];
    let front = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
    // The back inventory's face toward the sorter, and the front one's.
    let back_face = horizon_core::geometry::Direction::ALL[face_index(d)];
    let front_face = horizon_core::geometry::Direction::ALL[face_index([-d[0], -d[1], -d[2]])];
    let max = settings.max_move();
    let mut choices: Vec<Option<&str>> = settings.active().map(|f| Some(f.as_str())).collect();
    let item_tags = world_items!(game).item_tags.clone();
    let in_tag = |tag: &str, item: &str| item_tags.get(tag).is_some_and(|t| t.contains(item));
    if settings.auto_eject {
        choices.push(None);
    }
    for filter in choices {
        // A filter in size mode moves between its min and max items (`SorterFilter.mapInventory`);
        // auto-eject takes what no filter refuses the default for (`strictFinder`).
        let opts = filter.map(|f| settings.opts_of(f));
        let ranged = opts.filter(|o| o.size_mode && !settings.single_item);
        let take = ranged.map_or(max, |o| u32::from(o.max).clamp(1, max));
        let wanted = |s: &ItemStack| match filter {
            Some(f) => Sorter::matches_item(f, &s.item, &in_tag),
            None => settings.auto_eject_allows(&s.item, &in_tag),
        };
        let stack = game.native_extract_item_where(back, back_face, take, &wanted);
        let Some(stack) = stack else {
            continue;
        };
        if let Some(o) = ranged
            && stack.count < u32::from(o.min)
        {
            game.native_insert_item(back, back_face, stack);
            continue;
        }
        let left = if is_transporter(block_name(game, front)) {
            route_items(game, front, pos, stack)
        } else {
            game.native_insert_item(front, front_face, stack)
        };
        if let Some(left) = left
            && let Some(lost) = game.native_insert_item(back, back_face, left)
        {
            game.native_insert_item(back, horizon_core::geometry::Direction::Up, lost);
        }
        return;
    }
}

/// A right click on a sorter: an item in hand adds a filter for it or takes it away (sneaking: for the
/// item's first tag); sneaking with an empty hand switches auto-eject, and a plain click with an empty
/// hand switches single-item mode and lists the filters.
pub(super) fn use_sorter(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || kind(block_name(game, pos)) != Some(Kind::Sorter) {
        return false;
    }
    let held = game.inventory.get(game.selected).cloned().flatten();
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    let first_tag = held.as_ref().and_then(|h| {
        let items = world_items!(game);
        let mut tags: Vec<&String> = items.item_tags.iter().filter(|(_, t)| t.contains(&*h.item)).map(|(k, _)| k).collect();
        tags.sort();
        tags.first().map(|t| format!("#{t}"))
    });
    let Some(line) = game.native_with_mod_block_entity(pos, |m| {
        let mut s = load_sorter(&m.nbt);
        let line = match (&held, sneaking) {
            (Some(h), _) => {
                let filter = if sneaking { first_tag.clone().unwrap_or_else(|| h.item.to_string()) } else { h.item.to_string() };
                let added = s.toggle(&filter);
                format!("Logistical Sorter filter {} {filter}", if added { "added:" } else { "removed:" })
            }
            // An empty hand opens its screen.
            (None, _) => return String::new(),
        };
        save_sorter(&mut m.nbt, &s);
        line
    }) else {
        return false;
    };
    if line.is_empty() {
        return false;
    }
    tell(game, line);
    true
}

/// Walks the transporters joined to `start`, nearest first, offering `stack` to every container beside
/// them except `source`. Returns what no container took.
fn route_items(game: &mut Game<'_>, start: [i32; 3], source: [i32; 3], stack: ItemStack) -> Option<ItemStack> {
    let mut seen = std::collections::HashSet::from([start]);
    let mut queue = std::collections::VecDeque::from([start]);
    let mut stack = Some(stack);
    while let Some(node) = queue.pop_front() {
        for (_, d) in DIRECTIONS {
            let at = [node[0] + d[0], node[1] + d[1], node[2] + d[2]];
            if at == source || seen.contains(&at) {
                continue;
            }
            if is_transporter(block_name(game, at)) {
                if seen.len() < 512 {
                    seen.insert(at);
                    queue.push_back(at);
                }
                continue;
            }
            let face = horizon_core::geometry::Direction::ALL[face_index([-d[0], -d[1], -d[2]])];
            stack = game.native_insert_item(at, face, stack.take()?);
            if stack.is_none() {
                return None;
            }
        }
    }
    stack
}

// --- Induction Matrix --------------------------------------------------------------------------------

/// A formed matrix: its storage and rate, and its ports, the first of which keeps the energy.
struct FormedMatrix {
    matrix: mekanism_core::matrix::Matrix,
    ports: Vec<[i32; 3]>,
}

/// `MatrixValidator`, simplified: the frame blocks joined to `start` must be exactly the shell of a box
/// 3 to 18 on each side, and everything inside must be cells, providers or air.
fn find_matrix(game: &Game<'_>, start: [i32; 3]) -> Option<FormedMatrix> {
    use mekanism_core::matrix::{is_frame, MAX_SIZE, MIN_SIZE};
    let mut seen = std::collections::HashSet::from([start]);
    let mut todo = vec![start];
    let (mut lo, mut hi) = (start, start);
    while let Some(at) = todo.pop() {
        for k in 0..3 {
            lo[k] = lo[k].min(at[k]);
            hi[k] = hi[k].max(at[k]);
            if hi[k] - lo[k] >= MAX_SIZE {
                return None;
            }
        }
        for (_, d) in DIRECTIONS {
            let next = [at[0] + d[0], at[1] + d[1], at[2] + d[2]];
            if !seen.contains(&next) && is_frame(block_name(game, next)) {
                seen.insert(next);
                todo.push(next);
            }
        }
    }
    if (0..3).any(|k| hi[k] - lo[k] + 1 < MIN_SIZE) {
        return None;
    }
    let mut matrix = mekanism_core::matrix::Matrix::default();
    let mut ports = Vec::new();
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                let at = [x, y, z];
                let shell = x == lo[0] || x == hi[0] || y == lo[1] || y == hi[1] || z == lo[2] || z == hi[2];
                let name = block_name(game, at);
                if shell {
                    if !seen.contains(&at) {
                        return None;
                    }
                    if name == "mekanism:induction_port" {
                        ports.push(at);
                    }
                } else if !matrix.add_inside(name) {
                    return None;
                }
            }
        }
    }
    (matrix.capacity > 0).then_some(FormedMatrix { matrix, ports })
}

/// An Induction Port. Once a second it checks its matrix and writes the matrix's capacity and rate
/// into its NBT. The first port (`master`) holds the energy; others hand what they took in to it. A
/// face the Configurator set to Output gives energy out, up to the matrix's rate.
fn tick_induction_port(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    if ticker == 0 {
        let formed = find_matrix(game, pos);
        let (capacity, rate, master) = match &formed {
            Some(f) => (f.matrix.capacity, f.matrix.rate, f.ports.first().copied().unwrap_or(pos)),
            None => (0, 0, pos),
        };
        put_long(&mut entity.nbt, "capacity", capacity);
        put_long(&mut entity.nbt, "rate", rate);
        put_long(&mut entity.nbt, "master", u64::from(master == pos));
        for (key, v) in ["mx", "my", "mz"].into_iter().zip(master) {
            entity.nbt.insert(key.into(), Tag::Int(v));
        }
    }
    let rate = get_long(&entity.nbt, "rate");
    if rate == 0 {
        return;
    }
    let master = ["mx", "my", "mz"].map(|key| match entity.nbt.get(&JavaString::from(key)) {
        Some(Tag::Int(v)) => *v,
        _ => 0,
    });
    let is_master = master == pos;
    let capacity = get_long(&entity.nbt, "capacity");
    if !is_master {
        // Hand this port's buffer to the master.
        let mine = get_long(&entity.nbt, "energy");
        if mine > 0 {
            let taken = game
                .native_with_mod_block_entity(master, |m| {
                    let stored = get_long(&m.nbt, "energy");
                    let n = mine.min(get_long(&m.nbt, "capacity").saturating_sub(stored));
                    put_long(&mut m.nbt, "energy", stored + n);
                    n
                })
                .unwrap_or(0);
            put_long(&mut entity.nbt, "energy", mine - taken);
        }
    }
    let sides = sides_of(&entity.nbt, Transmission::Energy);
    for (i, (_, d)) in DIRECTIONS.into_iter().enumerate() {
        if sides.get(i) != mekanism_core::sides::SideMode::Output {
            continue;
        }
        // Draw up to the rate from the matrix, offer it out of this face, and put back what is left.
        let draw = |stored: u64| stored.min(rate);
        let mut out = EnergyContainer::new(rate, u64::MAX);
        if is_master {
            let stored = get_long(&entity.nbt, "energy").min(capacity);
            out.stored = draw(stored);
            put_long(&mut entity.nbt, "energy", stored - out.stored);
        } else {
            out.stored = game
                .native_with_mod_block_entity(master, |m| {
                    let stored = get_long(&m.nbt, "energy");
                    put_long(&mut m.nbt, "energy", stored - draw(stored));
                    draw(stored)
                })
                .unwrap_or(0);
        }
        if out.stored == 0 {
            continue;
        }
        push_energy(game, pos, &mut out, Some(d), false);
        let left = out.stored;
        if is_master {
            let stored = get_long(&entity.nbt, "energy");
            put_long(&mut entity.nbt, "energy", stored + left);
        } else {
            game.native_with_mod_block_entity(master, |m| {
                let stored = get_long(&m.nbt, "energy");
                put_long(&mut m.nbt, "energy", stored + left);
            });
        }
    }
}

// --- Thermal Evaporation Plant -----------------------------------------------------------------------

const EVAPORATION_PARTS: [&str; 3] = [
    "mekanism:thermal_evaporation_block",
    "mekanism:thermal_evaporation_valve",
    "mekanism:thermal_evaporation_controller",
];

fn block_name<'a>(game: &'a Game<'_>, at: [i32; 3]) -> &'a str {
    let state = game.world.block(at[0], at[1], at[2]);
    game.pack.assets.registry.state(state).map_or("", |s| s.name.as_str())
}

/// A formed tower: its north-west bottom corner and height.
#[derive(Clone, Copy)]
struct Tower {
    origin: [i32; 3],
    height: u32,
}

impl Tower {
    fn perimeter(self, y: i32) -> impl Iterator<Item = [i32; 3]> {
        let [x0, _, z0] = self.origin;
        (0..4).flat_map(move |dx| (0..4).map(move |dz| (dx, dz)))
            .filter(|&(dx, dz)| dx == 0 || dx == 3 || dz == 0 || dz == 3)
            .map(move |(dx, dz)| [x0 + dx, y, z0 + dz])
    }
}

/// `EvaporationValidator`, simplified: a 4x4 floor of tower blocks, then 4x4 rings of them with air
/// inside, 3 to 18 blocks high in all, with the controller in a ring.
fn find_tower(game: &Game<'_>, c: [i32; 3]) -> Option<Tower> {
    let part = |at: [i32; 3]| EVAPORATION_PARTS.contains(&block_name(game, at));
    let mut bottom = c[1];
    while part([c[0], bottom - 1, c[2]]) && c[1] - bottom < evap::MAX_HEIGHT as i32 {
        bottom -= 1;
    }
    for x0 in c[0] - 3..=c[0] {
        for z0 in c[2] - 3..=c[2] {
            let on_ring = c[0] == x0 || c[0] == x0 + 3 || c[2] == z0 || c[2] == z0 + 3;
            if !on_ring {
                continue;
            }
            let floor = (0..4).all(|dx| (0..4).all(|dz| part([x0 + dx, bottom, z0 + dz])));
            if !floor {
                continue;
            }
            let mut tower = Tower { origin: [x0, bottom, z0], height: 1 };
            while tower.height < evap::MAX_HEIGHT {
                let y = bottom + tower.height as i32;
                let ring = tower.perimeter(y).all(part);
                let hollow = (1..3).all(|dx| (1..3).all(|dz| block_name(game, [x0 + dx, y, z0 + dz]) == "minecraft:air"));
                if !(ring && hollow) {
                    break;
                }
                tower.height += 1;
            }
            if tower.height >= evap::MIN_HEIGHT && c[1] > bottom && c[1] < bottom + tower.height as i32 {
                return Some(tower);
            }
        }
    }
    None
}

/// The Thermal Evaporation Plant's controller runs the tower: water from buckets, heat from the
/// Advanced Solar Generators on its top corners, brine into buckets or out of its valves into the
/// separators beside them. The tower is checked again once a second.
fn tick_evaporation(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    let tower = if ticker == 0 || get_int(&entity.nbt, "height") == 0 {
        let t = find_tower(game, pos);
        put_int(&mut entity.nbt, "height", t.map_or(0, |t| t.height));
        if let Some(t) = t {
            for (key, v) in ["x0", "y0", "z0"].into_iter().zip(t.origin) {
                entity.nbt.insert(key.into(), Tag::Int(v));
            }
        }
        t
    } else {
        Some(Tower {
            origin: ["x0", "y0", "z0"].map(|key| match entity.nbt.get(&JavaString::from(key)) {
                Some(Tag::Int(v)) => *v,
                _ => 0,
            }),
            height: get_int(&entity.nbt, "height"),
        })
    };
    let Some(tower) = tower else {
        entity.data = vec![0, 1000, 0, 1000];
        return Some(false);
    };
    let mut water = get_tank(&entity.nbt, "water", evap::input_capacity(tower.height));
    let mut brine = get_tank(&entity.nbt, "brine", evap::OUTPUT_TANK);
    // FluidInventorySlot.fill (28, 20): the empty bucket goes to the slot below it (28, 51).
    fill_tank_from_bucket(game, entity, 0, Some(3), &mut water, |f| evap::evaporating(f).is_some());
    // The top corners: an Advanced Solar Generator that sees the sun heats the tower.
    let [x0, y0, z0] = tower.origin;
    let top = y0 + tower.height as i32;
    let sun = overworld(game) && game.sky_darken() < 4;
    let solars = [[0, 0], [3, 0], [0, 3], [3, 3]]
        .into_iter()
        .map(|[dx, dz]| [x0 + dx, top, z0 + dz])
        .filter(|&at| sun && block_name(game, at) == "mekanismgenerators:advanced_solar_generator" && can_see_sky(game, at))
        .count() as u32;
    let mut temperature = get_double(&entity.nbt, "temperature").unwrap_or(evap::AMBIENT_TEMP);
    let before = temperature;
    temperature = evap::step_temperature(temperature, solars, evap::AMBIENT_TEMP);
    let mut progress = get_int(&entity.nbt, "progress");
    let ops = evap::operations(evap::temp_multiplier(temperature, tower.height), &mut progress);
    // Water evaporates into brine, and brine into lithium.
    let made = water.chemical.as_deref().and_then(evap::evaporating);
    let ops = match made {
        Some(out) => ops
            .min(water.amount / evap::WATER_PER_BRINE)
            .min((brine.capacity - brine.amount) * u64::from(brine.accepts(out))),
        None => 0,
    };
    if let Some(out) = made
        && ops > 0
    {
        water.extract(ops * evap::WATER_PER_BRINE);
        brine.insert(out, ops);
    }
    // FluidInventorySlot.drain (152, 20): an empty bucket fills with brine into the slot below it (152, 51).
    drain_tank_into_bucket(game, entity, 1, Some(2), &mut brine);
    // Valves take water from the pipes and fluid tanks beside them and give brine to the pipes, tanks
    // and separators beside them.
    let valves: Vec<[i32; 3]> =
        (y0..top).flat_map(|y| tower.perimeter(y).collect::<Vec<_>>()).filter(|&at| block_name(game, at) == EVAPORATION_VALVE).collect();
    // A tower that is not making brine may take brine in, to make lithium.
    let takes_brine = water.chemical.as_deref() == Some("mekanism:brine")
        || (water.amount == 0 && brine.chemical.as_deref() != Some("mekanism:brine"));
    for valve in valves {
        pull_fluid(game, valve, &mut water, "minecraft:water");
        if takes_brine {
            pull_fluid(game, valve, &mut water, "mekanism:brine");
        }
        if brine.amount > 0 {
            push_chemical(game, valve, &mut brine, None, u64::MAX, false);
        }
    }
    put_tank(&mut entity.nbt, "water", &water);
    put_tank(&mut entity.nbt, "brine", &brine);
    put_int(&mut entity.nbt, "progress", progress);
    entity.nbt.insert("temperature".into(), Tag::Double(temperature.to_bits()));
    // `lastGain` (what the screen shows, in hundredths) and `lastEnvironmentLoss` (heat units, in hundredths of a kelvin).
    let multiplier = evap::temp_multiplier(temperature, tower.height);
    let gain = if ops == 0 || multiplier <= 0.0 { 0.0 } else if multiplier < 1.0 { 1.0 / (1.0 / multiplier).ceil() } else { multiplier };
    put_int(&mut entity.nbt, "gain", (gain * 100.0).round() as u32);
    put_long(&mut entity.nbt, "loss", ((before - temperature).abs() * f64::from(tower.height) * 100.0 * 100.0) as u64);
    // The flame shows the temperature up to 3,000 K, the arrow the brine tank.
    let heat = (temperature - evap::AMBIENT_TEMP) / (evap::MAX_MULTIPLIER_TEMP - evap::AMBIENT_TEMP);
    entity.data = vec![
        (heat.clamp(0.0, 1.0) * 1000.0) as i32,
        1000,
        (brine.amount * 1000 / evap::OUTPUT_TANK) as i32,
        1000,
    ];
    Some(ops > 0)
}

fn tick_gas_generator(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let mut energy = EnergyContainer::new(chem::GAS_GENERATOR_STORAGE, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(chem::GAS_GENERATOR_STORAGE);
    let mut fuel = get_tank(&entity.nbt, "fuel", chem::GAS_GENERATOR_TANK);
    let (made, _) = chem::burn_fuel(&mut fuel, energy.capacity - energy.stored);
    energy.insert(made, Action::Execute);
    charge_slot(entity, 0, &mut energy);
    push_energy(game, pos, &mut energy, None, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "fuel", &fuel);
    Some(made > 0)
}

/// A chemical tank takes from every side and gives out of its bottom at its tier's rate.
fn tick_chemical_tank(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, tier: BaseTier) {
    let (capacity, rate) = chem::chemical_tank(tier);
    let mut tank = get_tank(&entity.nbt, "chemical", capacity);
    let dumping = get_int(&entity.nbt, "dump") % 3;
    let creative = tier == BaseTier::Creative;
    // `ChemicalUtils.dump(tank, mode, capacity / 400, transfer rate)`: dumping removes a share, dumping excess
    // what is over 85% of the tank.
    if dumping != 0 && !creative && tank.amount > 0 {
        let stored = tank.amount;
        let to_dump = if dumping == 2 {
            capacity / 400
        } else {
            let keep = (capacity as f64 * 0.85) as u64;
            if keep < stored { (stored - keep).min(rate) } else { 0 }
        };
        tank.extract(to_dump);
        put_tank(&mut entity.nbt, "chemical", &tank);
    }
    // Dumping does not eject (`setCanEject`).
    if tank.amount > 0 && (creative || dumping != 2) {
        push_chemical(game, pos, &mut tank, Some([0, -1, 0]), rate, false);
        put_tank(&mut entity.nbt, "chemical", &tank);
    }
}

/// A pressurized tube feeds the tanks, machines and generators beside it, then lets the rest run on
/// to emptier tubes carrying the same chemical, as `tick_cable` does with energy.
fn tick_tube(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, rate: u64) {
    tick_carrier(game, pos, entity, rate, "chemical", |k| match k {
        Kind::Tube(rate) => Some(rate),
        _ => None,
    });
}

/// A mechanical pipe does with fluids what a pressurized tube does with chemicals.
fn tick_pipe(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, rate: u64) {
    tick_carrier(game, pos, entity, rate, "fluid", |k| match k {
        Kind::Pipe(rate) => Some(rate),
        _ => None,
    });
}

/// A tube or pipe holding `rate` in its `key` tank: it feeds the blocks beside it, then shares the rest
/// with the emptier carriers of its own kind (`same` gives their rate).
fn tick_carrier(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
    rate: u64,
    key: &str,
    same: fn(Kind) -> Option<u64>,
) {
    let mut tank = get_tank(&entity.nbt, key, rate);
    if tank.amount == 0 {
        return;
    }
    push_chemical(game, pos, &mut tank, None, u64::MAX, true);
    for (_, d) in DIRECTIONS {
        let Some(chemical) = tank.chemical.clone() else {
            break;
        };
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let mine = tank.amount;
        let given = game.native_with_mod_block_entity(at, |m| {
            let Some(their_rate) = kind(&m.id).and_then(same) else {
                return 0;
            };
            let mut theirs = get_tank(&m.nbt, key, their_rate);
            if !theirs.accepts(&chemical) {
                return 0;
            }
            let give = tier::cable_share(mine, theirs.amount);
            let n = theirs.insert(&chemical, give);
            put_tank(&mut m.nbt, key, &theirs);
            n
        });
        if let Some(n) = given {
            tank.extract(n);
        }
    }
    put_tank(&mut entity.nbt, key, &tank);
}

const EVAPORATION_VALVE: &str = "mekanism:thermal_evaporation_valve";

/// Fills `into` with `fluid` from the mechanical pipes and fluid tanks beside `pos`, as Mekanism's pipes
/// push into a block that takes the fluid.
fn pull_fluid(game: &mut Game<'_>, pos: [i32; 3], into: &mut ChemicalTank, fluid: &str) {
    pull_from_carriers(game, pos, into, fluid);
}

/// Fills `into` with `id` from the carriers and tanks of its kind beside `pos`: pipes and fluid tanks
/// for a fluid, tubes and chemical tanks for a chemical.
fn pull_from_carriers(game: &mut Game<'_>, pos: [i32; 3], into: &mut ChemicalTank, fluid: &str) {
    let is_fluid = FLUIDS.contains(&fluid);
    for (_, d) in DIRECTIONS {
        let room = into.capacity - into.amount;
        if room == 0 || !into.accepts(fluid) {
            return;
        }
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let taken = game.native_with_mod_block_entity(at, |m| {
            let (key, capacity) = match kind(&m.id)? {
                Kind::Pipe(rate) if is_fluid => ("fluid", rate),
                Kind::FluidTank(t) if is_fluid => ("fluid", tier::fluid_tank(t).0),
                Kind::Tube(rate) if !is_fluid => ("chemical", rate),
                Kind::ChemicalTank(t) if !is_fluid => ("chemical", chem::chemical_tank(t).0),
                _ => return None,
            };
            let mut theirs = get_tank(&m.nbt, key, capacity);
            if theirs.chemical.as_deref() != Some(fluid) {
                return None;
            }
            let n = theirs.extract(room);
            put_tank(&mut m.nbt, key, &theirs);
            Some(n)
        });
        if let Some(Some(n)) = taken {
            into.insert(fluid, n);
        }
    }
}

// --- Fluid tanks ------------------------------------------------------------------------------------

/// A fluid tank takes from every side and gives out of its bottom at its tier's rate, as a chemical
/// tank does.
fn tick_fluid_tank(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, tier: BaseTier) {
    let (capacity, rate) = tier::fluid_tank(tier);
    let mut tank = get_tank(&entity.nbt, "fluid", capacity);
    // `FluidInventorySlot.input` with the edit mode Both (`handleContainer`): an empty bucket fills from
    // the tank, a full one empties into it, and a bucket that can do neither goes to the output slot.
    let held = entity.items.first().cloned().flatten();
    if let Some(bucket) = held {
        let empty = &*bucket.item == "minecraft:bucket";
        let done = if empty {
            drain_tank_into_bucket(game, entity, 0, Some(1), &mut tank)
        } else {
            fill_tank_from_bucket(game, entity, 0, Some(1), &mut tank, |f| FLUIDS.contains(&f))
        };
        let can_fill = bucket_fluid(&bucket.item).is_some_and(|f| tank.chemical.as_deref().is_none_or(|c| c == f));
        // Nothing to take from the tank, or a bucket of something else than the tank holds.
        let stuck = if empty { tank.amount < 1_000 } else { !can_fill };
        if !done && stuck && (empty || bucket_fluid(&bucket.item).is_some()) && put_item(game, entity, 1, &bucket.item) {
            take_one(entity, 0);
        }
    }
    if tank.amount > 0 {
        push_chemical(game, pos, &mut tank, Some([0, -1, 0]), rate, false);
    }
    put_tank(&mut entity.nbt, "fluid", &tank);
}

/// A right click on a fluid tank (`BlockFluidTank.useItemOn`): a full bucket empties into it, an empty
/// bucket fills from it, and an empty hand shows what it holds on the action bar.
pub(super) fn use_fluid_tank(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) {
        return false;
    }
    let selected = game.selected;
    let held = game.inventory.get(selected).cloned().flatten();
    let creative = game.creative;
    let done = game.native_with_mod_block_entity(pos, |m| {
        let Some(Kind::FluidTank(t)) = kind(&m.id) else {
            return None;
        };
        let mut tank = get_tank(&m.nbt, "fluid", tier::fluid_tank(t).0);
        let mut gives = None;
        let mut used = false;
        match held.as_ref() {
            Some(h) if &*h.item == "minecraft:bucket" => {
                if let Some(bucket) = tank.chemical.as_deref().and_then(fluid_bucket)
                    && tank.amount >= 1_000
                {
                    tank.extract(1_000);
                    gives = Some(bucket);
                    used = true;
                }
            }
            Some(h) => {
                if let Some(f) = bucket_fluid(&h.item)
                    && FLUIDS.contains(&f.as_str())
                    && tank.accepts(&f)
                    && tank.amount + 1_000 <= tank.capacity
                {
                    tank.insert(&f, 1_000);
                    gives = Some("minecraft:bucket".to_owned());
                    used = true;
                }
            }
            None => {}
        }
        put_tank(&mut m.nbt, "fluid", &tank);
        let line = match tank.chemical.as_deref() {
            Some(f) if tank.amount > 0 => format!("Fluid Tank: {} / {} mB {f}", tank.amount, tank.capacity),
            _ => format!("Fluid Tank: empty / {} mB", tank.capacity),
        };
        Some((used, gives, line))
    });
    let Some(Some((used, gives, line))) = done else {
        return false;
    };
    if let Some(player) = game.native_acting_player() {
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
    let empty_hand = held.is_none();
    let Some(held) = held.filter(|_| used) else {
        return empty_hand;
    };
    if !creative && let Some(item) = gives {
        let left = held.count - 1;
        if left == 0 {
            game.inventory[selected] = Some(stack_of(&item, 1));
        } else {
            game.inventory[selected] = Some(ItemStack { count: left, ..held });
            game.give_or_drop(stack_of(&item, 1));
        }
    }
    true
}

// --- Energy between blocks --------------------------------------------------------------------------

const DIRECTIONS: [(&str, [i32; 3]); 6] = [
    ("down", [0, -1, 0]),
    ("up", [0, 1, 0]),
    ("north", [0, 0, -1]),
    ("south", [0, 0, 1]),
    ("west", [-1, 0, 0]),
    ("east", [1, 0, 0]),
];

fn direction(name: &str) -> Option<[i32; 3]> {
    DIRECTIONS.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}

/// Pushes `from`'s energy into the Mekanism blocks beside `pos` that accept it, on every side or only
/// `only`; split evenly the way Mekanism's emitter splits to its targets (what one refuses goes on).
fn push_energy(
    game: &mut Game<'_>,
    pos: [i32; 3],
    from: &mut EnergyContainer,
    only: Option<[i32; 3]>,
    from_cable: bool,
) {
    let sides: Vec<[i32; 3]> = DIRECTIONS
        .iter()
        .map(|(_, d)| *d)
        .filter(|d| only.is_none_or(|o| o == *d))
        .collect();
    for (i, d) in sides.iter().enumerate() {
        let offer = from.extract(u64::MAX, Action::Simulate) / (sides.len() - i) as u64;
        if offer == 0 || !gives_out(pos, *d, Transmission::Energy) {
            continue;
        }
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let back = [-d[0], -d[1], -d[2]];
        let accepted = game
            .native_block_property(at, "facing")
            .and_then(|f| direction(&f))
            .map_or(true, |front| front != back);
        let taken = game.native_with_mod_block_entity(at, |m| {
            if !takes_in(&m.nbt, back, Transmission::Energy) {
                return 0;
            }
            let Some((capacity, rate)) = receiver(&m.id, &m.nbt, accepted, from_cable) else {
                return 0;
            };
            let mut to = EnergyContainer::new(capacity, rate);
            to.stored = get_long(&m.nbt, "energy").min(capacity);
            let refused = to.insert(offer, Action::Execute);
            put_long(&mut m.nbt, "energy", to.stored);
            offer - refused
        });
        if let Some(taken) = taken {
            from.extract(taken, Action::Execute);
        }
    }
}

/// The energy a neighbour takes: (capacity, rate) when `id` is a Mekanism block that accepts energy on
/// the side it is offered; a cube refuses on its front (`facing_away` is false there).
fn receiver(id: &str, nbt: &Compound, facing_away: bool, from_cable: bool) -> Option<(u64, u64)> {
    match kind(id)? {
        Kind::Machine(_, usage) => Some((
            ElectricMachine::with_upgrades(usage, upgrades(nbt)).energy.capacity,
            u64::MAX,
        )),
        Kind::Factory(n, _, usage) => Some((Factory::new(n, usage, upgrades(nbt)).machine.energy.capacity, u64::MAX)),
        Kind::EnergyCube(t) if facing_away => Some(tier::energy_cube(t)),
        Kind::Cable(rate) if !from_cable => Some((rate, rate)),
        Kind::Oxidizer => Some((chem::OXIDIZER_USAGE * u64::from(chem::OXIDIZER_TICKS), u64::MAX)),
        Kind::Dissolution => Some((machine::STORAGE_MULTIPLIER * nuclear::DISSOLUTION_USAGE, u64::MAX)),
        Kind::Centrifuge => Some((machine::STORAGE_MULTIPLIER * nuclear::CENTRIFUGE_USAGE, u64::MAX)),
        Kind::Washer => Some((machine::STORAGE_MULTIPLIER * nuclear::WASHER_USAGE, u64::MAX)),
        Kind::Crystallizer => Some((machine::STORAGE_MULTIPLIER * nuclear::CRYSTALLIZER_USAGE, u64::MAX)),
        Kind::Separator => Some((machine::STORAGE_MULTIPLIER * chem::SEPARATOR_USAGE, u64::MAX)),
        Kind::Advanced(m) => Some((m.usage * u64::from(chem::ADVANCED_TICKS), u64::MAX)),
        Kind::Nucleosynthesizer => Some((machine::STORAGE_MULTIPLIER * chem::NUCLEOSYNTHESIZER_USAGE, u64::MAX)),
        Kind::Reaction => Some((reaction_capacity(nbt), u64::MAX)),
        Kind::Teleporter => Some((teleporter::STORAGE, u64::MAX)),
        // A laser takes energy on every side but its front; amplifiers and tractor beams only from lasers.
        Kind::Laser if facing_away => Some((laser::LASER_STORAGE, u64::MAX)),
        Kind::Vibrator => Some((seismic::VIBRATOR_STORAGE, u64::MAX)),
        Kind::Chargepad => Some((chargepad::STORAGE, u64::MAX)),
        // What it took in waits in its own buffer until its tick moves it into the frequency's.
        Kind::Entangloporter if nbt.contains_key(&JavaString::from("frequency")) => {
            Some((get_long(nbt, "energy") + qe::ENERGY_BUFFER.saturating_sub(get_long(nbt, "shared_energy")), u64::MAX))
        }
        Kind::ChemicalInfuser => Some((machine::STORAGE_MULTIPLIER * chem::CHEMICAL_INFUSER_USAGE, u64::MAX)),
        Kind::Rotary => Some((machine::STORAGE_MULTIPLIER * chem::ROTARY_USAGE, u64::MAX)),
        Kind::Heater => Some((machine::STORAGE_MULTIPLIER * heater_usage(nbt), u64::MAX)),
        Kind::Pump => {
            let u = upgrades(nbt);
            let usage = u.energy_per_tick(isotopes::PUMP_USAGE);
            Some((u.max_energy(machine::STORAGE_MULTIPLIER * isotopes::PUMP_USAGE).max(usage), u64::MAX))
        }
        Kind::Plenisher => {
            let u = upgrades(nbt);
            let usage = u.energy_per_tick(plenisher::PLENISHER_USAGE);
            Some((u.max_energy(machine::STORAGE_MULTIPLIER * plenisher::PLENISHER_USAGE).max(usage), u64::MAX))
        }
        // The matrix's first port holds its energy; the others take a tick's worth and pass it on.
        Kind::InductionPort if get_long(nbt, "rate") > 0 => {
            let rate = get_long(nbt, "rate");
            Some(if get_long(nbt, "master") == 1 { (get_long(nbt, "capacity"), rate) } else { (rate, rate) })
        }
        Kind::Infuser => Some((chem::INFUSER_USAGE * u64::from(chem::INFUSER_TICKS), u64::MAX)),
        // An SPS port takes energy in input mode (`TileEntitySPSPort`).
        Kind::SpsPort if get_long(nbt, "output") == 0 => Some((sps::PORT_STORAGE, u64::MAX)),
        Kind::DigitalMiner => {
            Some((mekanism_core::miner::STORAGE.max(miner_usage(nbt)), u64::MAX))
        }
        _ => None,
    }
}

// --- NBT and slots ----------------------------------------------------------------------------------

fn get_long(nbt: &Compound, key: &str) -> u64 {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::Long(v)) => (*v).max(0) as u64,
        Some(Tag::Int(v)) => (*v).max(0) as u64,
        _ => 0,
    }
}

fn get_int(nbt: &Compound, key: &str) -> u32 {
    get_long(nbt, key).min(u32::MAX as u64) as u32
}

fn get_double(nbt: &Compound, key: &str) -> Option<f64> {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::Double(bits)) => Some(f64::from_bits(*bits)),
        _ => None,
    }
}

fn put_long(nbt: &mut Compound, key: &str, v: u64) {
    nbt.insert(key.into(), Tag::Long(v.min(i64::MAX as u64) as i64));
}

fn put_int(nbt: &mut Compound, key: &str, v: u32) {
    nbt.insert(key.into(), Tag::Int(v.min(i32::MAX as u32) as i32));
}

fn stack_of(item: &str, count: u32) -> ItemStack {
    ItemStack { item: Arc::from(item), count, damage: 0, extra: None }
}

fn slot(stack: Option<ItemStack>) -> Slot {
    match stack {
        Some(s) => Slot { item: Some(s.item.to_string()), count: s.count },
        None => Slot::default(),
    }
}

/// Writes `slot` back to item slot `index`, keeping the stack's other data when the item is the same.
fn set_slot(entity: &mut ModBlockEntity, index: usize, slot: &Slot) {
    let Some(place) = entity.items.get_mut(index) else {
        return;
    };
    *place = match (&slot.item, place.take()) {
        (None, _) => None,
        (Some(item), Some(old)) if &*old.item == item.as_str() => Some(ItemStack { count: slot.count, ..old }),
        (Some(item), _) => Some(stack_of(item, slot.count)),
    };
}

// --- Energy kept in items ---------------------------------------------------------------------------

/// The key in an item's `minecraft:custom_data` that holds its energy (FE), as Mekanism's
/// `mekanism:energy` component does.
const ENERGY_KEY: &str = "mekanism:energy";

fn custom_data(stack: &ItemStack) -> Option<&Compound> {
    let (_, data) = stack.extra.as_ref()?.other.iter().find(|(k, _)| k == "minecraft:custom_data")?;
    match data {
        Tag::Compound(c) => Some(c),
        _ => None,
    }
}

fn stored_energy(stack: &ItemStack) -> u64 {
    custom_data(stack).map_or(0, |c| get_long(c, ENERGY_KEY))
}

fn with_energy(item: &str, energy: u64) -> ItemStack {
    let mut c = Compound::new();
    put_long(&mut c, ENERGY_KEY, energy);
    let data = horizon_client::item_data::StackData {
        other: vec![("minecraft:custom_data".to_owned(), Tag::Compound(c))],
        ..Default::default()
    };
    ItemStack { extra: Some(Box::new(data)), ..stack_of(item, 1) }
}

/// Sets the energy an item holds, keeping its other components and other custom data.
fn set_energy(stack: &mut ItemStack, energy: u64) {
    let extra = stack.extra.get_or_insert_with(Default::default);
    let at = extra.other.iter().position(|(k, d)| k == "minecraft:custom_data" && matches!(d, Tag::Compound(_)));
    let at = at.unwrap_or_else(|| {
        extra.other.retain(|(k, _)| k != "minecraft:custom_data");
        extra.other.push(("minecraft:custom_data".to_owned(), Tag::Compound(Compound::new())));
        extra.other.len() - 1
    });
    if let Tag::Compound(c) = &mut extra.other[at].1 {
        put_long(c, ENERGY_KEY, energy);
    }
}

/// The item in `entity`'s slot `index` and its energy container, when it holds energy.
fn energy_item(entity: &ModBlockEntity, index: usize) -> Option<(gear::ItemEnergy, u64)> {
    let stack = entity.items.get(index)?.as_ref()?;
    let item = item_energy_of(stack)?;
    Some((item, stored_energy(stack).min(item.capacity)))
}

fn put_item_energy(entity: &mut ModBlockEntity, index: usize, before: u64, after: u64) {
    if after != before
        && let Some(Some(stack)) = entity.items.get_mut(index)
    {
        set_energy(stack, after);
    }
}

/// `EnergyInventorySlot.charge`: fills the item in slot `index` from `source`.
fn charge_slot(entity: &mut ModBlockEntity, index: usize, source: &mut EnergyContainer) {
    if let Some((item, stored)) = energy_item(entity, index) {
        let after = gear::charge(item, stored, source);
        put_item_energy(entity, index, stored, after);
    }
}

/// The energy conversion recipes (`EnergyConversionRecipeProvider`): an item and the energy one of it makes.
const REDSTONE_ENERGY: [(&str, u64); 2] = [("minecraft:redstone", 10_000), ("minecraft:redstone_block", 90_000)];

/// `EnergyInventorySlot.fillContainerOrConvert`: empties the item in slot `index` into `target`, or, for an item
/// that is no battery, turns one of it into energy when all of that fits.
fn discharge_slot(entity: &mut ModBlockEntity, index: usize, target: &mut EnergyContainer) {
    if let Some((item, stored)) = energy_item(entity, index) {
        let after = gear::discharge(item, stored, target);
        put_item_energy(entity, index, stored, after);
    } else if let Some(Some(stack)) = entity.items.get_mut(index)
        && let Some(&(_, energy)) = REDSTONE_ENERGY.iter().find(|(id, _)| *stack.item == **id)
        && target.stored + energy <= target.capacity
    {
        target.insert(energy, Action::Execute);
        stack.count -= 1;
        if stack.count == 0 {
            entity.items[index] = None;
        }
    }
}

// --- Atomic Disassembler -----------------------------------------------------------------------------

const DISASSEMBLER: &str = "mekanism:atomic_disassembler";

/// The key in the disassembler's custom data that holds its mode (Mekanism's `disassembler_mode`).
const MODE_KEY: &str = "mekanism:disassembler_mode";

fn disassembler_mode(stack: &ItemStack) -> gear::DisassemblerMode {
    let name = custom_data(stack).and_then(|c| match c.get(&JavaString::from(MODE_KEY)) {
        Some(Tag::String(s)) => s.to_string().ok(),
        _ => None,
    });
    name.as_deref().and_then(gear::DisassemblerMode::from_name).unwrap_or_default()
}

fn set_disassembler_mode(stack: &mut ItemStack, mode: gear::DisassemblerMode) {
    let stored = stored_energy(stack);
    set_energy(stack, stored);
    if let Some(Tag::Compound(c)) = stack
        .extra
        .as_mut()
        .and_then(|e| e.other.iter_mut().find(|(k, _)| k == "minecraft:custom_data"))
        .map(|(_, d)| d)
    {
        c.insert(MODE_KEY.into(), Tag::String(mode.name().into()));
    }
}

/// `ItemAtomicDisassembler.getDestroySpeed` (hook point `native_destroy_speed`, API 25): its mode's
/// efficiency while it holds the energy for the block, slower as it runs out, not at all when empty.
pub(super) fn destroy_speed(game: &Game<'_>, stack: Option<&ItemStack>, hardness: f32, speed: &mut f32) {
    if let Some(tool) = stack.filter(|s| &*s.item == modules::MEKA_TOOL)
        && game.native_on(MOD_ID)
    {
        *speed = meka_tool_speed(tool, hardness);
        return;
    }
    let Some(stack) = stack.filter(|s| &*s.item == DISASSEMBLER) else {
        return;
    };
    if game.native_on(MOD_ID) {
        *speed = gear::destroy_speed(disassembler_mode(stack), stored_energy(stack), hardness);
    }
}

/// Sneak + right click with the disassembler cycles its mode (Mekanism's mode key), with the new mode on
/// the action bar. Hook point `native_use_item`, API 13.
pub(super) fn use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if !*claimed && &*stack.item == CONFIGURATOR {
        configurator_use(game, claimed);
        return;
    }
    if !*claimed && FREE_RUNNERS.contains(&&*stack.item) {
        free_runners_use(game, claimed);
        return;
    }
    if !*claimed && &*stack.item == teleporter::PORTABLE {
        portable_teleporter_use(game, claimed);
        return;
    }
    if !*claimed && &*stack.item == seismic::READER && game.native_on(MOD_ID) {
        seismic_reader_use(game, claimed);
        return;
    }
    if !*claimed && &*stack.item == modules::MEKA_TOOL && game.native_on(MOD_ID) {
        meka_tool_use(game, claimed);
        return;
    }
    if *claimed || &*stack.item != DISASSEMBLER || !game.native_on(MOD_ID) {
        return;
    }
    if !game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) {
        return;
    }
    let Some(held) = game.held_mut().as_mut().filter(|s| &*s.item == DISASSEMBLER) else {
        return;
    };
    let mode = disassembler_mode(held).next();
    set_disassembler_mode(held, mode);
    *claimed = true;
    if let Some(player) = game.native_acting_player() {
        let line = format!("Mode changed to: {} ({})", mode.label(), mode.efficiency());
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
}

/// Sneak + right click in the air with the Configurator switches the type it configures (items,
/// chemicals, energy), as its mode key does in Mekanism.
fn configurator_use(game: &mut Game<'_>, claimed: &mut bool) {
    if !game.native_on(MOD_ID) || !game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) {
        return;
    }
    let Some(held) = game.held_mut().as_mut().filter(|s| &*s.item == CONFIGURATOR) else {
        return;
    };
    let t = configurator_mode(held).next();
    set_custom_string(held, CONFIGURATOR_KEY, t.name());
    *claimed = true;
    if let Some(player) = game.native_acting_player() {
        let line = format!("Configurator: {}", t.name());
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
}

/// `ItemAtomicDisassembler.mineBlock`: breaking a block spends the disassembler's energy for it.
pub(super) fn spend_disassembler_energy(game: &mut Game<'_>, pos: [i32; 3]) {
    if !game.native_on(MOD_ID) || game.creative {
        return;
    }
    let state = game.world.block(pos[0], pos[1], pos[2]);
    let hardness = game.res.facts().get(state).hardness;
    if let Some(stack) = game.held_mut().as_mut().filter(|s| &*s.item == DISASSEMBLER) {
        let stored = stored_energy(stack);
        let cost = gear::destroy_energy(disassembler_mode(stack), hardness);
        set_energy(stack, stored.saturating_sub(cost));
    }
    // The blocks a vein or a blast takes pay their own way (`meka_tool_area`).
    if !AREA_MINING.load(std::sync::atomic::Ordering::Relaxed)
        && let Some(stack) = game.held_mut().as_mut().filter(|s| &*s.item == modules::MEKA_TOOL)
    {
        let stored = stored_energy(stack);
        let installed = modules_of(stack);
        let silk = installed.contains_key("mekanism:silk_touch_unit");
        let cost = modules::tool_destroy_energy(modules::tool_efficiency(&installed), hardness, silk);
        set_energy(stack, stored.saturating_sub(cost));
    }
}

/// The tank a block keeps in its item when broken (Mekanism's loot copies its fluid or chemical
/// component): its key in the block entity's NBT and in the item's custom data.
fn kept_tank(kind: Kind) -> Option<(&'static str, &'static str, u64)> {
    match kind {
        Kind::FluidTank(t) => Some(("fluid", FLUID_KEY, tier::fluid_tank(t).0)),
        Kind::ChemicalTank(t) => Some(("chemical", CHEMICAL_KEY, chem::chemical_tank(t).0)),
        _ => None,
    }
}

const FLUID_KEY: &str = "mekanism:fluid";
const CHEMICAL_KEY: &str = "mekanism:chemical";

/// Breaking an energy cube that holds energy, or a fluid or chemical tank that holds something, outside
/// creative: the block goes as `Level.destroyBlock` takes it (particles, sound, no loot) and drops its
/// item with what it held, as Mekanism's loot copies the energy and tank components. True when it did,
/// so the game's own break is canceled.
pub(super) fn break_keeping_energy(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || game.creative {
        return false;
    }
    let held = game.native_with_mod_block_entity(pos, |m| {
        let k = kind(&m.id)?;
        let stack = match k {
            Kind::EnergyCube(_) => {
                let energy = get_long(&m.nbt, "energy");
                (energy > 0).then(|| with_energy(&m.id, energy))?
            }
            // A bin keeps its items and lock (`BinUpgradeData` through its item's components).
            Kind::Bin(t) => {
                let b = settle_bin(m, t);
                put_bin(&mut m.nbt, &b);
                if b.count == 0 && b.lock.is_none() {
                    return None;
                }
                let mut c = Compound::new();
                put_bin(&mut c, &b);
                let mut stack = stack_of(&m.id, 1);
                set_custom_compound(&mut stack, BIN_KEY, c);
                stack
            }
            // Personal storage keeps its contents and owner (`PersonalStorageContentsLootFunction`).
            Kind::PersonalStorage => {
                let mut c = Compound::new();
                c.insert("Items".into(), horizon_items::item_nbt::encode_items(&m.items));
                for key in ["owner", "owner_name"] {
                    if let Some(v) = get_string(&m.nbt, key) {
                        c.insert(key.into(), Tag::String(v.as_str().into()));
                    }
                }
                let mut stack = stack_of(&m.id, 1);
                set_custom_compound(&mut stack, PERSONAL_KEY, c);
                m.items.iter_mut().for_each(|i| *i = None);
                stack
            }
            _ => {
                let (key, item_key, _) = kept_tank(k)?;
                let Some(Tag::Compound(tank)) = m.nbt.get(&JavaString::from(key)) else {
                    return None;
                };
                let mut stack = stack_of(&m.id, 1);
                set_custom_compound(&mut stack, item_key, tank.clone());
                stack
            }
        };
        Some((stack, m.items.clone()))
    });
    let Some(Some((stack, items))) = held else {
        return false;
    };
    let at = horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]);
    // The items in its slots drop beside it.
    let mut drops = items;
    drops.push(Some(stack));
    game.with_level(|b, level, ticks| {
        b.destroy_block(level, ticks, at, false);
        level.drop_lost_contents(at, drops);
    });
    true
}

/// Sets a compound in an item's custom data, keeping the rest of it.
fn set_custom_compound(stack: &mut ItemStack, key: &str, value: Compound) {
    let extra = stack.extra.get_or_insert_with(Default::default);
    let at = extra.other.iter().position(|(k, d)| k == "minecraft:custom_data" && matches!(d, Tag::Compound(_)));
    let at = at.unwrap_or_else(|| {
        extra.other.retain(|(k, _)| k != "minecraft:custom_data");
        extra.other.push(("minecraft:custom_data".to_owned(), Tag::Compound(Compound::new())));
        extra.other.len() - 1
    });
    if let Tag::Compound(c) = &mut extra.other[at].1 {
        c.insert(key.into(), Tag::Compound(value));
    }
}

/// The tank an item holds under `key` in its custom data.
fn item_tank(stack: &ItemStack, key: &str, capacity: u64) -> Option<ChemicalTank> {
    let mut c = Compound::new();
    c.insert("t".into(), custom_data(stack)?.get(&JavaString::from(key))?.clone());
    let tank = get_tank(&c, "t", capacity);
    (tank.amount > 0).then_some(tank)
}

/// Blocks a machine's other cells may replace.
fn cell_is_free(block: &str) -> bool {
    matches!(
        block,
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | "minecraft:water" | "minecraft:short_grass"
            | "minecraft:tall_grass" | "minecraft:snow" | "minecraft:fern" | "minecraft:dead_bush"
    )
}

/// The facing a machine (or its bounding block) was placed with.
fn machine_facing(game: &Game<'_>, pos: [i32; 3]) -> String {
    game.native_block_property(pos, "facing").unwrap_or_else(|| "north".into())
}

/// A multi-block machine just placed at `pos` fills its other cells with bounding blocks
/// (`AttributeHasBounding.placeBoundingBlocks`). When one of them is taken the machine does not stay:
/// it goes back to the player's hands.
pub(super) fn place_bounding_blocks(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    if !game.native_on(MOD_ID) {
        return;
    }
    let id = game.native_block(pos);
    if bounding::cells(&id).is_none() {
        return;
    }
    let cells = bounding::fill(&id, &machine_facing(game, pos), pos);
    if cells.iter().any(|(at, _)| !cell_is_free(&game.native_block(*at))) {
        game.native_set_block(pos, "minecraft:air", &[]);
        game.native_give_items(&stack.item, 1);
        return;
    }
    let facing = machine_facing(game, pos);
    for (at, bounding_id) in cells {
        game.native_set_block(at, &bounding_id, &[("facing", &facing)]);
    }
}

/// A machine broken at `pos` takes its bounding blocks with it (`removeBoundingBlocks`).
pub(super) fn clear_bounding_blocks(game: &mut Game<'_>, pos: [i32; 3]) {
    if !game.native_on(MOD_ID) {
        return;
    }
    let id = game.native_block(pos);
    if bounding::cells(&id).is_none() {
        return;
    }
    for (at, bounding_id) in bounding::fill(&id, &machine_facing(game, pos), pos) {
        if game.native_block(at) == bounding_id {
            game.native_set_block(at, "minecraft:air", &[]);
        }
    }
}

/// Where the machine that owns the bounding block at `pos` stands, if `pos` is one and its machine is still there.
pub(super) fn bounding_owner(game: &Game<'_>, pos: [i32; 3]) -> Option<[i32; 3]> {
    if !game.native_on(MOD_ID) {
        return None;
    }
    let id = game.native_block(pos);
    let (main, machine) = bounding::main_of(&id, &machine_facing(game, pos), pos)?;
    (game.native_block(main) == machine).then_some(main)
}

/// A placed energy cube or tank takes what its item held.
pub(super) fn placed(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    if !game.native_on(MOD_ID) {
        return;
    }
    // Personal storage belongs to whoever placed it, unless its item names an owner already.
    if bin::is_personal(&stack.item) {
        let kept = custom_data(stack).and_then(|c| match c.get(&JavaString::from(PERSONAL_KEY)) {
            Some(Tag::Compound(k)) => Some(k.clone()),
            _ => None,
        });
        let player = game.native_acting_player();
        game.native_with_mod_block_entity(pos, |m| {
            if let Some(k) = &kept {
                let items = horizon_items::item_nbt::decode_items(k.get(&JavaString::from("Items")), bin::PERSONAL_SLOTS);
                m.items = items;
            }
            let owner = kept.as_ref().and_then(|k| Some((get_string(k, "owner")?, get_string(k, "owner_name").unwrap_or_default())));
            let owner = owner.or_else(|| player.map(|p| (uuid_text(p.uuid), p.name)));
            if let Some((uuid, name)) = owner {
                m.nbt.insert("owner".into(), Tag::String(uuid.as_str().into()));
                m.nbt.insert("owner_name".into(), Tag::String(name.as_str().into()));
            }
        });
        return;
    }
    if custom_data(stack).is_none() {
        return;
    }
    let energy = stored_energy(stack);
    game.native_with_mod_block_entity(pos, |m| match kind(&m.id) {
        Some(Kind::EnergyCube(t)) if energy > 0 => {
            put_long(&mut m.nbt, "energy", energy.min(tier::energy_cube(t).0));
        }
        Some(Kind::Bin(t)) => {
            if let Some(Tag::Compound(c)) = custom_data(stack).and_then(|c| c.get(&JavaString::from(BIN_KEY))) {
                let mut b = bin_of(c);
                b.count = b.count.min(bin::capacity(t));
                put_bin(&mut m.nbt, &b);
            }
        }
        Some(k) => {
            if let Some((key, item_key, capacity)) = kept_tank(k)
                && let Some(tank) = item_tank(stack, item_key, capacity)
            {
                put_tank(&mut m.nbt, key, &tank);
            }
        }
        None => {}
    });
}

/// "Stored Energy: 1.2 MFE" under a Mekanism item that holds energy.
pub(super) fn tooltip(game: &Game<'_>, stack: &ItemStack, lines: &mut Vec<(horizon_client::text::Text, u32)>) {
    if !game.native_on(MOD_ID) || !stack.item.starts_with("mekanism") {
        return;
    }
    if &*stack.item == CONFIGURATOR {
        let line = format!("Configuring: {}", configurator_mode(stack).name());
        lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0xAA, 0xAA, 0xAA, 0xFF)));
    }
    if FREE_RUNNERS.contains(&&*stack.item) {
        let line = format!("Mode: {}", free_runner_mode(stack).label());
        lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0xAA, 0xAA, 0xAA, 0xFF)));
    }
    if &*stack.item == DISASSEMBLER {
        let mode = disassembler_mode(stack);
        let line = format!("Mode: {} (efficiency {})", mode.label(), mode.efficiency());
        lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0xAA, 0xAA, 0xAA, 0xFF)));
    }
    for key in [FLUID_KEY, CHEMICAL_KEY] {
        if let Some(tank) = item_tank(stack, key, u64::MAX)
            && let Some(id) = tank.chemical.as_deref()
        {
            let line = format!("Stored: {} mB {id}", tank.amount);
            lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0xAA, 0xAA, 0xAA, 0xFF)));
        }
    }
    if let Some(Tag::Compound(c)) = custom_data(stack).and_then(|c| c.get(&JavaString::from(BIN_KEY)))
        && let Some(t) = bin::tier_of(&stack.item)
    {
        let line = bin_line(t, &bin_of(c));
        lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0xAA, 0xAA, 0xAA, 0xFF)));
    }
    let installed = modules_of(stack);
    for (id, n) in &installed {
        let line = format!("{} x{n}", module_name(id));
        lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0x55, 0x55, 0xFF, 0xFF)));
    }
    if let Some(d) = drive_of(stack) {
        let line = format!("Stored: {} / {} items, {} / {} types", d.total(), d.count_limit, d.items.len(), d.type_limit);
        lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0xAA, 0xAA, 0xAA, 0xFF)));
    }
    if let Some(Tag::Compound(c)) = custom_data(stack).and_then(|c| c.get(&JavaString::from(PERSONAL_KEY)))
        && let Some(name) = get_string(c, "owner_name")
    {
        let line = format!("Owner: {name}");
        lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0xAA, 0xAA, 0xAA, 0xFF)));
    }
    let energy = stored_energy(stack);
    if energy == 0 {
        return;
    }
    let line = format!("Stored Energy: {}", mekanism_core::energy::format_fe(energy));
    lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0x55, 0xFF, 0xFF, 0xFF)));
}

// --- Jetpack ----------------------------------------------------------------------------------------

const JETPACKS: [&str; 2] = ["mekanism:jetpack", "mekanism:jetpack_armored"];
/// The client's word, each tick it thrusts, that its wearer spends a mB of hydrogen.
const JETPACK_CHANNEL: &str = "mekanism:jetpack";

/// A jetpack, or MekaSuit body armor with a Jetpack Unit (`ModuleJetpackUnit`).
fn is_jetpack(stack: &ItemStack) -> bool {
    JETPACKS.contains(&&*stack.item) || (&*stack.item == modules::BODYARMOR && modules_of(stack).contains_key("mekanism:jetpack_unit"))
}

fn jetpack_capacity(stack: &ItemStack) -> u64 {
    if &*stack.item == modules::BODYARMOR { modules::SUIT_JETPACK_CAPACITY } else { gear::JETPACK_CAPACITY }
}

fn jetpack_fuel(stack: &ItemStack) -> u64 {
    item_tank(stack, CHEMICAL_KEY, jetpack_capacity(stack))
        .filter(|t| t.chemical.as_deref() == Some("mekanism:hydrogen"))
        .map_or(0, |t| t.amount)
}

fn set_jetpack_fuel(stack: &mut ItemStack, amount: u64) {
    let mut tank = ChemicalTank::new(jetpack_capacity(stack));
    tank.insert("mekanism:hydrogen", amount);
    let mut c = Compound::new();
    put_tank(&mut c, "t", &tank);
    let value = match c.get(&JavaString::from("t")) {
        Some(Tag::Compound(t)) => t.clone(),
        _ => Compound::new(),
    };
    set_custom_compound(stack, CHEMICAL_KEY, value);
}

/// The window's tick (`CommonPlayerTickHandler.tickEnd` for the local player): holding jump with a
/// fuelled jetpack on, outside creative flight, thrusts in its normal mode and stops the fall, and tells
/// the server, which spends the hydrogen.
pub(super) fn client_tick(game: &mut Game<'_>) {
    if !game.native_window() || !game.native_on(MOD_ID) || !game.native_playing() {
        return;
    }
    free_runners_tick(game);
    mekasuit_sprint(game);
    mekasuit_client(game);
    let Some(chest) = game.armor[1].as_ref().filter(|s| is_jetpack(s)) else {
        return;
    };
    if jetpack_fuel(chest) == 0 || !game.keys.contains(&winit::keyboard::KeyCode::Space) {
        return;
    }
    let Some(p) = game.player.as_mut().filter(|p| !p.flying && !p.spectator) else {
        return;
    };
    let d = gear::jetpack_normal([p.velocity.x, p.velocity.y, p.velocity.z], gear::JETPACK_THRUST);
    p.velocity.x += d[0];
    p.velocity.y += d[1];
    p.velocity.z += d[2];
    p.fall_distance = 0.0;
    game.native_send_to_server(JETPACK_CHANNEL, &[1]);
}

/// A client's packet, with its player swapped in (API 26): a jetpack thrust spends one mB of the
/// hydrogen in the jetpack they wear and stops their fall here too.
pub(super) fn server_payload(game: &mut Game<'_>, channel: &str, data: &[u8]) {
    if channel == FREQUENCY_CHANNEL && game.native_on(MOD_ID) {
        frequency_payload(game, data);
        return;
    }
    if channel == ADAPTER_CHANNEL && game.native_on(MOD_ID) {
        adapter_payload(game, data);
        return;
    }
    if channel == FUSION_CHANNEL && game.native_on(MOD_ID) {
        fusion_payload(game, data);
        return;
    }
    if channel == FISSION_CHANNEL && game.native_on(MOD_ID) {
        fission_payload(game, data);
        return;
    }
    if channel == QIO_CHANNEL && game.native_on(MOD_ID) {
        qio_payload(game, data);
        return;
    }
    if channel == SORTER_CHANNEL && game.native_on(MOD_ID) {
        sorter_payload(game, data);
        return;
    }
    if channel == OD_CHANNEL && game.native_on(MOD_ID) {
        od_payload(game, data);
        return;
    }
    if channel == MINER_CHANNEL && game.native_on(MOD_ID) {
        miner_payload(game, data);
        return;
    }
    if channel == FREE_RUNNERS_CHANNEL && game.native_on(MOD_ID) {
        free_runners_fall(game);
        return;
    }
    if channel == MEKASUIT_CHANNEL && game.native_on(MOD_ID) {
        mekasuit_sprint_paid(game);
        return;
    }
    if game.native_on(MOD_ID) {
        match channel {
            BREATHING_CHANNEL => return mekasuit_breathe(game),
            CHARGE_CHANNEL => return mekasuit_distribute(game),
            SOLAR_CHANNEL => return mekasuit_solar(game),
            _ => {}
        }
    }
    if channel != JETPACK_CHANNEL || !game.native_on(MOD_ID) {
        return;
    }
    let Some(chest) = game.armor[1].as_mut().filter(|s| is_jetpack(s)) else {
        return;
    };
    let fuel = jetpack_fuel(chest);
    if fuel == 0 {
        return;
    }
    set_jetpack_fuel(chest, fuel - 1);
    if let Some(p) = game.player.as_mut() {
        p.fall_distance = 0.0;
    }
}

/// A right click on a chemical tank holding hydrogen with a jetpack in hand fills the jetpack, as
/// Mekanism's chemical tank fills an item in its slot.
pub(super) fn fill_jetpack(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) {
        return false;
    }
    let selected = game.selected;
    let Some(held) = game.inventory.get(selected).cloned().flatten().filter(is_jetpack) else {
        return false;
    };
    let fuel = jetpack_fuel(&held);
    let capacity = jetpack_capacity(&held);
    let room = capacity - fuel;
    let taken = game.native_with_mod_block_entity(pos, |m| {
        let Some(Kind::ChemicalTank(t)) = kind(&m.id) else {
            return None;
        };
        let mut tank = get_tank(&m.nbt, "chemical", chem::chemical_tank(t).0);
        if tank.chemical.as_deref() != Some("mekanism:hydrogen") {
            return Some(0);
        }
        let n = tank.extract(room);
        put_tank(&mut m.nbt, "chemical", &tank);
        Some(n)
    });
    let Some(Some(n)) = taken else {
        return false;
    };
    let mut held = held;
    set_jetpack_fuel(&mut held, fuel + n);
    game.inventory[selected] = Some(held);
    if let Some(player) = game.native_acting_player() {
        let line = format!("Jetpack: {} / {} mB hydrogen", fuel + n, capacity);
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
    true
}

// --- Free Runners -----------------------------------------------------------------------------------

const FREE_RUNNERS: [&str; 2] = ["mekanism:free_runners", "mekanism:free_runners_armored"];
/// The window tells the server that its player falls in free runners.
const FREE_RUNNERS_CHANNEL: &str = "mekanism:free_runners";
const FREE_RUNNER_MODE_KEY: &str = "mekanism:free_runner_mode";
const STEP_ASSIST_ID: &str = "mekanism:step_assist";

fn free_runner_mode(stack: &ItemStack) -> gear::FreeRunnerMode {
    let name = custom_data(stack).and_then(|c| match c.get(&JavaString::from(FREE_RUNNER_MODE_KEY)) {
        Some(Tag::String(s)) => s.to_string().ok(),
        _ => None,
    });
    name.as_deref().and_then(gear::FreeRunnerMode::from_name).unwrap_or_default()
}

/// The free runners on the feet, when worn.
fn worn_free_runners<'g>(game: &'g Game<'_>) -> Option<&'g ItemStack> {
    game.armor[3].as_ref().filter(|s| FREE_RUNNERS.contains(&&*s.item) || &*s.item == modules::BOOTS)
}

/// On the window each tick: the step assist of the normal mode (`PlayerState.updateStepAssist`, 0.5
/// more step height while not sneaking), and while falling in runners that guard falls, a note to
/// the server, which takes the fall's energy.
fn free_runners_tick(game: &mut Game<'_>) {
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    let runners = worn_free_runners(game).map(|s| (free_runner_mode(s), stored_energy(s)));
    // MekaSuit boots step as high as their Hydraulic Propulsion Unit reaches.
    let height = match worn_free_runners(game) {
        Some(s) if &*s.item == modules::BOOTS => modules::step_assist(&modules_of(s)),
        Some(s) if free_runner_mode(s).step_boost() => gear::FREE_RUNNER_STEP,
        _ => 0.0,
    };
    let step = height > 0.0 && runners.is_some_and(|(_, energy)| energy > 0) && !sneaking;
    let Some(p) = game.player.as_mut() else {
        return;
    };
    let _ = p.set_modifier("minecraft:step_height", STEP_ASSIST_ID, height, "add_value", step);
    let falling = p.fall_distance > 0.0 && !p.flying && !p.spectator;
    if falling && runners.is_some_and(|(mode, energy)| mode.prevents_fall_damage() && energy > 0) {
        game.native_send_to_server(FREE_RUNNERS_CHANNEL, &[1]);
    }
}

/// On the server, for the player who sent the note: the fall so far is paid for from the runners'
/// energy and forgotten, so the landing does not hurt (`CommonPlayerTickHandler.livingFall`).
fn free_runners_fall(game: &mut Game<'_>) {
    let Some(fallen) = game.player.as_ref().map(|p| f64::from(p.fall_distance)) else {
        return;
    };
    let Some(feet) = game.armor[3].as_mut().filter(|s| FREE_RUNNERS.contains(&&*s.item) || &*s.item == modules::BOOTS) else {
        return;
    };
    if fallen <= 0.0 || !free_runner_mode(feet).prevents_fall_damage() {
        return;
    }
    let stored = stored_energy(feet);
    let Some(cost) = gear::fall_cost(fallen, stored) else {
        return;
    };
    set_energy(feet, stored - cost);
    if let Some(p) = game.player.as_mut() {
        p.fall_distance = 0.0;
    }
}

/// Sneak + right click with free runners in hand cycles their mode, as Mekanism's mode key does.
fn free_runners_use(game: &mut Game<'_>, claimed: &mut bool) {
    if !game.native_on(MOD_ID) || !game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) {
        return;
    }
    let Some(held) = game.held_mut().as_mut().filter(|s| FREE_RUNNERS.contains(&&*s.item)) else {
        return;
    };
    let mode = free_runner_mode(held).next();
    let stored = stored_energy(held);
    set_energy(held, stored);
    set_custom_string(held, FREE_RUNNER_MODE_KEY, mode.name());
    *claimed = true;
    if let Some(player) = game.native_acting_player() {
        let line = format!("Free runners mode: {}", mode.label());
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
}

// --- Fission Reactor ---------------------------------------------------------------------------------

/// `FissionReactorValidator`, simplified: the frame blocks joined to `start` must be exactly the shell
/// of a box 3 to 18 on each side, with fuel assemblies, control rods or air inside. The reactor and its
/// ports.
fn find_reactor(game: &Game<'_>, start: [i32; 3]) -> Option<(fission::Reactor, Vec<[i32; 3]>)> {
    let (lo, hi, seen) = find_box(game, start, fission::is_frame, fission::MIN_SIZE, fission::MAX_SIZE)?;
    let size = |k: usize| (hi[k] - lo[k] + 1) as u64;
    let mut reactor = fission::Reactor { volume: size(0) * size(1) * size(2), ..Default::default() };
    let mut ports = Vec::new();
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                let at = [x, y, z];
                let shell = x == lo[0] || x == hi[0] || y == lo[1] || y == hi[1] || z == lo[2] || z == hi[2];
                let name = block_name(game, at);
                if shell {
                    if !seen.contains(&at) {
                        return None;
                    }
                    if name == FISSION_PORT {
                        ports.push(at);
                    }
                } else if !reactor.add_inside(name) {
                    return None;
                }
            }
        }
    }
    (reactor.assemblies > 0).then_some((reactor, ports))
}

const FISSION_PORT: &str = "mekanismgenerators:fission_reactor_port";

/// The frame blocks joined to `start` (`is_frame`) and the box they span, `min` to `max` on each side.
#[allow(clippy::type_complexity)]
fn find_box(
    game: &Game<'_>,
    start: [i32; 3],
    is_frame: fn(&str) -> bool,
    min: i32,
    max: i32,
) -> Option<([i32; 3], [i32; 3], std::collections::HashSet<[i32; 3]>)> {
    let mut seen = std::collections::HashSet::from([start]);
    let mut todo = vec![start];
    let (mut lo, mut hi) = (start, start);
    while let Some(at) = todo.pop() {
        for k in 0..3 {
            lo[k] = lo[k].min(at[k]);
            hi[k] = hi[k].max(at[k]);
            if hi[k] - lo[k] >= max {
                return None;
            }
        }
        for (_, d) in DIRECTIONS {
            let next = [at[0] + d[0], at[1] + d[1], at[2] + d[2]];
            if !seen.contains(&next) && is_frame(block_name(game, next)) {
                seen.insert(next);
                todo.push(next);
            }
        }
    }
    if (0..3).any(|k| hi[k] - lo[k] + 1 < min) {
        return None;
    }
    Some((lo, hi, seen))
}
const FISSILE_FUEL: &str = "mekanism:fissile_fuel";

fn get_pos(nbt: &Compound, key: &str) -> Option<[i32; 3]> {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::IntArray(v)) if v.len() == 3 => Some([v[0], v[1], v[2]]),
        _ => None,
    }
}

/// A Fission Reactor port. Once a second each port checks its reactor; the first port runs it, holding
/// its tanks: it takes fissile fuel from the tubes and chemical tanks beside the ports and water from the
/// pipes and fluid tanks beside them, burns the fuel at the set rate while switched on (water boils to
/// steam), and gives steam and nuclear waste out of every port.
/// The reactor's rate limit in tenths of a mB/t: the default 0.1 until set, never over the maximum.
fn fission_burn(nbt: &Compound, max: u64) -> u64 {
    match nbt.get(&JavaString::from("burn")) {
        Some(_) => get_long(nbt, "burn").min(max),
        None => fission::DEFAULT_BURN_TENTHS.min(max),
    }
}

fn tick_fission_port(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    if ticker == 0 {
        match find_reactor(game, pos) {
            Some((r, ports)) => {
                put_long(&mut entity.nbt, "assemblies", r.assemblies);
                put_long(&mut entity.nbt, "volume", r.volume);
                let master = ports.first().copied().unwrap_or(pos);
                entity.nbt.insert("master".into(), Tag::IntArray(master.to_vec()));
                entity.nbt.insert("ports".into(), Tag::IntArray(ports.concat()));
            }
            None => {
                put_long(&mut entity.nbt, "assemblies", 0);
                entity.nbt.remove(&JavaString::from("master"));
            }
        }
    }
    let r = fission::Reactor { assemblies: get_long(&entity.nbt, "assemblies"), volume: get_long(&entity.nbt, "volume") };
    if r.assemblies == 0 || get_pos(&entity.nbt, "master") != Some(pos) {
        return;
    }
    let ports: Vec<[i32; 3]> = match entity.nbt.get(&JavaString::from("ports")) {
        Some(Tag::IntArray(v)) => v.chunks(3).filter(|c| c.len() == 3).map(|c| [c[0], c[1], c[2]]).collect(),
        _ => vec![pos],
    };
    let mut fuel = get_tank(&entity.nbt, "fuel", r.fuel_capacity());
    let mut water = get_tank(&entity.nbt, "water", r.water_capacity());
    let mut steam = get_tank(&entity.nbt, "steam", r.steam_capacity());
    let mut waste = get_tank(&entity.nbt, "waste", r.fuel_capacity());
    for &p in &ports {
        pull_from_carriers(game, p, &mut fuel, FISSILE_FUEL);
        pull_from_carriers(game, p, &mut water, "minecraft:water");
    }
    if get_long(&entity.nbt, "active") == 1 {
        let rate = fission_burn(&entity.nbt, r.max_burn_tenths());
        let mut partial = get_long(&entity.nbt, "partial");
        let burned = fission::burn(
            rate,
            &mut partial,
            fuel.amount,
            if water.accepts("minecraft:water") { water.amount } else { 0 },
            (steam.capacity - steam.amount) * u64::from(steam.accepts("mekanism:steam")),
            (waste.capacity - waste.amount) * u64::from(waste.accepts("mekanism:nuclear_waste")),
        );
        match burned {
            Some(b) => {
                fuel.extract(b.fuel);
                water.extract(b.steam);
                steam.insert("mekanism:steam", b.steam);
                waste.insert("mekanism:nuclear_waste", b.waste);
                put_long(&mut entity.nbt, "partial", partial);
            }
            // SCRAM: too little water to take the heat, or nowhere for the steam or waste.
            None => {
                put_long(&mut entity.nbt, "active", 0);
                put_long(&mut entity.nbt, "scram", 1);
            }
        }
    }
    for &p in &ports {
        push_chemical(game, p, &mut steam, None, u64::MAX, false);
        push_chemical(game, p, &mut waste, None, u64::MAX, false);
    }
    put_tank(&mut entity.nbt, "fuel", &fuel);
    put_tank(&mut entity.nbt, "water", &water);
    put_tank(&mut entity.nbt, "steam", &steam);
    put_tank(&mut entity.nbt, "waste", &waste);
}

/// A right click with an empty hand on a fission port: sneaking switches the reactor on or off, else it
/// steps the burn rate. Either way the action bar shows the reactor's state.
pub(super) fn use_fission_port(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || game.inventory.get(game.selected).cloned().flatten().is_some() {
        return false;
    }
    // A plain click opens the reactor's screen.
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    if !sneaking {
        return false;
    }
    let master = game.native_with_mod_block_entity(pos, |m| {
        (kind(&m.id) == Some(Kind::FissionPort)).then(|| get_pos(&m.nbt, "master"))
    });
    let Some(Some(master)) = master else {
        return false;
    };
    let line = match master {
        None => "Fission Reactor: not formed".to_owned(),
        Some(master) => game
            .native_with_mod_block_entity(master, |m| {
                let r = fission::Reactor { assemblies: get_long(&m.nbt, "assemblies"), volume: get_long(&m.nbt, "volume") };
                let mut burn = get_long(&m.nbt, "burn").clamp(1, r.max_burn_tenths().max(1));
                if sneaking {
                    let on = get_long(&m.nbt, "active") == 0;
                    put_long(&mut m.nbt, "active", u64::from(on));
                    put_long(&mut m.nbt, "scram", 0);
                } else {
                    burn = fission::next_burn(burn, r.max_burn_tenths());
                }
                put_long(&mut m.nbt, "burn", burn);
                let amount = |key: &str| {
                    let mut c = Compound::new();
                    if let Some(t) = m.nbt.get(&JavaString::from(key)) {
                        c.insert("t".into(), t.clone());
                    }
                    get_tank(&c, "t", u64::MAX).amount
                };
                let state = match (get_long(&m.nbt, "active"), get_long(&m.nbt, "scram")) {
                    (1, _) => "on",
                    (_, 1) => "SCRAM (no coolant or full)",
                    _ => "off",
                };
                format!(
                    "Fission Reactor: {state}, burn {}.{} mB/t, fuel {}, water {}, steam {}, waste {}",
                    burn / 10,
                    burn % 10,
                    amount("fuel"),
                    amount("water"),
                    amount("steam"),
                    amount("waste"),
                )
            })
            .unwrap_or_else(|| "Fission Reactor: not formed".to_owned()),
    };
    if let Some(player) = game.native_acting_player() {
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
    true
}

/// The Electric Pump: every 19 ticks (fewer with speed upgrades) it takes the water source below it,
/// as a bucket of water or, with a filter upgrade, 10 mB of heavy water. Water left by the
/// `waterSourceConversion` rule is an infinite source, so the block stays, as in Mekanism. The fluid
/// goes up into the pipe or tank above.
fn tick_pump(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let u = upgrades(&entity.nbt);
    let usage = u.energy_per_tick(isotopes::PUMP_USAGE);
    let capacity = u.max_energy(machine::STORAGE_MULTIPLIER * isotopes::PUMP_USAGE).max(usage);
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut tank = get_tank(&entity.nbt, "fluid", isotopes::PUMP_TANK);
    let below = [pos[0], pos[1] - 1, pos[2]];
    let source = (game.native_block_property(below, "level").as_deref() == Some("0"))
        .then(|| isotopes::pumped(block_name(game, below), u.filter > 0))
        .flatten();
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    match source {
        Some((fluid, amount)) if tank.accepts(fluid) && tank.amount + amount <= tank.capacity => {
            if energy.consume(usage) {
                active = true;
                progress += 1;
                if progress >= u.ticks(isotopes::PUMP_TICKS) {
                    progress = 0;
                    tank.insert(fluid, amount);
                }
            }
        }
        _ => progress = 0,
    }
    // FluidInventorySlot.drain (28, 20): an empty bucket fills from the tank into the slot below (28, 51).
    drain_tank_into_bucket(game, entity, 0, Some(1), &mut tank);
    if tank.amount > 0 {
        push_chemical(game, pos, &mut tank, Some([0, 1, 0]), u64::MAX, false);
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "fluid", &tank);
    Some(active)
}

// --- Quantum Entangloporter --------------------------------------------------------------------------

/// The frequencies' contents (`InventoryFrequency`), by name. Every entangloporter keeps a copy of its
/// frequency's in its NBT, so the first one to tick after a load brings it back.
static FREQUENCIES: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, Frequency>>> =
    std::sync::LazyLock::new(Default::default);

fn qe_frequency(nbt: &Compound) -> Option<String> {
    match nbt.get(&JavaString::from("frequency")) {
        Some(Tag::String(s)) => s.to_string().ok(),
        _ => None,
    }
}

/// Whether an entangloporter takes `chemical` into its `kind` ("fluid" or "chemical") buffer: it has a
/// frequency, whose tank is empty or holds it, and its own buffer does too.
fn qe_takes(nbt: &Compound, kind: &str, chemical: &str) -> bool {
    let (shared, inbox, cap) = match kind {
        "fluid" => ("shared_fluid", "fluid_in", qe::FLUID_BUFFER),
        _ => ("shared_chemical", "chemical_in", qe::CHEMICAL_BUFFER),
    };
    qe_frequency(nbt).is_some() && get_tank(nbt, shared, cap).accepts(chemical) && get_tank(nbt, inbox, cap).accepts(chemical)
}

/// An entangloporter: what it took in goes into its frequency, and the frequency's energy, fluid and
/// chemical go out of its front into what is there.
fn tick_entangloporter(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let Some(name) = qe_frequency(&entity.nbt) else {
        return Some(false);
    };
    let saved = || Frequency {
        energy: get_long(&entity.nbt, "shared_energy").min(qe::ENERGY_BUFFER),
        fluid: get_tank(&entity.nbt, "shared_fluid", qe::FLUID_BUFFER),
        chemical: get_tank(&entity.nbt, "shared_chemical", qe::CHEMICAL_BUFFER),
    };
    let mut freq = FREQUENCIES.lock().ok()?.entry(name.clone()).or_insert_with(saved).clone();
    let mut energy = get_long(&entity.nbt, "energy");
    let mut fluid = get_tank(&entity.nbt, "fluid_in", qe::FLUID_BUFFER);
    let mut chemical = get_tank(&entity.nbt, "chemical_in", qe::CHEMICAL_BUFFER);
    freq.absorb(&mut energy, &mut fluid, &mut chemical);
    if let Some(front) = game.native_block_property(pos, "facing").and_then(|f| direction(&f)) {
        let mut out = EnergyContainer::new(qe::ENERGY_BUFFER, u64::MAX);
        out.stored = freq.energy;
        push_energy(game, pos, &mut out, Some(front), false);
        freq.energy = out.stored;
        push_chemical(game, pos, &mut freq.fluid, Some(front), u64::MAX, false);
        push_chemical(game, pos, &mut freq.chemical, Some(front), u64::MAX, false);
    }
    put_long(&mut entity.nbt, "energy", energy);
    put_tank(&mut entity.nbt, "fluid_in", &fluid);
    put_tank(&mut entity.nbt, "chemical_in", &chemical);
    put_long(&mut entity.nbt, "shared_energy", freq.energy);
    put_tank(&mut entity.nbt, "shared_fluid", &freq.fluid);
    put_tank(&mut entity.nbt, "shared_chemical", &freq.chemical);
    if let Ok(mut all) = FREQUENCIES.lock() {
        all.insert(name, freq);
    }
    Some(true)
}

/// A right click on an entangloporter: a named name tag sets its frequency; an empty hand shows it and
/// what the frequency holds.
pub(super) fn use_entangloporter(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || kind(block_name(game, pos)) != Some(Kind::Entangloporter) {
        return false;
    }
    let held = game.inventory.get(game.selected).cloned().flatten();
    if let Some(tag) = held.as_ref().filter(|s| &*s.item == "minecraft:name_tag").and_then(|s| s.custom_name()) {
        let entry = name_tag_frequency(game, tag.as_str());
        game.native_with_mod_block_entity(pos, |m| {
            put_block_frequency(&mut m.nbt, Some(&entry));
            qe_join(&mut m.nbt, &entry.key());
        });
        tell(game, format!("Quantum Entangloporter frequency: {tag}"));
        return true;
    }
    // An empty hand opens its screen (`GuiQuantumEntangloporter`), which shows its frequency.
    false
}

/// The Fluidic Plenisher: with a bucket in its tank and the energy for the tick, every 20 ticks
/// (fewer with speed upgrades) it places a bucket of water or lava in the next spot of its fill.
fn tick_plenisher(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let u = upgrades(&entity.nbt);
    let usage = u.energy_per_tick(plenisher::PLENISHER_USAGE);
    let capacity = u.max_energy(machine::STORAGE_MULTIPLIER * plenisher::PLENISHER_USAGE).max(usage);
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut tank = get_tank(&entity.nbt, "fluid", plenisher::PLENISHER_TANK);
    // FluidInventorySlot.fill (28, 20): the empty bucket goes to the slot below it (28, 51).
    fill_tank_from_bucket(game, entity, 0, Some(1), &mut tank, |f| plenisher::placeable(f).is_some());
    put_tank(&mut entity.nbt, "fluid", &tank);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    let block = tank.chemical.as_deref().and_then(plenisher::placeable);
    if let Some(block) = block.filter(|_| tank.amount >= plenisher::BUCKET)
        && energy.stored >= usage
    {
        let mut fill = load_plenisher(&entity.nbt);
        progress += 1;
        if progress >= u.ticks(plenisher::PLENISHER_TICKS) {
            progress = 0;
            if let Some(at) = fill.operate(pos, |at| plenish_spot(game, at))
                && let Some(state) = game.pack.assets.registry.default_state(block)
            {
                let at = horizon_core::position::BlockPos::new(at[0], at[1], at[2]);
                if game.with_level(|b, level, ticks| b.set_block(level, ticks, at, state, 3)) {
                    tank.extract(plenisher::BUCKET);
                    active = true;
                }
            }
            // A finished fill only spends energy when it places a block.
            if !fill.finished || active {
                energy.consume(usage);
                active = true;
            }
        } else if !fill.finished {
            energy.consume(usage);
            active = true;
        }
        save_plenisher(&mut entity.nbt, &fill);
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "fluid", &tank);
    Some(active)
}

/// What a plenisher sees at `at` (`canReplace`): air, flowing fluid and the blocks
/// fluids wash away are free, a source is passed through, the rest is in the way.
fn plenish_spot(game: &Game<'_>, at: [i32; 3]) -> Spot {
    if !game.world.loaded(at[0], at[2]) || !(game.world.bottom_y()..game.world.top_y()).contains(&at[1]) {
        return Spot::Blocked;
    }
    let facts = game.res.facts().get(game.world.block(at[0], at[1], at[2]));
    let name = &*facts.name;
    if matches!(name, "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air") {
        return Spot::Free;
    }
    if facts.fluid != horizon_client::blocks::Fluid::None {
        let source = facts.waterlogged || game.native_block_property(at, "level").as_deref() == Some("0");
        return if source { Spot::Source } else { Spot::Free };
    }
    // `canBeReplaced(fluid)`: water and lava both wash away replaceable blocks.
    if facts.replaceable { Spot::Free } else { Spot::Blocked }
}

fn load_plenisher(nbt: &Compound) -> Plenisher {
    let spots = |key: &str| match nbt.get(&JavaString::from(key)) {
        Some(Tag::IntArray(v)) => Plenisher::unflatten(v),
        _ => Vec::new(),
    };
    Plenisher { active: spots("active"), used: spots("used").into_iter().collect(), finished: get_int(nbt, "finished") == 1 }
}

fn save_plenisher(nbt: &mut Compound, fill: &Plenisher) {
    nbt.insert("active".into(), Tag::IntArray(Plenisher::flatten(fill.active.iter().copied())));
    nbt.insert("used".into(), Tag::IntArray(Plenisher::flatten(fill.used.iter().copied())));
    put_int(nbt, "finished", u32::from(fill.finished));
}

/// Sneak-right-click on a plenisher with an empty hand starts its fill over.
pub(super) fn use_plenisher(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || kind(block_name(game, pos)) != Some(Kind::Plenisher) {
        return false;
    }
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    if !sneaking || game.inventory.get(game.selected).cloned().flatten().is_some() {
        return false;
    }
    game.native_with_mod_block_entity(pos, |m| save_plenisher(&mut m.nbt, &Plenisher::default()));
    tell(game, "Reset the Fluidic Plenisher".to_owned());
    true
}

/// The Solar Neutron Activator: while it sees the sun, lithium piped in becomes tritium, up to 64 mB a
/// tick at noon, pushed to the tubes and tanks beside it.
fn tick_activator(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let mut input = get_tank(&entity.nbt, "input", isotopes::ACTIVATOR_TANK);
    let mut output = get_tank(&entity.nbt, "output", isotopes::ACTIVATOR_TANK);
    let sees_sun = overworld(game) && game.sky_darken() < 4 && can_see_sky(game, pos);
    let brightness = if sees_sun {
        let mut sky = game.res.environment().sample(game.world.day_time, None, None);
        let (rain, thunder) = game.world.weather.levels(1.0);
        horizon_client::environment::apply_weather(&mut sky, rain, thunder);
        f64::from(sky.sky_light_factor)
    } else {
        0.0
    };
    let mut made = 0;
    if let Some((per, out)) = input.chemical.as_deref().and_then(isotopes::activating)
        && output.accepts(out)
    {
        made = isotopes::activator_rate(sees_sun, brightness)
            .min(input.amount / per)
            .min(output.capacity - output.amount);
        input.extract(made * per);
        output.insert(out, made);
    }
    if output.amount > 0 {
        push_chemical(game, pos, &mut output, None, u64::MAX, false);
    }
    put_tank(&mut entity.nbt, "input", &input);
    put_tank(&mut entity.nbt, "output", &output);
    Some(made > 0)
}

// --- Fusion Reactor ----------------------------------------------------------------------------------

/// `FusionReactorValidator`, simplified: the reactor blocks joined to `start` span a 5×5×5 box whose
/// faces follow `ALLOWED_GRID`, with the controller in the middle of the top and air inside. The box's
/// low corner and its ports.
fn find_fusion(game: &Game<'_>, start: [i32; 3]) -> Option<([i32; 3], Vec<[i32; 3]>)> {
    let (lo, _, seen) = find_box(game, start, fusion::is_part, fusion::SIZE, fusion::SIZE)?;
    let mut ports = Vec::new();
    for x in 0..fusion::SIZE {
        for y in 0..fusion::SIZE {
            for z in 0..fusion::SIZE {
                let at = [lo[0] + x, lo[1] + y, lo[2] + z];
                let name = block_name(game, at);
                let controller_spot = [x, y, z] == [2, fusion::SIZE - 1, 2];
                if controller_spot != (name == fusion::CONTROLLER) {
                    return None;
                }
                match fusion::requirement([x, y, z]) {
                    None if !matches!(name, "minecraft:air" | "minecraft:cave_air") => return None,
                    Some(1) if name != fusion::FRAME || !seen.contains(&at) => return None,
                    Some(2) if !seen.contains(&at) => return None,
                    _ => {}
                }
                if name == fusion::PORT {
                    ports.push(at);
                }
            }
        }
    }
    (!ports.is_empty()).then_some((lo, ports))
}

/// The tanks a master port keeps, as `fusion::Tanks`.
fn fusion_tanks(nbt: &Compound, injection: u64) -> [ChemicalTank; 5] {
    [
        get_tank(nbt, "deuterium", fusion::FUEL_CAPACITY),
        get_tank(nbt, "tritium", fusion::FUEL_CAPACITY),
        get_tank(nbt, "fuel", fusion::FUEL_CAPACITY),
        get_tank(nbt, "water", fusion::water_capacity(injection)),
        get_tank(nbt, "steam", fusion::steam_capacity(injection)),
    ]
}

fn fusion_plasma(nbt: &Compound) -> fusion::Plasma {
    fusion::Plasma {
        plasma: get_double(nbt, "plasma").unwrap_or(fusion::AMBIENT),
        case: get_double(nbt, "case").unwrap_or(fusion::AMBIENT),
        burning: get_long(nbt, "burning") == 1,
    }
}

fn put_fusion_plasma(nbt: &mut Compound, p: &fusion::Plasma) {
    nbt.insert("plasma".into(), Tag::Double(p.plasma.to_bits()));
    nbt.insert("case".into(), Tag::Double(p.case.to_bits()));
    put_long(nbt, "burning", u64::from(p.burning));
}

fn fusion_injection(nbt: &Compound) -> u64 {
    match nbt.get(&JavaString::from("injection")) {
        None => fusion::DEFAULT_INJECTION,
        Some(_) => get_long(nbt, "injection").min(fusion::MAX_INJECTION),
    }
}

/// `GuiFusionReactorHeat.MAX_LEVEL`.
const FUSION_MAX_TEMP: u64 = 500_000_000;

/// A Fusion Reactor port. Once a second each port checks its reactor; the first port runs it: it takes
/// deuterium and tritium from the tubes and chemical tanks beside the ports and water from the pipes and
/// fluid tanks beside them, runs the plasma for a tick, and gives energy and steam out of every port.
fn tick_fusion_port(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    if ticker == 0 {
        match find_fusion(game, pos) {
            Some((_, ports)) => {
                entity.nbt.insert("master".into(), Tag::IntArray(ports[0].to_vec()));
                entity.nbt.insert("ports".into(), Tag::IntArray(ports.concat()));
            }
            None => {
                entity.nbt.remove(&JavaString::from("master"));
            }
        }
    }
    if get_pos(&entity.nbt, "master") != Some(pos) {
        return;
    }
    let ports: Vec<[i32; 3]> = match entity.nbt.get(&JavaString::from("ports")) {
        Some(Tag::IntArray(v)) => v.chunks(3).filter(|c| c.len() == 3).map(|c| [c[0], c[1], c[2]]).collect(),
        _ => vec![pos],
    };
    let injection = fusion_injection(&entity.nbt);
    let [mut deuterium, mut tritium, mut fuel, mut water, mut steam] = fusion_tanks(&entity.nbt, injection);
    for &p in &ports {
        pull_from_carriers(game, p, &mut deuterium, fusion::DEUTERIUM);
        pull_from_carriers(game, p, &mut tritium, fusion::TRITIUM);
        pull_from_carriers(game, p, &mut water, "minecraft:water");
    }
    let mut plasma = fusion_plasma(&entity.nbt);
    let mut t = fusion::Tanks {
        deuterium: deuterium.amount,
        tritium: tritium.amount,
        fuel: fuel.amount,
        water: if water.accepts("minecraft:water") { water.amount } else { 0 },
        steam: if steam.accepts("mekanism:steam") { steam.amount } else { steam.capacity },
    };
    let before = t;
    let out = fusion::tick(&mut plasma, &mut t, injection, steam.capacity);
    deuterium.extract(before.deuterium - t.deuterium);
    tritium.extract(before.tritium - t.tritium);
    if t.fuel > before.fuel {
        fuel.insert("mekanismgenerators:fusion_fuel", t.fuel - before.fuel);
    } else {
        fuel.extract(before.fuel - t.fuel);
    }
    water.extract(before.water - t.water);
    if t.steam > before.steam {
        steam.insert("mekanism:steam", t.steam - before.steam);
    }
    let mut energy = EnergyContainer::new(fusion::ENERGY_CAPACITY, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(energy.capacity);
    energy.insert(out.energy, Action::Execute);
    for &p in &ports {
        push_energy(game, p, &mut energy, None, false);
        if steam.amount > 0 {
            push_chemical(game, p, &mut steam, None, u64::MAX, false);
        }
    }
    put_fusion_plasma(&mut entity.nbt, &plasma);
    for (key, tank) in [
        ("deuterium", &deuterium),
        ("tritium", &tritium),
        ("fuel", &fuel),
        ("water", &water),
        ("steam", &steam),
    ] {
        put_tank(&mut entity.nbt, key, tank);
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_long(&mut entity.nbt, "production", out.energy);
}

/// A right click with an empty hand on a Fusion Reactor's controller or port: sneaking ignites the
/// reactor (what its lasers and a hohlraum do), else it steps the injection rate. Either way the action
/// bar shows the reactor's state.
pub(super) fn use_fusion(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || game.inventory.get(game.selected).cloned().flatten().is_some() {
        return false;
    }
    let master = if block_name(game, pos) == fusion::CONTROLLER {
        find_fusion(game, pos).map(|(_, ports)| ports[0])
    } else {
        match game.native_with_mod_block_entity(pos, |m| {
            (kind(&m.id) == Some(Kind::FusionPort)).then(|| get_pos(&m.nbt, "master"))
        }) {
            Some(Some(master)) => master,
            _ => return false,
        }
    };
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    // A plain click on a port opens the reactor's screen; the controller has none.
    if !sneaking && block_name(game, pos) != fusion::CONTROLLER {
        return false;
    }
    let line = master
        .and_then(|master| {
            game.native_with_mod_block_entity(master, |m| {
                let mut injection = fusion_injection(&m.nbt);
                let mut plasma = fusion_plasma(&m.nbt);
                if sneaking {
                    fusion::ignite(&mut plasma, injection);
                    put_fusion_plasma(&mut m.nbt, &plasma);
                } else {
                    injection = fusion::next_injection(injection);
                    put_long(&mut m.nbt, "injection", injection);
                }
                let [d, t, _, water, steam] = fusion_tanks(&m.nbt, injection);
                format!(
                    "Fusion Reactor: {}, injection {injection}, plasma {:.1} MK, case {:.1} MK, {} J/t, \
                     D {}, T {}, water {}, steam {}",
                    if plasma.burning { "burning" } else { "not burning" },
                    plasma.plasma / 1e6,
                    plasma.case / 1e6,
                    get_long(&m.nbt, "production"),
                    d.amount,
                    t.amount,
                    water.amount,
                    steam.amount,
                )
            })
        })
        .unwrap_or_else(|| "Fusion Reactor: not formed".to_owned());
    if let Some(player) = game.native_acting_player() {
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
    true
}

// --- Screens -----------------------------------------------------------------------------------------
//
// Mekanism's own screens for its machines (Horizon API 19/20), drawn with its GUI sprites
// (`assets/mekanism/textures/gui/sprites`). The menu's data slots are 16-bit, so a machine sends its
// energy as thousandths of its capacity for the bar, then the stored energy and capacity in 15-bit
// pieces for the tooltip, then its upgrades.

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
fn fill_tiled(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, texture: &str, rect: [f32; 4], argb: u32) {
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
    ("mekanism:chlorine", "Chlorine", 0xCFE800),
    ("mekanism:sodium", "Sodium", 0xE9FEF4),
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
    Horizontal([f32; 2]),
    /// The energy cube's wide energy gauge.
    Gauge([f32; 2]),
    /// A small-medium energy gauge (`GuiEnergyGauge` with `GaugeType.SMALL_MED`).
    SmallGauge([f32; 2]),
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
        Kind::Factory(..) => Layout {
            slots: [None, None, None],
            gauges: vec![],
            power: Some(PowerBar::Vertical([screen_width(block) - 12.0, 16.0])),
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
        Kind::Machine(recipe, _) => Layout {
            slots: [
                Some(([64.0, 17.0], "slot/normal")),
                Some(([64.0, 53.0], if recipe == "combining" { "slot/normal" } else { "slot/power" })),
                Some(([116.0, 35.0], "slot/output")),
            ],
            gauges: vec![],
            power: Some(PowerBar::Vertical([164.0, 16.0])),
            progress: Some(([86.0, 38.0], "progress/bar", [25.0, 9.0])),
        },
        // TileEntityChemicalOxidizer and GuiChemicalOxidizer.
        Kind::Oxidizer => Layout {
            slots: [Some(([26.0, 36.0], "slot/normal")), Some(([152.0, 14.0], "slot/power")), None],
            gauges: vec![gauge("output", chem::OXIDIZER_TANK, 131.0, 13.0, STANDARD)],
            power: Some(PowerBar::Horizontal([115.0, 75.0])),
            progress: large_right(64.0, 40.0),
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
            progress: None,
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
            gauges: vec![gauge("fuel", mekanism_core::generator::BIO_TANK, 55.0, 18.0, WIDE)],
            power: Some(PowerBar::Vertical([164.0, 15.0])),
            progress: None,
        },
        // The gas-burning generator's fuel gauge (GuiGasGenerator).
        Kind::GasGenerator => Layout {
            slots: [Some(([143.0, 35.0], "slot/power")), None, None],
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
        // TileEntityMetallurgicInfuser and its screen: the infuse item (17, 35), input (51, 43), output
        // (109, 43), the infusion bar (7, 15).
        Kind::Infuser => Layout {
            slots: [
                Some(([51.0, 43.0], "slot/normal")),
                Some(([17.0, 35.0], "slot/normal")),
                Some(([109.0, 43.0], "slot/output")),
            ],
            gauges: vec![gauge("infuse", chem::INFUSER_TANK, 7.0, 15.0, [6.0, 54.0])],
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
            gauges: vec![gauge("chemical", chem::ADVANCED_TANK, 68.0, 36.0, [8.0, 14.0])],
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
            slots: [None; 3],
            // `GuiChemicalBar`: a horizontal bar of 116 x 10 (drawn by `draw_tank_screen`).
            gauges: vec![gauge("chemical", chem::chemical_tank(t).0, 42.0, 16.0, [116.0, 10.0])],
            power: None,
            progress: None,
        },
        Kind::FluidTank(t) => Layout {
            slots: [Some(([146.0, 19.0], "slot/normal")), Some(([146.0, 51.0], "slot/output")), None],
            gauges: vec![gauge("fluid", tier::fluid_tank(t).0, 79.0, 13.0, STANDARD)],
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
            ],
            power: None,
            progress: None,
        },
        Kind::BoilerValve => Layout {
            slots: [None; 3],
            // `GuiThermoelectricBoiler`: the heated coolant and water tanks on the left, the steam and cooled
            // coolant tanks on the right (the port has no coolant, so those two stay empty).
            gauges: vec![
                gauge("heated_coolant", 0, 6.0, 13.0, STANDARD),
                gauge("water", 0, 26.0, 13.0, STANDARD),
                gauge("steam", 0, 174.0, 13.0, STANDARD),
                gauge("cooled_coolant", 0, 194.0, 13.0, STANDARD),
            ],
            power: None,
            progress: None,
        },
        Kind::TurbineValve => Layout {
            slots: [None; 3],
            // `GuiIndustrialTurbine`: a medium steam gauge, the energy bar at the right edge.
            // The two "stats" gauges carry the counts the statistics screen shows (never drawn as gauges).
            gauges: vec![
                gauge("steam", 0, 6.0, 13.0, MEDIUM),
                gauge("stats_a", 1 << 58, 0.0, 0.0, SMALL),
                gauge("stats_b", 1 << 58, 0.0, 0.0, SMALL),
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
        Kind::DynamicValve => Layout {
            slots: [None; 3],
            gauges: vec![gauge("fluid", 0, 79.0, 13.0, STANDARD)],
            power: None,
            progress: None,
        },
        // GuiSPS: the polonium gauge on the left, the antimatter gauge on the right.
        Kind::SpsPort => Layout {
            slots: [None; 3],
            gauges: vec![
                gauge("input", sps::INPUT_CAPACITY, 7.0, 17.0, STANDARD),
                gauge("output", sps::OUTPUT_CAPACITY, 151.0, 17.0, STANDARD),
            ],
            power: None,
            progress: None,
        },
        Kind::Vibrator => Layout {
            slots: [Some(([143.0, 35.0], "slot/power")), None, None],
            gauges: vec![],
            power: Some(PowerBar::Gauge([55.0, 18.0])),
            progress: None,
        },
        Kind::InductionPort | Kind::Laser | Kind::LaserAmplifier => Layout {
            slots: [None; 3],
            gauges: vec![],
            power: Some(PowerBar::Gauge([55.0, 18.0])),
            progress: None,
        },
        // TileEntityDigitalMiner and its screen: the energy slot (152, 20) and the power bar.
        // GuiSolarNeutronActivator: no energy, lit while the sun lets it work.
        Kind::Activator => Layout {
            slots: [None, None, None],
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
fn energy_capacity(entity: &ModBlockEntity) -> u64 {
    match kind(&entity.id) {
        Some(Kind::HeatGenerator) => HeatGenerator::new().energy.capacity,
        Some(Kind::Wind) => mekanism_core::generator::WIND_STORAGE,
        Some(Kind::BioGenerator) => mekanism_core::generator::BIO_STORAGE,
        Some(Kind::GasGenerator) => chem::GAS_GENERATOR_STORAGE,
        Some(Kind::Solar(_, storage)) => storage,
        Some(Kind::FusionPort) => fusion::ENERGY_CAPACITY,
        Some(Kind::TurbineValve) => turbine_of(&entity.nbt).energy_capacity(),
        Some(Kind::InductionPort) => get_long(&entity.nbt, "capacity"),
        Some(Kind::LaserAmplifier | Kind::TractorBeam) => laser::RECEPTOR_STORAGE,
        Some(Kind::FissionPort | Kind::BoilerValve | Kind::SpsPort | Kind::DynamicValve) => 0,
        _ => receiver(&entity.id, &entity.nbt, true, false).map_or(0, |r| r.0),
    }
}

/// The data a screen reads past the furnace's four slots, for the blocks that do not send it
/// themselves (the electric machines do, in `tick_machine`): energy, then each gauge's tank.
fn screen_data(entity: &mut ModBlockEntity, facing: usize) {
    let Some(l) = layout(&entity.id) else {
        return;
    };
    if !matches!(kind(&entity.id), Some(Kind::Machine(..))) {
        entity.data.resize(4, 0);
        let capacity = energy_capacity(entity);
        entity.data.extend(wide(get_long(&entity.nbt, "energy").min(capacity)));
        entity.data.extend(wide(capacity));
        let u = upgrades(&entity.nbt);
        entity.data.extend([u.get(Upgrade::Speed) as i32, u.get(Upgrade::Energy) as i32]);
        entity.data.extend(wide(0));
        // The plenisher's screen says whether it has filled all it can (in the progress slot).
        if kind(&entity.id) == Some(Kind::Plenisher) {
            entity.data[2] = get_int(&entity.nbt, "finished") as i32;
        }
        // The progress arrow is full while it converted something this tick (`didProcess`).
        if kind(&entity.id) == Some(Kind::Oredictionificator) {
            entity.data[2] = get_int(&entity.nbt, "did") as i32;
            entity.data[3] = 1;
        }
        // A heater's temperature in tenths of a kelvin: the ambient 300 K and its held heat over its
        // heat capacity (`HEAT_CAPACITY`, 100); then the fuelwood heater's burn time, or the
        // resistive heater's usage a tick.
        if kind(&entity.id) == Some(Kind::Evaporation) {
            let height = get_int(&entity.nbt, "height");
            entity.data[0] = i32::from(height > 0);
            entity.data[1] = height as i32;
            let temp = get_double(&entity.nbt, "temperature").unwrap_or(evap::AMBIENT_TEMP);
            entity.data[2] = (temp * 10.0).clamp(0.0, 32_767.0) as i32;
            entity.data[3] = get_int(&entity.nbt, "gain").min(32_767) as i32;
            entity.data.truncate(DATA_USAGE);
            entity.data.extend(wide(get_long(&entity.nbt, "loss")));
        }
        if let Some(k @ (Kind::Heater | Kind::FuelwoodHeater)) = kind(&entity.id) {
            let temp = 300.0 + get_double(&entity.nbt, "heat_out").unwrap_or(0.0) / 100.0;
            entity.data[2] = (temp * 10.0).clamp(0.0, 32_767.0) as i32;
            if k == Kind::FuelwoodHeater {
                entity.data[3] = get_int(&entity.nbt, "burn_time").min(32_767) as i32;
            } else {
                entity.data.truncate(DATA_USAGE);
                entity.data.extend(wide(heater_usage(&entity.nbt)));
            }
        }
    }
    for g in &l.gauges {
        let mut tank = get_tank(&entity.nbt, g.key, g.capacity);
        // The heat generator keeps its lava as a plain amount.
        if g.key == "lava" {
            tank = ChemicalTank::new(g.capacity);
            tank.insert("minecraft:lava", u64::from(get_int(&entity.nbt, "lava")));
        }
        if g.key.starts_with("stats_") && kind(&entity.id) == Some(Kind::TurbineValve) {
            let t = turbine_of(&entity.nbt);
            let max_production = (t.energy_multiplier() * t.max_flow()).clamp(0.0, f64::from(i32::MAX)) as u64;
            let packed = if g.key == "stats_a" {
                t.lower_volume & 0xF_FFFF | (t.dispersers & 0xFFF) << 20 | (t.vents & 0xFFF) << 32 | (t.blades() & 0xFF) << 44
            } else {
                t.coils & 0xFFF | (t.condensers & 0xFFF) << 12 | max_production << 24
            };
            tank = ChemicalTank::new(g.capacity);
            tank.insert("minecraft:lava", packed);
        }
        if matches!(g.key, "plasma" | "case") && kind(&entity.id) == Some(Kind::FusionPort) {
            let p = fusion_plasma(&entity.nbt);
            tank = ChemicalTank::new(g.capacity);
            tank.insert("minecraft:lava", (if g.key == "plasma" { p.plasma } else { p.case }).max(0.0) as u64);
        }
        if g.key == "water" && kind(&entity.id) == Some(Kind::Evaporation) {
            tank.capacity = evap::input_capacity(get_int(&entity.nbt, "height"));
        }
        if g.capacity == 0 {
            tank.capacity = multiblock_capacity(entity, g.key);
        }
        entity.data.extend(wide(tank.amount));
        entity.data.extend(wide(tank.capacity));
        entity.data.push(chemical_index(tank.chemical.as_deref()));
    }
    // Then the redstone mode, with whether the machine got a signal this tick and the last (2 bits).
    if tabs(&entity.id) & TAB_REDSTONE != 0 {
        entity.data.resize(DATA_GAUGES + l.gauges.len() * GAUGE_SLOTS, 0);
        let signal = get_int(&entity.nbt, "powered") & 3;
        entity.data.push((get_int(&entity.nbt, "control") & 3 | signal << 2 | (if matches!(kind(&entity.id), Some(Kind::ChemicalTank(_))) { get_int(&entity.nbt, "dump") % 3 } else if kind(&entity.id) == Some(Kind::DigitalMiner) { miner_flags(&entity.nbt) } else { get_int(&entity.nbt, "warn") & 15 | energy_warn(&entity.id, &entity.nbt) }) << 4) as i32);
        // Then, for the side configuration window, each type's faces in relative-side order and
        // which types' auto-eject is off.
        if tabs(&entity.id) & TAB_SIDES != 0 {
            for t in Transmission::ALL {
                entity.data.push(sides_of(&entity.nbt, t).relative(facing) as i32);
            }
            entity.data.push((get_int(&entity.nbt, "no_eject") & 7) as i32);
        }
        // Then a factory's progress on each process, as the height of its arrow (0 to 20).
        if let Some(Kind::Factory(n, _, usage)) = kind(&entity.id) {
            let ticks = i64::from(Factory::new(n, usage, upgrades(&entity.nbt)).machine.ticks_required.max(1));
            let saved = match entity.nbt.get(&JavaString::from("progress")) {
                Some(Tag::IntArray(v)) => v.clone(),
                _ => vec![],
            };
            for i in 0..n {
                let done = i64::from(saved.get(i).copied().unwrap_or(0).max(0));
                let warn = match entity.nbt.get(&JavaString::from("warn")) {
                    Some(Tag::IntArray(v)) => v.get(i).copied().unwrap_or(0) & 15,
                    _ => 0,
                };
                entity.data.push((done * 20 / ticks).clamp(0, 20) as i32 | warn << 5);
            }
            // Then whether auto-sort is on (the sorting tab).
            entity.data.resize(redstone_slot(&l) + 5 + 9, 0);
            entity.data.push(get_int(&entity.nbt, "sorting") as i32 & 1);
        }
        // The miner's to-mine count, in two slots after the redstone slot.
        if kind(&entity.id) == Some(Kind::DigitalMiner) {
            entity.data.resize(redstone_slot(&l) + 1, 0);
            let n = get_int(&entity.nbt, "to_mine");
            entity.data.extend([(n & 0x7FFF) as i32, (n >> 15 & 0x7FFF) as i32, miner_region(&entity.nbt).radius]);
        }
        // Last, the transporter window's ejector: the output colour, the six input colours (each the
        // dye + 1, or 0 for none) and strict input.
        if tabs(&entity.id) & TAB_SIDES != 0 {
            entity.data.resize(redstone_slot(&l) + EJECTOR_AT, 0);
            for key in ["ej_out", "ej_in0", "ej_in1", "ej_in2", "ej_in3", "ej_in4", "ej_in5", "ej_strict"] {
                entity.data.push(get_int(&entity.nbt, key) as i32);
            }
        }
    }
}

/// What the miner's screen reads: the warning (bit 0), the searcher's state (bits 4-5), running (6), the
/// Auto-eject, Auto-pull and silk touch switches (7-9) and low power (10).
fn miner_flags(nbt: &Compound) -> u32 {
    let flag = |key: &str, bit: u32| u32::from(get_int(nbt, key) == 1) << bit;
    let low = u32::from(miner_usage(nbt) > mekanism_core::miner::STORAGE) << 10;
    get_int(nbt, "warn") & 1 | (get_int(nbt, "mstate") & 3) << 4 | flag("running", 6) | flag("eject", 7) | flag("pull", 8) | flag("silk", 9) | low
}

/// What a machine takes a tick before upgrades (its `getEnergyPerTick`), for the machines whose bar warns.
fn base_usage(k: Kind) -> Option<u64> {
    Some(match k {
        Kind::Oxidizer => chem::OXIDIZER_USAGE,
        Kind::Dissolution => nuclear::DISSOLUTION_USAGE,
        Kind::Centrifuge => nuclear::CENTRIFUGE_USAGE,
        Kind::Washer => nuclear::WASHER_USAGE,
        Kind::Crystallizer => nuclear::CRYSTALLIZER_USAGE,
        Kind::Separator => chem::SEPARATOR_USAGE,
        Kind::Nucleosynthesizer => chem::NUCLEOSYNTHESIZER_USAGE,
        Kind::ChemicalInfuser => chem::CHEMICAL_INFUSER_USAGE,
        Kind::Rotary => chem::ROTARY_USAGE,
        Kind::Infuser => chem::INFUSER_USAGE,
        Kind::Advanced(m) => m.usage,
        Kind::Pump => isotopes::PUMP_USAGE,
        Kind::Plenisher => plenisher::PLENISHER_USAGE,
        _ => return None,
    })
}

/// `WarningType.NOT_ENOUGH_ENERGY` on a machine's power bar: the energy it holds is under its usage a tick
/// (bit 0 of the warning field).
fn energy_warn(id: &str, nbt: &Compound) -> u32 {
    let Some(base) = kind(id).and_then(base_usage) else {
        return 0;
    };
    u32::from(get_long(nbt, "energy") < upgrades(nbt).energy_per_tick(base))
}

/// A machine screen's height: the nucleosynthesizer's is 27 taller (`DEFAULT_IMAGE_HEIGHT + 27`).
fn screen_height(block: &str) -> f32 {
    match kind(block) {
        Some(Kind::Nucleosynthesizer) => 193.0,
        // `DEFAULT_IMAGE_HEIGHT + 5`.
        Some(Kind::Reaction) => 171.0,
        // `DEFAULT_IMAGE_HEIGHT + 16`.
        Some(Kind::SpsPort) => 182.0,
        // `DEFAULT_IMAGE_HEIGHT + 89`.
        Some(Kind::FissionPort) => 255.0,
        // `DEFAULT_IMAGE_HEIGHT + 76`.
        Some(Kind::DigitalMiner) => 242.0,
        // `DEFAULT_IMAGE_HEIGHT + 64`.
        Some(Kind::Oredictionificator) => 230.0,
        _ => 166.0,
    }
}

/// The item slots past the three of `Layout` (a hopper menu's fourth): the energy slot of the machines
/// that have more than an input, an output and one more slot (`EnergyInventorySlot.fillOrConvert`).
fn extra_slots(block: &str) -> Vec<([f32; 2], &'static str)> {
    match kind(block) {
        Some(Kind::Machine("combining", _)) | Some(Kind::Advanced(_)) => vec![([39.0, 35.0], "slot/power")],
        Some(Kind::Infuser) => vec![([143.0, 35.0], "slot/power")],
        Some(Kind::Nucleosynthesizer) => vec![([173.0, 69.0], "slot/power")],
        // The bucket slot under the water slot (`outputInputSlot`).
        Some(Kind::Evaporation) => vec![([28.0, 51.0], "slot/output")],
        // `ChemicalInventorySlot`s: the output's drain slot, the input's fill slot (`TileEntityChemical*`).
        Some(Kind::Oxidizer) => vec![([152.0, 55.0], "slot/normal")],
        Some(Kind::Dissolution) => vec![([8.0, 65.0], "slot/normal"), ([152.0, 55.0], "slot/normal")],
        Some(Kind::Crystallizer) => vec![([8.0, 65.0], "slot/normal")],
        Some(Kind::ChemicalInfuser) => vec![([80.0, 65.0], "slot/normal")],
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
fn draw_factory_slots(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, _block: &str, n: usize, l: &Layout, data: &[i32], origin: [f32; 2]) {
    let [ox, oy] = origin;
    let first = redstone_slot(l) + 5;
    sprite(game, batch, "slot/power", [0.0, 0.0, 18.0, 18.0], [ox + 6.0, oy + 12.0, 18.0, 18.0]);
    for (i, (input, output)) in factory_slots(n).into_iter().enumerate() {
        sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + input[0] - 1.0, oy + input[1] - 1.0, 18.0, 18.0]);
        sprite(game, batch, "slot/output", [0.0, 0.0, 18.0, 18.0], [ox + output[0] - 1.0, oy + output[1] - 1.0, 18.0, 18.0]);
        let (x, y) = (ox + input[0] + 4.0, oy + 33.0);
        sprite(game, batch, "progress/down/empty", [0.0, 0.0, 8.0, 20.0], [x, y, 8.0, 20.0]);
        let h = (data.get(first + i).copied().unwrap_or(0) & 31).clamp(0, 20) as f32;
        // `GuiSlot.warning` on the process's input and output.
        let warn = data.get(first + i).copied().unwrap_or(0) >> 5;
        if warn & 2 != 0 {
            sprite(game, batch, "slot/warning", [0.0, 0.0, 18.0, 18.0], [ox + input[0] - 1.0, oy + input[1] - 1.0, 18.0, 18.0]);
        }
        if warn & 4 != 0 {
            sprite(game, batch, "slot/warning", [0.0, 0.0, 18.0, 18.0], [ox + output[0] - 1.0, oy + output[1] - 1.0, 18.0, 18.0]);
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
/// keeps the signal of this tick and the last in `powered` (bit 0 now, bit 1 last tick).
fn redstone_allows(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> bool {
    let at = horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]);
    let powered = game.with_level(|b, level, ticks| b.has_neighbor_signal(level, ticks, at));
    let last = get_int(&entity.nbt, "powered") & 1 != 0;
    put_int(&mut entity.nbt, "powered", u32::from(powered) | u32::from(last) << 1);
    RedstoneControl::from_ordinal(get_int(&entity.nbt, "control")).allows(powered, last)
}

// --- Tabs -------------------------------------------------------------------------------------------
//
// The tabs Mekanism's screens add beside the panel (`GuiMekanismTile.addGenericTabs` and
// `GuiConfigurableTile`): each a holder (`holder_left`/`holder_right`, tinted with the tab's colour),
// a button inset 4 px and the tab's 18×18 icon on it.

const TAB_UPGRADE: u8 = 1;
const TAB_REDSTONE: u8 = 2;
const TAB_SECURITY: u8 = 4;
/// The side and transporter configuration tabs.
const TAB_SIDES: u8 = 8;

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
            | Kind::Dissolution
            | Kind::Crystallizer
            | Kind::Separator
            | Kind::Infuser
            | Kind::Advanced(_)
            | Kind::Rotary
            | Kind::Reaction
            | Kind::Nucleosynthesizer
            | Kind::ChemicalInfuser
            | Kind::Washer
            | Kind::Centrifuge,
        ) => all,
        Some(
            Kind::HeatGenerator | Kind::BioGenerator | Kind::GasGenerator | Kind::Wind | Kind::Solar(..) | Kind::DigitalMiner | Kind::Pump | Kind::Plenisher,
        ) => {
            TAB_UPGRADE | TAB_REDSTONE | TAB_SECURITY
        }
        Some(Kind::EnergyCube(_) | Kind::ChemicalTank(_)) => TAB_REDSTONE | TAB_SECURITY | TAB_SIDES,
        Some(Kind::Vibrator | Kind::Heater | Kind::FuelwoodHeater) => TAB_REDSTONE | TAB_SECURITY,
        Some(Kind::Oredictionificator) => TAB_REDSTONE | TAB_SECURITY | TAB_SIDES,
        Some(Kind::FluidTank(_)) => TAB_SECURITY,
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
    if kind(block) == Some(Kind::DigitalMiner) {
        out.push(Tab { at: [-26.0, 6.0], icon: "button/visuals", argb: 0xFFFF_FFFF, what: TabKind::Visuals });
    }
    if t & TAB_UPGRADE != 0 {
        out.push(Tab { at: [right, 6.0], icon: "button/upgrade", argb: 0xFFFF_FFFF, what: TabKind::Upgrade });
    }
    if t & TAB_SECURITY != 0 {
        out.push(Tab { at: [right, 34.0], icon: "security/public", argb: 0xFFFF_A160, what: TabKind::Security });
    }
    if t & TAB_REDSTONE != 0 {
        let icon = redstone_of(l, data).0.sprite();
        out.push(Tab { at: [right, 137.0], icon, argb: 0xFFC9_071F, what: TabKind::Redstone });
    }
    out
}

/// The part of a tab that takes clicks and shows its tooltip: the button inside the holder's border.
fn tab_button(tab: &Tab) -> [f32; 4] {
    [tab.at[0] + 4.0, tab.at[1] + 4.0, 18.0, 18.0]
}

/// A nine-slice sprite of `size` with a `border` px frame, tinted, drawn over `dst`.
fn nine_slice_tinted(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, name: &str, size: [f32; 2], border: f32, dst: [f32; 4], argb: u32) {
    let [x, y, w, h] = dst;
    let b = border;
    let cols = [(0.0, b, x, b), (b, size[0] - 2.0 * b, x + b, w - 2.0 * b), (size[0] - b, b, x + w - b, b)];
    let rows = [(0.0, b, y, b), (b, size[1] - 2.0 * b, y + b, h - 2.0 * b), (size[1] - b, b, y + h - b, b)];
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
fn draw_tabs(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, block: &str, l: &Layout, data: &[i32], origin: [f32; 2]) {
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
            let word = if data.get(redstone_slot(l) + 5 + 9).copied().unwrap_or(0) != 0 { "On" } else { "Off" };
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
        TabKind::Security => {
            lines.push("§7Security: Public".into());
            lines.push("No Owner".into());
        }
        TabKind::Redstone => lines.push(redstone_of(l, data).0.name().into()),
        TabKind::Sort => lines.push("Auto-Sort".into()),
        TabKind::Visuals => {
            let (shown, too_big) = (miner_visuals_on(data), miner_view(block, data).radius > 64);
            lines.push(format!("Visuals: {}", if shown { "On" } else { "Off" }));
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
        Some(Kind::Machine(..) | Kind::Factory(..) | Kind::EnergyCube(_)) => &[Transmission::Item, Transmission::Energy],
        Some(Kind::ChemicalTank(_)) => &[Transmission::Item, Transmission::Chemical],
        _ => &[Transmission::Item, Transmission::Chemical, Transmission::Energy],
    }
}

/// `TransmissionType`'s name, icon (`transmission/*`) and tab colour (`SpecialColors.TAB_*_CONFIG`).
fn transmission_look(t: Transmission) -> (&'static str, &'static str, u32) {
    match t {
        Transmission::Item => ("Items", "transmission/items", 0xFFCF_CFCF),
        Transmission::Chemical => ("Chemicals", "transmission/chemicals", 0xFFFF_DD4F),
        Transmission::Energy => ("Energy", "transmission/energy", 0xFF59_C15F),
    }
}

/// Whether a type can be ejected (`ConfigInfo.canEject`): a machine's energy only comes in.
fn can_eject(block: &str, t: Transmission) -> bool {
    t != Transmission::Energy || matches!(kind(block), Some(Kind::EnergyCube(_)))
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
/// auto-eject (260 + type), and the clear button (270 + 2 × type, + 1 for a right click).
const BUTTON_SIDE: i32 = 200;
const BUTTON_EJECT: i32 = 260;
const BUTTON_CLEAR: i32 = 270;

/// A rectangle of the window in the screen's coordinates.
fn in_window(r: [f32; 4]) -> [f32; 4] {
    [SIDE_WINDOW[0] + r[0], SIDE_WINDOW[1] + r[1], r[2], r[3]]
}

/// The window's type tabs (`GuiConfigTypeTab`): on its left, 28 px apart; the one it shows is hidden.
fn window_type_tabs(block: &str, shown: usize) -> Vec<(usize, [f32; 4])> {
    transmissions(block)
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != shown && *i < 4)
        .map(|(i, _)| (i, in_window([-26.0, 2.0 + 28.0 * i as f32, 26.0, 26.0])))
        .collect()
}

/// The faces of type index `t` in relative-side order and whether its auto-eject is off, from the
/// screen's data (after the redstone slot).
fn window_sides(l: &Layout, data: &[i32], t: Transmission) -> (u32, bool) {
    let at = redstone_slot(l) + 1;
    let packed = data.get(at + t as usize).copied().unwrap_or(0).max(0) as u32;
    let no_eject = data.get(at + 3).copied().unwrap_or(0).max(0) as u32 & (1 << t as u32) != 0;
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
fn draw_button(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, r: [f32; 4], hovered: bool, active: bool, argb: u32, icon: Option<(&str, f32)>) {
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
fn centred(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, text: &str, x: f32, width: f32, y: f32, rgb: u32) {
    let w = game.native_text_width(text);
    game.native_text(batch, text, [x + (width - w) / 2.0, y], rgb, false);
}

/// Draws the open window (hook point `native_screen_overlay`, API 32): the shadow and panel, the
/// type tabs, the close button, the title, the eject screen and button, the face buttons with the
/// block beside each face, the clear button and the "Slots" label.
pub(super) fn screen_overlay(game: &Game<'_>, block: &str, data: &[i32], batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    if miner_cfg_open(block) {
        draw_miner_windows(game, batch, origin);
        return;
    }
    if qio_select_open(block) {
        return;
    }
    let Some(id) = open_window_id(block) else {
        return;
    };
    follow_drag(game, id, origin);
    save_moves(game);
    let off = window_offset(id);
    let moved = [origin[0] + off[0], origin[1] + off[1]];
    draw_window(game, block, data, batch, moved);
    if let Some(r) = pin_button(id) {
        let [x, y, w, h] = [moved[0] + r[0], moved[1] + r[1], r[2], r[3]];
        let cursor = game.native_cursor();
        let over = inside([cursor[0] - moved[0], cursor[1] - moved[1]], r);
        let name = if window_pinned(id) { "button/pinned" } else { "button/unpinned" };
        let tint = if over { 0xFFFF_FFFF } else { 0xFFE0_E0E0 };
        game.native_sprite_tinted(batch, &format!("{SPRITES}{name}"), [0.0, 0.0, 24.0, 12.0], [x, y, w, h], tint);
    }
}

fn draw_window(game: &Game<'_>, block: &str, data: &[i32], batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    FILTER_OD.set(is_od(block));
    if od_edit_open() && is_od(block) {
        return draw_od_window(game, batch, origin);
    }
    if is_filter_screen(block) {
        return draw_filter_window(game, batch, origin);
    }
    let shown = match window(block) {
        Some(Window::Sides(shown)) => shown,
        Some(Window::Upgrades(selected)) => return draw_upgrade_window(game, block, data, batch, origin, selected),
        Some(Window::Transporter) => return draw_transporter_window(game, block, data, batch, origin),
        None => return,
    };
    let Some(l) = layout(block) else {
        return;
    };
    let types = transmissions(block);
    let shown = shown.min(types.len() - 1);
    let t = types[shown];
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    let at = [cursor[0] - ox, cursor[1] - oy];
    let screen = |r: [f32; 4]| {
        let [x, y, w, h] = in_window(r);
        [ox + x, oy + y, w, h]
    };
    let [x, y, w, h] = screen([0.0, 0.0, SIDE_WINDOW[2], SIDE_WINDOW[3]]);
    nine_slice_tinted(game, batch, "shadow", [9.0, 9.0], 3.0, [x - 3.0, y - 3.0, w + 6.0, h + 6.0], 0xBFFF_FFFF);
    nine_slice_tinted(game, batch, "base", [16.0, 16.0], 4.0, [x, y, w, h], 0xFFFF_FFFF);
    for (i, r) in window_type_tabs(block, shown) {
        let (_, icon, argb) = transmission_look(types[i]);
        let [tx, ty] = [ox + r[0], oy + r[1]];
        nine_slice_tinted(game, batch, "holder_left", [26.0, 9.0], 4.0, [tx, ty, 26.0, 26.0], argb);
        let hovered = inside(at, [r[0] + 4.0, r[1] + 4.0, 18.0, 18.0]);
        draw_button(game, batch, [tx + 5.0, ty + 4.0, 18.0, 18.0], hovered, true, 0xFFFF_FFFF, Some((icon, 18.0)));
    }
    let hover = |r: [f32; 4]| inside(at, in_window(r));
    draw_button(game, batch, screen(WINDOW_CLOSE), hover(WINDOW_CLOSE), true, 0xFFFF_FFFF, Some(("button/close", 12.0)));
    centred(game, batch, &format!("{} Config", transmission_look(t).0), x, w, y + 5.0, 0x40_4040);
    let (packed, no_eject) = window_sides(&l, data, t);
    let ejects = can_eject(block, t);
    let s = screen(WINDOW_SCREEN);
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, s, 0xFFFF_FFFF);
    let line = if !ejects {
        "Can't Eject".to_owned()
    } else {
        format!("Eject: {}", if no_eject { "Off" } else { "On" })
    };
    game.native_text(batch, &line, [s[0] + 3.0, s[1] + 3.0], 0x3C_FE9A, false);
    draw_button(game, batch, screen(WINDOW_EJECT), hover(WINDOW_EJECT), ejects, 0xFFFF_FFFF, Some(("button/auto_eject", 14.0)));
    draw_button(game, batch, screen(WINDOW_CLEAR), hover(WINDOW_CLEAR), true, 0xFFFF_FFFF, Some(("button/clear_sides", 14.0)));
    // The block beside each face shows on its button (`SideDataButton`).
    let pos = game.container.map(|(x, y, z)| [x, y, z]);
    let facing = pos.and_then(|p| game.native_block_property(p, "facing")).and_then(|f| direction(&f)).map_or(2, face_index);
    for (side, [bx, by]) in SIDE_BUTTONS.iter().enumerate() {
        let r = [*bx, *by, 22.0, 22.0];
        let (_, rgb, _) = Sides::relative_get(packed, side).data_type();
        draw_button(game, batch, screen(r), hover(r), true, button_tint(rgb), None);
        if let Some(p) = pos {
            let d = DIRECTIONS[side_config::relative_face(facing, side)].1;
            let beside = game.native_block([p[0] + d[0], p[1] + d[1], p[2] + d[2]]);
            if !beside.ends_with(":air") {
                let [sx, sy, ..] = screen(r);
                game.native_item(batch, &beside, [sx + 3.0, sy + 3.0], 16.0);
            }
        }
    }
    centred(game, batch, "Slots", x, w, y + 120.0, 0x78_7878);
}

/// Mekanism's creative tabs (hook point `native_creative_tabs`, API 34): the Mekanism, Generators, Additions and
/// Tools tabs, their items in the order of the Java registries (`tools/gen_creative_tabs.py`).
pub(super) fn creative_tabs(tabs: &mut Vec<crate::game::native_hooks::NativeCreativeTab>) {
    use crate::game::native_hooks::{NativeCreativeTab, NativeStack};
    for tab in mekanism_core::creative_tabs::CREATIVE_TABS {
        tabs.push(NativeCreativeTab {
            id: tab.id.to_owned(),
            title: tab.title.to_owned(),
            icon: tab.icon.to_owned(),
            items: tab
                .items
                .iter()
                .map(|item| NativeStack { item: (*item).to_owned(), count: 1, components: String::new() })
                .collect(),
        });
    }
}

/// What Mekanism's own parts cover on a screen (hook point `native_screen_covers`, API 32): its tabs
/// beside the panel, and the open window with its type tabs.
pub(super) fn screen_covers(block: &str, at: [f32; 2], covered: &mut bool) {
    if miner_cfg_open(block) {
        *covered |= miner_cfg_covers(at);
        return;
    }
    if qio_select_open(block) {
        return;
    }
    if is_qio_tile(block) {
        *covered |= inside(at, QIO_TAB);
    }
    screen_covers_base(block, at, covered);
}

fn screen_covers_base(block: &str, at: [f32; 2], covered: &mut bool) {
    let wat = window_at(block, at);
    if is_od(block) && od_edit_open() {
        *covered |= inside(wat, OD_WINDOW);
        return;
    }
    if is_filter_screen(block) {
        *covered |= filter_covers(wat);
        return;
    }
    let Some(l) = layout(block) else {
        return;
    };
    let tab = screen_tabs(block, &l, &[]).iter().any(|t| inside(at, [t.at[0], t.at[1], 26.0, 26.0]));
    let at = wat;
    let open = match window(block) {
        Some(Window::Sides(shown)) => inside(at, SIDE_WINDOW) || window_type_tabs(block, shown).iter().any(|(_, r)| inside(at, *r)),
        Some(Window::Upgrades(_)) => inside(at, UPGRADE_WINDOW),
        Some(Window::Transporter) => inside(at, TRANSPORTER_WINDOW),
        None => false,
    };
    *covered |= tab || open;
}


// --- Window dragging and pinning (GuiWindow) ---------------------------------------------------------------

/// The windows that move: the three of a machine's screen and the sorter's two.
const WINDOW_SIDES: usize = 0;
const WINDOW_UPGRADES: usize = 1;
const WINDOW_TRANSPORTER: usize = 2;
const WINDOW_FILTER: usize = 3;
const WINDOW_SELECT: usize = 4;

/// Where each window has been moved to (kept per type, as `SelectedWindowData.updateLastPosition`
/// does), which are pinned, and the window being dragged with where the cursor took hold.
#[derive(Default)]
struct WindowMoves {
    offsets: [[f32; 2]; 5],
    pinned: [bool; 5],
    drag: Option<(usize, [f32; 2])>,
}

thread_local! {
    /// The window places changed since they were saved.
    static MOVES_DIRTY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Where the windows were and which were pinned, in the mod's folder (Mekanism keeps them in its client
/// config): per window `x y pinned`, one line each.
const WINDOWS_FILE: &str = "windows";

fn load_moves(game: &Game<'_>) {
    let Some(text) = game.native_setting(MOD_ID, WINDOWS_FILE) else {
        return;
    };
    MOVES.with_borrow_mut(|m| {
        for (i, line) in text.lines().take(5).enumerate() {
            let v: Vec<&str> = line.split_whitespace().collect();
            if let [x, y, p] = v[..]
                && let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>())
            {
                m.offsets[i] = [x, y];
                m.pinned[i] = p == "1";
            }
        }
    });
}

fn save_moves(game: &Game<'_>) {
    if !MOVES_DIRTY.replace(false) {
        return;
    }
    let text = MOVES.with_borrow(|m| {
        (0..5).map(|i| format!("{} {} {}\n", m.offsets[i][0], m.offsets[i][1], u8::from(m.pinned[i]))).collect::<String>()
    });
    game.native_set_setting(MOD_ID, WINDOWS_FILE, &text);
}

thread_local! {
    static MOVES: std::cell::RefCell<WindowMoves> = std::cell::RefCell::new(WindowMoves::default());
}

/// The slots of `Layout` that are chemical slots (they take chemical tank items).
fn chemical_layout_slots(block: &str) -> &'static [usize] {
    match kind(block) {
        Some(Kind::Centrifuge) => &[1, 2],
        Some(Kind::ChemicalInfuser) => &[1, 2],
        Some(Kind::Washer) => &[1],
        _ => &[],
    }
}

/// The window open on `block`'s screen, as an index into `WindowMoves`.
/// What each container slot of a machine's menu takes (hook point `native_slot_rules`): the energy slot
/// only items that hold energy (`EnergyInventorySlot`), the output slots nothing (`OutputInventorySlot`).
pub(super) fn slot_rules(block: &str, rules: &mut Vec<horizon_client::lan::menu::SlotRule>) {
    use horizon_client::lan::menu::SlotRule;
    let energy = || SlotRule::Items(gear::energy_item_ids());
    let convert = || {
        let mut ids = gear::energy_item_ids();
        ids.extend(REDSTONE_ENERGY.iter().map(|(id, _)| (*id).to_owned()));
        SlotRule::Items(ids)
    };
    if let Some(Kind::Factory(n, ..)) = kind(block) {
        *rules = vec![SlotRule::Any; FACTORY_ENERGY + 1];
        for i in 0..factory_slots(n).len() {
            rules[FACTORY_ROW + i] = SlotRule::Refuse;
        }
        rules[FACTORY_ENERGY] = convert();
    } else if kind(block) == Some(Kind::Teleporter) {
        *rules = vec![convert(), SlotRule::Any, SlotRule::Any];
    } else if let Some(l) = layout(block) {
        *rules = l
            .slots
            .iter()
            .enumerate()
            .map(|(i, s)| match s.map(|(_, sprite)| sprite) {
                // `EnergyInventorySlot.fillOrConvert`: a consumer's slot also takes redstone, which it turns into energy.
                Some("slot/power") if !matches!(kind(block), Some(Kind::HeatGenerator | Kind::BioGenerator | Kind::GasGenerator)) && !(matches!(kind(block), Some(Kind::EnergyCube(_))) && i == 0) => convert(),
                Some("slot/power") => energy(),
                Some(sprite) if sprite.starts_with("slot/output") => SlotRule::Refuse,
                _ => SlotRule::Any,
            })
            .collect();
        for &i in chemical_layout_slots(block) {
            rules[i] = SlotRule::Items(chemical_item_ids());
        }
        let extra = extra_slots(block);
        if !extra.is_empty() {
            rules.resize(3, SlotRule::Any);
            let chemical = !matches!(kind(block), Some(Kind::Washer | Kind::DigitalMiner));
            rules.extend(extra.iter().map(|(_, sprite)| match *sprite {
                "slot/normal" if !chemical => SlotRule::Any,
                "slot/normal" => SlotRule::Items(chemical_item_ids()),
                "slot/output" => SlotRule::Refuse,
                _ => energy(),
            }));
        }
    }
}

/// The wheel on a screen (hook point `native_screen_wheel`): it scrolls the filter list of the sorter, the
/// QIO importer and exporter and the miner's config screen (`GuiFilterHolder.mouseScrolled`), a row a notch.
pub(super) fn screen_wheel(game: &Game<'_>, block: &str, steps: f32, taken: &mut bool) {
    let notch = |first: usize, count: usize| {
        let hidden = count.saturating_sub(SORTER_ROWS);
        if steps > 0.0 { first.min(hidden).saturating_sub(1) } else { (first + 1).min(hidden) }
    };
    if miner_cfg_open(block) {
        if let Some(c) = miner_cfg(game)
            && MINER_UI.with_borrow(|ui| !ui.select && ui.edit.is_none())
        {
            MINER_UI.with_borrow_mut(|ui| ui.scroll = notch(ui.scroll, c.filters.len()));
            *taken = true;
        }
    } else if is_od(block) {
        if !od_edit_open() && let Some((pos, filters)) = od_view(game) {
            let hidden = filters.len().saturating_sub(OD_ROWS);
            let first = od_scroll(pos).min(hidden);
            OD_SCROLL.set((pos, if steps > 0.0 { first.saturating_sub(1) } else { (first + 1).min(hidden) }));
            *taken = true;
        }
    } else if is_filter_screen(block)
        && FILTER_EDIT.with_borrow(|e| e.is_none())
        && FILTER_SELECT.get().is_none()
        && let Some((pos, view)) = sorter_view(game)
    {
        SORTER_SCROLL.set((pos, notch(sorter_scroll(pos), view.filters.len())));
        *taken = true;
    }
}

/// Escape on a screen (hook point `native_screen_escape`): it closes the open window and not the screen.
pub(super) fn screen_escape(block: &str, consumed: &mut bool) {
    if miner_cfg_open(block) {
        miner_cfg_escape(consumed);
        return;
    }
    if is_filter_screen(block) {
        if FILTER_EDIT.with_borrow(|e| e.is_some()) {
            close_filter_window();
            *consumed = true;
        } else if FILTER_SELECT.get().is_some() {
            FILTER_SELECT.set(None);
            *consumed = true;
        }
        return;
    }
    if is_od(block) && od_edit_open() {
        OD_EDIT.with_borrow_mut(|e| *e = None);
        *consumed = true;
        return;
    }
    if window(block).is_some() {
        set_window(block, None);
        *consumed = true;
    }
}

fn open_window_id(block: &str) -> Option<usize> {
    FILTER_OD.set(is_od(block));
    if is_od(block) && od_edit_open() {
        return Some(WINDOW_FILTER);
    }
    if is_filter_screen(block) {
        return if FILTER_SELECT.get().is_some() {
            Some(WINDOW_SELECT)
        } else if FILTER_EDIT.with_borrow(|e| e.is_some()) {
            Some(WINDOW_FILTER)
        } else {
            None
        };
    }
    Some(match window(block)? {
        Window::Sides(_) => WINDOW_SIDES,
        Window::Upgrades(_) => WINDOW_UPGRADES,
        Window::Transporter => WINDOW_TRANSPORTER,
    })
}

/// A window's rectangle where it opens.
fn window_home(id: usize) -> [f32; 4] {
    [SIDE_WINDOW, UPGRADE_WINDOW, TRANSPORTER_WINDOW, filter_window(), select_window()][id]
}

/// The pin button (`GuiPinButton`: 16 by 8, 16 in and 6 down) of the windows that can be pinned
/// (`WindowType.canPin`: side configuration, transporter configuration, upgrades).
fn pin_button(id: usize) -> Option<[f32; 4]> {
    (id <= WINDOW_TRANSPORTER).then(|| {
        let [x, y, ..] = window_home(id);
        [x + 16.0, y + 6.0, 16.0, 8.0]
    })
}

fn window_offset(id: usize) -> [f32; 2] {
    MOVES.with_borrow(|m| m.offsets[id])
}

fn window_pinned(id: usize) -> bool {
    MOVES.with_borrow(|m| m.pinned[id])
}

/// `at` in the open window's own frame: the screen's, less how far the window was moved.
fn window_at(block: &str, at: [f32; 2]) -> [f32; 2] {
    match open_window_id(block) {
        Some(id) => {
            let [dx, dy] = window_offset(id);
            [at[0] - dx, at[1] - dy]
        }
        None => at,
    }
}

/// Runs a press handler of the open window on `at` in the window's frame.
fn windowed(block: &str, at: [f32; 2], handler: impl FnOnce([f32; 2]) -> bool) -> bool {
    let Some(id) = open_window_id(block) else {
        return handler(at);
    };
    let wat = window_at(block, at);
    let [x, y, w, _] = window_home(id);
    // `GuiWindow.mouseClicked`: a press in the top 18 px picks the window up (the buttons there act first).
    let grip = inside(wat, [x, y, w, 18.0]) && !pin_button(id).is_some_and(|r| inside(wat, r));
    let handled = handler(wat);
    if handled && grip && open_window_id(block) == Some(id) && !window_close_hit(id, wat) {
        MOVES.with_borrow_mut(|m| m.drag = Some((id, [at[0] - m.offsets[id][0], at[1] - m.offsets[id][1]])));
    }
    handled
}

/// Whether a press at `wat` is on the close button, which gives no grip.
fn window_close_hit(id: usize, wat: [f32; 2]) -> bool {
    let [x, y, ..] = window_home(id);
    inside(wat, [x + WINDOW_CLOSE[0], y + WINDOW_CLOSE[1], WINDOW_CLOSE[2], WINDOW_CLOSE[3]])
}

/// The window follows the cursor while the left button stays down (`GuiWindow.onDrag`), and stays
/// inside the screen's top left.
fn follow_drag(game: &Game<'_>, id: usize, origin: [f32; 2]) {
    MOVES.with_borrow_mut(|m| {
        let Some((dragged, grab)) = m.drag else {
            return;
        };
        if dragged != id || !game.native_mouse_down() {
            m.drag = None;
            MOVES_DIRTY.set(true);
            return;
        }
        let cursor = game.native_cursor();
        let home = window_home(id);
        let off = [(cursor[0] - grab[0]).max(-(origin[0] + home[0])), (cursor[1] - grab[1]).max(-(origin[1] + home[1]))];
        m.offsets[id] = off;
    });
}

/// A press in the window's pin button (true when it was): toggles it. Handled apart from the
/// window's own buttons since it is not one of theirs.
fn pin_click(block: &str, at: [f32; 2]) -> bool {
    let Some(id) = open_window_id(block) else {
        return false;
    };
    let Some(r) = pin_button(id) else {
        return false;
    };
    if !inside(window_at(block, at), r) {
        return false;
    }
    MOVES.with_borrow_mut(|m| m.pinned[id] = !m.pinned[id]);
    MOVES_DIRTY.set(true);
    true
}

/// A press on a machine's window, with the pin button and the drag grip.
fn windowed_click(block: &str, data: &[i32], at: [f32; 2], right: bool, button: &mut Option<i32>) -> bool {
    if pin_click(block, at) {
        return true;
    }
    windowed(block, at, |wat| window_click(block, data, wat, right, button))
}

/// A press on the open window: true when it was on the window (`button` set for the server's part).
fn window_click(block: &str, data: &[i32], at: [f32; 2], right: bool, button: &mut Option<i32>) -> bool {
    let shown = match window(block) {
        Some(Window::Sides(shown)) => shown,
        Some(Window::Upgrades(selected)) => return upgrade_window_click(block, data, at, selected, button),
        Some(Window::Transporter) => return transporter_window_click(block, data, at, right, button),
        None => return false,
    };
    let types = transmissions(block);
    let shown = shown.min(types.len() - 1);
    if let Some((i, _)) = window_type_tabs(block, shown).into_iter().find(|(_, r)| inside(at, *r)) {
        set_window(block, Some(Window::Sides(i)));
        return true;
    }
    if !inside(at, SIDE_WINDOW) {
        return false;
    }
    let t = types[shown] as i32;
    let on = |r: [f32; 4]| inside(at, in_window(r));
    if on(WINDOW_CLOSE) {
        set_window(block, None);
    } else if on(WINDOW_EJECT) && can_eject(block, types[shown]) {
        *button = Some(BUTTON_EJECT + t);
    } else if on(WINDOW_CLEAR) {
        *button = Some(BUTTON_CLEAR + 2 * t + i32::from(right));
    } else if let Some(side) = SIDE_BUTTONS.iter().position(|[x, y]| on([*x, *y, 22.0, 22.0])) {
        *button = Some(BUTTON_SIDE + 16 * t + 2 * side as i32 + i32::from(right));
    }
    true
}

/// The open window's tooltip: true when the cursor is on the window.
fn window_tooltip(block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) -> bool {
    let shown = match window(block) {
        Some(Window::Sides(shown)) => shown,
        Some(Window::Upgrades(_)) => return upgrade_window_tooltip(block, data, at, lines),
        Some(Window::Transporter) => return transporter_window_tooltip(block, data, at, lines),
        None => return false,
    };
    let Some(l) = layout(block) else {
        return false;
    };
    let types = transmissions(block);
    let shown = shown.min(types.len() - 1);
    if let Some((i, _)) = window_type_tabs(block, shown).into_iter().find(|(_, r)| inside(at, *r)) {
        lines.push(transmission_look(types[i]).0.into());
        return true;
    }
    if !inside(at, SIDE_WINDOW) {
        return false;
    }
    let t = types[shown];
    let (packed, _) = window_sides(&l, data, t);
    let on = |r: [f32; 4]| inside(at, in_window(r));
    if on(WINDOW_CLOSE) {
        lines.push("Close".into());
    } else if on(WINDOW_EJECT) {
        lines.push("Auto-eject".into());
    } else if on(WINDOW_CLEAR) {
        if clear_target(packed, true) == SideMode::Off {
            lines.push("Clear Side Config (sets all sides to none)".into());
        } else {
            lines.push("Increment Side Config (changes all sides to the next, or previous, configuration type)".into());
        }
    } else if on(WINDOW_SCREEN) && !can_eject(block, t) {
        lines.push("Auto-eject is not supported, manual extraction may still be possible.".into());
    } else if let Some(side) = SIDE_BUTTONS.iter().position(|[x, y]| on([*x, *y, 22.0, 22.0])) {
        let (name, _, code) = Sides::relative_get(packed, side).data_type();
        lines.push(side_config::RELATIVE[side].into());
        lines.push(format!("§{code}{name}"));
    }
    true
}

/// A press on the window, on the server (hook point `native_menu_button`): a face's next or
/// previous mode (`PacketSideData`), a type's auto-eject (`PacketEjectConfiguration`), or every face
/// of a type at once (`PacketBatchConfiguration`). False for any other button.
fn window_button(game: &Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, id: i32) -> bool {
    if tabs(&entity.id) & TAB_SIDES == 0 || !(BUTTON_SIDE..BUTTON_CLEAR + 6).contains(&id) {
        return false;
    }
    let facing = game.native_block_property(pos, "facing").and_then(|f| direction(&f)).map_or(2, face_index);
    let (t, rest) = match id {
        BUTTON_EJECT.. if id < BUTTON_CLEAR => {
            let bit = 1 << (id - BUTTON_EJECT);
            let flags = get_int(&entity.nbt, "no_eject") ^ bit;
            put_int(&mut entity.nbt, "no_eject", flags);
            return true;
        }
        BUTTON_CLEAR.. => ((id - BUTTON_CLEAR) / 2, None),
        _ => ((id - BUTTON_SIDE) / 16, Some(((id - BUTTON_SIDE) % 16 / 2) as usize)),
    };
    let Some(&t) = Transmission::ALL.get(t as usize) else {
        return true;
    };
    let right = id % 2 == 1;
    let mut sides = sides_of(&entity.nbt, t);
    match rest {
        Some(side) if side < 6 => {
            let face = side_config::relative_face(facing, side);
            let now = sides.get(face);
            sides.set(face, if right { now.window_previous() } else { now.window_next() });
        }
        Some(_) => return true,
        None => {
            let target = clear_target(sides.relative(facing), !right);
            for face in 0..6 {
                sides.set(face, target);
            }
        }
    }
    put_long(&mut entity.nbt, t.key(), u64::from(sides.0));
    true
}

// --- The upgrade window -----------------------------------------------------------------------------

/// Mekanism's upgrades in their registry's order (`MekanismDatapackRegistries`): id, name, colour
/// (`EnumColor`), description, and the port's upgrade when it installs that one.
const UPGRADE_TYPES: [(&str, &str, u32, &str, Option<Upgrade>); 7] = [
    ("mekanism:anchor", "Anchor", 0x59C15F, "Keeps a machine's chunk loaded.", None),
    ("mekanism:chemical", "Chemical", 0xFFDD4F, "Increases the efficiency of chemical-using machinery.", None),
    ("mekanism:energy", "Energy", 0x75FF89, "Increases energy efficiency and capacity of machinery.", Some(Upgrade::Energy)),
    ("mekanism:filter", "Filter", 0x00F3D0, "A filter that separates heavy water from regular water.", Some(Upgrade::Filter)),
    ("mekanism:muffling", "Muffling", 0x559EFF, "Reduces noise generated by machinery.", None),
    ("mekanism:speed", "Speed", 0xFF383C, "Increases speed of machinery.", Some(Upgrade::Speed)),
    ("mekanism:stone_generator", "Stone Generator", 0xFFA160, "Generates stone or cobblestone as needed.", None),
];

/// The upgrades the port installs in a block (`install_upgrade`).
fn supported_upgrades(block: &str) -> &'static [Upgrade] {
    match kind(block) {
        Some(Kind::Machine(..) | Kind::Factory(..) | Kind::DigitalMiner | Kind::Plenisher) => &[Upgrade::Speed, Upgrade::Energy],
        Some(Kind::Pump) => &[Upgrade::Speed, Upgrade::Energy, Upgrade::Filter],
        _ => &[],
    }
}

/// The installed upgrades' registry indices, from the screen's data (speed and energy counts).
fn installed_upgrades(data: &[i32]) -> Vec<(usize, u32)> {
    let count = |u: Upgrade| match u {
        Upgrade::Speed => data.get(DATA_SPEED).copied().unwrap_or(0).max(0) as u32,
        Upgrade::Energy => data.get(DATA_ENERGY_UPGRADES).copied().unwrap_or(0).max(0) as u32,
        Upgrade::Filter => 0,
    };
    UPGRADE_TYPES
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.4.map(|u| (i, count(u))))
        .filter(|(_, n)| *n > 0)
        .collect()
}

thread_local! {
    static UPGRADE_STACKS: std::cell::RefCell<std::collections::HashMap<String, Option<ItemStack>>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// An upgrade's item, as its crafting recipe makes it (its name and the model its
/// `custom_model_data` picks).
fn upgrade_stack(game: &Game<'_>, id: &str) -> Option<ItemStack> {
    if let Some(found) = UPGRADE_STACKS.with_borrow(|c| c.get(id).cloned()) {
        return found;
    }
    let found = world_recipes!(game).crafting.iter().find_map(|(_, r)| {
        let result = match r {
            horizon_client::crafting::Recipe::Shaped { result, .. } | horizon_client::crafting::Recipe::Shapeless { result, .. } => result,
            _ => return None,
        };
        let stack = ItemStack { item: result.item.clone()?, count: 1, damage: result.damage, extra: result.extra.clone() };
        (upgrade_type_of(&stack).as_deref() == Some(id)).then_some(stack)
    });
    UPGRADE_STACKS.with_borrow_mut(|c| c.insert(id.to_owned(), found.clone()));
    found
}

/// `GuiUpgradeWindow`: 198 wide, centred, 15 down; one row of supported upgrades makes it 94 tall.
const UPGRADE_WINDOW: [f32; 4] = [-11.0, 15.0, 198.0, 94.0];
const UPGRADE_LIST: [f32; 4] = [6.0, 18.0, 108.0, 50.0];
const UPGRADE_SCREEN: [f32; 4] = [114.0, 18.0, 59.0, 50.0];
const UPGRADE_REMOVE: [f32; 4] = [115.0, 54.0, 56.0, 12.0];
const UPGRADE_SUPPORTED: [f32; 4] = [6.0, 68.0, 167.0, 14.0];
/// The menu buttons: uninstall one upgrade (290 + its registry index).
const BUTTON_UNINSTALL: i32 = 290;

fn in_upgrades(r: [f32; 4]) -> [f32; 4] {
    [UPGRADE_WINDOW[0] + r[0], UPGRADE_WINDOW[1] + r[1], r[2], r[3]]
}

/// Where "Supported:"'s icons start (`GuiSupportedUpgrades.getFirstRowStart`): the label's width in
/// the default font (54 px) and one more.
const SUPPORTED_START: f32 = 55.0;

/// Words wrapped to `width` at `scale` (`WrappedTextRenderer`).
fn wrap(game: &Game<'_>, text: &str, width: f32, scale: f32) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for word in text.split(' ') {
        match lines.last_mut() {
            Some(last) if game.native_text_width(&format!("{last} {word}")) * scale <= width => {
                last.push(' ');
                last.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

/// The upgrade's effect line (`IUpgradeTile.getUpgradeWindowInfo`): the max multiplier to the
/// power of installed / max, to two places.
fn upgrade_effect(u: Upgrade, installed: u32) -> Option<String> {
    (u.max() > 1).then(|| {
        let effect = mekanism_core::upgrades::MAX_UPGRADE_MULTIPLIER.powf(f64::from(installed) / f64::from(u.max()));
        format!("Effect: {}x", (effect * 100.0).round() / 100.0)
    })
}

/// Draws the upgrade window: the panel, the close button and title, the list of installed upgrades
/// in their colours, the screen with the selected one's count and effect, the uninstall button and
/// progress arrows, the upgrade slots, and the supported upgrades (the rest faded).
fn draw_upgrade_window(game: &Game<'_>, block: &str, data: &[i32], batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2], selected: Option<usize>) {
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    let at = [cursor[0] - ox, cursor[1] - oy];
    let screen = |r: [f32; 4]| {
        let [x, y, w, h] = in_upgrades(r);
        [ox + x, oy + y, w, h]
    };
    let hover = |r: [f32; 4]| inside(at, in_upgrades(r));
    let [x, y, w, h] = screen([0.0, 0.0, UPGRADE_WINDOW[2], UPGRADE_WINDOW[3]]);
    nine_slice_tinted(game, batch, "shadow", [9.0, 9.0], 3.0, [x - 3.0, y - 3.0, w + 6.0, h + 6.0], 0xBFFF_FFFF);
    nine_slice_tinted(game, batch, "base", [16.0, 16.0], 4.0, [x, y, w, h], 0xFFFF_FFFF);
    draw_button(game, batch, screen(WINDOW_CLOSE), hover(WINDOW_CLOSE), true, 0xFFFF_FFFF, Some(("button/close", 12.0)));
    centred(game, batch, "Upgrades", x, w, y + 5.0, 0x40_4040);
    // The list (`GuiUpgradeScrollList`): its holder, its scroll bar, a row a type.
    let list = screen(UPGRADE_LIST);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, list, 0xFFFF_FFFF);
    nine_slice_tinted(game, batch, "scroll/list_border", [6.0, 6.0], 2.0, [list[0] + 101.0, list[1] + 1.0, 6.0, 48.0], 0xFFFF_FFFF);
    sprite(game, batch, "scroll/list_bar", [0.0, 0.0, 4.0, 4.0], [list[0] + 102.0, list[1] + 2.0, 4.0, 4.0]);
    let installed = installed_upgrades(data);
    for (row, (i, _)) in installed.iter().enumerate().take(4) {
        let (id, name, rgb, _, _) = UPGRADE_TYPES[*i];
        let ry = list[1] + 1.0 + 12.0 * row as f32;
        let rect = [UPGRADE_LIST[0] + 1.0, UPGRADE_LIST[1] + 1.0 + 12.0 * row as f32, 100.0, 12.0];
        let (base, slot) = if selected == Some(*i) {
            ("list/selected", "list/slot_selected")
        } else if hover(rect) {
            ("list/hovered", "list/slot_hovered")
        } else {
            ("list/base", "list/slot")
        };
        nine_slice_tinted(game, batch, base, [12.0, 12.0], 1.0, [list[0] + 1.0, ry, 100.0, 12.0], 0xFF00_0000 | rgb);
        game.native_sprite_tinted(batch, &format!("{SPRITES}{slot}"), [0.0, 0.0, 10.0, 10.0], [list[0] + 2.0, ry + 1.0, 10.0, 10.0], 0xFF00_0000 | rgb);
        if let Some(stack) = upgrade_stack(game, id) {
            game.native_item_stack(batch, &stack, [list[0] + 3.0, ry + 2.0], 8.0);
        }
        game.native_text(batch, name, [list[0] + 13.0, ry + 2.0], 0x40_4040, false);
    }
    // The screen beside it: the selected upgrade, its count and effect (0.6 scale).
    let s = screen(UPGRADE_SCREEN);
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, s, 0xFFFF_FFFF);
    let picked = selected.and_then(|i| installed.iter().find(|(j, _)| *j == i));
    match picked {
        Some((i, n)) => {
            let (_, name, _, _, upgrade) = UPGRADE_TYPES[*i];
            let lines = wrap(game, &format!("{name} Upgrade"), 55.0, 0.6);
            for (k, line) in lines.iter().enumerate() {
                game.native_text_scaled(batch, line, [s[0] + 2.0, s[1] + 2.0 + 6.0 * k as f32], 0x3C_FE9A, 0.6);
            }
            let mut ty = s[1] + 4.0 + 6.0 * lines.len() as f32;
            let max = upgrade.map_or(1, Upgrade::max);
            game.native_text_scaled(batch, &format!("Amount: {n}/{max}"), [s[0] + 2.0, ty], 0x3C_FE9A, 0.6);
            if let Some(effect) = upgrade.and_then(|u| upgrade_effect(u, *n)) {
                ty += 6.0;
                game.native_text_scaled(batch, &effect, [s[0] + 2.0, ty], 0x3C_FE9A, 0.6);
            }
        }
        None => {
            for (k, line) in wrap(game, "No selection.", 56.0, 0.8).iter().enumerate() {
                game.native_text_scaled(batch, line, [s[0] + 2.0, s[1] + 2.0 + 8.0 * k as f32], 0x3C_FE9A, 0.8);
            }
        }
    }
    // Uninstall (`DigitalButton`), and the installing and uninstalling arrows.
    let r = screen(UPGRADE_REMOVE);
    let active = picked.is_some();
    let base = match (active, hover(UPGRADE_REMOVE)) {
        (false, _) => "button_digital_inactive",
        (true, true) => "button_digital_focus",
        (true, false) => "button_digital",
    };
    nine_slice_tinted(game, batch, base, [200.0, 20.0], 2.0, r, 0xFFFF_FFFF);
    centred(game, batch, "Uninstall", r[0], r[2], r[1] + 2.0, if active { 0x3C_FE9A } else { 0x78_7878 });
    let [px, py, ..] = screen([176.0, 37.0, 0.0, 0.0]);
    sprite(game, batch, "progress/installing/empty", [0.0, 0.0, 10.0, 14.0], [px, py, 10.0, 14.0]);
    if let Some((_, _, p)) = upgrade_in_view(game) {
        let w = (10.0 * p.clamp(0.0, 1.0)).floor();
        if w > 0.0 {
            sprite(game, batch, "progress/installing/full", [0.0, 0.0, w, 14.0], [px, py, w, 14.0]);
        }
    }
    sprite(game, batch, "progress/uninstalling/empty", [0.0, 0.0, 12.0, 12.0], [px, py + 21.0, 12.0, 12.0]);
    for (sy, output) in [(18.0, false), (72.0, true)] {
        let [sx, sy, ..] = screen([175.0, sy, 0.0, 0.0]);
        sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [sx, sy, 18.0, 18.0]);
        let shown = if output { upgrade_out_view(game) } else { upgrade_in_view(game).map(|(id, n, _)| (id, n)) };
        if let Some((id, n)) = shown
            && let Some(stack) = upgrade_stack(game, &id)
        {
            game.native_item_stack(batch, &ItemStack { count: n, ..stack }, [sx + 1.0, sy + 1.0], 16.0);
            if n > 1 {
                let label = n.to_string();
                game.native_text(batch, &label, [sx + 17.0 - game.native_text_width(&label), sy + 9.0], 0xFF_FFFF, true);
            }
        }
    }
    // The supported upgrades (`GuiSupportedUpgrades`), the others faded.
    let sup = screen(UPGRADE_SUPPORTED);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, sup, 0xFFFF_FFFF);
    game.native_text(batch, "Supported:", [sup[0] + 2.0, sup[1] + 3.0], 0x40_4040, false);
    let start = SUPPORTED_START;
    let supported = supported_upgrades(block);
    for (i, (id, _, _, _, upgrade)) in UPGRADE_TYPES.iter().enumerate() {
        let ix = sup[0] + 1.0 + start + 12.0 * i as f32;
        if let Some(stack) = upgrade_stack(game, id) {
            game.native_item_stack(batch, &stack, [ix, sup[1] + 1.0], 12.0);
        }
        if !upgrade.is_some_and(|u| supported.contains(&u)) {
            game.native_fill(batch, [ix, sup[1] + 1.0, 12.0, 12.0], 0x80_8B8B8B);
        }
    }
}

/// A press on the upgrade window: a row selects its upgrade, Uninstall sends the selected one's
/// button, the close button closes it.
fn upgrade_window_click(block: &str, data: &[i32], at: [f32; 2], selected: Option<usize>, button: &mut Option<i32>) -> bool {
    if !inside(at, UPGRADE_WINDOW) {
        return false;
    }
    let on = |r: [f32; 4]| inside(at, in_upgrades(r));
    let installed = installed_upgrades(data);
    if on(WINDOW_CLOSE) {
        set_window(block, None);
    } else if let Some((i, _)) = installed.iter().take(4).enumerate().find(|(row, _)| on([7.0, 19.0 + 12.0 * *row as f32, 100.0, 12.0])).map(|(_, t)| *t) {
        set_window(block, Some(Window::Upgrades(Some(i))));
    } else if on(UPGRADE_REMOVE)
        && let Some(i) = selected.filter(|i| installed.iter().any(|(j, _)| j == i))
    {
        *button = Some(BUTTON_UNINSTALL + i as i32);
    }
    true
}

fn upgrade_window_tooltip(block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) -> bool {
    if !inside(at, UPGRADE_WINDOW) {
        return false;
    }
    let on = |r: [f32; 4]| inside(at, in_upgrades(r));
    let installed = installed_upgrades(data);
    if on(WINDOW_CLOSE) {
        lines.push("Close".into());
    } else if let Some((i, _)) = installed.iter().take(4).enumerate().find(|(row, _)| on([7.0, 19.0 + 12.0 * *row as f32, 100.0, 12.0])).map(|(_, t)| *t) {
        lines.push(UPGRADE_TYPES[i].3.into());
    } else if on(UPGRADE_REMOVE) {
        lines.push("Uninstalls a single upgrade, hold shift to uninstall all.".into());
    } else if on(UPGRADE_SUPPORTED) {
        // Each icon's name (`GuiSupportedUpgrades.updateTooltip`).
        let x = at[0] - in_upgrades(UPGRADE_SUPPORTED)[0] - 1.0 - SUPPORTED_START;
        if x >= 0.0
            && let Some((_, name, _, _, upgrade)) = UPGRADE_TYPES.get((x / 12.0) as usize)
        {
            if upgrade.is_some_and(|u| supported_upgrades(block).contains(&u)) {
                lines.push((*name).into());
            } else {
                lines.push(format!("{name} (Not Supported)"));
            }
        }
    }
    true
}

/// Uninstall pressed, on the server (`PacketRemoveUpgrade`): one of the upgrade comes out of the
/// machine and drops beside it (Mekanism puts it in the window's output slot).
fn upgrade_button(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, id: i32) -> bool {
    let Some(&(type_id, _, _, _, Some(upgrade))) = usize::try_from(id - BUTTON_UNINSTALL).ok().and_then(|i| UPGRADE_TYPES.get(i)) else {
        return false;
    };
    if tabs(&entity.id) & TAB_UPGRADE == 0 {
        return false;
    }
    let mut u = upgrades(&entity.nbt);
    let field = match upgrade {
        Upgrade::Speed => &mut u.speed,
        Upgrade::Energy => &mut u.energy,
        Upgrade::Filter => &mut u.filter,
    };
    if *field == 0 {
        return true;
    }
    // It goes to the window's output slot, which must take it (`getUpgradeOutputSlot`).
    let out = upgrade_out_of(&entity.nbt);
    let max = upgrade_stack(game, type_id).map_or(64, |s| max_stack(game, &s.item));
    let fits = match &out {
        None => true,
        Some((id, n)) => id == type_id && *n < max,
    };
    if !fits {
        return true;
    }
    *field -= 1;
    set_upgrades(&mut entity.nbt, &u);
    set_upgrade_out(&mut entity.nbt, Some((type_id.to_owned(), out.map_or(0, |(_, n)| n) + 1)));
    let _ = pos;
    true
}

/// The uninstalled upgrades waiting in the window's output slot: the type's id and how many.
fn upgrade_out_of(nbt: &Compound) -> Option<(String, u32)> {
    let Some(Tag::Compound(c)) = nbt.get(&JavaString::from("upgrade_out")) else {
        return None;
    };
    let id = get_string(c, "id")?;
    let n = get_int(c, "n");
    (n > 0).then_some((id, n))
}

fn set_upgrade_out(nbt: &mut Compound, out: Option<(String, u32)>) {
    match out {
        Some((id, n)) if n > 0 => {
            let mut c = Compound::new();
            c.insert("id".into(), Tag::String(id.as_str().into()));
            put_int(&mut c, "n", n);
            nbt.insert("upgrade_out".into(), Tag::Compound(c));
        }
        _ => {
            nbt.remove(&JavaString::from("upgrade_out"));
        }
    }
}

/// The window's virtual slots on the server (`getUpgradeSlot`, `getUpgradeOutputSlot`): the upgrade on the
/// cursor goes in (one on a right click), and the output takes out (half on a right click).
fn upgrade_slot_button(game: &mut Game<'_>, entity: &mut ModBlockEntity, id: i32) -> bool {
    let (base, right) = match id {
        BUTTON_UPGRADE_INSTALL | BUTTON_UPGRADE_INSTALL_ONE => (BUTTON_UPGRADE_INSTALL, id == BUTTON_UPGRADE_INSTALL_ONE),
        BUTTON_UPGRADE_TAKE | BUTTON_UPGRADE_TAKE_HALF => (BUTTON_UPGRADE_TAKE, id == BUTTON_UPGRADE_TAKE_HALF),
        BUTTON_UPGRADE_UNSLOT | BUTTON_UPGRADE_UNSLOT_HALF => (BUTTON_UPGRADE_UNSLOT, id == BUTTON_UPGRADE_UNSLOT_HALF),
        _ => return false,
    };
    if tabs(&entity.id) & TAB_UPGRADE == 0 {
        return true;
    }
    if base == BUTTON_UPGRADE_INSTALL {
        let Some(held) = game.carried.clone() else {
            return true;
        };
        let Some(_) = upgrade_of(&held).filter(|k| supported_upgrades(&entity.id).contains(k)) else {
            return true;
        };
        // The upgrade goes into the window's input slot (`getUpgradeSlot`); the machine installs it
        // after a second (`TileComponentUpgrade.tickServer`).
        let id = held.item.to_string();
        let slot = upgrade_in_of(&entity.nbt);
        if slot.as_ref().is_some_and(|(sid, _)| *sid != id) {
            return true;
        }
        let have = slot.map_or(0, |(_, n)| n);
        let n = (if right { 1 } else { held.count }).min(max_stack(game, &held.item).saturating_sub(have));
        if n > 0 {
            set_upgrade_in(&mut entity.nbt, Some((id, have + n)));
            put_int(&mut entity.nbt, "upgrade_check", 1);
            game.carried = (held.count > n).then(|| ItemStack { count: held.count - n, ..held });
        }
        return true;
    }
    if base == BUTTON_UPGRADE_UNSLOT {
        if game.carried.is_some() {
            return true;
        }
        let Some((type_id, n)) = upgrade_in_of(&entity.nbt) else {
            return true;
        };
        let Some(stack) = upgrade_stack(game, &type_id) else {
            return true;
        };
        let take = if right { n.div_ceil(2) } else { n };
        game.carried = Some(ItemStack { count: take, ..stack });
        set_upgrade_in(&mut entity.nbt, Some((type_id, n - take)));
        put_int(&mut entity.nbt, "upgrade_check", 1);
        return true;
    }
    if game.carried.is_some() {
        return true;
    }
    let Some((type_id, n)) = upgrade_out_of(&entity.nbt) else {
        return true;
    };
    let Some(stack) = upgrade_stack(game, &type_id) else {
        return true;
    };
    let take = if right { n.div_ceil(2) } else { n };
    game.carried = Some(ItemStack { count: take, ..stack });
    set_upgrade_out(&mut entity.nbt, Some((type_id, n - take)));
    true
}

/// The upgrades waiting in the window's input slot.
fn upgrade_in_of(nbt: &Compound) -> Option<(String, u32)> {
    let Some(Tag::Compound(c)) = nbt.get(&JavaString::from("upgrade_in")) else {
        return None;
    };
    let id = get_string(c, "id")?;
    let n = get_int(c, "n");
    (n > 0).then_some((id, n))
}

fn set_upgrade_in(nbt: &mut Compound, slot: Option<(String, u32)>) {
    match slot {
        Some((id, n)) if n > 0 => {
            let mut c = Compound::new();
            c.insert("id".into(), Tag::String(id.as_str().into()));
            put_int(&mut c, "n", n);
            nbt.insert("upgrade_in".into(), Tag::Compound(c));
        }
        _ => {
            nbt.remove(&JavaString::from("upgrade_in"));
        }
    }
}

/// `TileComponentUpgrade.tickServer`: while the input slot changed and holds a supported upgrade that is
/// not at its maximum, count a second, then install as many as fit.
fn upgrade_tick(game: &Game<'_>, entity: &mut ModBlockEntity) {
    if get_int(&entity.nbt, "upgrade_check") == 0 {
        return;
    }
    let mut ticks = get_int(&entity.nbt, "upgrade_ticks");
    if let Some((id, n)) = upgrade_in_of(&entity.nbt)
        && let Some(kind) = upgrade_stack(game, &id).and_then(|s| upgrade_of(&s)).filter(|k| supported_upgrades(&entity.id).contains(k))
    {
        let mut u = upgrades(&entity.nbt);
        let mut probe = u.clone();
        if probe.install(kind, 1) > 0 {
            if ticks < UPGRADE_TICKS {
                put_int(&mut entity.nbt, "upgrade_ticks", ticks + 1);
                return;
            }
            let added = u.install(kind, n);
            if added > 0 {
                set_upgrades(&mut entity.nbt, &u);
                set_upgrade_in(&mut entity.nbt, Some((id, n - added)));
            }
        }
    }
    ticks = 0;
    put_int(&mut entity.nbt, "upgrade_ticks", ticks);
    put_int(&mut entity.nbt, "upgrade_check", 0);
}

/// What the upgrade window shows beyond the screen's data: the slots and the installing progress.
fn upgrade_sync(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if tabs(&entity.id) & TAB_UPGRADE == 0 {
        return;
    }
    upgrade_tick(game, entity);
    let mut client = Compound::new();
    if let Some((id, n)) = upgrade_in_of(&entity.nbt) {
        client.insert("upg_in_id".into(), Tag::String(id.as_str().into()));
        put_int(&mut client, "upg_in_n", n);
        put_int(&mut client, "upg_ticks", get_int(&entity.nbt, "upgrade_ticks"));
    }
    if let Some((id, n)) = upgrade_out_of(&entity.nbt) {
        client.insert("upg_id".into(), Tag::String(id.as_str().into()));
        put_int(&mut client, "upg_n", n);
    }
    put_client(game, pos, entity, &["upg_id", "upg_n", "upg_in_id", "upg_in_n", "upg_ticks"], client);
}

const BUTTON_UPGRADE_INSTALL: i32 = 280;
const BUTTON_UPGRADE_INSTALL_ONE: i32 = 281;
const BUTTON_UPGRADE_TAKE: i32 = 282;
const BUTTON_UPGRADE_TAKE_HALF: i32 = 283;
const BUTTON_UPGRADE_UNSLOT: i32 = 284;
const BUTTON_UPGRADE_UNSLOT_HALF: i32 = 285;
/// `TileComponentUpgrade.UPGRADE_TICKS_REQUIRED`: a second.
const UPGRADE_TICKS: u32 = 20;
const UPGRADE_IN_SLOT: [f32; 4] = [175.0, 18.0, 18.0, 18.0];
const UPGRADE_OUT_SLOT: [f32; 4] = [175.0, 72.0, 18.0, 18.0];

/// The output slot as the screen has it, from the machine's `client` compound.
fn upgrade_out_view(game: &Game<'_>) -> Option<(String, u32)> {
    let (x, y, z) = game.container?;
    let (_, c) = game.native_client_block_entity([x, y, z])?;
    let id = get_string(c, "upg_id")?;
    let n = get_int(c, "upg_n");
    (n > 0).then_some((id, n))
}

/// The input slot and the installing progress (0 to 1) as the screen has them.
fn upgrade_in_view(game: &Game<'_>) -> Option<(String, u32, f32)> {
    let (x, y, z) = game.container?;
    let (_, c) = game.native_client_block_entity([x, y, z])?;
    let id = get_string(c, "upg_in_id")?;
    let n = get_int(c, "upg_in_n");
    (n > 0).then(|| (id, n, get_int(c, "upg_ticks") as f32 / UPGRADE_TICKS as f32))
}

/// A press on the window's virtual slots: a click with an upgrade on the cursor installs it, an empty
/// cursor takes from the output.
fn upgrade_slot_click(game: &Game<'_>, block: &str, at: [f32; 2], right: bool, button: &mut Option<i32>) -> bool {
    if !matches!(window(block), Some(Window::Upgrades(_))) {
        return false;
    }
    let wat = window_at(block, at);
    let on = |r: [f32; 4]| inside(wat, in_upgrades(r));
    if on(UPGRADE_IN_SLOT) {
        if game.carried.as_ref().is_some_and(|c| upgrade_of(c).is_some()) {
            *button = Some(BUTTON_UPGRADE_INSTALL + i32::from(right));
        } else if game.carried.is_none() && upgrade_in_view(game).is_some() {
            *button = Some(BUTTON_UPGRADE_UNSLOT + i32::from(right));
        }
        return true;
    }
    if on(UPGRADE_OUT_SLOT) {
        if game.carried.is_none() && upgrade_out_view(game).is_some() {
            *button = Some(BUTTON_UPGRADE_TAKE + i32::from(right));
        }
        return true;
    }
    false
}

/// The menu buttons the redstone tab sends: the next mode (left click), the previous (right).
const BUTTON_REDSTONE_NEXT: i32 = 100;
const BUTTON_REDSTONE_PREVIOUS: i32 = 101;
/// The factory's sorting tab (`GuiInteraction.AUTO_SORT_BUTTON`).
const BUTTON_SORT: i32 = 102;

/// The capacity of a multiblock's tank, from the size its master saved.
fn multiblock_capacity(entity: &ModBlockEntity, key: &str) -> u64 {
    let nbt = &entity.nbt;
    match kind(&entity.id) {
        Some(Kind::FissionPort) => {
            let r = fission::Reactor { assemblies: get_long(nbt, "assemblies"), volume: get_long(nbt, "volume") };
            match key {
                "water" => r.water_capacity(),
                "steam" => r.steam_capacity(),
                _ => r.fuel_capacity(),
            }
        }
        Some(Kind::FusionPort) => {
            let injection = fusion_injection(nbt);
            match key {
                "water" => fusion::water_capacity(injection),
                "steam" => fusion::steam_capacity(injection),
                "plasma" | "case" => FUSION_MAX_TEMP,
                _ => fusion::FUEL_CAPACITY,
            }
        }
        Some(Kind::BoilerValve) => {
            let b = boiler_of(nbt);
            match key {
                "water" => b.water_capacity(),
                "steam" => b.steam_capacity(),
                _ => 0,
            }
        }
        Some(Kind::TurbineValve) => turbine_of(nbt).steam_capacity(),
        Some(Kind::DynamicValve) => dynamic_tank::capacity(get_long(nbt, "volume")),
        _ => 0,
    }
}

/// Where a multiblock port's master is, if the block is one: the port whose data its screen shows.
fn multiblock_master(entity: &ModBlockEntity, pos: [i32; 3]) -> Option<Option<[i32; 3]>> {
    Some(match kind(&entity.id)? {
        Kind::FissionPort | Kind::FusionPort | Kind::BoilerValve | Kind::TurbineValve | Kind::SpsPort | Kind::DynamicValve => {
            get_pos(&entity.nbt, "master")
        }
        Kind::InductionPort => (get_long(&entity.nbt, "rate") > 0).then(|| {
            ["mx", "my", "mz"].map(|key| match entity.nbt.get(&JavaString::from(key)) {
                Some(Tag::Int(v)) => *v,
                _ => pos[0],
            })
        }),
        _ => return None,
    })
}

/// A multiblock's screen: the master writes its state into the first four data slots (formed, then
/// two or three numbers) and what it makes or boils a tick into the usage slots; the other ports show
/// the master's data.
fn multiblock_data(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let Some(master) = multiblock_master(entity, pos) else {
        return;
    };
    let Some(master) = master else {
        entity.data[..4].fill(0);
        return;
    };
    if master != pos {
        if let Some(data) = game.native_with_mod_block_entity(master, |m| m.data.clone())
            && !data.is_empty()
        {
            entity.data = data;
        }
        return;
    }
    let nbt = &entity.nbt;
    let small = |v: u64| v.min(i16::MAX as u64) as i32;
    let (state, usage) = match kind(&entity.id) {
        Some(Kind::FissionPort) => {
            let r = fission::Reactor { assemblies: get_long(nbt, "assemblies"), volume: get_long(nbt, "volume") };
            let on = match (get_long(nbt, "active"), get_long(nbt, "scram")) {
                (1, _) => 1,
                (_, 1) => 2,
                _ => 0,
            };
            let burn = fission_burn(nbt, r.max_burn_tenths());
            ([1, on, small(burn), small(r.max_burn_tenths())], 0)
        }
        Some(Kind::FusionPort) => {
            let plasma = fusion_plasma(nbt);
            let mk = |k: f64| small((k / 1e6).max(0.0) as u64);
            ([1, i32::from(plasma.burning), small(fusion_injection(nbt)), mk(plasma.plasma)], get_long(nbt, "production"))
        }
        Some(Kind::BoilerValve) => {
            let b = boiler_of(nbt);
            let heat = get_double(nbt, "heat").unwrap_or(b.heat_capacity() * boiler::BOIL_TEMP);
            ([1, small((heat / b.heat_capacity().max(1.0)) as u64), small(b.elements), small(b.max_boil(heat))], get_long(nbt, "boil_rate"))
        }
        Some(Kind::TurbineValve) => {
            let flow = get_long(nbt, "flow");
            ([1, (flow & 0x7FFF) as i32, ((flow >> 15) & 0x7FFF) as i32, get_int(nbt, "dump") as i32], get_long(nbt, "production"))
        }
        Some(Kind::DynamicValve) => ([1, small(get_long(nbt, "volume")), 0, 0], 0),
        Some(Kind::SpsPort) => {
            let progress = (sps_of(nbt).scaled_progress() * 1_000.0) as u64;
            let processed = get_double(nbt, "processed").unwrap_or(0.0).max(0.0) as u64;
            ([1, small(get_long(nbt, "coils")), small(progress), small(processed)], get_long(nbt, "received"))
        }
        _ => ([1, 0, 0, 0], get_long(nbt, "rate")),
    };
    entity.data[..4].copy_from_slice(&state);
    entity.data.splice(DATA_USAGE..DATA_USAGE + 4, wide(usage));
    // The turbine's max flow rate and the boiler's max boil rate, in the two upgrade slots (30 bits).
    if matches!(kind(&entity.id), Some(Kind::TurbineValve | Kind::BoilerValve)) {
        let max = if kind(&entity.id) == Some(Kind::BoilerValve) {
            let b = boiler_of(&entity.nbt);
            b.max_boil(get_double(&entity.nbt, "heat").unwrap_or(b.heat_capacity() * boiler::BOIL_TEMP))
        } else {
            turbine_of(&entity.nbt).max_flow() as u64
        };
        entity.data[DATA_SPEED] = (max & 0x7FFF) as i32;
        entity.data[DATA_ENERGY_UPGRADES] = ((max >> 15) & 0x7FFF) as i32;
    }
}

/// The text a multiblock's screen shows between its gauges.
fn multiblock_lines(block: &str, data: &[i32]) -> Vec<String> {
    let get = |i: usize| data.get(i).copied().unwrap_or(0).max(0) as u64;
    // Only a multiblock's port has a formed state to report; a machine's first data slot is its furnace slot.
    let port = matches!(
        kind(block),
        Some(Kind::FissionPort | Kind::FusionPort | Kind::BoilerValve | Kind::TurbineValve | Kind::InductionPort | Kind::DynamicValve | Kind::SpsPort)
    );
    if !port {
        return vec![];
    }
    if get(0) == 0 {
        return vec!["Not formed".to_owned()];
    }
    let usage = read_wide(data, DATA_USAGE);
    let fe = mekanism_core::energy::format_fe;
    match kind(block) {
        // The fission screen draws its own inner screen (`GuiFissionReactor`).
        Some(Kind::FissionPort) => vec![],
        Some(Kind::FusionPort) => vec![
            if get(1) == 1 { "Burning".to_owned() } else { "Not burning".to_owned() },
            format!("Injection: {}", get(2)),
            format!("Plasma: {} MK", get(3)),
            format!("{}/t", fe(usage)),
            format!("Stored: {}", fe(read_wide(data, DATA_STORED))),
        ],
        // The boiler's screen draws its own lines (`GuiThermoelectricBoiler`'s inner screen).
        Some(Kind::BoilerValve) => vec![],
        // The turbine's screen draws its own inner screen (`GuiIndustrialTurbine`).
        Some(Kind::TurbineValve) => vec![],
        Some(Kind::InductionPort) => vec![format!("Max I/O: {}/t", fe(usage))],
        Some(Kind::DynamicValve) => vec![format!("Volume: {} blocks", get(1))],
        // The SPS screen draws its own inner screen (`GuiSPS`).
        Some(Kind::SpsPort) => vec![],
        _ => vec![],
    }
}

/// The reactors' buttons: [x, y, w, h] on the screen, label, and the menu button id.
fn buttons(block: &str) -> &'static [([f32; 4], &'static str)] {
    match kind(block) {
        Some(Kind::FissionPort) => &[([6.0, 75.0, 81.0, 16.0], "Activate"), ([89.0, 75.0, 81.0, 16.0], "SCRAM")],
        // The port has no lasers or hohlraum, so a button ignites the reactor.
        Some(Kind::FusionPort) => &[([63.0, 36.0, 60.0, 16.0], "Ignite")],
        // `GuiGasMode`: the steam dump mode, a 10 px image button.
        Some(Kind::TurbineValve) => &[([173.0, 72.0, 10.0, 10.0], "")],
        Some(Kind::ChemicalTank(_)) => &[([159.0, 72.0, 10.0, 10.0], "")],
        // `GuiDigitalMiner`: the four buttons, then the three switches (15 wide, icon underneath).
        Some(Kind::DigitalMiner) => &[
            ([87.0, 19.0, 61.0, 18.0], "Start"),
            ([87.0, 36.0, 61.0, 18.0], "Stop"),
            ([87.0, 53.0, 61.0, 18.0], "Config"),
            ([87.0, 70.0, 61.0, 18.0], "Reset"),
            ([19.0, 56.0, 15.0, 31.0], ""),
            ([38.0, 56.0, 15.0, 31.0], ""),
            ([57.0, 56.0, 15.0, 31.0], ""),
        ],
        _ => &[],
    }
}

/// A press on a reactor's button (hook point `native_screen_click`, API 20).
pub(super) fn screen_click(game: &Game<'_>, block: &str, data: &[i32], at: [f32; 2], right: bool, button: &mut Option<i32>) {
    if miner_cfg_open(block) {
        miner_cfg_click(game, at, right);
        return;
    }
    // Config opens the filter screen while the miner is idle.
    if kind(block) == Some(Kind::DigitalMiner) && !right && inside(at, [87.0, 53.0, 61.0, 18.0]) {
        if miner_view(block, data).state == 0 && let Some((x, y, z)) = game.container {
            MINER_VIEW.set(Some(([x, y, z], game.ticks as u64)));
        }
        return;
    }
    if kind(block) == Some(Kind::FusionPort) && fusion_click(game, block, at, right) {
        return;
    }
    if matches!(kind(block), Some(Kind::BoilerValve | Kind::TurbineValve)) && !right && inside(at, STATS_TAB_BUTTON) {
        if STATS_VIEW.get().is_some() {
            STATS_VIEW.set(None);
        } else if let Some((x, y, z)) = game.container {
            STATS_VIEW.set(Some(([x, y, z], game.ticks as u64)));
        }
        return;
    }
    if boiler_stats_open(block) || turbine_stats_open(block) {
        return;
    }
    if kind(block) == Some(Kind::FissionPort) {
        if fission_stats_open(block) {
            fission_stats_click(game, data, at, right);
            return;
        }
        if !right && inside(at, STATS_TAB_BUTTON) && let Some((x, y, z)) = game.container {
            STATS_VIEW.set(Some(([x, y, z], game.ticks as u64)));
            return;
        }
    }
    if is_qio_tile(block) {
        let select = qio_select_open(block);
        if select || inside(at, QIO_TAB_BUTTON) {
            qio_select_click(game, block, at, right);
            return;
        }
    }
    screen_click_base(game, block, data, at, right, button);
}

fn screen_click_base(game: &Game<'_>, block: &str, data: &[i32], at: [f32; 2], right: bool, button: &mut Option<i32>) {
    if is_freq_screen(block) {
        frequency_click(game, block, at, right);
        return;
    }
    if is_filter_screen(block) {
        if is_qio_filter(block) {
            qio_filter_click(game, block, at, right);
        } else {
            sorter_click(game, block, at, right);
        }
        return;
    }
    if is_od(block) && od_click(game, block, at) {
        return;
    }
    if is_qio_screen(block) {
        qio_click(game, block, at);
        return;
    }
    if kind(block) == Some(Kind::QioRedstoneAdapter) {
        adapter_click(game, at);
        return;
    }
    if upgrade_slot_click(game, block, at, right, button) {
        return;
    }
    if windowed_click(block, data, at, right, button) {
        return;
    }
    if let Some(i) = buttons(block).iter().position(|(r, _)| inside(at, *r)) {
        *button = Some(i as i32);
        return;
    }
    let Some(l) = layout(block) else {
        return;
    };
    match screen_tabs(block, &l, data).iter().find(|t| inside(at, tab_button(t))).map(|t| t.what) {
        Some(TabKind::Redstone) => *button = Some(if right { BUTTON_REDSTONE_PREVIOUS } else { BUTTON_REDSTONE_NEXT }),
        // The side configuration tab opens its window on the first type (`GuiWindowCreatorTab`).
        Some(TabKind::Sides) if !right => set_window(block, Some(Window::Sides(0))),
        Some(TabKind::Upgrade) if !right => set_window(block, Some(Window::Upgrades(None))),
        Some(TabKind::Transporter) if !right => set_window(block, Some(Window::Transporter)),
        Some(TabKind::Sort) if !right => *button = Some(BUTTON_SORT),
        Some(TabKind::Visuals) if !right => miner_toggle_visuals(game),
        _ => {}
    }
}

/// A reactor's button pressed, on the server (hook point `native_menu_button`, API 20): the fission
/// reactor switches on or off (0) or steps its burn rate (1); the fusion reactor ignites (0) or steps
/// its injection rate (1). The press goes to the reactor's master port.
pub(super) fn menu_button(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, id: i32) {
    if !game.native_on(MOD_ID) {
        return;
    }
    // The redstone tab (`GuiInteraction.NEXT_REDSTONE_CONTROL` / `PREVIOUS_REDSTONE_CONTROL`).
    if matches!(id, BUTTON_REDSTONE_NEXT | BUTTON_REDSTONE_PREVIOUS) && tabs(&entity.id) & TAB_REDSTONE != 0 {
        let now = RedstoneControl::from_ordinal(get_int(&entity.nbt, "control"));
        let next = if id == BUTTON_REDSTONE_NEXT { now.next() } else { now.previous() };
        put_int(&mut entity.nbt, "control", next.ordinal());
        return;
    }
    // `GasMode.getNext` on the chemical tank's button.
    if id == 0 && matches!(kind(&entity.id), Some(Kind::ChemicalTank(_))) {
        let next = (get_int(&entity.nbt, "dump") + 1) % 3;
        put_int(&mut entity.nbt, "dump", next);
        return;
    }
    // The Digital Miner's Start, Stop, Config (with the filters), Reset and its three switches.
    if kind(&entity.id) == Some(Kind::DigitalMiner) {
        let (state, running) = (get_int(&entity.nbt, "mstate"), get_int(&entity.nbt, "running") == 1);
        let flip = |nbt: &mut Compound, key: &str| put_int(nbt, key, u32::from(get_int(nbt, key) == 0));
        match id {
            // `start()`: the searcher starts when idle, and mining is allowed.
            0 if state == MINER_IDLE || !running => {
                if state == MINER_IDLE {
                    put_int(&mut entity.nbt, "mstate", MINER_SEARCHING);
                    put_int(&mut entity.nbt, "cursor", 0);
                    entity.nbt.remove(&JavaString::from("found"));
                }
                put_int(&mut entity.nbt, "running", 1);
            }
            // `stop()`: a search in progress is reset, a finished one stops mining.
            1 if state != MINER_IDLE && running => {
                if state == MINER_SEARCHING {
                    miner_reset(&mut entity.nbt);
                } else {
                    put_int(&mut entity.nbt, "running", 0);
                }
            }
            3 => miner_reset(&mut entity.nbt),
            4 => flip(&mut entity.nbt, "eject"),
            5 => flip(&mut entity.nbt, "pull"),
            6 => flip(&mut entity.nbt, "silk"),
            _ => {}
        }
        return;
    }
    if id == BUTTON_SORT && matches!(kind(&entity.id), Some(Kind::Factory(..))) {
        let on = get_int(&entity.nbt, "sorting") & 1 == 0;
        put_int(&mut entity.nbt, "sorting", u32::from(on));
        return;
    }
    if transporter_button(entity, id) || window_button(game, pos, entity, id) || upgrade_slot_button(game, entity, id) || upgrade_button(game, pos, entity, id) {
        return;
    }
    let Some(Some(master)) = multiblock_master(entity, pos) else {
        return;
    };
    let press = |m: &mut ModBlockEntity| match (kind(&m.id), id) {
        // Activate (0) and SCRAM (1): `FISSION_ACTIVE` with 1 and 0.
        (Some(Kind::FissionPort), 0) => {
            put_long(&mut m.nbt, "active", 1);
            put_long(&mut m.nbt, "scram", 0);
        }
        (Some(Kind::FissionPort), 1) => {
            put_long(&mut m.nbt, "active", 0);
            put_long(&mut m.nbt, "scram", 0);
        }
        // `GasMode.getNext`: idle, dumping excess, dumping.
        (Some(Kind::TurbineValve), 0) => {
            let next = (get_int(&m.nbt, "dump") + 1) % 3;
            put_int(&mut m.nbt, "dump", next)
        }
        (Some(Kind::FusionPort), 0) => {
            let mut plasma = fusion_plasma(&m.nbt);
            fusion::ignite(&mut plasma, fusion_injection(&m.nbt));
            put_fusion_plasma(&mut m.nbt, &plasma);
        }
        (Some(Kind::FusionPort), 1) => {
            let next = fusion::next_injection(fusion_injection(&m.nbt));
            put_long(&mut m.nbt, "injection", next);
        }
        _ => {}
    };
    if master == pos {
        press(entity);
    } else {
        game.native_with_mod_block_entity(master, press);
    }
}

pub(super) fn screen(block: &str, screen: &mut Option<super::native_hooks::NativeScreen>) {
    if screen.is_some() {
        return;
    }
    screen_base(block, screen);
    // `armorSideBar`: the player's armor slots and the off hand on the screen's side.
    if let (Some(s), Some((_, _, at))) = (screen.as_mut(), armor_sidebar(block)) {
        s.armor = Some(at);
        s.offhand = Some([at[0], at[1] + 72.0]);
    }
    // The QIO blocks swap to the selector's layout while their tab's screen is open: 176 x 155, none
    // of the inventory (it is moved out of the way).
    if matches!(kind(block), Some(Kind::FissionPort | Kind::FusionPort | Kind::BoilerValve | Kind::TurbineValve)) && let Some(s) = screen.as_mut() {
        // `GuiFissionReactorStats` (the default 176 x 166), `GuiFusionReactorInfo` (186 wide): no inventory.
        s.alt = Some(Box::new(super::native_hooks::NativeScreen {
            width: match kind(block) {
                Some(Kind::FusionPort) => 186.0,
                Some(Kind::TurbineValve) => 190.0,
                _ => 176.0,
            },
            height: 166.0,
            slots: vec![None; s.slots.len()],
            inventory_y: 4000.0,
            inventory_x: 0.0,
            own_labels: true,
            alt: None,
            ..Default::default()
        }));
    }
    if kind(block) == Some(Kind::DigitalMiner) && let Some(s) = screen.as_mut() {
        // `GuiDigitalMinerConfig`: the filter holder's 276 x 254, none of the miner's slots.
        s.alt = Some(Box::new(super::native_hooks::NativeScreen {
            width: 276.0,
            height: 254.0,
            slots: vec![None; s.slots.len()],
            inventory_y: 172.0,
            inventory_x: 58.0,
            own_labels: true,
            alt: None,
            ..Default::default()
        }));
    }
    if is_qio_tile(block) && let Some(s) = screen.as_mut() {
        s.alt = Some(Box::new(super::native_hooks::NativeScreen {
            width: 176.0,
            height: 155.0,
            slots: vec![None; s.slots.len()],
            inventory_y: 4000.0,
            inventory_x: 0.0,
            own_labels: true,
            alt: None,
            ..Default::default()
        }));
    }
}

/// The screens with an armor sidebar (`ContainerBuilder.armorSideBar` and `GuiSideHolder`): the holder's
/// top left and side, and the first armor slot (the off hand sits 72 below, `offhandOffset` 0).
fn armor_sidebar(block: &str) -> Option<([f32; 2], bool, [f32; 2])> {
    match kind(block)? {
        // `armorHolder` (-26, 62), `armorSideBar()` (-20, 67).
        Kind::FluidTank(_) | Kind::ChemicalTank(_) | Kind::DynamicValve => Some(([-26.0, 62.0], true, [-20.0, 67.0])),
        // `GuiSideHolder.create(imageWidth, 36, ...)`, `armorSideBar(180, 41, 0)`.
        Kind::EnergyCube(_) => Some(([176.0, 36.0], false, [180.0, 41.0])),
        // `GuiSideHolder.create(-26, 36, ...)`, `armorSideBar(-20, 41, 0)`.
        Kind::InductionPort => Some(([-26.0, 36.0], true, [-20.0, 41.0])),
        _ => None,
    }
}

fn screen_base(block: &str, screen: &mut Option<super::native_hooks::NativeScreen>) {
    if screen.is_some() {
        return;
    }
    // `GuiQIORedstoneAdapter`: 26 px taller than a machine's, no slots of its own.
    if kind(block) == Some(Kind::QioRedstoneAdapter) {
        *screen = Some(super::native_hooks::NativeScreen { width: 176.0, height: 192.0, slots: vec![None; 3], inventory_y: 110.0, inventory_x: 8.0, own_labels: true, alt: None, ..Default::default() });
        return;
    }
    // `GuiPersonalStorageTile`: six rows of nine over the inventory, 56 px taller than a machine's.
    if kind(block) == Some(Kind::PersonalStorage) {
        let slots = (0..bin::PERSONAL_SLOTS).map(|i| Some([8.0 + 18.0 * (i % 9) as f32, 18.0 + 18.0 * (i / 9) as f32])).collect();
        *screen = Some(super::native_hooks::NativeScreen { width: 176.0, height: 222.0, slots, inventory_y: 140.0, inventory_x: 8.0, own_labels: true, alt: None, ..Default::default() });
        return;
    }
    // `GuiQIODriveArray` and `GuiQIODashboard`.
    if is_qio_screen(block) {
        *screen = Some(qio_screen(block));
        return;
    }
    // `GuiQIOFilterHandler`: 60 wider and 74 taller than a machine's, no slots of its own.
    if is_qio_filter(block) {
        *screen = Some(super::native_hooks::NativeScreen { width: 236.0, height: 240.0, slots: vec![None; 3], inventory_y: 158.0, inventory_x: 38.0, own_labels: true, alt: None, ..Default::default() });
        return;
    }
    // `GuiLogisticalSorter`: 100 wider and 88 taller than a machine's, no slots of its own.
    if is_sorter(block) {
        *screen = Some(super::native_hooks::NativeScreen {
            width: 276.0,
            height: 254.0,
            slots: vec![None, None, None],
            inventory_y: 172.0,
            inventory_x: 58.0,
            own_labels: true,
            alt: None,
            ..Default::default()
        });
        return;
    }
    // `GuiTeleporter` and `GuiQuantumEntangloporter`: 74 taller, the teleporter's energy slot.
    if is_freq_screen(block) {
        let energy = (kind(block) == Some(Kind::Teleporter)).then_some([153.0, 7.0]);
        *screen = Some(super::native_hooks::NativeScreen {
            width: 176.0,
            height: 240.0,
            slots: vec![energy, None, None],
            inventory_y: 158.0,
            inventory_x: 8.0,
            own_labels: true,
            alt: None,
            ..Default::default()
        });
        return;
    }
    let Some(l) = layout(block) else {
        return;
    };
    if let Some(Kind::Factory(n, ..)) = kind(block) {
        let mut slots = vec![None; FACTORY_ENERGY + 1];
        slots[FACTORY_ENERGY] = Some([7.0, 13.0]);
        for (i, (input, output)) in factory_slots(n).into_iter().enumerate() {
            slots[i] = Some(input);
            slots[FACTORY_ROW + i] = Some(output);
        }
        let w = screen_width(block);
        *screen = Some(super::native_hooks::NativeScreen { width: w, height: 166.0, slots, inventory_y: 84.0, inventory_x: 8.0 + ((w - 176.0) / 2.0).floor(), own_labels: false, alt: None, ..Default::default() });
        return;
    }
    let (w, h) = (screen_width(block), screen_height(block));
    let mut slots: Vec<Option<[f32; 2]>> = l.slots.iter().map(|s| s.map(|(at, _)| at)).collect();
    let extra = extra_slots(block);
    if !extra.is_empty() {
        slots.resize((3 + extra.len()).max(5), None);
        for (i, (at, _)) in extra.iter().enumerate() {
            slots[3 + i] = Some(*at);
        }
    }
    *screen = Some(super::native_hooks::NativeScreen {
        width: w,
        height: h,
        slots,
        inventory_y: h - 82.0,
        inventory_x: 8.0 + ((w - 176.0) / 2.0).floor(),
        own_labels: matches!(kind(block), Some(Kind::Nucleosynthesizer | Kind::Evaporation | Kind::BoilerValve | Kind::TurbineValve | Kind::SpsPort | Kind::FissionPort | Kind::FusionPort)),
        alt: None,
        ..Default::default()
    });
}

/// A nine-slice sprite (`gui.scaling.type = nine_slice`) of `size` px with a `border` px frame, drawn
/// over `dst`.
fn nine_slice(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, name: &str, size: f32, border: f32, dst: [f32; 4]) {
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

fn sprite(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, name: &str, src: [f32; 4], dst: [f32; 4]) {
    game.native_sprite(batch, &format!("{SPRITES}{name}"), src, dst);
}

/// The rectangle a power bar covers.
fn power_rect(p: PowerBar) -> [f32; 4] {
    match p {
        PowerBar::Vertical([x, y]) => [x, y, 6.0, 54.0],
        PowerBar::Horizontal([x, y]) => [x, y, 54.0, 6.0],
        PowerBar::Gauge([x, y]) => [x, y, WIDE[0], WIDE[1]],
        PowerBar::SmallGauge([x, y]) => [x, y, SMALL_MED[0], SMALL_MED[1]],
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

/// `GuiDigitalMiner`'s widgets beyond the generic ones: the four buttons (`TranslationButton`, greyed by
/// `setCheckActive`), the three `GuiDigitalSwitch`es and the missing-block slot with its check.
fn draw_miner_widgets(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, block: &str, data: &[i32], origin: [f32; 2]) {
    let [ox, oy] = origin;
    let m = miner_view(block, data);
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    // Start (idle or not running), Stop (started and running), Config (idle), Reset (always).
    let active = [m.state == 0 || !m.running, m.state != 0 && m.running, m.state == 0, true];
    for (i, ([x, y, w, h], label)) in buttons(block).iter().take(4).enumerate() {
        let r = [ox + x, oy + y, *w, *h];
        draw_mek_button(game, batch, r, active[i], active[i] && inside(mouse, [*x, *y, *w, *h]));
        let tw = game.native_text_width(label);
        game.native_text(batch, label, [r[0] + (w - tw) / 2.0, r[1] + (h - 8.0) / 2.0], if active[i] { 0xFF_FFFF } else { 0xA0_A0A0 }, true);
    }
    for (i, (icon, on)) in [("switch/eject", m.eject), ("switch/input", m.pull), ("switch/silk", m.silk)].into_iter().enumerate() {
        let (x, y) = (ox + 19.0 + 19.0 * i as f32, oy + 56.0);
        let (top, bottom) = if on { ("switch/selected", "switch/unselected") } else { ("switch/unselected", "switch/selected") };
        sprite(game, batch, top, [0.0, 0.0, 15.0, 8.0], [x, y, 15.0, 8.0]);
        sprite(game, batch, bottom, [0.0, 0.0, 15.0, 8.0], [x, y + 9.0, 15.0, 8.0]);
        sprite(game, batch, icon, [0.0, 0.0, 5.0, 5.0], [x + 5.0, y + 21.0, 5.0, 5.0]);
        for (j, text) in ["On", "Off"].into_iter().enumerate() {
            let tw = game.native_text_width(text) * 0.6;
            game.native_text_scaled(batch, text, [x + (15.0 - tw) / 2.0, y + 9.0 * j as f32 + 1.0], 0xFF10_1010, 0.6);
        }
    }
    // The slot that shows a replacement block the miner is missing: always the check until filters exist.
    sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 64.0, oy + 21.0, 18.0, 18.0]);
    sprite(game, batch, "slot/overlay/check", [0.0, 0.0, 18.0, 18.0], [ox + 64.0, oy + 21.0, 18.0, 18.0]);
}

/// What a screen draws beyond its `Layout`: progress arrows that fill while the machine is lit
/// (`GuiProgress` on `tile::getActive`), an inner screen with text lines, and down arrows.
struct Extras {
    active_progress: Vec<([f32; 2], &'static str, [f32; 2])>,
    screen: Option<[f32; 4]>,
    arrows: Vec<[f32; 2]>,
}

fn extras(block: &str) -> Extras {
    let none = Extras { active_progress: vec![], screen: None, arrows: vec![] };
    let large = |x, y| ([x, y], "progress/large_right", [48.0, 8.0]);
    match kind(block) {
        Some(Kind::Activator | Kind::Centrifuge) => Extras { active_progress: vec![large(64.0, 39.0)], ..none },
        Some(Kind::ChemicalInfuser) => Extras {
            active_progress: vec![([47.0, 39.0], "progress/small_right", [28.0, 8.0]), ([101.0, 39.0], "progress/small_left", [28.0, 8.0])],
            ..none
        },
        Some(Kind::Pump | Kind::Plenisher) => Extras { screen: Some([54.0, 23.0, 80.0, 42.0]), arrows: vec![[32.0, 39.0]], ..none },
        // `GuiTexturedElement.downArrow(imageWidth + 8, 91)` between the washer's two fluid slots.
        Some(Kind::Washer) => Extras { active_progress: vec![large(64.0, 39.0)], arrows: vec![[184.0, 91.0]], ..none },
        Some(Kind::Heater) => Extras { screen: Some([48.0, 23.0, 80.0, 42.0]), ..none },
        Some(Kind::DigitalMiner) => Extras { screen: Some([7.0, 19.0, 77.0, 69.0]), ..none },
        // `GuiThermalEvaporationController`: the screen between the gauges and the arrows under the buckets.
        Some(Kind::BoilerValve) => Extras { screen: Some([54.0, 23.0, 110.0, 40.0]), ..none },
        Some(Kind::TurbineValve) => Extras { screen: Some([50.0, 18.0, 126.0, 50.0]), ..none },
        Some(Kind::SpsPort) => Extras { screen: Some([27.0, 17.0, 122.0, 60.0]), ..none },
        Some(Kind::Evaporation) => Extras { screen: Some([48.0, 19.0, 100.0, 40.0]), arrows: vec![[32.0, 39.0], [156.0, 39.0]], ..none },
        Some(Kind::FuelwoodHeater) => Extras {
            screen: Some([48.0, 23.0, 80.0, 28.0]),
            active_progress: vec![([144.0, 31.0], "progress/flame", [13.0, 13.0])],
            ..none
        },
        _ => none,
    }
}

/// The inner screen's lines: the pump's energy and fluid; the plenisher's energy, whether it has
/// finished, and its fluid.
/// `Double.toString` for the small values the screens show: `1.0`, `0.25`.
fn java_double(v: f64) -> String {
    let text = format!("{v}");
    if text.contains('.') { text } else { format!("{text}.0") }
}

fn screen_lines(block: &str, data: &[i32]) -> Vec<String> {
    let fe = mekanism_core::energy::format_fe;
    let energy = format!("{}/{}", fe(read_wide(data, DATA_STORED)), fe(read_wide(data, DATA_CAPACITY)));
    let fluid = || {
        let amount = read_wide(data, DATA_GAUGES);
        let name = usize::try_from(data.get(DATA_GAUGES + 8).copied().unwrap_or(-1)).ok().and_then(|c| CHEMICALS.get(c)).map(|c| c.1);
        match (amount, name) {
            (0, _) | (_, None) => "No fluid".to_owned(),
            (_, Some(name)) => format!("{name}: {amount} mB"),
        }
    };
    match kind(block) {
        Some(Kind::DigitalMiner) => {
            let m = miner_view(block, data);
            vec![
                if m.low_power { "Low Power" } else if m.running { "Running" } else { "Idle" }.to_owned(),
                ["Not ready", "Searching", "Ready", "Paused"][m.state.min(2) as usize].to_owned(),
                format!("To mine: {}", thousands(u64::from(m.to_mine))),
            ]
        }
        Some(Kind::Pump) => vec![energy, fluid()],
        Some(Kind::SpsPort) if data.first().copied().unwrap_or(0) > 0 => {
            let processed = data.get(3).copied().unwrap_or(0).max(0);
            let active = processed > 0;
            let mut lines = vec![format!("Status: {}", if active { "Active" } else { "Idle" })];
            if active {
                lines.push(format!("Energy Input: {}/t", fe(read_wide(data, DATA_USAGE))));
                lines.push(format!("Process Rate: {} mB/t", java_double(f64::from(processed) / 1000.0)));
            }
            lines
        }
        Some(Kind::TurbineValve) if data.first().copied().unwrap_or(0) > 0 => {
            let flow = (data.get(1).copied().unwrap_or(0) as u64 & 0x7FFF) | (data.get(2).copied().unwrap_or(0) as u64 & 0x7FFF) << 15;
            let max = (data.get(DATA_SPEED).copied().unwrap_or(0) as u64 & 0x7FFF) | (data.get(DATA_ENERGY_UPGRADES).copied().unwrap_or(0) as u64 & 0x7FFF) << 15;
            vec![
                format!("Production: {}", fe(read_wide(data, DATA_USAGE))),
                format!("Flow rate: {flow} mB/t"),
                format!("Capacity: {} mB", read_wide(data, DATA_GAUGES + 4)),
                format!("Max flow: {max} mB/t"),
            ]
        }
        Some(Kind::BoilerValve) => vec![
            format!("Temp: {}", mekanism_core::energy::format_unit(f64::from(data.get(1).copied().unwrap_or(0).max(0)), "K")),
            format!("Boil Rate: {}", read_wide(data, DATA_USAGE)),
            format!("Max Boil: {}", data.get(3).copied().unwrap_or(0).max(0)),
        ],
        Some(Kind::Evaporation) => {
            let kelvin = f64::from(data.get(2).copied().unwrap_or(3000).max(0)) / 10.0;
            let gain = f64::from(data.get(3).copied().unwrap_or(0).max(0)) / 100.0;
            vec![
                "Formed".to_owned(),
                format!("Height: {}", data.get(1).copied().unwrap_or(0)),
                format!("Temp: {}", mekanism_core::energy::format_unit(kelvin, "K")),
                format!("Production: {} mB/t", java_double(gain)),
            ]
        }
        Some(Kind::Plenisher) => {
            let finished = if data.get(2).copied().unwrap_or(0) == 1 { "Yes" } else { "No" };
            vec![energy, format!("Finished: {finished}"), fluid()]
        }
        // The heaters' temperature (kelvin, sent in tenths) and the usage or the fuel left.
        Some(Kind::Heater | Kind::FuelwoodHeater) => {
            let temp = f64::from(data.get(2).copied().unwrap_or(3000).max(0)) / 10.0;
            let temp = format!("Temp: {}", mekanism_core::energy::format_unit(temp, "K"));
            if kind(block) == Some(Kind::Heater) {
                vec![temp, format!("Usage: {}/t", fe(read_wide(data, DATA_USAGE)))]
            } else {
                vec![temp, format!("Fuel: {}", data.get(3).copied().unwrap_or(0).max(0))]
            }
        }
        _ => vec![],
    }
}

// The nucleosynthesizer's own parts (`GuiAntiprotonicNucleosynthesizer`): the inner screen with the
// process rate and the bolts between the antimatter and the energy, and the wide progress bar below.
thread_local! {
    static BOLTS: std::cell::RefCell<mekanism_core::bolt::BoltRenderer> = std::cell::RefCell::new(mekanism_core::bolt::BoltRenderer::new(
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64),
    ));
    /// The game tick last seen and when it began: the frame's share of a tick.
    static BOLT_CLOCK: std::cell::RefCell<(u64, std::time::Instant)> = std::cell::RefCell::new((0, std::time::Instant::now()));
}

fn draw_nucleosynthesizer(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, data: &[i32], origin: [f32; 2]) {
    let [ox, oy] = origin;
    let lit = game.container.map(|(x, y, z)| [x, y, z]).and_then(|p| game.native_block_property(p, "lit")).as_deref() == Some("true");
    let screen = [ox + 45.0, oy + 18.0, 104.0, 68.0];
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, screen, 0xFFFF_FFFF);
    let rate = if lit { "100%" } else { "0%" };
    let line = format!("Process Rate: {rate}");
    let w = game.native_text_width(&line) * 0.8;
    game.native_text_scaled(batch, &line, [screen[0] + (screen[2] - w) / 2.0, screen[1] + screen[3] - 2.0 - 8.0 * 0.8], 0x3C_FE9A, 0.8);
    // `BoltEffect` through `BoltRenderer`: `min(ceil(rate / 8), 20)` bolts of fifteen segments across the
    // screen's middle, a new one every tick (`CONSECUTIVE`, lifespan 1, no fade).
    let rate = if lit { 1.0_f32 } else { 0.0 };
    let count = ((rate / 8.0).ceil() as i32).min(20);
    if count > 0 {
        use mekanism_core::bolt::{BoltEffect, BoltRenderInfo, Timestamp, V3};
        let from = V3(45.0 + 2.0, 18.0 + (68.0 - 4.0) / 2.0, 0.0);
        let effect = BoltEffect {
            info: BoltRenderInfo::with_color(0.45, 0.45, 0.5, 1.0),
            start: from,
            end: V3(from.0 + (104.0 - 4.0), from.1, 0.0),
            segments: 15,
            count,
            size: 1.0,
        };
        let partial = BOLT_CLOCK.with_borrow_mut(|c| {
            if c.0 != game.ticks {
                *c = (game.ticks, std::time::Instant::now());
            }
            (c.1.elapsed().as_secs_f32() * 20.0).min(0.999)
        });
        let now = Timestamp { ticks: game.ticks as i64, partial };
        BOLTS.with_borrow_mut(|r| {
            for segment in r.states(&effect, 1.0, now) {
                for q in segment.chunks(4) {
                    let p = |v: &V3| [ox + v.0, oy + v.1];
                    game.native_fill_quad(batch, [p(&q[0]), p(&q[1]), p(&q[2]), p(&q[3])], effect.info.color, true);
                }
            }
        });
    }
    let get = |i: usize| data.get(i).copied().unwrap_or(0).max(0) as f32;
    draw_dynamic_rate_bar(game, batch, [ox + 5.0, oy + 88.0], 183.0, (get(2) / get(3).max(1.0)).clamp(0.0, 1.0));
}

/// `GuiDynamicHorizontalRateBar`: the dynamic-rate sprite multiplied by a colour that runs from
/// (60, 45, 74) to (100, 30, 170) along the bar.
// --- Fusion Reactor screens (GuiFusionReactorController, ...Heat, ...Fuel) ---------------------------------

/// The injection rate packet: the port's position and the rate (a byte).
const FUSION_CHANNEL: &str = "mekanism:fusion_injection";
const FUSION_TAB_Y: [f32; 2] = [6.0, 34.0];
const FUSION_BACK: [f32; 4] = [6.0, 6.0, 14.0, 14.0];
const FUSION_FIELD: [f32; 4] = [103.0, 115.0, 26.0, 11.0];

thread_local! {
    /// The Fusion Reactor screen open beside the main one: the port, the tick it was last drawn and the view
    /// (1 heat, 2 fuel).
    static FUSION_VIEW: std::cell::Cell<Option<([i32; 3], u64, u8)>> = const { std::cell::Cell::new(None) };
    static FUSION_TEXT: std::cell::RefCell<freq::TextBox> = std::cell::RefCell::new(freq::TextBox::default());
}

fn fusion_view(block: &str) -> Option<u8> {
    if kind(block) != Some(Kind::FusionPort) {
        return None;
    }
    FUSION_VIEW.get().map(|v| v.2)
}

/// The tabs a view shows (view numbers 1 heat, 2 fuel): the others', at their own heights.
fn fusion_tabs(view: u8) -> Vec<(u8, f32)> {
    (1..=2u8).filter(|&v| v != view).map(|v| (v, FUSION_TAB_Y[usize::from(v) - 1])).collect()
}

fn fusion_tab_icon(view: u8) -> String {
    if view == 1 { format!("{SPRITES}transmission/heat") } else { "mekanismgenerators:gui/sprites/button/fuel".to_owned() }
}

fn draw_fusion_tabs(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2], view: u8) {
    for (v, y) in fusion_tabs(view) {
        draw_tab(game, batch, origin, y, &fusion_tab_icon(v));
    }
    let [ox, oy] = origin;
    sprite(game, batch, "tab/energy_info", [0.0, 0.0, 26.0, 26.0], [ox - 26.0, oy + 137.0, 26.0, 26.0]);
    sprite(game, batch, "tab/heat_info_k", [0.0, 0.0, 26.0, 26.0], [ox - 26.0, oy + 109.0, 26.0, 26.0]);
}

/// The controller: the title, the formed line and the tabs.
fn draw_fusion_main(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2], width: f32, height: f32) {
    let [ox, oy] = origin;
    let title = game.native_screen_title().unwrap_or_default();
    centred(game, batch, &title, ox, width, oy + 3.0, 0x40_4040);
    game.native_text(batch, "Inventory", [ox + 13.0, oy + height - 94.0], 0x40_4040, false);
    game.native_text(batch, "Multiblock formed", [ox + 13.0, oy + 16.0], 0x40_4040, false);
    draw_fusion_tabs(game, batch, origin, 0);
}

/// A gauge of the master's data: its box, the fill and the overlay.
fn draw_data_gauge(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2], at: [f32; 2], size: [f32; 2], fill: f32, (texture, argb): (&str, u32)) {
    let [x, y] = [origin[0] + at[0], origin[1] + at[1]];
    nine_slice(game, batch, "gauge/normal", 5.0, 2.0, [x, y, size[0], size[1]]);
    let h = (size[1] - 2.0) * fill.clamp(0.0, 1.0);
    if h > 0.0 {
        fill_tiled(game, batch, texture, [x + 1.0, y + size[1] - 1.0 - h, size[0] - 2.0, h], argb);
    }
    let overlay = match size {
        SMALL => "small",
        SMALL_MED => "small_med",
        WIDE => "wide",
        _ => "standard",
    };
    sprite(game, batch, &format!("gauge/overlay/{overlay}"), [0.0, 0.0, size[0] - 2.0, size[1] - 2.0], [x + 1.0, y + 1.0, size[0] - 2.0, size[1] - 2.0]);
}

/// The Fusion Reactor layout's gauge `index` in the data: amount, capacity and its chemical.
fn fusion_gauge(data: &[i32], index: usize) -> (u64, u64, Option<&'static (&'static str, &'static str, u32)>) {
    let at = DATA_GAUGES + index * GAUGE_SLOTS;
    let chemical = usize::try_from(data.get(at + 8).copied().unwrap_or(-1)).ok().and_then(|c| CHEMICALS.get(c));
    (read_wide(data, at), read_wide(data, at + 4).max(1), chemical)
}

fn draw_fusion_chemical_gauge(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, data: &[i32], origin: [f32; 2], index: usize, at: [f32; 2], size: [f32; 2]) {
    let (amount, capacity, chemical) = fusion_gauge(data, index);
    let (texture, argb) = gauge_texture(chemical);
    draw_data_gauge(game, batch, origin, at, size, amount as f32 / capacity as f32, (&texture, argb));
}

/// `GuiFusionReactorHeat` and `GuiFusionReactorFuel`.
fn draw_fusion_view(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, data: &[i32], origin: [f32; 2], view: u8) {
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, 186.0, 166.0]);
    let title = game.native_screen_title().unwrap_or_default();
    centred(game, batch, &title, ox + 18.0, 168.0, oy + 3.0, 0x40_4040);
    draw_mek_button(game, batch, [ox + FUSION_BACK[0], oy + FUSION_BACK[1], 14.0, 14.0], true, inside(mouse, FUSION_BACK));
    sprite(game, batch, "button/back", [0.0, 0.0, 14.0, 14.0], [ox + FUSION_BACK[0], oy + FUSION_BACK[1], 14.0, 14.0]);
    draw_fusion_tabs(game, batch, origin, view);
    let progress = |game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, name: &str, at: [f32; 2], full: bool| {
        sprite(game, batch, &format!("progress/{name}/{}", if full { "full" } else { "empty" }), [0.0, 0.0, 28.0, 8.0], [ox + at[0], oy + at[1], 28.0, 8.0]);
    };
    if view == 1 {
        let (plasma, _, _) = fusion_gauge(data, 5);
        let (case, _, _) = fusion_gauge(data, 6);
        let lava = ("block/lava_still", 0xFFFF_FFFF);
        draw_data_gauge(game, batch, origin, [12.0, 50.0], STANDARD, (plasma as f64 / FUSION_MAX_TEMP as f64).min(1.0) as f32, lava);
        draw_data_gauge(game, batch, origin, [66.0, 50.0], STANDARD, (case as f64 / FUSION_MAX_TEMP as f64).min(1.0) as f32, lava);
        let (water, _, _) = fusion_gauge(data, 2);
        let (steam, steam_cap, _) = fusion_gauge(data, 3);
        progress(game, batch, "small_right", [34.0, 76.0], plasma > case);
        progress(game, batch, "small_right", [88.0, 61.0], case > 0);
        progress(game, batch, "small_right", [88.0, 91.0], case > 0 && water > 0 && steam < steam_cap);
        draw_fusion_chemical_gauge(game, batch, data, origin, 2, [120.0, 84.0], SMALL);
        draw_fusion_chemical_gauge(game, batch, data, origin, 3, [156.0, 84.0], SMALL);
        let capacity = read_wide(data, DATA_CAPACITY).max(1);
        let energy = (read_wide(data, DATA_STORED) as f32 / capacity as f32).clamp(0.0, 1.0);
        draw_data_gauge(game, batch, origin, [120.0, 46.0], SMALL, energy, (&format!("{SPRITES_ROOT}gauge_fluid/energy"), 0xFFFF_FFFF));
        return;
    }
    let burning = data.get(1).copied().unwrap_or(0) == 1;
    draw_fusion_chemical_gauge(game, batch, data, origin, 0, [30.0, 64.0], SMALL);
    draw_fusion_chemical_gauge(game, batch, data, origin, 4, [84.0, 50.0], STANDARD);
    draw_fusion_chemical_gauge(game, batch, data, origin, 1, [138.0, 64.0], SMALL);
    progress(game, batch, "small_right", [52.0, 76.0], burning);
    progress(game, batch, "small_left", [106.0, 76.0], burning);
    centred(game, batch, &format!("Injection Rate: {}", data.get(2).copied().unwrap_or(0)), ox + 16.0, 154.0, oy + 35.0, 0x40_4040);
    let label = "Edit Rate:";
    game.native_text(batch, label, [ox + 101.0 - game.native_text_width(label), oy + 117.0], 0x40_4040, false);
    let [fx, fy, fw, fh] = FUSION_FIELD;
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, [ox + fx - 1.0, oy + fy - 1.0, fw + 2.0, fh + 2.0], 0xFFFF_FFFF);
    FUSION_TEXT.with_borrow(|t| {
        let text_at = [ox + fx + 2.0, oy + fy + 2.0];
        game.native_text(batch, &t.value, text_at, 0xE0_E0E0, true);
        if t.focused && (game.ticks / 6) % 2 == 0 {
            let before: String = t.value.chars().take(t.cursor).collect();
            game.native_text(batch, "_", [text_at[0] + game.native_text_width(&before), text_at[1]], 0xE0_E0E0, true);
        }
    });
}

/// Clicks on the controller's tabs and the other views. True when it took the click.
fn fusion_click(game: &Game<'_>, block: &str, at: [f32; 2], right: bool) -> bool {
    let Some((x, y, z)) = game.container else {
        return false;
    };
    let view = fusion_view(block).unwrap_or(0);
    let open = |v: u8| FUSION_VIEW.set(Some(([x, y, z], game.ticks as u64, v)));
    if !right {
        for (v, ty) in fusion_tabs(view) {
            if inside(at, [STATS_TAB_BUTTON[0], ty + 4.0, 18.0, 18.0]) {
                open(v);
                return true;
            }
        }
        if view != 0 && inside(at, FUSION_BACK) {
            FUSION_VIEW.set(None);
            return true;
        }
    }
    if view == 2 {
        FUSION_TEXT.with_borrow_mut(|t| t.focused = !right && inside(at, FUSION_FIELD));
    }
    view != 0
}

fn fusion_key(game: &Game<'_>, key: &str, text: Option<&str>) {
    let Some((pos, ..)) = FUSION_VIEW.get() else {
        return;
    };
    FUSION_TEXT.with_borrow_mut(|t| {
        if t.key(key, text, |c| c.is_ascii_digit(), 2) == freq::Typed::Enter && !t.value.is_empty() {
            if let Ok(rate) = t.value.parse::<u8>() {
                let mut data = Vec::new();
                for c in pos {
                    data.extend(c.to_be_bytes());
                }
                data.push(rate);
                game.native_send_to_server(FUSION_CHANNEL, &data);
            }
            *t = freq::TextBox { focused: true, ..Default::default() };
        }
    });
}

/// `setInjectionRate`: an even rate from 0 up to the maximum.
fn fusion_payload(game: &mut Game<'_>, data: &[u8]) {
    let (Some(pos), Some(&rate), Some(player)) = (data.get(..12), data.get(12), game.native_acting_player()) else {
        return;
    };
    let int = |i: usize| i32::from_be_bytes([pos[i], pos[i + 1], pos[i + 2], pos[i + 3]]);
    let pos = [int(0), int(4), int(8)];
    let near = (0..3).map(|k| (player.position[k] - f64::from(pos[k]) - 0.5).powi(2)).sum::<f64>() <= 64.0;
    if !near || kind(block_name(game, pos)) != Some(Kind::FusionPort) {
        return;
    }
    let Some(Some(Some(master))) = game.native_with_mod_block_entity(pos, |m| multiblock_master(m, pos)) else {
        return;
    };
    let rate = u64::from(rate - rate % 2).min(fusion::MAX_INJECTION);
    game.native_with_mod_block_entity(master, |m| put_long(&mut m.nbt, "injection", rate));
}

/// The tooltips of the tabs, the back button and the gauges. True when it wrote some.
fn fusion_tooltip(block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) -> bool {
    let view = fusion_view(block).unwrap_or(0);
    for (v, ty) in fusion_tabs(view) {
        if inside(at, [STATS_TAB_BUTTON[0], ty + 4.0, 18.0, 18.0]) {
            lines.push(if v == 1 { "Heat" } else { "Fuel" }.to_owned());
            return true;
        }
    }
    if view != 0 && inside(at, FUSION_BACK) {
        lines.push("Back".to_owned());
        return true;
    }
    if inside(at, [-26.0, 137.0, 26.0, 26.0]) {
        lines.push(format!("Storing: {}", mekanism_core::energy::format_fe(read_wide(data, DATA_STORED))));
        lines.push(format!("Producing: {}/t", mekanism_core::energy::format_fe(read_wide(data, DATA_USAGE))));
        return true;
    }
    let chemical = |index: usize, lines: &mut Vec<String>| {
        let (amount, capacity, chem) = fusion_gauge(data, index);
        lines.push(match (amount, chem) {
            (0, _) => "Empty".to_owned(),
            (_, Some(c)) => format!("{}: {amount} mB", c.1),
            (_, None) => format!("{amount} mB"),
        });
        lines.push(format!("§7Capacity: {capacity} mB"));
    };
    let hit = |x: f32, y: f32, size: [f32; 2]| inside(at, [x, y, size[0], size[1]]);
    match view {
        1 => {
            if hit(12.0, 50.0, STANDARD) || hit(66.0, 50.0, STANDARD) {
                let plasma = hit(12.0, 50.0, STANDARD);
                let (t, ..) = fusion_gauge(data, if plasma { 5 } else { 6 });
                lines.push(format!("{}: {}", if plasma { "Plasma" } else { "Case" }, mekanism_core::energy::format_unit(t as f64, "K")));
            } else if hit(120.0, 84.0, SMALL) {
                chemical(2, lines);
            } else if hit(156.0, 84.0, SMALL) {
                chemical(3, lines);
            } else if hit(120.0, 46.0, SMALL) {
                lines.push(format!(
                    "{} / {} FE",
                    read_wide(data, DATA_STORED),
                    read_wide(data, DATA_CAPACITY)
                ));
            }
            true
        }
        2 => {
            if hit(30.0, 64.0, SMALL) {
                chemical(0, lines);
            } else if hit(84.0, 50.0, STANDARD) {
                chemical(4, lines);
            } else if hit(138.0, 64.0, SMALL) {
                chemical(1, lines);
            }
            true
        }
        _ => false,
    }
}

/// The max rate the master wrote into the two upgrade slots (30 bits).
fn wide_max(data: &[i32]) -> u64 {
    (data.get(DATA_SPEED).copied().unwrap_or(0) as u64 & 0x7FFF) | (data.get(DATA_ENERGY_UPGRADES).copied().unwrap_or(0) as u64 & 0x7FFF) << 15
}

/// `TextUtils.format`: digits in groups of three.
fn thousands(n: u64) -> String {
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

thread_local! {
    /// `GuiLongGraph`s of the boiler's statistics: the tick sampled, the boil rates, the max boil rates and the
    /// first graph's scale (it only grows, from 10).
    static BOILER_GRAPH: std::cell::RefCell<(u64, Vec<u64>, Vec<u64>, u64)> = const { std::cell::RefCell::new((0, Vec::new(), Vec::new(), 10)) };
}

const BOILER_GRAPHS: [[f32; 4]; 2] = [[7.0, 82.0, 162.0, 38.0], [7.0, 121.0, 162.0, 38.0]];

fn turbine_stats_open(block: &str) -> bool {
    kind(block) == Some(Kind::TurbineValve) && STATS_VIEW.get().is_some()
}

/// `GuiTurbineStats`.
fn draw_turbine_stats(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, data: &[i32], origin: [f32; 2]) {
    let [ox, oy] = origin;
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, 190.0, 166.0]);
    centred(game, batch, "Turbine Statistics", ox, 190.0, oy + 4.0, 0x40_4040);
    draw_stats_tab(game, batch, origin, "transmission/chemicals");
    sprite(game, batch, "tab/energy_info", [0.0, 0.0, 26.0, 26.0], [ox - 26.0, oy + 137.0, 26.0, 26.0]);
    let (a, _, _) = fusion_gauge(data, 1);
    let (b, _, _) = fusion_gauge(data, 2);
    let (volume, dispersers, vents, blades) = (a & 0xF_FFFF, a >> 20 & 0xFFF, a >> 32 & 0xFFF, a >> 44 & 0xFF);
    let (coils, condensers, max_production) = (b & 0xFFF, b >> 12 & 0xFFF, b >> 24);
    let flow = |n: u64, per: f64| n as f64 * per;
    let produced = volume as f64 * dispersers as f64 * turbine::DISPERSER_FLOW;
    let dispersers_limiting = produced < flow(vents, turbine::VENT_FLOW);
    let vents_limiting = produced > flow(vents, turbine::VENT_FLOW);
    let per_coil = turbine::BLADES_PER_COIL;
    let text = |batch: &mut horizon_client::hud::HudBatch, t: &str, x: f32, y: f32, rgb: u32, limiting: bool| {
        game.native_text(batch, t, [ox + x, oy + y], rgb, false);
        if limiting {
            game.native_text(batch, "(Limiting)", [ox + x + game.native_text_width(t) + game.native_text_width(" "), oy + y], 0xAA_0000, false);
        }
    };
    let (g, h) = (0x40_4040, 0x20_2020);
    text(batch, &format!("Tank Volume: {volume}"), 6.0, 26.0, g, false);
    text(batch, "Steam Flow", 6.0, 40.0, h, false);
    text(batch, &format!("Dispersers: {dispersers}"), 10.0, 49.0, g, dispersers_limiting);
    text(batch, &format!("Vents: {vents}"), 10.0, 58.0, g, vents_limiting);
    text(batch, "Production", 6.0, 72.0, h, false);
    text(batch, &format!("Blades: {blades}"), 10.0, 81.0, g, coils * per_coil > blades);
    text(batch, &format!("Coils: {coils}"), 10.0, 90.0, g, coils * per_coil < blades);
    text(batch, &format!("Max Production: {}", mekanism_core::energy::format_fe(max_production)), 6.0, 104.0, g, false);
    text(batch, &format!("Max Water Output: {} mB/t", thousands(condensers * turbine::CONDENSER_RATE)), 6.0, 113.0, g, false);
}

fn boiler_stats_open(block: &str) -> bool {
    kind(block) == Some(Kind::BoilerValve) && STATS_VIEW.get().is_some()
}

/// A `GuiGraph`: the screen, then a column per sample (the graphed value, its integral, the hovered column).
fn draw_long_graph(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2], rect: [f32; 4], samples: &[u64], scale: u64, mouse: [f32; 2]) {
    let [ox, oy] = origin;
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, [ox + rect[0], oy + rect[1], rect[2], rect[3]], 0xFFFF_FFFF);
    let height = rect[3] - 2.0;
    let hover = (mouse[0] - rect[0]).floor();
    for (i, &v) in samples.iter().enumerate() {
        let h = (v.min(scale) as f64 * f64::from(height) / scale as f64) as f32;
        let h = h.floor();
        let (x, y) = (ox + rect[0] + 1.0 + i as f32, oy + rect[1] + 1.0 + height - h);
        game.native_fill(batch, [x, y, 1.0, 1.0], 0xFF28_FF74);
        let alpha = ((0.2 + 0.8 * i as f32 / samples.len() as f32) * 255.0).round() as u32;
        game.native_fill(batch, [x, y, 1.0, h], alpha << 24 | 0x24AF5A);
        if hover == i as f32 && mouse[1] >= rect[1] && mouse[1] < rect[1] + height {
            game.native_fill(batch, [x, oy + rect[1] + 1.0, 1.0, height], 0x806D_B8FB);
            game.native_fill(batch, [x, y, 1.0, 1.0], 0xFFAD_DCFF);
        }
    }
}

/// `GuiBoilerStats`.
fn draw_boiler_stats(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, data: &[i32], origin: [f32; 2]) {
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, 176.0, 166.0]);
    centred(game, batch, "Boiler Statistics", ox, 176.0, oy + 4.0, 0x40_4040);
    draw_stats_tab(game, batch, origin, "transmission/chemicals");
    sprite(game, batch, "tab/heat_info_k", [0.0, 0.0, 26.0, 26.0], [ox - 26.0, oy + 109.0, 26.0, 26.0]);
    let (_, water_cap, _) = fusion_gauge(data, 1);
    let (_, steam_cap, _) = fusion_gauge(data, 2);
    let elements = data.get(2).copied().unwrap_or(0).max(0) as u64;
    let capacity = elements * 16_000_000 / 10;
    let mut text = |t: &str, x: f32, y: f32, rgb: u32| game.native_text(batch, t, [ox + x, oy + y], rgb, false);
    text(&format!("Max Water: {} mB", thousands(water_cap)), 8.0, 26.0, 0x40_4040);
    text(&format!("Max Steam: {} mB", thousands(steam_cap)), 8.0, 35.0, 0x40_4040);
    text("Heat Transfer", 8.0, 49.0, 0x20_2020);
    text(&format!("Superheaters: {elements}"), 14.0, 58.0, 0x40_4040);
    text(&format!("Boil Capacity: {} mB/t", thousands(capacity)), 8.0, 72.0, 0x40_4040);
    BOILER_GRAPH.with_borrow_mut(|(last, rates, maxes, scale)| {
        if *last != game.ticks as u64 {
            *last = game.ticks as u64;
            for (v, value) in [(&mut *rates, read_wide(data, DATA_USAGE)), (&mut *maxes, wide_max(data))] {
                if v.len() >= 160 {
                    v.remove(0);
                }
                v.push(value);
            }
            *scale = rates.iter().copied().fold(*scale, u64::max);
        }
        draw_long_graph(game, batch, origin, BOILER_GRAPHS[0], rates, *scale, mouse);
        draw_long_graph(game, batch, origin, BOILER_GRAPHS[1], maxes, capacity.max(1), mouse);
    });
}

/// The graphs' tooltip for the hovered column.
fn boiler_graph_tooltip(at: [f32; 2], lines: &mut Vec<String>) {
    BOILER_GRAPH.with_borrow(|(_, rates, maxes, _)| {
        for (rect, samples, label) in [(BOILER_GRAPHS[0], rates, "Boil Rate"), (BOILER_GRAPHS[1], maxes, "Max Boil")] {
            let index = (at[0] - rect[0]).floor();
            if inside(at, rect) && index >= 0.0 && let Some(v) = samples.get(index as usize) {
                lines.push(format!("{label}: {} mB/t", thousands(*v)));
            }
        }
    });
}

/// The fission rate limit packet: the port's position and the limit as a big-endian f64.
const FISSION_CHANNEL: &str = "mekanism:fission_limit";
/// The tab (`GuiTabElementType`: at -26, 6) and its button.
const STATS_TAB_BUTTON: [f32; 4] = [-22.0, 10.0, 18.0, 18.0];
const STATS_FIELD: [f32; 4] = [77.0, 128.0, 54.0, 12.0];
const STATS_CHECK: [f32; 4] = [119.0, 128.0, 12.0, 12.0];

fn fission_stats_open(block: &str) -> bool {
    kind(block) == Some(Kind::FissionPort) && STATS_VIEW.get().is_some()
}

fn draw_stats_tab(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2], icon: &str) {
    draw_tab(game, batch, origin, 6.0, &format!("{SPRITES}{icon}"));
}

/// A `GuiTabElementType` at (-26, y): the holder and its 18 px button with the icon (a full sprite name).
fn draw_tab(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2], y: f32, icon: &str) {
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    nine_slice_tinted(game, batch, "holder_left", [26.0, 9.0], 4.0, [ox - 26.0, oy + y, 26.0, 26.0], 0xFFFF_FFFF);
    let button = [STATS_TAB_BUTTON[0], y + 4.0, 18.0, 18.0];
    let hovered = inside([cursor[0] - ox, cursor[1] - oy], button);
    let b = [ox + button[0] + 1.0, oy + button[1], 18.0, 18.0];
    nine_slice_tinted(game, batch, if hovered { "button_focus" } else { "button" }, [200.0, 20.0], 2.0, b, 0xFFFF_FFFF);
    game.native_sprite(batch, icon, [0.0, 0.0, 18.0, 18.0], b);
}

/// `GuiFissionReactorStats`: the title, the fuel statistics, the current burn rate bar and the rate limit
/// field. (The heat statistics need the heat model the port does not have yet.)
fn draw_fission_stats(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, data: &[i32], origin: [f32; 2]) {
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, 176.0, 166.0]);
    let title = game.native_screen_title().unwrap_or_default();
    centred(game, batch, &format!("{title} Statistics"), ox, 176.0, oy + 4.0, 0x40_4040);
    let get = |i: usize| data.get(i).copied().unwrap_or(0).max(0);
    let (limit, max) = (f64::from(get(2)) / 10.0, f64::from(get(3)) / 10.0);
    let active = get(1) == 1;
    let at = |y: f32| [ox + 6.0, oy + y];
    game.native_text(batch, "Fuel Statistics", at(68.0), 0x20_2020, false);
    game.native_text(batch, &format!("Max Burn Rate: {} mB/t", java_double(max)), at(80.0), 0x40_4040, false);
    game.native_text(batch, &format!("Rate Limit: {} mB/t", java_double(limit)), at(90.0), 0x40_4040, false);
    game.native_text(batch, "Current Burn Rate:", at(104.0), 0x40_4040, false);
    let current = if active { limit } else { 0.0 };
    draw_dynamic_rate_bar(game, batch, [ox + 5.0, oy + 114.0], 183.0, if max > 0.0 { (current / max).min(1.0) as f32 } else { 0.0 });
    let label = "Set Rate Limit:";
    game.native_text(batch, label, [ox + 75.0 - game.native_text_width(label), oy + 130.0], 0x40_4040, false);
    let [fx, fy, fw, fh] = STATS_FIELD;
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, [ox + fx - 1.0, oy + fy - 1.0, fw + 2.0, fh + 2.0], 0xFFFF_FFFF);
    STATS_TEXT.with_borrow(|t| {
        let text_at = [ox + fx + 2.0, oy + fy + 2.0];
        game.native_text(batch, &t.value, text_at, 0xE0_E0E0, true);
        if t.focused && (game.ticks / 6) % 2 == 0 {
            let before: String = t.value.chars().take(t.cursor).collect();
            game.native_text(batch, "_", [text_at[0] + game.native_text_width(&before), text_at[1]], 0xE0_E0E0, true);
        }
        let typed = !t.value.is_empty();
        draw_mek_button(game, batch, [ox + STATS_CHECK[0], oy + STATS_CHECK[1], STATS_CHECK[2], STATS_CHECK[3]], typed, typed && inside(mouse, STATS_CHECK));
        sprite(game, batch, "button/checkmark", [0.0, 0.0, 12.0, 12.0], [ox + STATS_CHECK[0], oy + STATS_CHECK[1], 12.0, 12.0]);
    });
    draw_stats_tab(game, batch, origin, "button/radioactive");
}

fn fission_stats_click(game: &Game<'_>, data: &[i32], at: [f32; 2], right: bool) {
    if right {
        return;
    }
    if inside(at, STATS_TAB_BUTTON) {
        STATS_VIEW.set(None);
        return;
    }
    STATS_TEXT.with_borrow_mut(|t| {
        t.focused = inside(at, STATS_FIELD) && !inside(at, STATS_CHECK);
        if inside(at, STATS_CHECK) && !t.value.is_empty() {
            send_fission_limit(game, data, &t.value.clone());
            *t = freq::TextBox::default();
        }
    });
}

fn fission_stats_key(game: &Game<'_>, key: &str, text: Option<&str>) {
    let Some((pos, _)) = STATS_VIEW.get() else {
        return;
    };
    let max = game.native_client_block_entity(pos).map_or(0, |(_, c)| get_int(c, "max_burn"));
    // Room for the largest whole rate, a point and two decimals.
    let room = (max.saturating_sub(1) / 10).to_string().len() + 3;
    STATS_TEXT.with_borrow_mut(|t| {
        if t.key(key, text, |c| c.is_ascii_digit() || c == '.', room) == freq::Typed::Enter && !t.value.is_empty() {
            let value = t.value.clone();
            send_fission_limit_at(game, pos, &value);
            *t = freq::TextBox::default();
        }
    });
}

fn send_fission_limit(game: &Game<'_>, _data: &[i32], value: &str) {
    if let Some((pos, _)) = STATS_VIEW.get() {
        send_fission_limit_at(game, pos, value);
    }
}

/// `setRateLimit`: a number from 0 up to the maximum burn rate, rounded to two decimals.
fn send_fission_limit_at(game: &Game<'_>, pos: [i32; 3], value: &str) {
    let Ok(limit) = value.parse::<f64>() else {
        return;
    };
    let mut data = Vec::new();
    for c in pos {
        data.extend(c.to_be_bytes());
    }
    data.extend(((limit * 100.0).floor() / 100.0).to_be_bytes());
    game.native_send_to_server(FISSION_CHANNEL, &data);
}

/// A player's rate limit for a reactor, within reach: tenths of a mB/t, from 0 to the maximum.
fn fission_payload(game: &mut Game<'_>, data: &[u8]) {
    let (Some(pos), Some(limit), Some(player)) = (data.get(..12), data.get(12..20), game.native_acting_player()) else {
        return;
    };
    let int = |i: usize| i32::from_be_bytes([pos[i], pos[i + 1], pos[i + 2], pos[i + 3]]);
    let pos = [int(0), int(4), int(8)];
    let near = (0..3).map(|k| (player.position[k] - f64::from(pos[k]) - 0.5).powi(2)).sum::<f64>() <= 64.0;
    let limit = f64::from_be_bytes([limit[0], limit[1], limit[2], limit[3], limit[4], limit[5], limit[6], limit[7]]);
    if !near || !limit.is_finite() || limit < 0.0 || kind(block_name(game, pos)) != Some(Kind::FissionPort) {
        return;
    }
    let Some(Some(Some(master))) = game.native_with_mod_block_entity(pos, |m| multiblock_master(m, pos)) else {
        return;
    };
    let set = |m: &mut ModBlockEntity| {
        let max = fission::Reactor { assemblies: get_long(&m.nbt, "assemblies"), volume: get_long(&m.nbt, "volume") }.max_burn_tenths();
        let tenths = (limit * 10.0).round() as u64;
        if tenths <= max {
            put_long(&mut m.nbt, "burn", tenths);
        }
    };
    if master == pos {
        game.native_with_mod_block_entity(pos, set);
    } else {
        game.native_with_mod_block_entity(master, set);
    }
}

thread_local! {
    /// The fission reactor whose statistics screen is open (its position and the tick it was last drawn).
    static STATS_VIEW: std::cell::Cell<Option<([i32; 3], u64)>> = const { std::cell::Cell::new(None) };
    static STATS_TEXT: std::cell::RefCell<freq::TextBox> = std::cell::RefCell::new(freq::TextBox::default());
    /// The fission reactor's temperature, one sample per client tick (`GuiDoubleGraph`).
    static FISSION_TEMPS: std::cell::RefCell<(u64, Vec<f64>)> = const { std::cell::RefCell::new((0, Vec::new())) };
}

/// A coloured value after its label, at the screen's 0.8 text scale.
fn labelled(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, label: &str, value: &str, at: [f32; 2], argb: u32) {
    game.native_text_scaled(batch, label, at, 0x3C_FE9A, 0.8);
    game.native_text_scaled(batch, value, [at[0] + game.native_text_width(label) * 0.8, at[1]], argb, 0.8);
}

/// `GuiFissionReactor`'s elements past the gauges: the inner screen, the two buttons and the big light,
/// the temperature bar and graph. The port keeps no heat model, so the reactor sits at the ambient 300 K
/// with no damage.
fn draw_fission(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, data: &[i32], origin: [f32; 2]) {
    let [ox, oy] = origin;
    let get = |i: usize| data.get(i).copied().unwrap_or(0).max(0);
    let status = get(1);
    let active = status == 1;
    let burn = f64::from(get(2)) / 10.0;
    let temperature = 300.0;
    // The inner screen (45, 17, 105, 56): five lines, 1 px of spacing, centred, 0.8 scale.
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, [ox + 45.0, oy + 17.0, 105.0, 56.0], 0xFFFF_FFFF);
    let line = |i: usize| [ox + 50.0, oy + 20.0 + 10.0 * i as f32];
    let (state, color) = if active { ("Active", 0xFF55_FF55) } else { ("Disabled", 0xFFFF_5555) };
    labelled(game, batch, "Status: ", state, line(0), color);
    game.native_text_scaled(batch, &format!("Burn Rate: {} mB/t", java_double(if active { burn } else { 0.0 })), line(1), 0x3C_FE9A, 0.8);
    let heating = if active { get(2) as u64 * fission::STEAM_PER_TENTH } else { 0 };
    game.native_text_scaled(batch, &format!("Heating Rate: {heating} mB/t"), line(2), 0x3C_FE9A, 0.8);
    labelled(game, batch, "Temp: ", &mekanism_core::energy::format_unit(temperature, "K"), line(3), 0xFF55_FF55);
    labelled(game, batch, "Damage: ", "0%", line(4), 0xFF55_FF55);
    // The buttons: `TranslationButton`s tinted dark green and dark red, active only when pressing them changes something.
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    for (i, (rect, label)) in buttons("mekanismgenerators:fission_reactor_port").iter().enumerate() {
        let enabled = if i == 0 { !active } else { active };
        let tint = if i == 0 { 0xFF00_AA00 } else { 0xFFAA_0000 };
        draw_button(game, batch, [ox + rect[0], oy + rect[1], rect[2], rect[3]], inside(mouse, *rect), enabled, tint, None);
        let tw = game.native_text_width(label);
        let rgb = if enabled { 0xFF_FFFF } else { 0xA0_A0A0 };
        game.native_text(batch, label, [ox + rect[0] + (rect[2] - tw) / 2.0, oy + rect[1] + (rect[3] - 8.0) / 2.0], rgb, true);
    }
    // The statistics tab (`GuiFissionReactorTab.STAT`).
    draw_stats_tab(game, batch, origin, "button/stats");
    // `GuiBigLight` (173, 76).
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, [ox + 173.0, oy + 76.0, 14.0, 14.0], 0xFFFF_FFFF);
    let light = if active { "light/big/enabled" } else { "light/big/disabled" };
    sprite(game, batch, light, [0.0, 0.0, 12.0, 12.0], [ox + 174.0, oy + 77.0, 12.0, 12.0]);
    // The temperature over `MAX_DAMAGE_TEMPERATURE` (1,800 K) and its graph.
    draw_dynamic_rate_bar(game, batch, [ox + 5.0, oy + 102.0], 183.0, (temperature / 1_800.0).min(1.0) as f32);
    let samples = FISSION_TEMPS.with_borrow_mut(|(last, v)| {
        if *last != game.ticks as u64 {
            *last = game.ticks as u64;
            if v.len() >= 183 {
                v.remove(0);
            }
            v.push(temperature);
        }
        v.clone()
    });
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, [ox + 5.0, oy + 123.0, 185.0, 38.0], 0xFFFF_FFFF);
    let scale = samples.iter().copied().fold(1_600.0_f64, f64::max);
    for (i, value) in samples.iter().enumerate() {
        let h = (value.min(scale) * 36.0 / scale) as f32;
        let (x, y) = (ox + 6.0 + i as f32, oy + 124.0 + 36.0 - h);
        game.native_fill(batch, [x, y, 1.0, h.max(1.0)], {
            let alpha = ((0.2 + 0.8 * i as f32 / samples.len() as f32) * 255.0) as u32;
            alpha << 24 | 0x24AF5A
        });
        game.native_fill(batch, [x, y, 1.0, 1.0], 0xFF28_FF74);
    }
}

fn draw_dynamic_rate_bar(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, at: [f32; 2], width: f32, progress: f32) {
    let [x, y] = at;
    nine_slice(game, batch, "bar/base", 5.0, 1.0, [x, y, width, 8.0]);
    let inner = width - 2.0;
    let filled = (progress * inner).floor() as u32;
    for i in 0..filled {
        let f = i as f32 / inner;
        let lerp = |a: f32, b: f32| (a + (b - a) * f).round() as u32;
        let argb = 0xFF00_0000 | lerp(60.0, 100.0) << 16 | lerp(45.0, 30.0) << 8 | lerp(74.0, 170.0);
        game.native_sprite_tinted(batch, &format!("{SPRITES}bar/dynamic_rate"), [0.0, 0.0, 1.0, 6.0], [x + 1.0 + i as f32, y + 1.0, 1.0, 6.0], argb);
    }
}

/// Draws the extras: an arrow full while the block is lit (its `lit` property, seen from the
/// client's world), the screen and its text, the down arrows.
fn draw_extras(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, block: &str, data: &[i32], origin: [f32; 2]) {
    let e = extras(block);
    let [ox, oy] = origin;
    let lit = game.container.map(|(x, y, z)| [x, y, z]).and_then(|p| game.native_block_property(p, "lit")).as_deref() == Some("true");
    for (at, name, [w, h]) in &e.active_progress {
        let state = if lit { "full" } else { "empty" };
        sprite(game, batch, &format!("{name}/{state}"), [0.0, 0.0, *w, *h], [ox + at[0], oy + at[1], *w, *h]);
    }
    if let Some([x, y, w, h]) = e.screen {
        nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, [ox + x, oy + y, w, h], 0xFFFF_FFFF);
        for (i, line) in screen_lines(block, data).iter().enumerate() {
            // `GuiInnerScreen`'s default format is centred lines at 0.8 scale with 2 px of spacing;
            // the evaporation screen clears the spacing. The others here keep their own plain style.
            let pad = if matches!(kind(block), Some(Kind::TurbineValve | Kind::SpsPort | Kind::DigitalMiner)) { 5.0 } else { 3.0 };
            let at = |dy: f32, step: f32| [ox + x + pad, oy + y + dy + step * i as f32];
            match kind(block) {
                Some(Kind::Evaporation) => game.native_text_scaled(batch, line, at(2.0, 9.0), 0x3C_FE9A, 0.8),
                // `clearSpacing().clearFormat()`: top-aligned, 5 px in, 9 px a line.
                Some(Kind::DigitalMiner) => game.native_text_scaled(batch, line, at(0.0, 9.0), 0x3C_FE9A, 0.8),
                Some(Kind::BoilerValve) => game.native_text_scaled(batch, line, at(4.0, 11.0), 0x3C_FE9A, 0.8),
                Some(Kind::TurbineValve) => game.native_text_scaled(batch, line, at(4.0, 11.0), 0x3C_FE9A, 0.8),
                // Two or three lines centred in the 60 px screen: 14.5 and 3.5 px down from the middle.
                Some(Kind::SpsPort) => {
                    let n = screen_lines(block, data).len() as f32;
                    let top = ((60.0 - (11.0 * n - 2.0)) / 2.0).floor();
                    game.native_text_scaled(batch, line, at(top, 11.0), 0x3C_FE9A, 0.8)
                }
                _ => game.native_text(batch, line, at(3.0, 10.0), 0x3C_FE9A, false),
            }
        }
    }
    for [x, y] in &e.arrows {
        sprite(game, batch, "arrow/down", [0.0, 0.0, 8.0, 9.0], [ox + x, oy + y, 8.0, 9.0]);
    }
}

/// Mekanism's screen for a block: the base panel, its slots and the player's, its gauges, its power
/// bar or energy gauge, its progress bar, and the energy tab on the left.
pub(super) fn screen_draw(game: &Game<'_>, block: &str, data: &[i32], batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    if miner_cfg_open(block) {
        draw_miner_config(game, batch, origin);
        return;
    }
    if fission_stats_open(block) {
        draw_fission_stats(game, batch, data, origin);
        return;
    }
    if let Some(view) = fusion_view(block) {
        draw_fusion_view(game, batch, data, origin, view);
        return;
    }
    if boiler_stats_open(block) {
        draw_boiler_stats(game, batch, data, origin);
        return;
    }
    if turbine_stats_open(block) {
        draw_turbine_stats(game, batch, data, origin);
        return;
    }
    if qio_select_open(block) {
        draw_qio_select(game, batch, block, origin);
        return;
    }
    screen_draw_base(game, block, data, batch, origin);
    if is_qio_tile(block) {
        draw_qio_tab(game, batch, origin);
    }
}

fn screen_draw_base(game: &Game<'_>, block: &str, data: &[i32], batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    if is_freq_screen(block) {
        draw_frequency_screen(game, batch, block, data, origin);
        return;
    }
    if is_sorter(block) {
        draw_sorter_screen(game, batch, origin);
        return;
    }
    if is_qio_filter(block) {
        draw_qio_filter_screen(game, batch, block, origin);
        return;
    }
    if is_qio_screen(block) {
        draw_qio_screen(game, batch, block, origin);
        return;
    }
    if kind(block) == Some(Kind::PersonalStorage) {
        draw_storage_screen(game, batch, origin);
        return;
    }
    if kind(block) == Some(Kind::QioRedstoneAdapter) {
        draw_adapter_screen(game, batch, origin);
        return;
    }
    let Some(l) = layout(block) else {
        return;
    };
    let [ox, oy] = origin;
    let get = |i: usize| data.get(i).copied().unwrap_or(0).max(0) as f32;
    let width = screen_width(block);
    let inv_dx = ((width - 176.0) / 2.0).floor();
    let height = screen_height(block);
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, width, height]);
    if let Some(Kind::Factory(n, ..)) = kind(block) {
        draw_factory_slots(game, batch, block, n, &l, data, origin);
    }
    // `GuiSideHolder.create(imageWidth, 66, 57, false, true, ...)`: the washer's holder for its fluid slots.
    if kind(block) == Some(Kind::Washer) {
        nine_slice_tinted(game, batch, "holder_right", [26.0, 9.0], 4.0, [ox + 176.0, oy + 66.0, 26.0, 57.0], 0xFFFF_FFFF);
    }
    // The armor holder is drawn behind its slots (`GuiSideHolder`, slot holder): 26 wide, 98 tall, and a slot
    // frame for each armor slot and the off hand (`GuiSlot` NORMAL at the slot's position less one).
    if let Some((at, left, first)) = armor_sidebar(block) {
        let holder = if left { "holder_left" } else { "holder_right" };
        nine_slice_tinted(game, batch, holder, [26.0, 9.0], 4.0, [ox + at[0], oy + at[1], 26.0, 98.0], 0xFFFF_FFFF);
        for i in 0..5 {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + first[0] - 1.0, oy + first[1] + 18.0 * i as f32 - 1.0, 18.0, 18.0]);
        }
    }
    let warn = warning_bits(block, &l, data);
    let extra = extra_slots(block);
    let slots = l.slots.iter().enumerate().filter_map(|(i, s)| s.as_ref().map(|s| (i, s))).chain(extra.iter().map(|s| (9, s)));
    for (i, (at, name)) in slots {
        // `GuiSlot.warning`: the input slot without a recipe or enough input, the output slot without room.
        let name = if (i == 0 && warn & 2 != 0) || (i == 2 && warn & 4 != 0) { &"slot/warning" } else { name };
        sprite(game, batch, name, [0.0, 0.0, 18.0, 18.0], [ox + at[0] - 1.0, oy + at[1] - 1.0, 18.0, 18.0]);
    }
    if kind(block) == Some(Kind::Nucleosynthesizer) {
        let title = game.native_screen_title().unwrap_or_default();
        centred(game, batch, &title, ox, width, oy + 4.0, 0x40_4040);
        game.native_text(batch, "Inventory", [ox + 8.0, oy + height - 93.0], 0x40_4040, false);
    }
    if kind(block) == Some(Kind::FissionPort) {
        // `inventoryLabelX -= 2`, `inventoryLabelY += 2`, `titleLabelY -= 1`.
        let title = game.native_screen_title().unwrap_or_default();
        centred(game, batch, &title, ox, width, oy + 3.0, 0x40_4040);
        game.native_text(batch, "Inventory", [ox + 6.0, oy + height - 92.0], 0x40_4040, false);
        game.native_text(batch, "Temperature:", [ox + 5.0, oy + 93.0], 0x40_4040, false);
        game.native_text(batch, "Heat Graph:", [ox + 5.0, oy + 114.0], 0x40_4040, false);
    }
    if kind(block) == Some(Kind::SpsPort) {
        // `inventoryLabelY += 2`.
        let title = game.native_screen_title().unwrap_or_default();
        centred(game, batch, &title, ox, width, oy + 4.0, 0x40_4040);
        game.native_text(batch, "Inventory", [ox + 8.0, oy + height - 92.0], 0x40_4040, false);
    }
    if kind(block) == Some(Kind::TurbineValve) {
        let title = game.native_screen_title().unwrap_or_default();
        centred(game, batch, &title, ox, width, oy + 3.0, 0x40_4040);
        game.native_text(batch, "Inventory", [ox + 15.0, oy + height - 92.0], 0x40_4040, false);
    }
    if kind(block) == Some(Kind::BoilerValve) {
        // `titleLabelY -= 1`, the inventory label 21 px in and 2 px down.
        let title = game.native_screen_title().unwrap_or_default();
        centred(game, batch, &title, ox, width, oy + 3.0, 0x40_4040);
        game.native_text(batch, "Inventory", [ox + 29.0, oy + height - 92.0], 0x40_4040, false);
    }
    if kind(block) == Some(Kind::Evaporation) {
        // `renderTitleTextWithOffset(inputGauge.getRelativeRight(), OUTPUT_GAUGE_X)`: centred between the two
        // gauges; the inventory label is 10 px in and 2 px down.
        let title = game.native_screen_title().unwrap_or_default();
        centred(game, batch, &title, ox + 22.0, 150.0, oy + 4.0, 0x40_4040);
        game.native_text(batch, "Inventory", [ox + 18.0, oy + height - 92.0], 0x40_4040, false);
    }
    for row in 0..4 {
        let y = if row < 3 { height - 82.0 + 18.0 * row as f32 } else { height - 24.0 };
        for col in 0..9 {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 7.0 + inv_dx + 18.0 * col as f32, oy + y - 1.0, 18.0, 18.0]);
        }
    }
    let gauge = |batch: &mut horizon_client::hud::HudBatch, at: [f32; 2], size: [f32; 2], fill: f32, (texture, argb): (&str, u32)| {
        let [x, y] = [ox + at[0], oy + at[1]];
        nine_slice(game, batch, "gauge/normal", 5.0, 2.0, [x, y, size[0], size[1]]);
        let h = (size[1] - 2.0) * fill.clamp(0.0, 1.0);
        if h > 0.0 {
            fill_tiled(game, batch, texture, [x + 1.0, y + size[1] - 1.0 - h, size[0] - 2.0, h], argb);
        }
        let overlay = match size {
            SMALL => "small",
            SMALL_MED => "small_med",
            WIDE => "wide",
            _ => "standard",
        };
        sprite(game, batch, &format!("gauge/overlay/{overlay}"), [0.0, 0.0, size[0] - 2.0, size[1] - 2.0], [x + 1.0, y + 1.0, size[0] - 2.0, size[1] - 2.0]);
    };
    for (i, g) in l.gauges.iter().enumerate().filter(|(_, g)| !matches!(kind(block), Some(Kind::FusionPort | Kind::ChemicalTank(_))) && !g.key.starts_with("stats_")) {
        let at = DATA_GAUGES + i * GAUGE_SLOTS;
        let (amount, capacity) = (read_wide(data, at), read_wide(data, at + 4).max(1));
        let chemical = usize::try_from(data.get(at + 8).copied().unwrap_or(-1)).ok().and_then(|c| CHEMICALS.get(c));
        let (texture, argb) = gauge_texture(chemical);
        gauge(batch, g.at, g.size, amount as f32 / capacity as f32, (&texture, argb));
    }
    // The stored energy and capacity, else the furnace's first two slots.
    let capacity = read_wide(data, DATA_CAPACITY);
    let energy = if capacity > 0 {
        read_wide(data, DATA_STORED) as f32 / capacity as f32
    } else {
        (get(0) / get(1).max(1.0)).clamp(0.0, 1.0)
    };
    match l.power {
        Some(p @ PowerBar::Vertical(_)) => {
            let [x, y, w, h] = power_rect(p);
            nine_slice(game, batch, "bar/base", 5.0, 1.0, [ox + x, oy + y, w, h]);
            // `GuiBar.warning`: the warning background behind the contents (not enough energy).
            if warn & 1 != 0 {
                fill_tiled(game, batch, &format!("{SPRITES}warning_background"), [ox + x + 1.0, oy + y + 1.0, w - 2.0, h - 2.0], 0xFFFF_FFFF);
            }
            let filled = energy * (h - 2.0);
            if filled > 0.0 {
                sprite(game, batch, "bar/vertical_power", [0.0, 0.0, 2.0, 2.0], [ox + x + 1.0, oy + y + h - 1.0 - filled, w - 2.0, filled]);
            }
        }
        Some(p @ PowerBar::Horizontal(_)) => {
            let [x, y, w, h] = power_rect(p);
            nine_slice(game, batch, "bar/base", 5.0, 1.0, [ox + x, oy + y, w, h]);
            if warn & 1 != 0 {
                fill_tiled(game, batch, &format!("{SPRITES}warning_background"), [ox + x + 1.0, oy + y + 1.0, w - 2.0, h - 2.0], 0xFFFF_FFFF);
            }
            let filled = energy * (w - 2.0);
            if filled > 0.0 {
                sprite(game, batch, "bar/horizontal_power", [0.0, 0.0, 2.0, 2.0], [ox + x + 1.0, oy + y + 1.0, filled, h - 2.0]);
            }
        }
        // `GuiEnergyGauge` fills with Mekanism's energy texture.
        Some(PowerBar::Gauge(at)) => gauge(batch, at, WIDE, energy, (&format!("{SPRITES_ROOT}gauge_fluid/energy"), 0xFFFF_FFFF)),
        Some(PowerBar::SmallGauge(at)) => gauge(batch, at, SMALL_MED, energy, (&format!("{SPRITES_ROOT}gauge_fluid/energy"), 0xFFFF_FFFF)),
        None => {}
    }
    if kind(block) == Some(Kind::Nucleosynthesizer) {
        draw_nucleosynthesizer(game, batch, data, origin);
    }
    if let Some((at, name, [w, h])) = l.progress {
        sprite(game, batch, &format!("{name}/empty"), [0.0, 0.0, w, h], [ox + at[0], oy + at[1], w, h]);
        // `GuiProgress`: with `INPUT_DOESNT_PRODUCE_OUTPUT` it draws its warning texture, full.
        if warning_bits(block, &l, data) & 8 != 0 {
            sprite(game, batch, &format!("{name}/warning"), [0.0, 0.0, w, h], [ox + at[0], oy + at[1], w, h]);
        }
        let done = if warning_bits(block, &l, data) & 8 != 0 { 0.0 } else { (get(2) / get(3).max(1.0)).clamp(0.0, 1.0) * w };
        if done > 0.0 {
            sprite(game, batch, &format!("{name}/full"), [0.0, 0.0, done, h], [ox + at[0], oy + at[1], done, h]);
        }
    }
    if l.power.is_some() {
        sprite(game, batch, "tab/energy_info", [0.0, 0.0, 26.0, 26.0], [ox - 26.0, oy + 137.0, 26.0, 26.0]);
    }
    // The heaters' heat tab (`GuiHeatTab`, in kelvin) above the energy tab.
    if matches!(kind(block), Some(Kind::Heater | Kind::FuelwoodHeater | Kind::Evaporation | Kind::BoilerValve)) {
        sprite(game, batch, "tab/heat_info_k", [0.0, 0.0, 26.0, 26.0], [ox - 26.0, oy + 109.0, 26.0, 26.0]);
    }
    draw_extras(game, batch, block, data, origin);
    if is_od(block) {
        draw_od_list(game, batch, origin);
    }
    if let Some(Kind::ChemicalTank(t)) = kind(block) {
        draw_tank_screen(game, batch, &l, data, origin, t == BaseTier::Creative);
    }
    draw_warning_tab(game, batch, block, &l, data, origin, width);
    if kind(block) == Some(Kind::FissionPort) && get(0) > 0.0 {
        draw_fission(game, batch, data, origin);
    }
    if kind(block) == Some(Kind::FusionPort) && get(0) > 0.0 {
        draw_fusion_main(game, batch, origin, width, height);
    }
    if kind(block) == Some(Kind::SpsPort) && get(0) > 0.0 {
        draw_dynamic_rate_bar(game, batch, [ox + 7.0, oy + 79.0], 160.0, (get(2) / 1000.0).clamp(0.0, 1.0));
    }
    if kind(block) == Some(Kind::TurbineValve) && get(0) > 0.0 {
        draw_stats_tab(game, batch, origin, "button/stats");
        // The steam input's `GuiVerticalRateBar` at (40, 13).
        let flow = (data.get(1).copied().unwrap_or(0) as u64 & 0x7FFF) | (data.get(2).copied().unwrap_or(0) as u64 & 0x7FFF) << 15;
        let max = (data.get(DATA_SPEED).copied().unwrap_or(0) as u64 & 0x7FFF) | (data.get(DATA_ENERGY_UPGRADES).copied().unwrap_or(0) as u64 & 0x7FFF) << 15;
        nine_slice(game, batch, "bar/base", 5.0, 1.0, [ox + 40.0, oy + 13.0, 6.0, 58.0]);
        let level = if max == 0 { 0.0 } else { (flow as f64 / max as f64).min(1.0) };
        let h = (level * 56.0).floor() as f32;
        if h > 0.0 {
            sprite(game, batch, "bar/vertical_rate", [0.0, 56.0 - h, 4.0, h], [ox + 41.0, oy + 14.0 + 56.0 - h, 4.0, h]);
        }
    }
    if kind(block) == Some(Kind::BoilerValve) && get(0) > 0.0 {
        draw_stats_tab(game, batch, origin, "button/stats");
        // The two `GuiVerticalRateBar`s: the boil rate over the max, and the max over what the elements carry.
        let boil = read_wide(data, DATA_USAGE) as f64;
        let max = ((data.get(DATA_SPEED).copied().unwrap_or(0) as u64 & 0x7FFF) | (data.get(DATA_ENERGY_UPGRADES).copied().unwrap_or(0) as u64 & 0x7FFF) << 15) as f64;
        let elements = get(2) as u64;
        let level2 = if elements == 0 { 0.0 } else { max * 10.0 / (elements as f64 * 16_000_000.0) };
        for (bx, level) in [(44.0, if max > 0.0 { boil / max } else { 0.0 }), (166.0, level2)] {
            nine_slice(game, batch, "bar/base", 5.0, 1.0, [ox + bx, oy + 13.0, 6.0, 58.0]);
            let h = (level.clamp(0.0, 1.0) * 56.0).floor() as f32;
            if h > 0.0 {
                sprite(game, batch, "bar/vertical_rate", [0.0, 56.0 - h, 4.0, h], [ox + bx + 1.0, oy + 14.0 + 56.0 - h, 4.0, h]);
            }
        }
    }
    if kind(block) == Some(Kind::Evaporation) {
        // `GuiHorizontalRateBar` at (58, 62): the temperature over 3,000 K.
        let (x, y) = (ox + 58.0, oy + 62.0);
        nine_slice(game, batch, "bar/base", 5.0, 1.0, [x, y, 78.0, 8.0]);
        let level = (f64::from(get(2)) / 10.0 / evap::MAX_MULTIPLIER_TEMP).clamp(0.0, 1.0);
        let w = (level * 76.0).floor() as f32;
        if w > 0.0 {
            sprite(game, batch, "bar/horizontal_rate", [0.0, 0.0, w, 6.0], [x + 1.0, y + 1.0, w, 6.0]);
        }
    }
    if kind(block) == Some(Kind::DigitalMiner) {
        draw_miner_widgets(game, batch, block, data, origin);
    }
    draw_tabs(game, batch, block, &l, data, origin);
    let text_x = if l.gauges.len() > 2 { 50.0 } else { 30.0 };
    for (i, line) in multiblock_lines(block, data).iter().enumerate() {
        game.native_text(batch, line, [ox + text_x, oy + 16.0 + 10.0 * i as f32], 0x40_4040, false);
    }
    if kind(block) == Some(Kind::TurbineValve) && data.first().copied().unwrap_or(0) > 0 {
        let mode = data.get(3).copied().unwrap_or(0);
        let (sprite_name, text) = match mode {
            1 => ("chemical_mode/dump_excess", "Dumping Excess"),
            2 => ("chemical_mode/dump", "Dumping"),
            _ => ("chemical_mode/idle", "Idle"),
        };
        sprite(game, batch, sprite_name, [0.0, 0.0, 10.0, 10.0], [ox + 173.0, oy + 72.0, 10.0, 10.0]);
        // The mode's name right-aligned against the button (`TextAlignment.RIGHT`, 69 wide, 2 px of padding).
        let tw = game.native_text_width(text);
        game.native_text(batch, text, [ox + 171.0 - tw, oy + 73.0], 0x40_4040, false);
    }
    for ([x, y, w, h], label) in buttons(block) {
        if matches!(kind(block), Some(Kind::TurbineValve | Kind::FissionPort | Kind::ChemicalTank(_) | Kind::DigitalMiner)) {
            continue;
        }
        game.native_fill(batch, [ox + x, oy + y, *w, *h], 0xFF00_0000);
        game.native_fill(batch, [ox + x + 1.0, oy + y + 1.0, w - 2.0, h - 2.0], 0xFF6F_6F6F);
        let tw = game.native_text_width(label);
        game.native_text(batch, label, [ox + x + (w - tw) / 2.0, oy + y + (h - 8.0) / 2.0], 0xFF_FFFF, true);
    }
}

fn inside(at: [f32; 2], rect: [f32; 4]) -> bool {
    at[0] >= rect[0] && at[0] < rect[0] + rect[2] && at[1] >= rect[1] && at[1] < rect[1] + rect[3]
}

/// The tooltips: a gauge's chemical and amount, the power bar's stored and maximum energy, the
/// progress bar's percent, and the energy tab's usage and upgrades.
/// `GuiChemicalTank`: the horizontal bar, the inner screen with the chemical and the amount, and the dump mode
/// button with its name.
fn draw_tank_screen(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, l: &Layout, data: &[i32], origin: [f32; 2], creative: bool) {
    let [ox, oy] = origin;
    let (amount, capacity, chemical) = fusion_gauge(data, 0);
    nine_slice(game, batch, "bar/base", 5.0, 1.0, [ox + 42.0, oy + 16.0, 116.0, 10.0]);
    let filled = (amount as f64 / capacity as f64 * 114.0).floor() as f32;
    if filled > 0.0 {
        let (texture, argb) = gauge_texture(chemical);
        fill_tiled(game, batch, &texture, [ox + 43.0, oy + 17.0, filled, 8.0], argb);
    }
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, [ox + 42.0, oy + 37.0, 118.0, 28.0], 0xFFFF_FFFF);
    let name = if amount == 0 { "None" } else { chemical.map_or("None", |c| c.1) };
    let total = if creative { "Infinite".to_owned() } else { thousands(capacity) };
    let count = if amount == 0 { "0".to_owned() } else { thousands(amount) };
    let lines = [format!("Chemical: {name}"), if creative && amount > 0 { "Infinite".to_owned() } else { format!("{count}/{total}") }];
    for (i, line) in lines.iter().enumerate() {
        game.native_text_scaled(batch, line, [ox + 47.0, oy + 41.0 + 11.0 * i as f32], 0x3C_FE9A, 0.8);
    }
    let mode = (data.get(redstone_slot(l)).copied().unwrap_or(0).max(0) >> 4) & 3;
    let (icon, text) = match mode {
        1 => ("chemical_mode/dump_excess", "Dumping Excess"),
        2 => ("chemical_mode/dump", "Dumping"),
        _ => ("chemical_mode/idle", "Idle"),
    };
    sprite(game, batch, icon, [0.0, 0.0, 10.0, 10.0], [ox + 159.0, oy + 72.0, 10.0, 10.0]);
    game.native_text(batch, text, [ox + 157.0 - game.native_text_width(text), oy + 73.0], 0x40_4040, false);
}

/// The machine's issue bits (`RecipeError`s) from its data: 0 energy, 1 input, 2 output room, 3 no recipe.
fn warning_bits(block: &str, l: &Layout, data: &[i32]) -> u32 {
    if tabs(block) & TAB_REDSTONE == 0 {
        return 0;
    }
    if let Some(Kind::Factory(n, ..)) = kind(block) {
        return (0..n).fold(0, |all, i| all | (data.get(redstone_slot(l) + 5 + i).copied().unwrap_or(0).max(0) as u32) >> 5 & 15);
    }
    if kind(block) == Some(Kind::DigitalMiner) {
        return (data.get(redstone_slot(l)).copied().unwrap_or(0).max(0) as u32) >> 4 & 15;
    }
    if kind(block).is_some_and(|k| base_usage(k).is_some()) {
        return (data.get(redstone_slot(l)).copied().unwrap_or(0).max(0) as u32) >> 4 & 15;
    }
    if !matches!(kind(block), Some(Kind::Machine(..))) {
        return 0;
    }
    (data.get(redstone_slot(l)).copied().unwrap_or(0).max(0) as u32) >> 4
}

/// `GuiWarningTab`: the issues listed by the warning tab, in `WarningType` order.
fn warnings(block: &str, l: &Layout, data: &[i32]) -> Vec<&'static str> {
    if tabs(block) & TAB_REDSTONE == 0 {
        return vec![];
    }
    let bits = warning_bits(block, l, data);
    let (control, now, last) = redstone_of(l, data);
    let mut out = vec![];
    for (bit, text) in [
        (3, " - Input does not produce output"),
        (1, " - No matching recipe or not enough input"),
        (2, " - Not enough room in output"),
        (0, " - Not enough energy to operate"),
    ] {
        if bits & (1 << bit) != 0 {
            out.push(text);
        }
    }
    match control {
        RedstoneControl::High if !now => out.push(" - Redstone signal absent, provide one to activate."),
        RedstoneControl::Low if now => out.push(" - Redstone signal present, remove it to activate."),
        RedstoneControl::Pulse if !(now && !last) => out.push(" - Redstone pulse required to activate."),
        _ => {}
    }
    out
}

/// The warning tab: left of the screen at 109, on the right where a heat tab takes the left.
fn warning_rect(block: &str, width: f32) -> [f32; 4] {
    if matches!(kind(block), Some(Kind::Heater | Kind::FuelwoodHeater)) { [width, 109.0, 26.0, 26.0] } else { [-26.0, 109.0, 26.0, 26.0] }
}

fn draw_warning_tab(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, block: &str, l: &Layout, data: &[i32], origin: [f32; 2], width: f32) {
    if warnings(block, l, data).is_empty() {
        return;
    }
    let r = warning_rect(block, width);
    let side = if r[0] < 0.0 { "left" } else { "right" };
    sprite(game, batch, &format!("tab/warning_info_{side}"), [0.0, 0.0, 26.0, 26.0], [origin[0] + r[0], origin[1] + r[1], 26.0, 26.0]);
}

fn warning_tooltip(block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) -> bool {
    let Some(l) = layout(block) else {
        return false;
    };
    let list = warnings(block, &l, data);
    if list.is_empty() || !inside(at, warning_rect(block, screen_width(block))) {
        return false;
    }
    lines.push("§eIssues:".to_owned());
    lines.extend(list.iter().map(|t| (*t).to_owned()));
    true
}

pub(super) fn screen_tooltip(game: &Game<'_>, block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) {
    if miner_cfg_open(block) {
        miner_cfg_tooltip(game, at, lines);
        return;
    }
    if !qio_select_open(block) && warning_tooltip(block, data, at, lines) {
        return;
    }
    if qio_select_open(block) {
        if inside(at, QIO_BACK) {
            lines.push("Back".to_owned());
        } else if let Some(i) = FREQ_MODES.iter().position(|r| inside([at[0], at[1] - QIO_SELECT_DY], *r)) {
            lines.push(["Public Mode", "Private Mode", "Trusted Mode"][i].to_owned());
        }
        return;
    }
    if is_qio_tile(block) && inside(at, QIO_TAB_BUTTON) {
        lines.push("Set Frequency".to_owned());
        return;
    }
    if kind(block) == Some(Kind::FusionPort) && fusion_tooltip(block, data, at, lines) {
        return;
    }
    if kind(block) == Some(Kind::TurbineValve) && turbine_stats_open(block) {
        if inside(at, STATS_TAB_BUTTON) {
            lines.push("Main".to_owned());
        } else if inside(at, [-26.0, 137.0, 26.0, 26.0]) {
            lines.push(format!("Storing: {}", mekanism_core::energy::format_fe(read_wide(data, DATA_STORED))));
        }
        return;
    }
    if kind(block) == Some(Kind::TurbineValve) && inside(at, STATS_TAB_BUTTON) {
        lines.push("Turbine Statistics".to_owned());
        return;
    }
    if kind(block) == Some(Kind::BoilerValve) {
        if inside(at, STATS_TAB_BUTTON) {
            lines.push(if boiler_stats_open(block) { "Main" } else { "Boiler Statistics" }.to_owned());
            return;
        }
        if boiler_stats_open(block) {
            boiler_graph_tooltip(at, lines);
            return;
        }
    }
    if kind(block) == Some(Kind::FissionPort) {
        if inside(at, STATS_TAB_BUTTON) {
            lines.push(if fission_stats_open(block) { "Main" } else { "Stats" }.to_owned());
            return;
        }
        if fission_stats_open(block) {
            if inside(at, [5.0, 114.0, 183.0, 8.0]) {
                let burn = f64::from(data.get(2).copied().unwrap_or(0).max(0)) / 10.0;
                let active = data.get(1).copied().unwrap_or(0) == 1;
                lines.push(format!("Burn Rate: {} mB/t", java_double(if active { burn } else { 0.0 })));
            }
            return;
        }
    }
    screen_tooltip_base(game, block, data, at, lines);
}

fn screen_tooltip_base(game: &Game<'_>, block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) {
    if frequency_tooltip(block, data, at, lines) {
        return;
    }
    if is_freq_screen(block) {
        frequency_tooltip_view(game, block, at, lines);
        return;
    }
    if is_filter_screen(block) {
        if !filter_window_tooltip(game, window_at(block, at), lines) {
            if is_qio_filter(block) {
                qio_filter_tooltip(game, block, at, lines);
            } else {
                sorter_tooltip(game, at, lines);
            }
        }
        return;
    }
    if is_od(block) && (od_edit_open() || open_window_id(block).is_none()) && od_tooltip(game, window_at(block, at), lines) {
        return;
    }
    if is_qio_screen(block) {
        qio_tooltip(game, block, at, lines);
        return;
    }
    if kind(block) == Some(Kind::QioRedstoneAdapter) {
        if inside(at, ADAPTER_INVERT) {
            lines.push("Toggle Signal".into());
        } else if inside(at, ADAPTER_FUZZY) {
            lines.push("Fuzzy Mode".into());
        }
        return;
    }
    let Some(l) = layout(block) else {
        return;
    };
    if window_tooltip(block, data, window_at(block, at), lines) || tab_tooltip(block, &l, data, at, lines) {
        return;
    }
    if kind(block) == Some(Kind::DigitalMiner) {
        if inside(at, [64.0, 21.0, 18.0, 18.0]) {
            lines.push("All is well!".to_owned());
            return;
        }
        for (rect, name) in [([19.0, 56.0, 15.0, 31.0], "Auto-eject"), ([38.0, 56.0, 15.0, 31.0], "Auto-pull"), ([57.0, 56.0, 15.0, 31.0], "Silk touch")] {
            if inside(at, rect) {
                lines.push(name.to_owned());
                return;
            }
        }
    }
    if kind(block) == Some(Kind::SpsPort) && inside(at, [7.0, 79.0, 160.0, 8.0]) {
        lines.push(format!("Progress: {:.0}%", f64::from(data.get(2).copied().unwrap_or(0).max(0)) / 10.0));
        return;
    }
    if kind(block) == Some(Kind::TurbineValve) && inside(at, [173.0, 72.0, 10.0, 10.0]) {
        match data.get(3).copied().unwrap_or(0) {
            1 => lines.push("Dumping excess Steam".to_owned()),
            2 => lines.push("Dumping Steam".to_owned()),
            _ => return,
        }
        lines.push("§cWater will NOT be recycled".to_owned());
        return;
    }
    if kind(block) == Some(Kind::TurbineValve) && inside(at, [40.0, 13.0, 6.0, 58.0]) {
        let flow = (data.get(1).copied().unwrap_or(0) as u64 & 0x7FFF) | (data.get(2).copied().unwrap_or(0) as u64 & 0x7FFF) << 15;
        lines.push(format!("Steam Input: {flow} mB/t"));
        return;
    }
    if kind(block) == Some(Kind::BoilerValve) {
        if inside(at, [44.0, 13.0, 6.0, 58.0]) {
            lines.push(format!("Boil Rate: {} mB/t", read_wide(data, DATA_USAGE)));
            return;
        }
        if inside(at, [166.0, 13.0, 6.0, 58.0]) {
            lines.push(format!("Max Boil: {} mB/t", wide_max(data)));
            return;
        }
    }
    if kind(block) == Some(Kind::Evaporation) {
        if inside(at, [-26.0, 109.0, 26.0, 26.0]) {
            let loss = mekanism_core::energy::format_unit(read_wide(data, DATA_USAGE) as f64 / 100.0, "K");
            lines.push(format!("Dissipated: {loss}/t"));
            lines.push("Unit: Kelvin".to_owned());
            return;
        }
        if inside(at, [58.0, 62.0, 78.0, 8.0]) {
            lines.push(mekanism_core::energy::format_unit(f64::from(data.get(2).copied().unwrap_or(0).max(0)) / 10.0, "K"));
            return;
        }
    }
    let get = |i: usize| data.get(i).copied().unwrap_or(0).max(0);
    for (i, g) in l.gauges.iter().enumerate().filter(|(_, g)| kind(block) != Some(Kind::FusionPort) && !g.key.starts_with("stats_")) {
        if inside(at, [g.at[0], g.at[1], g.size[0], g.size[1]]) {
            let d = DATA_GAUGES + i * GAUGE_SLOTS;
            let amount = read_wide(data, d);
            let name = usize::try_from(data.get(d + 8).copied().unwrap_or(-1)).ok().and_then(|c| CHEMICALS.get(c)).map(|c| c.1);
            lines.push(match (amount, name) {
                (0, _) => "Empty".to_owned(),
                (_, Some(name)) => format!("{name}: {amount} mB"),
                (_, None) => format!("{amount} mB"),
            });
            lines.push(format!("§7Capacity: {} mB", read_wide(data, d + 4)));
            return;
        }
    }
    if l.power.is_some_and(|p| inside(at, power_rect(p))) {
        lines.push(format!("{} / {} FE", read_wide(data, DATA_STORED), read_wide(data, DATA_CAPACITY)));
    } else if let Some((p, _, [w, h])) = l.progress
        && inside(at, [p[0], p[1], w, h])
    {
        lines.push(format!("{}%", get(2) * 100 / get(3).max(1)));
    } else if l.power.is_some() && inside(at, [-26.0, 137.0, 26.0, 26.0]) {
        let usage = read_wide(data, DATA_USAGE);
        if usage > 0 {
            lines.push(format!("Using: {usage} FE/t"));
        }
        lines.push(format!("Stored: {} FE", read_wide(data, DATA_STORED)));
        if matches!(kind(block), Some(Kind::Machine(..))) {
            lines.push(format!("§7Speed upgrades: {}/8", get(DATA_SPEED)));
            lines.push(format!("§7Energy upgrades: {}/8", get(DATA_ENERGY_UPGRADES)));
        }
    }
}

// --- Teleporters ------------------------------------------------------------------------------------

const FREQUENCY_KEY: &str = "mekanism:frequency";

/// The teleporters this server has seen, by frequency: their dimension and position. A teleporter
/// adds itself once a second; an entry is checked against the world before anyone travels to it.
static TELEPORTERS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, Vec<(super::Dim, [i32; 3])>>>> =
    std::sync::LazyLock::new(Default::default);

fn get_string(nbt: &Compound, key: &str) -> Option<String> {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::String(s)) => s.to_string().ok().filter(|s| !s.is_empty()),
        _ => None,
    }
}

/// A teleporter: once a second it looks for its frame (`getFrameDirection`) and puts itself on its
/// frequency.
fn tick_teleporter(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    if ticker != 0 {
        return;
    }
    let dir = teleporter::frame_direction(|o| block_name(game, [pos[0] + o[0], pos[1] + o[1], pos[2] + o[2]]) == teleporter::FRAME);
    match dir {
        Some(d) => {
            entity.nbt.insert("dir".into(), Tag::IntArray(d.to_vec()));
        }
        None => {
            entity.nbt.remove(&JavaString::from("dir"));
        }
    }
    let dim = game.dim;
    if let Ok(mut all) = TELEPORTERS.lock() {
        for list in all.values_mut() {
            list.retain(|&(d, p)| (d, p) != (dim, pos));
        }
        if let Some(f) = get_string(&entity.nbt, "frequency") {
            all.entry(f).or_default().push((dim, pos));
        }
    }
    let status = teleporter_status(game, pos, entity);
    put_int(&mut entity.nbt, "status", status);
}

/// The closest other formed teleporter on `frequency` in this dimension, and where a player arrives:
/// the block in front of it, toward its frame.
fn teleport_target(game: &mut Game<'_>, frequency: &str, from: [f64; 3], not: Option<[i32; 3]>) -> Option<([i32; 3], [f64; 3])> {
    let dim = game.dim;
    let mut list: Vec<[i32; 3]> = TELEPORTERS
        .lock()
        .ok()?
        .get(frequency)?
        .iter()
        .filter(|&&(d, p)| d == dim && Some(p) != not)
        .map(|&(_, p)| p)
        .collect();
    let dist = |p: &[i32; 3]| (0..3).map(|k| (from[k] - f64::from(p[k])).powi(2)).sum::<f64>();
    list.sort_by(|a, b| dist(a).total_cmp(&dist(b)));
    list.into_iter().find_map(|p| {
        let dir = game.native_with_mod_block_entity(p, |m| {
            (kind(&m.id) == Some(Kind::Teleporter) && get_string(&m.nbt, "frequency").as_deref() == Some(frequency))
                .then(|| get_pos(&m.nbt, "dir"))
                .flatten()
        })??;
        let feet = [p[0] + dir[0], p[1] + dir[1], p[2] + dir[2]];
        Some((p, [f64::from(feet[0]) + 0.5, f64::from(feet[1]), f64::from(feet[2]) + 0.5]))
    })
}

fn place_player(game: &mut Game<'_>, at: [f64; 3]) {
    if let Some(p) = game.player.as_mut() {
        p.position = horizon_core::geometry::Vec3::new(at[0], at[1], at[2]);
        p.velocity = horizon_core::geometry::Vec3::new(0.0, 0.0, 0.0);
        p.fall_distance = 0.0;
    }
    game.previous_position = horizon_core::geometry::Vec3::new(at[0], at[1], at[2]);
}

fn tell(game: &mut Game<'_>, line: String) {
    if let Some(player) = game.native_acting_player() {
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
}

/// A right click on a teleporter or its frame:
/// - with a named name tag on the teleporter, sets its frequency to the name;
/// - with a portable teleporter on the teleporter, tunes the portable one to its frequency;
/// - with an empty hand, sends the player to the closest other teleporter on the frequency, paid from
///   this teleporter's energy (`TileEntityTeleporter.teleport`).
pub(super) fn use_teleporter(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) {
        return false;
    }
    let name = block_name(game, pos);
    if name != teleporter::TELEPORTER && name != teleporter::FRAME {
        return false;
    }
    let held = game.inventory.get(game.selected).cloned().flatten();
    if name == teleporter::TELEPORTER {
        if let Some(tag) = held.as_ref().filter(|s| &*s.item == "minecraft:name_tag").and_then(|s| s.custom_name()) {
            let entry = name_tag_frequency(game, tag.as_str());
            game.native_with_mod_block_entity(pos, |m| put_block_frequency(&mut m.nbt, Some(&entry)));
            tell(game, format!("Teleporter frequency: {tag}"));
            return true;
        }
        if held.as_ref().is_some_and(|s| &*s.item == teleporter::PORTABLE) {
            let f = game.native_with_mod_block_entity(pos, |m| get_string(&m.nbt, "frequency")).flatten();
            let Some(f) = f else {
                tell(game, "Teleporter: no frequency (name it with a name tag)".to_owned());
                return true;
            };
            let selected = game.selected;
            if let Some(stack) = game.inventory[selected].as_mut() {
                let stored = stored_energy(stack);
                set_energy(stack, stored);
                set_custom_string(stack, FREQUENCY_KEY, &f);
            }
            tell(game, format!("Portable Teleporter: {f}"));
            return true;
        }
    }
    // An empty hand on the teleporter opens its screen (`GuiTeleporter`); on its frame, it travels.
    if held.is_some() || name == teleporter::TELEPORTER {
        return false;
    }
    // The teleporter this block belongs to: itself, or one whose frame or opening holds it.
    let source = {
        let mut found = None;
        'search: for dx in -3..=3 {
            for dy in -3..=3 {
                for dz in -3..=3 {
                    let t = [pos[0] + dx, pos[1] + dy, pos[2] + dz];
                    if block_name(game, t) != teleporter::TELEPORTER {
                        continue;
                    }
                    let dir = game.native_with_mod_block_entity(t, |m| get_pos(&m.nbt, "dir")).flatten();
                    if let Some(d) = dir
                        && [false, true].into_iter().any(|r| teleporter::frame_offsets(d, r).contains(&[-dx, -dy, -dz]))
                    {
                        found = Some(t);
                        break 'search;
                    }
                }
            }
        }
        found
    };
    let Some(source) = source else {
        return false;
    };
    let (frequency, formed, stored) = game
        .native_with_mod_block_entity(source, |m| (get_string(&m.nbt, "frequency"), get_pos(&m.nbt, "dir").is_some(), get_long(&m.nbt, "energy")))
        .unwrap_or((None, false, 0));
    if !formed {
        tell(game, "Teleporter: no frame".to_owned());
        return true;
    }
    let Some(frequency) = frequency else {
        tell(game, "Teleporter: no frequency (name it with a name tag)".to_owned());
        return true;
    };
    let Some(from) = game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]) else {
        return false;
    };
    let Some((target, at)) = teleport_target(game, &frequency, from, Some(source)) else {
        tell(game, "Teleporter: no link found".to_owned());
        return true;
    };
    let cost = teleporter::energy_cost(from, target, true);
    if stored < cost {
        tell(game, format!("Teleporter: needs {cost} FE"));
        return true;
    }
    game.native_with_mod_block_entity(source, |m| put_long(&mut m.nbt, "energy", stored - cost));
    place_player(game, at);
    true
}

/// A right click with the Portable Teleporter: to the closest teleporter on its frequency, paid from
/// its own energy (`ItemPortableTeleporter`).
fn portable_teleporter_use(game: &mut Game<'_>, claimed: &mut bool) {
    let Some(held) = game.held_mut().clone().filter(|s| &*s.item == teleporter::PORTABLE) else {
        return;
    };
    *claimed = true;
    let frequency = custom_data(&held).and_then(|c| get_string(c, FREQUENCY_KEY));
    let Some(frequency) = frequency else {
        tell(game, "Portable Teleporter: no frequency (right click a teleporter with it)".to_owned());
        return;
    };
    let Some(from) = game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]) else {
        return;
    };
    let Some((target, at)) = teleport_target(game, &frequency, from, None) else {
        tell(game, "Portable Teleporter: no link found".to_owned());
        return;
    };
    let cost = teleporter::energy_cost(from, target, true);
    let stored = stored_energy(&held);
    if stored < cost {
        tell(game, format!("Portable Teleporter: needs {cost} FE"));
        return;
    }
    if let Some(stack) = game.held_mut().as_mut() {
        set_energy(stack, stored - cost);
    }
    place_player(game, at);
}

// --- Dynamic Tank -----------------------------------------------------------------------------------

/// `TankValidator`: the casing, valves and glass joined to `start` make the shell of a box 3 to 18 on
/// each side, its edges casing, with air inside. The box's volume and its valves.
fn find_dynamic_tank(game: &Game<'_>, start: [i32; 3]) -> Option<(u64, Vec<[i32; 3]>)> {
    let (lo, hi, seen) = find_box(game, start, dynamic_tank::is_frame, dynamic_tank::MIN_SIZE, dynamic_tank::MAX_SIZE)?;
    let size = [0, 1, 2].map(|k| hi[k] - lo[k] + 1);
    let mut valves = Vec::new();
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                let at = [x, y, z];
                let rel = [x - lo[0], y - lo[1], z - lo[2]];
                let shell = (0..3).any(|k| rel[k] == 0 || rel[k] == size[k] - 1);
                let name = block_name(game, at);
                if shell {
                    if !seen.contains(&at) || (dynamic_tank::on_edge(rel, size) && name != dynamic_tank::CASING) {
                        return None;
                    }
                    if name == dynamic_tank::VALVE {
                        valves.push(at);
                    }
                } else if !matches!(name, "minecraft:air" | "minecraft:cave_air") {
                    return None;
                }
            }
        }
    }
    let volume = size.iter().map(|&s| s as u64).product();
    (!valves.is_empty()).then_some((volume, valves))
}

/// Whether a valve's buffer takes `chemical` from a pipe or tube: input mode, and the buffer empty or
/// holding it.
fn valve_takes(nbt: &Compound, chemical: &str) -> bool {
    get_long(nbt, "output") == 0 && get_tank(nbt, "buffer", dynamic_tank::VALVE_BUFFER).accepts(chemical)
}

/// A Dynamic Tank's valve. Once a second each valve checks the tank; the first valve holds it, takes in
/// what the other valves' buffers caught, and gives out through valves in output mode.
fn tick_dynamic_valve(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    if ticker == 0 {
        match find_dynamic_tank(game, pos) {
            Some((volume, valves)) => {
                put_long(&mut entity.nbt, "volume", volume);
                entity.nbt.insert("master".into(), Tag::IntArray(valves[0].to_vec()));
                entity.nbt.insert("valves".into(), Tag::IntArray(valves.concat()));
            }
            None => {
                put_long(&mut entity.nbt, "volume", 0);
                entity.nbt.remove(&JavaString::from("master"));
            }
        }
    }
    if get_pos(&entity.nbt, "master") != Some(pos) {
        return;
    }
    let mut tank = get_tank(&entity.nbt, "fluid", dynamic_tank::capacity(get_long(&entity.nbt, "volume")));
    // Each valve's buffer into the tank.
    let drain = |nbt: &mut Compound, tank: &mut ChemicalTank| {
        let mut buffer = get_tank(nbt, "buffer", dynamic_tank::VALVE_BUFFER);
        if let Some(id) = buffer.chemical.clone()
            && tank.accepts(&id)
        {
            let n = buffer.amount.min(tank.capacity - tank.amount);
            buffer.extract(n);
            tank.insert(&id, n);
            put_tank(nbt, "buffer", &buffer);
        }
        get_long(nbt, "output") == 1
    };
    let mut outputs = Vec::new();
    for p in positions(&entity.nbt, "valves") {
        let out = if p == pos {
            drain(&mut entity.nbt, &mut tank)
        } else {
            game.native_with_mod_block_entity(p, |m| drain(&mut m.nbt, &mut tank)).unwrap_or(false)
        };
        if out {
            outputs.push(p);
        }
    }
    for p in outputs {
        if tank.amount > 0 {
            push_chemical(game, p, &mut tank, None, u64::MAX, false);
        }
    }
    put_tank(&mut entity.nbt, "fluid", &tank);
}

/// A sneaking right click with an empty hand on a dynamic valve switches it between filling the tank
/// and emptying it into the pipes and tubes beside it; a plain one opens the tank's screen.
pub(super) fn use_dynamic_valve(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID)
        || game.inventory.get(game.selected).cloned().flatten().is_some()
        || !game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft)
    {
        return false;
    }
    let output = game.native_with_mod_block_entity(pos, |m| {
        (kind(&m.id) == Some(Kind::DynamicValve)).then(|| {
            let output = get_long(&m.nbt, "output") == 0;
            put_long(&mut m.nbt, "output", u64::from(output));
            output
        })
    });
    let Some(Some(output)) = output else {
        return false;
    };
    let line = format!("Dynamic Valve: {}", if output { "output" } else { "input" });
    if let Some(player) = game.native_acting_player() {
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
    true
}

// --- Supercritical Phase Shifter ---------------------------------------------------------------------

/// `SPSValidator`: the box of casing and ports joined to `start`, 7 on each side, with air and coils
/// inside, every coil against a port. Its low corner, its ports, and the ports with a coil behind them.
#[allow(clippy::type_complexity)]
fn find_sps(game: &Game<'_>, start: [i32; 3]) -> Option<(Vec<[i32; 3]>, Vec<[i32; 3]>)> {
    let (lo, _, seen) = find_box(game, start, sps::is_frame, sps::SIZE, sps::SIZE)?;
    let mut ports = Vec::new();
    let mut coils = 0;
    for x in 0..sps::SIZE {
        for y in 0..sps::SIZE {
            for z in 0..sps::SIZE {
                let at = [lo[0] + x, lo[1] + y, lo[2] + z];
                let name = block_name(game, at);
                match sps::requirement([x, y, z]) {
                    None if name == sps::COIL => coils += 1,
                    None if !matches!(name, "minecraft:air" | "minecraft:cave_air") => return None,
                    Some(1) if name != sps::CASING || !seen.contains(&at) => return None,
                    Some(2) if !seen.contains(&at) => return None,
                    _ => {}
                }
                if name == sps::PORT && seen.contains(&at) {
                    ports.push(at);
                }
            }
        }
    }
    let inside = |p: [i32; 3]| (0..3).all(|k| p[k] > lo[k] && p[k] < lo[k] + sps::SIZE - 1);
    let coiled: Vec<[i32; 3]> = ports
        .iter()
        .copied()
        .filter(|p| {
            DIRECTIONS.iter().any(|(_, d)| {
                let at = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
                inside(at) && block_name(game, at) == sps::COIL
            })
        })
        .collect();
    // A coil not against a port fails the structure.
    (!ports.is_empty() && coiled.len() == coils).then_some((ports, coiled))
}

fn sps_of(nbt: &Compound) -> sps::Sps {
    sps::Sps {
        input: get_tank(nbt, "input", sps::INPUT_CAPACITY).amount,
        output: get_tank(nbt, "output", sps::OUTPUT_CAPACITY).amount,
        progress: get_double(nbt, "progress").unwrap_or(0.0),
        input_processed: get_long(nbt, "input_processed"),
    }
}

/// An SPS port. Once a second each port checks its structure; the first port runs it: polonium in
/// through the input-mode ports, the energy the coiled ports hold into the coils, antimatter out of the
/// output-mode ports.
fn tick_sps_port(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    if ticker == 0 {
        match find_sps(game, pos) {
            Some((ports, coiled)) => {
                entity.nbt.insert("master".into(), Tag::IntArray(ports[0].to_vec()));
                entity.nbt.insert("ports".into(), Tag::IntArray(ports.concat()));
                entity.nbt.insert("coiled".into(), Tag::IntArray(coiled.concat()));
                put_long(&mut entity.nbt, "coils", coiled.len() as u64);
            }
            None => {
                entity.nbt.remove(&JavaString::from("master"));
            }
        }
    }
    if get_pos(&entity.nbt, "master") != Some(pos) {
        return;
    }
    let ports = positions(&entity.nbt, "ports");
    let coiled = positions(&entity.nbt, "coiled");
    let mut input = get_tank(&entity.nbt, "input", sps::INPUT_CAPACITY);
    let mut output = get_tank(&entity.nbt, "output", sps::OUTPUT_CAPACITY);
    let output_mode = |game: &mut Game<'_>, entity: &ModBlockEntity, p: [i32; 3]| {
        if p == pos {
            get_long(&entity.nbt, "output") == 1
        } else {
            game.native_with_mod_block_entity(p, |m| get_long(&m.nbt, "output") == 1).unwrap_or(false)
        }
    };
    for &p in &ports {
        if !output_mode(game, entity, p) {
            pull_from_carriers(game, p, &mut input, sps::POLONIUM);
        }
    }
    let mut s = sps_of(&entity.nbt);
    s.input = input.amount;
    // Each coiled port hands the coils what it holds, at most an int's worth (`getAmountAsInt`).
    let mut received = 0u64;
    if s.can_operate() {
        for &p in &coiled {
            let take = |nbt: &mut Compound| {
                let stored = get_long(nbt, "energy");
                let n = stored.min(i32::MAX as u64);
                put_long(nbt, "energy", stored - n);
                n
            };
            let n = if p == pos { take(&mut entity.nbt) } else { game.native_with_mod_block_entity(p, |m| take(&mut m.nbt)).unwrap_or(0) };
            received = received.saturating_add(n).min(i32::MAX as u64);
        }
    }
    let processed = s.tick(received);
    input.extract(input.amount - s.input);
    if s.output > output.amount {
        output.insert(sps::ANTIMATTER, s.output - output.amount);
    }
    for &p in &ports {
        if output.amount > 0 && output_mode(game, entity, p) {
            push_chemical(game, p, &mut output, None, u64::MAX, false);
        }
    }
    put_tank(&mut entity.nbt, "input", &input);
    put_tank(&mut entity.nbt, "output", &output);
    entity.nbt.insert("progress".into(), Tag::Double(s.progress.to_bits()));
    entity.nbt.insert("processed".into(), Tag::Double(processed.to_bits()));
    put_long(&mut entity.nbt, "input_processed", s.input_processed);
    put_long(&mut entity.nbt, "received", received);
}

/// A sneaking right click with an empty hand on an SPS port switches it between input and output
/// (`TileEntitySPSPort.onSneakRightClick`); a plain one opens the SPS's screen.
pub(super) fn use_sps_port(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID)
        || game.inventory.get(game.selected).cloned().flatten().is_some()
        || !game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft)
    {
        return false;
    }
    let output = game.native_with_mod_block_entity(pos, |m| {
        (kind(&m.id) == Some(Kind::SpsPort)).then(|| {
            let output = get_long(&m.nbt, "output") == 0;
            put_long(&mut m.nbt, "output", u64::from(output));
            output
        })
    });
    let Some(Some(output)) = output else {
        return false;
    };
    let line = format!("SPS Port: {} mode", if output { "output" } else { "input" });
    if let Some(player) = game.native_acting_player() {
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
    true
}

// --- Thermoelectric Boiler ---------------------------------------------------------------------------

/// `BoilerValidator`, simplified: the frame blocks joined to `start` must be the shell of a box 3 to 18
/// on each side, with air, superheating elements and one full layer of pressure dispersers inside, the
/// elements below the dispersers. The boiler, its valves and the Resistive Heaters touching its walls.
#[allow(clippy::type_complexity)]
fn find_boiler(game: &Game<'_>, start: [i32; 3]) -> Option<(boiler::Boiler, Vec<[i32; 3]>, Vec<[i32; 3]>)> {
    let (lo, hi, seen) = find_box(game, start, boiler::is_frame, boiler::MIN_SIZE, boiler::MAX_SIZE)?;
    let size = |k: usize| (hi[k] - lo[k] + 1) as u64;
    let mut b = boiler::Boiler { volume: size(0) * size(1) * size(2), ..Default::default() };
    let mut valves = Vec::new();
    let mut dispersers = Vec::new();
    let mut elements = Vec::new();
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                let at = [x, y, z];
                let shell = x == lo[0] || x == hi[0] || y == lo[1] || y == hi[1] || z == lo[2] || z == hi[2];
                let name = block_name(game, at);
                if shell {
                    if !seen.contains(&at) {
                        return None;
                    }
                    if name == boiler::VALVE {
                        valves.push(at);
                    }
                    continue;
                }
                match name {
                    boiler::DISPERSER => dispersers.push(at),
                    boiler::ELEMENT => elements.push(at),
                    "minecraft:air" | "minecraft:cave_air" => {}
                    _ => return None,
                }
            }
        }
    }
    // One full layer of dispersers, with the elements below it.
    let layer = dispersers.first()?[1];
    let inner = (size(0) - 2) * (size(2) - 2);
    if dispersers.len() as u64 != inner || dispersers.iter().any(|d| d[1] != layer) || elements.is_empty() || elements.iter().any(|e| e[1] >= layer) {
        return None;
    }
    b.elements = elements.len() as u64;
    b.water_volume = inner * (layer - lo[1] - 1) as u64;
    b.steam_volume = inner * (hi[1] - layer - 1) as u64;
    if b.water_volume == 0 || b.steam_volume == 0 {
        return None;
    }
    // Heaters against a wall, outside the box.
    let mut heaters = Vec::new();
    for x in lo[0] - 1..=hi[0] + 1 {
        for y in lo[1] - 1..=hi[1] + 1 {
            for z in lo[2] - 1..=hi[2] + 1 {
                let at = [x, y, z];
                let outside = (0..3).filter(|&k| at[k] < lo[k] || at[k] > hi[k]).count();
                if outside == 1 && [boiler::HEATER, boiler::FUELWOOD_HEATER].contains(&block_name(game, at)) {
                    heaters.push(at);
                }
            }
        }
    }
    Some((b, valves, heaters))
}

fn positions(nbt: &Compound, key: &str) -> Vec<[i32; 3]> {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::IntArray(v)) => v.chunks(3).filter(|c| c.len() == 3).map(|c| [c[0], c[1], c[2]]).collect(),
        _ => Vec::new(),
    }
}

fn boiler_of(nbt: &Compound) -> boiler::Boiler {
    boiler::Boiler {
        elements: get_long(nbt, "elements"),
        volume: get_long(nbt, "volume"),
        water_volume: get_long(nbt, "water_volume"),
        steam_volume: get_long(nbt, "steam_volume"),
    }
}

/// A Thermoelectric Boiler valve. Once a second each valve checks its boiler; the first valve runs it:
/// it takes water from the pipes and fluid tanks beside the valves and the heat of the Resistive
/// Heaters against its walls, loses a little heat to the air, boils water over 100 °C into steam and
/// gives the steam out of every valve.
fn tick_boiler_valve(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    if ticker == 0 {
        match find_boiler(game, pos) {
            Some((b, valves, heaters)) => {
                for (key, v) in [
                    ("elements", b.elements),
                    ("volume", b.volume),
                    ("water_volume", b.water_volume),
                    ("steam_volume", b.steam_volume),
                ] {
                    put_long(&mut entity.nbt, key, v);
                }
                entity.nbt.insert("master".into(), Tag::IntArray(valves.first().copied().unwrap_or(pos).to_vec()));
                entity.nbt.insert("valves".into(), Tag::IntArray(valves.concat()));
                entity.nbt.insert("heaters".into(), Tag::IntArray(heaters.concat()));
            }
            None => {
                put_long(&mut entity.nbt, "volume", 0);
                entity.nbt.remove(&JavaString::from("master"));
            }
        }
    }
    let b = boiler_of(&entity.nbt);
    if b.volume == 0 || get_pos(&entity.nbt, "master") != Some(pos) {
        return None;
    }
    let mut water = get_tank(&entity.nbt, "water", b.water_capacity());
    let mut steam = get_tank(&entity.nbt, "steam", b.steam_capacity());
    for &v in &positions(&entity.nbt, "valves") {
        pull_from_carriers(game, v, &mut water, "minecraft:water");
    }
    let mut casing = mekanism_core::heat::HeatCapacitor::new(
        b.heat_capacity(),
        boiler::CASING_INVERSE_CONDUCTION,
        boiler::CASING_INVERSE_INSULATION,
    );
    if let Some(heat) = get_double(&entity.nbt, "heat") {
        casing.heat = heat;
    }
    for &h in &positions(&entity.nbt, "heaters") {
        let given = game.native_with_mod_block_entity(h, |m| {
            let heat = get_double(&m.nbt, "heat_out").unwrap_or(0.0);
            m.nbt.insert("heat_out".into(), Tag::Double(0f64.to_bits()));
            heat
        });
        casing.handle_heat(given.unwrap_or(0.0));
    }
    casing.lose_to_air(boiler::CASING_INVERSE_INSULATION);
    let water_in = if water.accepts("minecraft:water") { water.amount } else { 0 };
    let room = if steam.accepts("mekanism:steam") { steam.capacity - steam.amount } else { 0 };
    let boiled = b.boil(&mut casing.heat, water_in, room);
    water.extract(boiled);
    steam.insert("mekanism:steam", boiled);
    for &v in &positions(&entity.nbt, "valves") {
        if steam.amount > 0 {
            push_chemical(game, v, &mut steam, None, u64::MAX, false);
        }
    }
    entity.nbt.insert("heat".into(), Tag::Double(casing.heat.to_bits()));
    put_tank(&mut entity.nbt, "water", &water);
    put_tank(&mut entity.nbt, "steam", &steam);
    put_long(&mut entity.nbt, "boil_rate", boiled);
    Some(boiled > 0)
}

/// The Resistive Heater: each tick it turns its energy usage (100 FE by default) into 60% as much heat,
/// which a boiler beside it takes. Unused heat stays for a second at most.
fn tick_heater(entity: &mut ModBlockEntity) -> Option<bool> {
    let usage = heater_usage(&entity.nbt);
    let mut energy = EnergyContainer::new(machine::STORAGE_MULTIPLIER * usage, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(energy.capacity);
    let active = energy.consume(usage);
    let mut heat = get_double(&entity.nbt, "heat_out").unwrap_or(0.0);
    if active {
        let made = usage as f64 * boiler::HEATER_EFFICIENCY;
        heat = (heat + made).min(made * 20.0);
    }
    entity.nbt.insert("heat_out".into(), Tag::Double(heat.to_bits()));
    put_long(&mut entity.nbt, "energy", energy.stored);
    Some(active)
}

fn heater_usage(nbt: &Compound) -> u64 {
    match get_long(nbt, "usage") {
        0 => boiler::HEATER_USAGE,
        n => n,
    }
}

/// A right click with an empty hand on a Resistive Heater steps its energy usage; on a boiler valve it
/// shows the boiler's temperature, boil rate and tanks.
pub(super) fn use_boiler(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || game.inventory.get(game.selected).cloned().flatten().is_some() {
        return false;
    }
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    let line = game.native_with_mod_block_entity(pos, |m| match kind(&m.id) {
        Some(Kind::Heater) => {
            let usage = boiler::next_heater_usage(heater_usage(&m.nbt));
            put_long(&mut m.nbt, "usage", usage);
            Some(Ok(format!("Resistive Heater: {usage} FE/t")))
        }
        // A plain click on a valve opens the boiler's screen.
        Some(Kind::BoilerValve) if sneaking => Some(Err(get_pos(&m.nbt, "master"))),
        _ => None,
    });
    let line = match line.flatten() {
        None => return false,
        Some(Ok(line)) => line,
        Some(Err(master)) => master
            .and_then(|master| {
                game.native_with_mod_block_entity(master, |m| {
                    let b = boiler_of(&m.nbt);
                    let heat = get_double(&m.nbt, "heat").unwrap_or(b.heat_capacity() * boiler::BOIL_TEMP);
                    let [water, steam] = ["water", "steam"].map(|k| get_tank(&m.nbt, k, u64::MAX).amount);
                    format!(
                        "Boiler: {:.1} K, boiling {} mB/t, {} superheaters, water {water}, steam {steam}",
                        heat / b.heat_capacity().max(1.0),
                        get_long(&m.nbt, "boil_rate"),
                        b.elements,
                    )
                })
            })
            .unwrap_or_else(|| "Boiler: not formed".to_owned()),
    };
    if let Some(player) = game.native_acting_player() {
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
    true
}

// --- Industrial Turbine ------------------------------------------------------------------------------

const TURBINE_VALVE: &str = "mekanismgenerators:turbine_valve";

/// `TurbineValidator`, simplified: the frame blocks joined to `start` must be exactly the shell of a
/// box 3 to 17 on each side, with rotors, a rotational complex, pressure dispersers, coils, condensers
/// or air inside; the steam is below the lowest disperser. The turbine and its valves.
fn find_turbine(game: &Game<'_>, start: [i32; 3]) -> Option<(turbine::Turbine, Vec<[i32; 3]>)> {
    let (lo, hi, seen) = find_box(game, start, turbine::is_frame, turbine::MIN_SIZE, turbine::MAX_SIZE)?;
    let size = |k: usize| (hi[k] - lo[k] + 1) as u64;
    let mut t = turbine::Turbine { volume: size(0) * size(1) * size(2), ..Default::default() };
    let mut valves = Vec::new();
    let mut disperser_y = None;
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                let at = [x, y, z];
                let shell = x == lo[0] || x == hi[0] || y == lo[1] || y == hi[1] || z == lo[2] || z == hi[2];
                let name = block_name(game, at);
                if shell {
                    if !seen.contains(&at) {
                        return None;
                    }
                    match name {
                        TURBINE_VALVE => valves.push(at),
                        "mekanismgenerators:turbine_vent" => t.vents += 1,
                        _ => {}
                    }
                } else {
                    if !t.add_inside(name) {
                        return None;
                    }
                    if name == "mekanism:pressure_disperser" {
                        disperser_y = Some(disperser_y.map_or(y, |d: i32| d.min(y)));
                    }
                }
            }
        }
    }
    t.lower_volume = size(0) * size(2) * (disperser_y? - lo[1]) as u64;
    (t.rotors > 0 && t.coils > 0).then_some((t, valves))
}

/// An Industrial Turbine valve. Once a second each valve checks its turbine; the first valve runs it: it
/// takes steam from the tubes and chemical tanks beside the valves, turns it into energy given out of
/// every valve, and gives condensed water to the pipes and fluid tanks beside them.
fn turbine_of(nbt: &Compound) -> turbine::Turbine {
    let g = |key: &str| get_long(nbt, key);
    turbine::Turbine {
        rotors: g("rotors"),
        dispersers: g("dispersers"),
        coils: g("coils"),
        condensers: g("condensers"),
        vents: g("vents"),
        volume: g("volume"),
        lower_volume: g("lower_volume"),
    }
}

fn tick_turbine_valve(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    if ticker == 0 {
        match find_turbine(game, pos) {
            Some((t, valves)) => {
                for (key, v) in [
                    ("rotors", t.rotors),
                    ("dispersers", t.dispersers),
                    ("coils", t.coils),
                    ("condensers", t.condensers),
                    ("vents", t.vents),
                    ("volume", t.volume),
                    ("lower_volume", t.lower_volume),
                ] {
                    put_long(&mut entity.nbt, key, v);
                }
                entity.nbt.insert("master".into(), Tag::IntArray(valves.first().copied().unwrap_or(pos).to_vec()));
                entity.nbt.insert("valves".into(), Tag::IntArray(valves.concat()));
            }
            None => {
                put_long(&mut entity.nbt, "rotors", 0);
                entity.nbt.remove(&JavaString::from("master"));
            }
        }
    }
    let t = turbine_of(&entity.nbt);
    if t.rotors == 0 || get_pos(&entity.nbt, "master") != Some(pos) {
        return None;
    }
    let valves: Vec<[i32; 3]> = match entity.nbt.get(&JavaString::from("valves")) {
        Some(Tag::IntArray(v)) => v.chunks(3).filter(|c| c.len() == 3).map(|c| [c[0], c[1], c[2]]).collect(),
        _ => vec![pos],
    };
    let mut steam = get_tank(&entity.nbt, "steam", t.steam_capacity());
    let mut water = get_tank(&entity.nbt, "water", (t.condensers * turbine::CONDENSER_RATE).max(1_000));
    let mut energy = EnergyContainer::new(t.energy_capacity(), u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(energy.capacity);
    for &v in &valves {
        pull_from_carriers(game, v, &mut steam, "mekanism:steam");
    }
    let (used, made, condensed) = t.tick(steam.amount, energy.capacity - energy.stored);
    steam.extract(used);
    energy.insert(made, Action::Execute);
    if water.accepts("minecraft:water") {
        water.insert("minecraft:water", condensed);
    }
    for &v in &valves {
        push_energy(game, v, &mut energy, None, false);
        if water.amount > 0 {
            push_chemical(game, v, &mut water, None, u64::MAX, false);
        }
    }
    // `ChemicalUtils.dump`: dumping removes a share of the steam, dumping excess what is over 85% of the tank.
    let dump = get_int(&entity.nbt, "dump");
    if dump != 0 && steam.amount > 0 {
        let stored = steam.amount;
        let amount = (used * 2).max(stored / 50).min(stored);
        let to_dump = if dump == 2 {
            amount
        } else {
            let keep = (steam.capacity as f64 * 0.85) as u64;
            if keep < stored { (stored - keep).min(amount) } else { 0 }
        };
        steam.extract(to_dump);
    }
    put_tank(&mut entity.nbt, "steam", &steam);
    put_tank(&mut entity.nbt, "water", &water);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_long(&mut entity.nbt, "production", made);
    put_long(&mut entity.nbt, "flow", used);
    Some(made > 0)
}

/// A right click with an empty hand on a turbine valve shows the turbine's steam, production and energy.
pub(super) fn show_turbine(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || game.inventory.get(game.selected).cloned().flatten().is_some() {
        return false;
    }
    // A plain click opens the turbine's screen; a sneaking one shows its state.
    if !game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) {
        return false;
    }
    let master = game.native_with_mod_block_entity(pos, |m| {
        (kind(&m.id) == Some(Kind::TurbineValve)).then(|| get_pos(&m.nbt, "master"))
    });
    let Some(Some(master)) = master else {
        return false;
    };
    let line = master
        .and_then(|master| {
            game.native_with_mod_block_entity(master, |m| {
                let mut c = Compound::new();
                if let Some(t) = m.nbt.get(&JavaString::from("steam")) {
                    c.insert("t".into(), t.clone());
                }
                format!(
                    "Industrial Turbine: steam {} mB, producing {}/t, stored {}",
                    get_tank(&c, "t", u64::MAX).amount,
                    mekanism_core::energy::format_fe(get_long(&m.nbt, "production")),
                    mekanism_core::energy::format_fe(get_long(&m.nbt, "energy")),
                )
            })
        })
        .unwrap_or_else(|| "Industrial Turbine: not formed".to_owned());
    if let Some(player) = game.native_acting_player() {
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
    true
}

// --- Bins and personal storage ----------------------------------------------------------------------

const BIN_KEY: &str = "mekanism:bin";
const PERSONAL_KEY: &str = "mekanism:personal_storage";

fn bin_of(nbt: &Compound) -> Bin {
    let count = get_long(nbt, "count");
    Bin { item: get_string(nbt, "item").filter(|_| count > 0), count, lock: get_string(nbt, "lock") }
}

fn put_bin(nbt: &mut Compound, b: &Bin) {
    nbt.insert("item".into(), Tag::String(b.item.as_deref().unwrap_or("").into()));
    put_long(nbt, "count", b.count);
    nbt.insert("lock".into(), Tag::String(b.lock.as_deref().unwrap_or("").into()));
}

/// A stack a bin can hold: no damage and no components (a bin keeps only the item id).
fn plain(stack: &ItemStack) -> bool {
    stack.damage == 0 && stack.extra.is_none()
}

/// The bin with what its output slot shows put back in it (the output slot is a window on the bin for
/// hoppers below; the bin refills it each tick).
fn settle_bin(entity: &mut ModBlockEntity, tier: BaseTier) -> Bin {
    let mut b = bin_of(&entity.nbt);
    if entity.items.len() < 2 {
        entity.items.resize(2, None);
    }
    if let Some(s) = entity.items[1].take() {
        if tier == BaseTier::Creative || b.item.as_deref() == Some(&*s.item) {
            if tier != BaseTier::Creative {
                b.count += u64::from(s.count);
            }
        } else if b.insert(tier, &s.item, u64::from(s.count), false) < u64::from(s.count) {
            entity.items[1] = Some(s);
        }
    }
    b
}

/// A bin's tick: what hoppers put in the input slot goes in, and the output slot shows a stack of it for
/// hoppers below to take (`BinInventorySlot` through the item handler).
fn tick_bin(game: &mut Game<'_>, entity: &mut ModBlockEntity, tier: BaseTier) {
    let mut b = settle_bin(entity, tier);
    if let Some(s) = entity.items[0].clone().filter(plain) {
        let took = b.insert(tier, &s.item, u64::from(s.count), false) as u32;
        if took > 0 {
            entity.items[0] = (s.count > took).then(|| ItemStack { count: s.count - took, ..s });
        }
    }
    if entity.items[1].is_none()
        && let Some(item) = b.item.clone()
    {
        let max = max_stack(game, &item);
        if let Some((item, n)) = b.extract(tier, u64::from(max)) {
            entity.items[1] = Some(stack_of(&item, n as u32));
        }
    }
    let ticks = get_int(&entity.nbt, "add_ticks");
    put_int(&mut entity.nbt, "add_ticks", ticks.saturating_sub(1));
    put_bin(&mut entity.nbt, &b);
}

fn bin_line(tier: BaseTier, b: &Bin) -> String {
    let name = format!("{}{} Bin", tier.name()[..1].to_uppercase(), &tier.name()[1..]);
    let lock = if b.lock.is_some() { " (locked)" } else { "" };
    match b.item_type() {
        Some(item) => {
            let count = if tier == BaseTier::Creative { "∞".to_owned() } else { format!("{} / {}", b.count, bin::capacity(tier)) };
            format!("{name}: {count} {item}{lock}")
        }
        None => format!("{name}: empty / {}", bin::capacity(tier)),
    }
}

/// A right click on a bin (`BlockBin.useItemOn`; Horizon has no left click hook, so the empty hand
/// takes items out here):
/// - with an item, puts the held stack in; a second click within 5 ticks puts in every matching stack of
///   the inventory;
/// - with an empty hand, takes a stack out (vanilla's sneaking left click);
/// - sneaking with an empty hand, locks the bin to its item, or unlocks it.
pub(super) fn use_bin(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) {
        return false;
    }
    let Some(tier) = bin::tier_of(block_name(game, pos)) else {
        return false;
    };
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    let selected = game.selected;
    let held = game.inventory.get(selected).cloned().flatten();
    let Some((mut b, armed)) = game.native_with_mod_block_entity(pos, |m| (settle_bin(m, tier), get_int(&m.nbt, "add_ticks"))) else {
        return false;
    };
    let mut add_ticks = 0;
    match held {
        Some(h) => {
            if plain(&h) {
                let took = b.insert(tier, &h.item, u64::from(h.count), true) as u32;
                if took > 0 {
                    game.inventory[selected] = (h.count > took).then(|| ItemStack { count: h.count - took, ..h });
                }
            }
            add_ticks = bin::DOUBLE_CLICK_TICKS;
        }
        None if sneaking => {
            b.toggle_lock(tier);
        }
        None if armed > 0 && b.item_type().is_some() => {
            let item = b.item_type().unwrap_or_default().to_owned();
            for i in 0..game.inventory.len().min(36) {
                let Some(s) = game.inventory[i].clone().filter(|s| *s.item == *item && plain(s)) else {
                    continue;
                };
                let took = b.insert(tier, &item, u64::from(s.count), true) as u32;
                game.inventory[i] = (s.count > took).then(|| ItemStack { count: s.count - took, ..s });
            }
        }
        None => {
            let max = b.item.as_deref().map_or(64, |i| max_stack(game, i));
            if let Some((item, n)) = b.extract(tier, u64::from(max)) {
                game.give_or_drop(stack_of(&item, n as u32));
            }
        }
    }
    let line = bin_line(tier, &b);
    game.native_with_mod_block_entity(pos, |m| {
        put_bin(&mut m.nbt, &b);
        put_int(&mut m.nbt, "add_ticks", add_ticks);
    });
    tell(game, line);
    true
}

fn uuid_text(uuid: u128) -> String {
    format!("{uuid:032x}")
}

/// A right click on someone else's personal chest or barrel: it stays shut (`TileEntityPersonalStorage`
/// with its owner's security). The owner's click opens the six-row menu as a chest's.
pub(super) fn use_personal_storage(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || !bin::is_personal(block_name(game, pos)) {
        return false;
    }
    let Some(player) = game.native_acting_player() else {
        return false;
    };
    let owner = game.native_with_mod_block_entity(pos, |m| (get_string(&m.nbt, "owner"), get_string(&m.nbt, "owner_name"))).unwrap_or_default();
    match owner {
        (Some(uuid), name) if uuid != uuid_text(player.uuid) => {
            tell(game, format!("This belongs to {}", name.unwrap_or_else(|| "another player".to_owned())));
            true
        }
        _ => false,
    }
}

/// Only its owner breaks a personal chest or barrel.
pub(super) fn protect_personal(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    use_personal_storage(game, pos)
}

// --- QIO --------------------------------------------------------------------------------------------

const QIO_KEY: &str = "mekanism:qio";

/// The drive arrays this server has seen, by frequency, as `TELEPORTERS`.
#[allow(clippy::type_complexity)]
static QIO_ARRAYS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, Vec<(super::Dim, [i32; 3])>>>> =
    std::sync::LazyLock::new(Default::default);

fn is_qio(name: &str) -> bool {
    [qio::DRIVE_ARRAY, qio::DASHBOARD, qio::IMPORTER, qio::EXPORTER, qio::REDSTONE_ADAPTER].contains(&name)
}

/// What a drive item holds (`QIODriveData` in its custom data).
fn drive_of(stack: &ItemStack) -> Option<qio::Drive> {
    let (count_limit, type_limit) = qio::drive_tier(&stack.item)?;
    let mut items = std::collections::BTreeMap::new();
    if let Some(Tag::Compound(c)) = custom_data(stack).and_then(|c| c.get(&JavaString::from(QIO_KEY))) {
        for (k, v) in c.iter() {
            if let (Ok(id), Tag::Long(n)) = (k.to_string(), v)
                && *n > 0
            {
                items.insert(id, *n as u64);
            }
        }
    }
    Some(qio::Drive { count_limit, type_limit, items })
}

fn put_drive(stack: &mut ItemStack, d: &qio::Drive) {
    let mut c = Compound::new();
    for (k, v) in &d.items {
        c.insert(k.as_str().into(), Tag::Long(*v as i64));
    }
    set_custom_compound(stack, QIO_KEY, c);
}

/// The drives of `frequency`'s arrays in this dimension, and where each one is (array, slot).
fn load_network(game: &mut Game<'_>, frequency: &str) -> (qio::Network, Vec<([i32; 3], usize)>) {
    let dim = game.dim;
    let arrays: Vec<[i32; 3]> = QIO_ARRAYS
        .lock()
        .ok()
        .and_then(|all| all.get(frequency).map(|l| l.iter().filter(|(d, _)| *d == dim).map(|(_, p)| *p).collect()))
        .unwrap_or_default();
    let mut net = qio::Network::default();
    let mut at = Vec::new();
    for pos in arrays {
        let drives = game.native_with_mod_block_entity(pos, |m| {
            if m.id != qio::DRIVE_ARRAY || get_string(&m.nbt, "frequency").as_deref() != Some(frequency) {
                return Vec::new();
            }
            m.items.iter().take(qio::DRIVE_SLOTS).enumerate().filter_map(|(i, s)| Some((i, drive_of(s.as_ref()?)?))).collect()
        });
        for (slot, d) in drives.unwrap_or_default() {
            net.drives.push(d);
            at.push((pos, slot));
        }
    }
    (net, at)
}

fn save_network(game: &mut Game<'_>, net: &qio::Network, at: &[([i32; 3], usize)]) {
    for (d, &(pos, slot)) in net.drives.iter().zip(at) {
        game.native_with_mod_block_entity(pos, |m| {
            if let Some(Some(stack)) = m.items.get_mut(slot) {
                put_drive(stack, d);
            }
        });
    }
}

/// Puts the plain stacks of `slots` into the network, keeping what doesn't fit.
fn return_to_network(net: &mut qio::Network, slots: &mut [Option<ItemStack>]) {
    for slot in slots.iter_mut() {
        let Some(s) = slot.clone().filter(plain) else {
            continue;
        };
        let took = net.insert(&s.item, u64::from(s.count)) as u32;
        *slot = (s.count > took).then(|| ItemStack { count: s.count - took, ..s });
    }
}

/// A drive array puts itself on its frequency once a second.
fn tick_drive_array(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    if ticker != 0 {
        return;
    }
    let dim = game.dim;
    if let Ok(mut all) = QIO_ARRAYS.lock() {
        for list in all.values_mut() {
            list.retain(|&(d, p)| (d, p) != (dim, pos));
        }
        if let Some(f) = get_string(&entity.nbt, "frequency") {
            all.entry(f).or_default().push((dim, pos));
        }
    }
    let net = get_string(&entity.nbt, "frequency").map(|f| load_network(game, &f).0);
    qio_sync(game, pos, entity, net.as_ref());
}

/// The dashboard's six rows are a window on its frequency: each tick what is in them goes back into the
/// drives, and the most stored items fill them again, a stack of each. Taking a stack out of the menu
/// takes it from the drives; putting one in stores it.
fn tick_dashboard(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let Some(frequency) = get_string(&entity.nbt, "frequency") else {
        qio_sync(game, pos, entity, None);
        return;
    };
    let (mut net, at) = load_network(game, &frequency);
    qio_sync(game, pos, entity, Some(&net));
    if at.is_empty() {
        return;
    }
    let search = get_string(&entity.nbt, "search").unwrap_or_default();
    let before = net.clone();
    if entity.items.len() < qio::DASHBOARD_SLOTS {
        entity.items.resize(qio::DASHBOARD_SLOTS, None);
    }
    return_to_network(&mut net, &mut entity.items);
    let mut list = net.list().into_iter().filter(|(item, _)| qio::matches_search(item, &search));
    for slot in entity.items.iter_mut().filter(|s| s.is_none()) {
        let Some((item, _)) = list.next() else {
            break;
        };
        let max = max_stack(game, &item);
        let n = net.extract(&item, u64::from(max)) as u32;
        *slot = (n > 0).then(|| stack_of(&item, n));
    }
    if net != before {
        save_network(game, &net, &at);
    }
}

/// The block an importer or exporter works on (the one behind its face) and that block's face toward it.
fn qio_target(game: &Game<'_>, pos: [i32; 3]) -> Option<([i32; 3], horizon_core::geometry::Direction)> {
    let d = game.native_block_property(pos, "facing").and_then(|f| direction(&f))?;
    Some(([pos[0] - d[0], pos[1] - d[1], pos[2] - d[2]], horizon_core::geometry::Direction::ALL[face_index(d)]))
}

/// An importer, each half second, takes up to 64 items from the block behind it (and what hoppers put
/// in it) into its frequency (`TileEntityQIOImporter.tryImport`).
fn tick_importer(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % qio::TRANSIT_DELAY);
    let Some(frequency) = get_string(&entity.nbt, "frequency").filter(|_| ticker == 0) else {
        return;
    };
    let (mut net, at) = load_network(game, &frequency);
    if at.is_empty() {
        return;
    }
    let before = net.clone();
    return_to_network(&mut net, &mut entity.items);
    // With enabled filters only what they name goes in; with none, everything does.
    let filters = load_sorter(&entity.nbt);
    let active: Vec<String> = filters.active().cloned().collect();
    // The tags are copied out (only for tag filters to read) so the game stays free to move the items.
    let tags = if active.is_empty() { Default::default() } else { world_items!(game).item_tags.clone() };
    let in_tag = |tag: &str, item: &str| tags.get(tag).is_some_and(|t| t.contains(item));
    let wanted = |s: &ItemStack| active.is_empty() || active.iter().any(|f| Sorter::matches_item(f, &s.item, &in_tag));
    if let Some((back, face)) = qio_target(game, pos)
        && let Some(stack) = game.native_extract_item_where(back, face, qio::TRANSIT_COUNT, &wanted)
    {
        let mut left = [Some(stack)];
        return_to_network(&mut net, &mut left);
        if let [Some(rest)] = left {
            game.native_insert_item(back, face, rest);
        }
    }
    if net != before {
        save_network(game, &net, &at);
    }
}

/// An exporter, each half second, puts up to 64 items its enabled filters name from its frequency into
/// the block behind it (`TileEntityQIOExporter.tryEject`): the first filter that finds something, or,
/// with round robin, the next one after the last that did. With no enabled filter it exports anything
/// only when "export without filter" is on.
fn tick_exporter(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % qio::TRANSIT_DELAY);
    if ticker != 0 {
        return;
    }
    let Some(frequency) = get_string(&entity.nbt, "frequency") else {
        return;
    };
    let settings = load_sorter(&entity.nbt);
    let active: Vec<String> = settings.active().cloned().collect();
    if active.is_empty() && !settings.auto_eject {
        return;
    }
    let Some((back, face)) = qio_target(game, pos) else {
        return;
    };
    let (mut net, at) = load_network(game, &frequency);
    let in_tag = |tag: &str, item: &str| world_items!(game).item_tags.get(tag).is_some_and(|t| t.contains(item));
    let stored = net.list();
    let pick = |filter: Option<&String>| -> Option<String> {
        stored.iter().find(|(item, n)| *n > 0 && filter.is_none_or(|f| Sorter::matches_item(f, item, &in_tag))).map(|(item, _)| item.clone())
    };
    let count = active.len().max(1);
    let start = if settings.round_robin { get_int(&entity.nbt, "round_robin_at") as usize % count } else { 0 };
    let mut chosen = None;
    for k in 0..count {
        let i = (start + k) % count;
        let found = if active.is_empty() { pick(None) } else { pick(active.get(i)) };
        if let Some(item) = found {
            chosen = Some((i, item));
            break;
        }
    }
    let Some((i, item)) = chosen else {
        return;
    };
    if settings.round_robin {
        put_int(&mut entity.nbt, "round_robin_at", ((i + 1) % count) as u32);
    }
    let n = net.extract(&item, u64::from(qio::TRANSIT_COUNT.min(max_stack(game, &item)))) as u32;
    if n == 0 {
        return;
    }
    if let Some(rest) = game.native_insert_item(back, face, stack_of(&item, n)) {
        net.insert(&item, u64::from(rest.count));
    }
    save_network(game, &net, &at);
}

/// A redstone adapter, each half second, powers its sides (its lever template's `powered`) while its
/// frequency holds at least its count of its filter item, or the other way round when inverted
/// (`TileEntityQIORedstoneAdapter.onUpdateServer`).
fn tick_redstone_adapter(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % qio::TRANSIT_DELAY);
    if ticker != 0 {
        return;
    }
    let stored = get_string(&entity.nbt, "frequency").and_then(|f| {
        let (net, at) = load_network(game, &f);
        (!at.is_empty()).then(|| get_string(&entity.nbt, "filter").map_or(0, |item| net.stored(&item)))
    });
    let emit = stored.is_some_and(|n| {
        qio::adapter_emits(n, get_long(&entity.nbt, "count"), get_int(&entity.nbt, "inverted") != 0)
    });
    let powered = game.native_block_property(pos, "powered").as_deref() == Some("true");
    if emit != powered {
        game.native_set_block_property(pos, "powered", if emit { "true" } else { "false" });
    }
}

fn qio_line(game: &mut Game<'_>, frequency: &str) -> String {
    let (net, _) = load_network(game, frequency);
    let (count, count_limit, types, type_limit) = net.totals();
    format!("QIO {frequency}: {count} / {count_limit} items, {types} / {type_limit} types")
}

/// A right click on a QIO block:
/// - with a named name tag, sets its frequency to the name;
/// - on an exporter with an item, makes that item its filter (sneaking with an empty hand clears it);
/// - sneaking with an empty hand, shows the frequency's totals.
///
/// Otherwise the drive array and the dashboard open their menus.
pub(super) fn use_qio(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) {
        return false;
    }
    let name = block_name(game, pos).to_owned();
    if !is_qio(&name) {
        return false;
    }
    let held = game.inventory.get(game.selected).cloned().flatten();
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    if let Some(tag) = held.as_ref().filter(|s| &*s.item == "minecraft:name_tag").and_then(|s| s.custom_name()) {
        let entry = name_tag_frequency(game, &tag);
        game.native_with_mod_block_entity(pos, |m| put_block_frequency(&mut m.nbt, Some(&entry)));
        tell(game, format!("QIO frequency: {tag}"));
        return true;
    }
    let frequency = game.native_with_mod_block_entity(pos, |m| get_string(&m.nbt, "frequency")).flatten();
    if name == qio::EXPORTER || name == qio::IMPORTER {
        // Their screens hold the filters (`GuiQIOExporter`, `GuiQIOImporter`).
        return false;
    }
    if name == qio::REDSTONE_ADAPTER {
        // Its screen sets the item, count and signal (`GuiQIORedstoneAdapter`).
        return false;
    }
    if name == qio::REDSTONE_ADAPTER && false {
        let get = |game: &mut Game<'_>, key: &str| game.native_with_mod_block_entity(pos, |m| get_long(&m.nbt, key)).unwrap_or(0);
        match (&held, sneaking) {
            (Some(h), false) => {
                let item = h.item.to_string();
                game.native_with_mod_block_entity(pos, |m| m.nbt.insert("filter".into(), Tag::String(item.as_str().into())));
                tell(game, format!("QIO Redstone Adapter filter: {item}"));
            }
            (Some(h), true) => {
                let count = u64::from(h.count);
                game.native_with_mod_block_entity(pos, |m| put_long(&mut m.nbt, "count", count));
                tell(game, format!("QIO Redstone Adapter count: {count}"));
            }
            (None, true) => {
                let inverted = get(game, "inverted") == 0;
                game.native_with_mod_block_entity(pos, |m| put_int(&mut m.nbt, "inverted", u32::from(inverted)));
                tell(game, format!("QIO Redstone Adapter inverted: {inverted}"));
            }
            (None, false) => {
                let filter = game.native_with_mod_block_entity(pos, |m| get_string(&m.nbt, "filter")).flatten();
                let count = get(game, "count");
                tell(game, format!("QIO Redstone Adapter: {} >= {count} on {}", filter.as_deref().unwrap_or("no filter"), frequency.as_deref().unwrap_or("no frequency")));
            }
        }
        return true;
    }
    if held.is_none() && (sneaking || name == qio::IMPORTER) {
        let line = match frequency {
            Some(f) => qio_line(game, &f),
            None => "QIO: no frequency (name it with a name tag)".to_owned(),
        };
        tell(game, line);
        return true;
    }
    false
}

/// Before a dashboard breaks, what its rows show goes back into the drives, so it doesn't spill them.
pub(super) fn before_break(game: &mut Game<'_>, pos: [i32; 3]) {
    if !game.native_on(MOD_ID) || block_name(game, pos) != qio::DASHBOARD {
        return;
    }
    let Some(Some((frequency, mut items))) = game.native_with_mod_block_entity(pos, |m| Some((get_string(&m.nbt, "frequency")?, m.items.clone()))) else {
        return;
    };
    let (mut net, at) = load_network(game, &frequency);
    if at.is_empty() {
        return;
    }
    return_to_network(&mut net, &mut items);
    save_network(game, &net, &at);
    game.native_with_mod_block_entity(pos, |m| m.items = items);
}

// --- Modules and the Meka-Tool ----------------------------------------------------------------------

/// The modules an item has installed (Mekanism's `module_container` component), in its custom data.
const MODULES_KEY: &str = "mekanism:modules";
/// One of each installed module's own item, kept so that removing a module gives back the same item.
const MODULE_ITEMS_KEY: &str = "mekanism:module_items";
const MODULE_TYPE_KEY: &str = "mekanism:module_type";

fn modules_of(stack: &ItemStack) -> modules::Installed {
    let mut out = modules::Installed::new();
    if let Some(Tag::Compound(c)) = custom_data(stack).and_then(|c| c.get(&JavaString::from(MODULES_KEY))) {
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

fn module_type(stack: &ItemStack) -> Option<String> {
    if &*stack.item != modules::MODULE_ITEM {
        return None;
    }
    get_string(custom_data(stack)?, MODULE_TYPE_KEY)
}

/// An item's energy limits; the Meka-Tool's and the MekaSuit's grow with their energy units.
fn item_energy_of(stack: &ItemStack) -> Option<gear::ItemEnergy> {
    let mut e = gear::item_energy(&stack.item)?;
    if modules::is_container(&stack.item) {
        (e.capacity, e.rate) = modules::energy(&modules_of(stack));
    }
    Some(e)
}

fn module_name(id: &str) -> String {
    let path = id.split_once(':').map_or(id, |(_, p)| p);
    path.split('_').map(|w| w[..1].to_uppercase() + &w[1..]).collect::<Vec<_>>().join(" ")
}

/// A right click on the Modification Station holding a Meka-Tool or MekaSuit piece: every module in the
/// inventory that it takes goes in, up to each module's limit; sneaking takes them all out again
/// (`ModificationStationContainer`, without its screen).
pub(super) fn use_modification_station(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || block_name(game, pos) != modules::STATION {
        return false;
    }
    let selected = game.selected;
    let Some(held) = game.inventory.get(selected).cloned().flatten().filter(|s| modules::is_container(&s.item)) else {
        tell(game, "Modification Station: hold a Meka-Tool or a MekaSuit piece".to_owned());
        return true;
    };
    let mut installed = modules_of(&held);
    let mut kept = match custom_data(&held).and_then(|c| c.get(&JavaString::from(MODULE_ITEMS_KEY))) {
        Some(Tag::Compound(c)) => c.clone(),
        _ => Compound::new(),
    };
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    if sneaking {
        for (id, n) in std::mem::take(&mut installed) {
            let item = kept.get(&JavaString::from(id.as_str())).and_then(horizon_items::item_nbt::item_from_nbt);
            if let Some(item) = item {
                game.give_or_drop(ItemStack { count: n, ..item });
            }
        }
        kept = Compound::new();
    } else {
        for i in 0..game.inventory.len().min(36) {
            let Some(s) = game.inventory[i].clone() else {
                continue;
            };
            let Some(id) = module_type(&s) else {
                continue;
            };
            let put = modules::install(&held.item, &mut installed, &id, s.count);
            if put > 0 {
                kept.insert(id.as_str().into(), horizon_items::item_nbt::item_to_nbt(&ItemStack { count: 1, ..s.clone() }));
                game.inventory[i] = (s.count > put).then(|| ItemStack { count: s.count - put, ..s });
            }
        }
    }
    let mut c = Compound::new();
    for (id, n) in &installed {
        c.insert(id.as_str().into(), Tag::Int(*n as i32));
    }
    let mut stack = held;
    let stored = stored_energy(&stack);
    set_custom_compound(&mut stack, MODULES_KEY, c);
    set_custom_compound(&mut stack, MODULE_ITEMS_KEY, kept);
    // Silk touch, fortune and frost walker units are those enchantments on the item.
    let mut levels: Vec<(Arc<str>, u32)> =
        stack.enchantments().iter().filter(|(e, _)| !modules::MODULE_ENCHANTMENTS.contains(&&**e)).cloned().collect();
    levels.extend(modules::enchantments(&installed).into_iter().map(|(e, l)| (Arc::from(e), l)));
    stack.set_crafting_enchantments(levels);
    // Fewer energy units hold less (`ModuleEnergyUnit.onRemoved`).
    let capacity = item_energy_of(&stack).map_or(stored, |e| e.capacity);
    set_energy(&mut stack, stored.min(capacity));
    let line = if installed.is_empty() {
        "No modules installed".to_owned()
    } else {
        installed.iter().map(|(id, n)| format!("{} x{n}", module_name(id))).collect::<Vec<_>>().join(", ")
    };
    game.inventory[selected] = Some(stack);
    tell(game, line);
    true
}

/// The Meka-Tool's mining speed: its efficiency while it holds the energy for the block.
fn meka_tool_speed(stack: &ItemStack, hardness: f32) -> f32 {
    let installed = modules_of(stack);
    let silk = installed.contains_key("mekanism:silk_touch_unit");
    modules::tool_destroy_speed(modules::tool_efficiency(&installed), stored_energy(stack), hardness, silk)
}

/// A right click with the Meka-Tool and its Teleportation Unit: to the top of the block looked at, up to
/// 100 blocks away, for 1,000 FE per 10 blocks² (`ItemMekaTool.use`).
fn meka_tool_use(game: &mut Game<'_>, claimed: &mut bool) {
    let Some(held) = game.held_mut().clone().filter(|s| &*s.item == modules::MEKA_TOOL) else {
        return;
    };
    if !modules_of(&held).contains_key("mekanism:teleportation_unit") {
        return;
    }
    let Some((origin, dir)) = game.aim() else {
        return;
    };
    let Some(hit) = game.clip_ray(origin, dir, modules::TOOL_TELEPORT_REACH) else {
        return;
    };
    let p = hit.pos;
    let free = |game: &Game<'_>, dy: i32| block_name(game, [p[0], p[1] + dy, p[2]]).ends_with("air");
    if !free(game, 1) || !free(game, 2) {
        return;
    }
    let Some(at) = game.player.as_ref().map(|pl| [pl.position.x, pl.position.y, pl.position.z]) else {
        return;
    };
    let distance_sq: f64 = (0..3).map(|k| (at[k] - f64::from(p[k])).powi(2)).sum();
    if distance_sq < 5.0 {
        return;
    }
    let cost = modules::teleport_energy(distance_sq);
    let creative = game.creative;
    if let Some(stack) = game.held_mut().as_mut() {
        let stored = stored_energy(stack);
        if !creative {
            if stored < cost {
                return;
            }
            set_energy(stack, stored - cost);
        }
    }
    *claimed = true;
    place_player(game, [f64::from(p[0]) + 0.5, f64::from(p[1]) + 1.0, f64::from(p[2]) + 0.5]);
}

// --- MekaSuit ---------------------------------------------------------------------------------------

/// The window tells the server that its player's Locomotive Boosting Unit pushed them this tick.
const MEKASUIT_CHANNEL: &str = "mekanism:mekasuit";

/// The window's tick: sprinting in MekaSuit pants with a Locomotive Boosting Unit and the energy for it
/// pushes the player forward (`ModuleLocomotiveBoostingUnit.tickClient`); the server pays for it.
fn mekasuit_sprint(game: &mut Game<'_>) {
    let Some(pants) = game.armor[2].as_ref().filter(|s| &*s.item == modules::PANTS) else {
        return;
    };
    let boost = modules::sprint_boost(&modules_of(pants));
    if boost <= 0.0 || stored_energy(pants) < modules::sprint_energy(boost) {
        return;
    }
    let Some(p) = game.player.as_mut().filter(|p| p.sprinting && !p.fall_flying && !p.flying) else {
        return;
    };
    let [x, z] = modules::sprint_push(boost, p.yaw, p.ground.on_ground, p.swimming);
    p.velocity.x += x;
    p.velocity.z += z;
    game.native_send_to_server(MEKASUIT_CHANNEL, &[1]);
}

/// On the server: the boost's energy comes out of the pants (`canFunction`).
fn mekasuit_sprint_paid(game: &mut Game<'_>) {
    let Some(pants) = game.armor[2].as_mut().filter(|s| &*s.item == modules::PANTS) else {
        return;
    };
    let cost = modules::sprint_energy(modules::sprint_boost(&modules_of(pants)));
    let stored = stored_energy(pants);
    set_energy(pants, stored.saturating_sub(cost));
}

/// Set while the Meka-Tool breaks the extra blocks of a vein or a blast, so those breaks don't start
/// another one.
static AREA_MINING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn is_vein_ore(game: &Game<'_>, name: &str) -> bool {
    let items = world_items!(game);
    items.block_in_tag(mekanism_core::miner::ORE_TAG, name) || items.block_in_tag("minecraft:logs", name)
}

/// `IBlastingItem.canBlastBlock`: not air, not a liquid, and breakable.
fn can_blast(game: &Game<'_>, at: [i32; 3]) -> bool {
    let facts = game.res.facts().get(game.world.block(at[0], at[1], at[2]));
    facts.hardness > 0.0 && !matches!(&*facts.name, "minecraft:water" | "minecraft:lava")
}

/// A Meka-Tool breaking `pos` (`ItemMekaTool.mineBlock`, outside creative): its Blasting Unit takes the
/// blocks around it on the face's plane (not while sneaking), and its Vein Mining Unit follows every
/// ore or log among them through the blocks of the same kind touching it, up to 128 of each. Each extra
/// block costs `area_energy` by how far it is; the work stops when the tool runs out.
pub(super) fn meka_tool_area(game: &mut Game<'_>, pos: [i32; 3]) {
    use std::sync::atomic::Ordering::Relaxed;
    if !game.native_on(MOD_ID) || game.creative || AREA_MINING.load(Relaxed) {
        return;
    }
    let Some(installed) = game.held().filter(|s| &*s.item == modules::MEKA_TOOL).map(modules_of) else {
        return;
    };
    let silk = installed.contains_key("mekanism:silk_touch_unit");
    let mut initial = vec![pos];
    let radius = modules::blast_radius(&installed);
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    if radius > 0 && !sneaking && can_blast(game, pos)
        && let Some((_, dir)) = game.aim()
    {
        let axis = (0..3).max_by(|&a, &b| dir[a].abs().total_cmp(&dir[b].abs())).unwrap_or(1);
        for o in modules::blast_offsets(radius, axis) {
            let at = [pos[0] + o[0], pos[1] + o[1], pos[2] + o[2]];
            if can_blast(game, at) {
                initial.push(at);
            }
        }
    }
    // `ModuleVeinMiningUnit.findPositions`, without extended mode: only ores spread.
    let mut found: Vec<([i32; 3], u32)> = Vec::new();
    if modules::veins(&installed) {
        let mut ores = std::collections::HashSet::new();
        for &at in &initial {
            let name = block_name(game, at).to_owned();
            if is_vein_ore(game, &name) {
                ores.insert(name);
            }
        }
        let max = initial.len() + modules::VEIN_COUNT * ores.len();
        let mut seen: std::collections::HashSet<[i32; 3]> = initial.iter().copied().collect();
        let mut frontier: std::collections::VecDeque<([i32; 3], u32)> = initial.iter().map(|&p| (p, 0)).collect();
        while let Some((at, d)) = frontier.pop_front() {
            found.push((at, d));
            if found.len() >= max {
                break;
            }
            let name = block_name(game, at).to_owned();
            if !ores.contains(&name) {
                continue;
            }
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let next = [at[0] + dx, at[1] + dy, at[2] + dz];
                        if !seen.contains(&next) && block_name(game, next) == name {
                            seen.insert(next);
                            frontier.push_back((next, d + 1));
                        }
                    }
                }
            }
        }
    } else {
        found = initial.iter().map(|&p| (p, 0)).collect();
    }
    if found.len() <= 1 {
        return;
    }
    AREA_MINING.store(true, Relaxed);
    for (at, d) in found {
        if at == pos {
            continue;
        }
        let facts = game.res.facts().get(game.world.block(at[0], at[1], at[2])).clone();
        if facts.hardness < 0.0 || &*facts.name == "minecraft:air" {
            continue;
        }
        let ore = is_vein_ore(game, &facts.name);
        let cost = modules::area_energy(silk, facts.hardness, d.max(1), ore);
        let Some(tool) = game.held_mut().as_mut().filter(|s| &*s.item == modules::MEKA_TOOL) else {
            break;
        };
        let stored = stored_energy(tool);
        if stored < cost {
            break;
        }
        set_energy(tool, stored - cost);
        let _ = game.break_block_as_player(at);
    }
    AREA_MINING.store(false, Relaxed);
}

/// The window tells the server, each tick its player is short of air, to run the helmet's Electrolytic
/// Breathing Unit; and once a second to run the Charge Distribution and Solar Recharging Units. The
/// server does the work on its own copy of the suit.
const BREATHING_CHANNEL: &str = "mekanism:mekasuit_breathing";
const CHARGE_CHANNEL: &str = "mekanism:mekasuit_charge";
const SOLAR_CHANNEL: &str = "mekanism:mekasuit_solar";
/// `Player.getMaxAirSupply`.
const MAX_AIR: i32 = 300;

static SUIT_TICKS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn worn_module(game: &Game<'_>, slot: usize, item: &str, module: &str) -> bool {
    game.armor[slot].as_ref().is_some_and(|s| &*s.item == item && modules_of(s).contains_key(module))
}

fn mekasuit_client(game: &mut Game<'_>) {
    let tick = SUIT_TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let short_of_air = game.player.as_ref().is_some_and(|p| p.air < MAX_AIR);
    if short_of_air && worn_module(game, 0, modules::HELMET, "mekanism:electrolytic_breathing_unit") {
        game.native_send_to_server(BREATHING_CHANNEL, &[1]);
    }
    if tick % 20 != 0 {
        return;
    }
    if worn_module(game, 1, modules::BODYARMOR, "mekanism:charge_distribution_unit") {
        game.native_send_to_server(CHARGE_CHANNEL, &[1]);
    }
    if worn_module(game, 0, modules::HELMET, "mekanismgenerators:solar_recharging_unit") {
        game.native_send_to_server(SOLAR_CHANNEL, &[1]);
    }
}

/// `ModuleElectrolyticBreathingUnit.tickServer`, under water: air for energy, and the hydrogen into a
/// jetpack on the chest.
fn mekasuit_breathe(game: &mut Game<'_>) {
    let Some(helmet) = game.armor[0].as_ref().filter(|s| &*s.item == modules::HELMET) else {
        return;
    };
    let rate = modules::breathing_rate(&modules_of(helmet));
    let stored = stored_energy(helmet);
    let Some(air) = game.player.as_ref().map(|p| p.air) else {
        return;
    };
    let room = game.armor[1].as_ref().filter(|s| is_jetpack(s)).map_or(0, |c| jetpack_capacity(c) - jetpack_fuel(c));
    let (oxygen, hydrogen, cost) = modules::breathe(rate, (MAX_AIR - air).max(0) as u32, room);
    if rate == 0 || cost == 0 || stored < cost {
        return;
    }
    if let Some(helmet) = game.armor[0].as_mut() {
        set_energy(helmet, stored - cost);
    }
    if let Some(p) = game.player.as_mut() {
        p.air = (p.air + oxygen as i32).min(MAX_AIR);
    }
    if hydrogen > 0
        && let Some(chest) = game.armor[1].as_mut()
    {
        let fuel = jetpack_fuel(chest);
        set_jetpack_fuel(chest, fuel + hydrogen);
    }
}

/// `ModuleChargeDistributionUnit.tickServer`, for the second since the last note: the body armor charges
/// the held item and then the rest of the inventory, at most 10 kFE a tick; then the worn suit pieces
/// share their energy evenly.
fn mekasuit_distribute(game: &mut Game<'_>) {
    let Some(body) = game.armor[1].as_ref().filter(|s| &*s.item == modules::BODYARMOR) else {
        return;
    };
    let mut available = stored_energy(body).min(modules::INVENTORY_CHARGE_RATE * 20);
    let mut given = 0;
    let selected = game.selected;
    let order = std::iter::once(selected).chain((0..game.inventory.len().min(36)).filter(|&i| i != selected));
    for i in order.collect::<Vec<_>>() {
        if available == 0 {
            break;
        }
        let Some(stack) = game.inventory.get_mut(i).and_then(|s| s.as_mut()) else {
            continue;
        };
        let Some(e) = item_energy_of(stack) else {
            continue;
        };
        let stored = stored_energy(stack);
        let put = e.capacity.saturating_sub(stored).min(e.rate * 20).min(available);
        if put > 0 {
            set_energy(stack, stored + put);
            available -= put;
            given += put;
        }
    }
    if let Some(body) = game.armor[1].as_mut() {
        let stored = stored_energy(body);
        set_energy(body, stored - given);
    }
    let pieces: Vec<usize> = (0..4).filter(|&i| game.armor[i].as_ref().is_some_and(|s| modules::is_container(&s.item))).collect();
    if pieces.len() < 2 {
        return;
    }
    let total: u64 = pieces.iter().filter_map(|&i| game.armor[i].as_ref()).map(stored_energy).sum();
    let caps: Vec<u64> = pieces.iter().map(|&i| game.armor[i].as_ref().and_then(item_energy_of).map_or(0, |e| e.capacity)).collect();
    for (&i, share) in pieces.iter().zip(modules::split_evenly(total, &caps)) {
        if let Some(piece) = game.armor[i].as_mut() {
            set_energy(piece, share);
        }
    }
}

/// `ModuleSolarRechargingUnit.tickServer`, for the second since the last note: the helmet charges as a
/// solar panel above the player's eyes does, 500 FE a tick at its peak for each installed unit.
fn mekasuit_solar(game: &mut Game<'_>) {
    let Some(helmet) = game.armor[0].as_ref().filter(|s| &*s.item == modules::HELMET) else {
        return;
    };
    let units = modules_of(helmet).get("mekanismgenerators:solar_recharging_unit").copied().unwrap_or(0);
    let capacity = item_energy_of(helmet).map_or(0, |e| e.capacity);
    let stored = stored_energy(helmet);
    let Some(at) = game.player.as_ref().map(|p| [p.position.x, p.position.y + 1.62 + 0.2, p.position.z].map(|v| v.floor() as i32)) else {
        return;
    };
    let made = sun_production(game, at, modules::SOLAR_RATE * u64::from(units)) * 20;
    if made > 0
        && let Some(helmet) = game.armor[0].as_mut()
    {
        set_energy(helmet, (stored + made).min(capacity.max(stored)));
    }
}

/// `ItemMekaTool.hurtEnemy` with its Attack Amplification Unit, outside creative: the unit's damage on
/// top of the hit, scaled by the attack strength as the attribute would be, and its energy spent; with
/// too little energy, less damage for what there is.
pub(super) fn player_attack(game: &mut Game<'_>, strength: f32, amount: &mut f32) {
    if !game.native_on(MOD_ID) {
        return;
    }
    let creative = game.creative;
    let Some(tool) = game.held_mut().as_mut().filter(|s| &*s.item == modules::MEKA_TOOL) else {
        return;
    };
    let stored = stored_energy(tool);
    let bonus = modules::attack_bonus(&modules_of(tool));
    let (extra, cost) = modules::attack(bonus, if creative { u64::MAX } else { stored });
    if extra <= 0.0 {
        return;
    }
    // `Player.attack` scales the attribute's damage by 0.2 + strength² * 0.8.
    *amount += extra * (0.2 + strength * strength * 0.8);
    if !creative {
        set_energy(tool, stored - cost);
    }
}

/// A hit on a player (hook point `native_player_damage`, API 28): the worn MekaSuit pieces pay for the
/// damage they took and say how much of the next hits they take (`ItemMekaSuitArmor.getDamageAbsorbed`).
pub(super) fn player_damage(game: &mut Game<'_>, absorbed: f32, share: &mut f32) {
    if !game.native_on(MOD_ID) {
        return;
    }
    charge_on_pad(game);
    let pieces = [modules::HELMET, modules::BODYARMOR, modules::PANTS, modules::BOOTS];
    let stored = |game: &Game<'_>| -> [Option<u64>; 4] {
        std::array::from_fn(|i| {
            game.armor[i].as_ref().filter(|s| &*s.item == pieces[i]).map(stored_energy)
        })
    };
    let creative = game.creative;
    let costs = modules::suit_costs(absorbed, stored(game));
    if !creative {
        for (i, cost) in costs.into_iter().enumerate() {
            if cost > 0
                && let Some(piece) = game.armor[i].as_mut()
            {
                let e = stored_energy(piece);
                set_energy(piece, e.saturating_sub(cost));
            }
        }
    }
    *share = modules::suit_share(stored(game));
}

/// A player standing on a chargepad gets the first item in their inventory (then armor, then the
/// offhand) that can take energy charged with ten ticks' worth, as `TileEntityChargepad` charges one
/// item a tick. Runs with the player's state swapped in, every 10 ticks.
fn charge_on_pad(game: &mut Game<'_>) {
    if game.spectator() {
        return;
    }
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let (x, y, z) = (p.position.x.floor() as i32, p.position.y, p.position.z.floor() as i32);
    let pad_y = chargepad::pad_below(y);
    let at = [x, pad_y, z];
    if block_name(game, at) != chargepad::CHARGEPAD || !chargepad::on_pad(y, pad_y) {
        return;
    }
    let Some(pad) = game.native_with_mod_block_entity(at, |m| get_long(&m.nbt, "energy")) else {
        return;
    };
    if pad == 0 {
        return;
    }
    let slots = (0..game.inventory.len()).map(Some).chain((0..4).map(|i| Some(100 + i))).chain([None]);
    let mut given = 0;
    for slot in slots {
        let stack = match slot {
            Some(i) if i >= 100 => game.armor.get_mut(i - 100).and_then(Option::as_mut),
            Some(i) => game.inventory.get_mut(i).and_then(Option::as_mut),
            None => game.offhand.as_mut(),
        };
        let Some(stack) = stack else {
            continue;
        };
        let Some(e) = item_energy_of(stack) else {
            continue;
        };
        let stored = stored_energy(stack).min(e.capacity);
        given = chargepad::charge(pad, e.capacity, stored, e.rate);
        if given > 0 {
            set_energy(stack, stored + given);
            break;
        }
    }
    if given > 0 {
        let now = game.world.game_time.max(0) as u64;
        game.native_with_mod_block_entity(at, |m| {
            put_long(&mut m.nbt, "energy", pad - given);
            put_long(&mut m.nbt, "charged_at", now);
        });
    }
}

// --- Lasers -----------------------------------------------------------------------------------------

/// Blocks a laser's beam passes through (`ClipContext.Block.OUTLINE` with no fluids): those with no
/// outline.
fn beam_passes(name: &str) -> bool {
    matches!(
        name,
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | "minecraft:water" | "minecraft:lava"
            | "minecraft:bubble_column" | "minecraft:light" | "minecraft:structure_void"
    )
}

/// A laser, amplifier or tractor beam's tick (`TileEntityBasicLaser.onUpdateServer`): it fires what it
/// can along its facing, up to 64 blocks. The beam's energy goes into an amplifier or tractor beam it
/// meets, or digs into the block it meets until that breaks; a tractor beam keeps the drops.
fn tick_laser(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = if entity.id == laser::LASER { laser::LASER_STORAGE } else { laser::RECEPTOR_STORAGE };
    let stored = get_long(&entity.nbt, "energy").min(capacity);
    let max = match get_long(&entity.nbt, "max_threshold") {
        0 => laser::RECEPTOR_STORAGE,
        n => n,
    };
    let firing = laser::to_fire(&entity.id, stored, get_long(&entity.nbt, "min_threshold"), max);
    let facing = game.native_block_property(pos, "facing").and_then(|f| direction(&f));
    let (Some(d), true) = (facing, firing > 0) else {
        put_long(&mut entity.nbt, "progress", 0);
        put_long(&mut entity.nbt, "energy", stored);
        return Some(false);
    };
    put_long(&mut entity.nbt, "energy", stored - firing);
    let hit = (1..=laser::RANGE).map(|k| [pos[0] + d[0] * k, pos[1] + d[1] * k, pos[2] + d[2] * k]).find(|&at| {
        let state = game.world.block(at[0], at[1], at[2]);
        !beam_passes(&game.res.facts().get(state).name)
    });
    // The beam reaches the face of the block it hits, or its full range.
    let length = hit.map_or(f64::from(laser::RANGE), |at| {
        let k = (0..3).map(|i| (at[i] - pos[i]) * d[i]).sum::<i32>();
        f64::from(k) - 1.002
    });
    let firing = laser_hit_entities(game, entity, pos, d, length, firing);
    if firing == 0 {
        return Some(true);
    }
    let Some(at) = hit else {
        put_long(&mut entity.nbt, "progress", 0);
        return Some(true);
    };
    // A receptor takes the energy; what it has no room for is lost.
    let name = block_name(game, at).to_owned();
    if laser::is_receptor(&name) {
        game.native_with_mod_block_entity(at, |m| {
            let e = get_long(&m.nbt, "energy");
            put_long(&mut m.nbt, "energy", e.saturating_add(firing).min(laser::RECEPTOR_STORAGE));
        });
        return Some(true);
    }
    // Coordinates can be negative: read them as they were written.
    let coord = |k: &str| match entity.nbt.get(&JavaString::from(k)) {
        Some(Tag::Int(v)) => Some(*v),
        _ => None,
    };
    let target = [coord("dig_x"), coord("dig_y"), coord("dig_z")];
    let mut progress = get_long(&entity.nbt, "progress");
    if target != at.map(Some) {
        progress = 0;
        for (k, v) in ["dig_x", "dig_y", "dig_z"].into_iter().zip(at) {
            entity.nbt.insert(k.into(), Tag::Int(v));
        }
    }
    let state = game.world.block(at[0], at[1], at[2]);
    let hardness = game.res.facts().get(state).hardness;
    let (progress, broke) = laser::dig(progress, firing, hardness);
    put_long(&mut entity.nbt, "progress", progress);
    if broke {
        laser_break(game, entity, at);
    }
    Some(true)
}

/// The mobs and dropped items in the beam, nearest first (`fireLaser`): a tractor beam takes the items
/// into its rows; each mob takes the damage the energy left does (2,500 FE a point) and burns. The
/// energy left after them goes on to the block.
fn laser_hit_entities(game: &mut Game<'_>, entity: &mut ModBlockEntity, pos: [i32; 3], d: [i32; 3], length: f64, firing: u64) -> u64 {
    let (min, max) = laser::beam_box(pos, d, length);
    let from = pos.map(|c| f64::from(c) + 0.5);
    let mut left = firing;
    for hit in game.native_entities_in(min, max, from) {
        if hit.kind == "minecraft:item" {
            if entity.id == laser::TRACTOR_BEAM
                && let Some(stack) = game.native_take_item_entity(hit.id)
                && let Some(rest) = store_drop(game, &mut entity.items, stack)
            {
                // What has no room drops where the item was.
                let at = hit.position.map(|c| c.floor() as i32);
                let block = horizon_core::position::BlockPos::new(at[0], at[1], at[2]);
                game.with_level(|_, level, _| level.drop_lost_contents(block, vec![Some(rest)]));
            }
            continue;
        }
        let (damage, fire) = laser::damage(left);
        let lost = game.native_hurt_mob(hit.id, damage, fire);
        left -= laser::spent(lost, left);
        if left == 0 {
            break;
        }
    }
    left
}

/// The beam breaks the block at `at` (`withFakePlayer`, with a disassembler's drops): they fall where it
/// was, or go into a tractor beam's rows (what has no room falls there too).
fn laser_break(game: &mut Game<'_>, entity: &mut ModBlockEntity, at: [i32; 3]) {
    let state = game.world.block(at[0], at[1], at[2]);
    let registry = game.pack.assets.registry;
    let drops = match &game.world_tables {
        Some(t) => &t.items,
        None => game.res.items(),
    }
    .block_drops_with(registry, state, None, &[], &mut game.world.random);
    let block = horizon_core::position::BlockPos::new(at[0], at[1], at[2]);
    game.with_level(|b, level, ticks| b.destroy_block(level, ticks, block, false));
    let mut lost = Vec::new();
    for stack in drops.into_iter().filter(|s| s.count > 0 && &*s.item != "minecraft:air") {
        let left = if entity.id == laser::TRACTOR_BEAM { store_drop(game, &mut entity.items, stack) } else { Some(stack) };
        if let Some(left) = left {
            lost.push(Some(left));
        }
    }
    if !lost.is_empty() {
        game.with_level(|_, level, _| level.drop_lost_contents(block, lost));
    }
}

/// Puts `stack` into `slots`, onto stacks of the same item first, then into empty slots; gives back
/// what has no room.
fn store_drop(game: &Game<'_>, slots: &mut Vec<Option<ItemStack>>, mut stack: ItemStack) -> Option<ItemStack> {
    if slots.len() < laser::TRACTOR_SLOTS {
        slots.resize(laser::TRACTOR_SLOTS, None);
    }
    let max = max_stack(game, &stack.item);
    for slot in slots.iter_mut().flatten() {
        if slot.item == stack.item && slot.extra == stack.extra && slot.count < max {
            let put = (max - slot.count).min(stack.count);
            slot.count += put;
            stack.count -= put;
            if stack.count == 0 {
                return None;
            }
        }
    }
    if let Some(empty) = slots.iter_mut().find(|s| s.is_none()) {
        *empty = Some(stack);
        return None;
    }
    Some(stack)
}

/// A right click with an empty hand on a laser or amplifier, or a sneaking one on a tractor beam, shows
/// its energy and digging; sneaking on an amplifier with a stack sets its lower threshold to the
/// stack's size times 10,000 (an empty sneaking hand clears it).
pub(super) fn use_laser(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) {
        return false;
    }
    let name = block_name(game, pos).to_owned();
    let k = kind(&name);
    if !matches!(k, Some(Kind::Laser | Kind::LaserAmplifier | Kind::TractorBeam)) {
        return false;
    }
    let held = game.inventory.get(game.selected).cloned().flatten();
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    if k == Some(Kind::LaserAmplifier) && sneaking {
        let min = held.as_ref().map_or(0, |h| u64::from(h.count) * 10_000);
        game.native_with_mod_block_entity(pos, |m| put_long(&mut m.nbt, "min_threshold", min));
        tell(game, format!("Laser Amplifier lower threshold: {min} FE"));
        return true;
    }
    if held.is_some() || (k == Some(Kind::TractorBeam) && !sneaking) {
        return false;
    }
    let Some((energy, progress)) = game.native_with_mod_block_entity(pos, |m| (get_long(&m.nbt, "energy"), get_long(&m.nbt, "progress"))) else {
        return false;
    };
    tell(game, format!("{name}: {energy} FE stored, {progress} FE into the block it digs"));
    true
}

// --- Seismic Vibrator -------------------------------------------------------------------------------

/// The vibrators running now (`Mekanism.activeVibrators`), by dimension.
static VIBRATORS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<(super::Dim, [i32; 3])>>> =
    std::sync::LazyLock::new(Default::default);

/// A vibrator runs while it has its 5 FE for the tick.
fn tick_vibrator(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let mut energy = EnergyContainer::new(seismic::VIBRATOR_STORAGE, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(energy.capacity);
    let active = energy.consume(seismic::VIBRATOR_USAGE);
    put_long(&mut entity.nbt, "energy", energy.stored);
    if let Ok(mut all) = VIBRATORS.lock() {
        if active {
            all.insert((game.dim, pos));
        } else {
            all.remove(&(game.dim, pos));
        }
    }
    Some(active)
}

/// `ItemSeismicReader.use`: in a chunk a running vibrator shakes, for 250 FE (free in creative), it
/// lists the ores in the column under the player, top first, and how deep the column goes.
fn seismic_reader_use(game: &mut Game<'_>, claimed: &mut bool) {
    *claimed = true;
    let Some(at) = game.player.as_ref().map(|p| [p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32]) else {
        return;
    };
    let dim = game.dim;
    let vibrating = VIBRATORS.lock().map(|all| all.iter().any(|&(d, v)| d == dim && seismic::same_chunk(v, [at[0], at[2]]) && block_name(game, v) == seismic::VIBRATOR)).unwrap_or(false);
    if !vibrating {
        tell(game, "Seismic Reader: no vibrations detected".to_owned());
        return;
    }
    if !game.creative {
        let Some(held) = game.held_mut().as_mut().filter(|s| &*s.item == seismic::READER) else {
            return;
        };
        let stored = stored_energy(held);
        if stored < seismic::READER_USAGE {
            tell(game, "Seismic Reader: needs energy".to_owned());
            return;
        }
        set_energy(held, stored - seismic::READER_USAGE);
    }
    let bottom = game.world.bottom_y();
    let column: Vec<(i32, String)> = (bottom..at[1])
        .rev()
        .map(|y| {
            let state = game.world.block(at[0], y, at[2]);
            (y, game.res.facts().get(state).name.to_string())
        })
        .collect();
    let ores: Vec<String> = seismic::runs(&column)
        .into_iter()
        .filter(|(name, ..)| world_items!(game).block_in_tag(mekanism_core::miner::ORE_TAG, name))
        .map(|(name, top, low)| if top == low { format!("{name} at y {top}") } else { format!("{name} at y {top} to {low}") })
        .collect();
    tell(game, format!("Seismic Reader: {} blocks down to y {bottom} at x {}, z {}", column.len(), at[0], at[2]));
    if ores.is_empty() {
        tell(game, "  no ores".to_owned());
    }
    for line in ores {
        tell(game, format!("  {line}"));
    }
}

// --- Bio-Generator and Fuelwood Heater --------------------------------------------------------------

/// The bioethanol an item gives the bio-generator (`biofuelFromItem`).
fn bio_fuel_of(game: &Game<'_>, item: &str) -> u64 {
    use mekanism_core::generator as g;
    let items = world_items!(game);
    let in_tag = |tag: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
    if in_tag("c:fuels/bio") {
        g::BIO_FUEL_PER_ITEM
    } else if in_tag("c:fuels/block/bio") {
        9 * g::BIO_FUEL_PER_ITEM
    } else {
        0
    }
}

/// A bio-generator tick: a bio fuel in the middle slot becomes bioethanol when it all fits, pipes
/// bring more, and 1 mB a tick makes 25 FE while there is room for it.
fn tick_bio_generator(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    use mekanism_core::generator as g;
    let mut energy = EnergyContainer::new(g::BIO_STORAGE, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(g::BIO_STORAGE);
    let mut fuel = get_tank(&entity.nbt, "fuel", g::BIO_TANK);
    if let Some(Some(item)) = entity.items.get(1).cloned() {
        let amount = bio_fuel_of(game, &item.item);
        if amount > 0 && fuel.accepts(g::BIOETHANOL) && fuel.capacity - fuel.amount >= amount {
            fuel.insert(g::BIOETHANOL, amount);
            entity.items[1] = (item.count > 1).then(|| ItemStack { count: item.count - 1, ..item });
        }
    }
    pull_fluid(game, pos, &mut fuel, g::BIOETHANOL);
    let (used, made) = g::bio_tick(fuel.amount, energy.capacity - energy.stored);
    fuel.extract(used);
    energy.insert(made, Action::Execute);
    charge_slot(entity, 0, &mut energy);
    push_energy(game, pos, &mut energy, None, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "fuel", &fuel);
    Some(made > 0)
}

/// A fuelwood heater tick: when its burn runs out it burns the next furnace fuel of its slot; while it
/// burns it makes 400 heat a tick for the boiler beside it (kept up to 20 ticks' worth, as the
/// resistive heater's).
fn tick_fuelwood_heater(game: &mut Game<'_>, entity: &mut ModBlockEntity) -> Option<bool> {
    let mut burn = get_int(&entity.nbt, "burn_time");
    if burn == 0
        && let Some(Some(fuel)) = entity.items.first().cloned()
    {
        let time = game.res.providers().burn_time(world_items!(game), &fuel, false).max(0) as u32;
        if time > 0 {
            burn = time;
            entity.items[0] = if fuel.count > 1 {
                Some(ItemStack { count: fuel.count - 1, ..fuel })
            } else if &*fuel.item == "minecraft:lava_bucket" {
                Some(stack_of("minecraft:bucket", 1))
            } else {
                None
            };
        }
    }
    let active = burn > 0;
    let mut heat = get_double(&entity.nbt, "heat_out").unwrap_or(0.0);
    if active {
        burn -= 1;
        heat = (heat + boiler::HEAT_PER_FUEL_TICK).min(boiler::HEAT_PER_FUEL_TICK * 20.0);
    }
    put_int(&mut entity.nbt, "burn_time", burn);
    entity.nbt.insert("heat_out".into(), Tag::Double(heat.to_bits()));
    Some(active)
}

// --- Pressurized Reaction Chamber -------------------------------------------------------------------

const REACTION_CHAMBER: &str = "mekanism:pressurized_reaction_chamber";
/// `TileEntityPressurizedReactionChamber.MAX_FLUID` and `MAX_GAS`.
const REACTION_TANK: u64 = 10_000;
/// `UsageConfig.pressurizedReactionBase`: 2 FE a tick, plus the recipe's own.
const REACTION_USAGE: u64 = 2;

/// The energy it holds: four ticks of its base usage, or one tick of the current recipe's when that is
/// more (`AttributeEnergy.getStorage`).
fn reaction_capacity(nbt: &Compound) -> u64 {
    (machine::STORAGE_MULTIPLIER * REACTION_USAGE).max(REACTION_USAGE + get_long(nbt, "recipe_energy"))
}

/// A reaction chamber tick: while the first slot, the water and the chemical make a recipe and there is
/// room for its products, it works through the recipe's duration, then takes the inputs and gives the
/// item to the last slot and the chemical to its output tank, which tubes beside it empty.
fn tick_reaction(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let mut fluid = get_tank(&entity.nbt, "fluid", REACTION_TANK);
    let mut input = get_tank(&entity.nbt, "input", REACTION_TANK);
    let mut output = get_tank(&entity.nbt, "output", REACTION_TANK);
    pull_fluid(game, pos, &mut fluid, "minecraft:water");
    let items = world_items!(game);
    let in_tag = |tag: &str, item: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
    let stack = entity.items.first().cloned().flatten();
    let recipe = stack.as_ref().and_then(|s| {
        mekanism_core::recipes::find_reaction(&s.item, s.count, fluid.chemical.as_deref(), input.chemical.as_deref(), &in_tag)
    });
    put_long(&mut entity.nbt, "recipe_energy", recipe.map_or(0, |r| r.energy));
    let capacity = reaction_capacity(&entity.nbt);
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut progress = get_int(&entity.nbt, "progress");
    let out = entity.items.get(2).cloned().flatten();
    let mut active = false;
    let mut ticks = 0;
    match (recipe, stack) {
        (Some(r), Some(stack)) => {
            ticks = r.duration.max(1);
            let item_fits = r.output.is_none_or(|(id, n)| {
                let max = items.get(id).map_or(64, |i| i.max_stack);
                out.as_ref().is_none_or(|o| &*o.item == id && o.count + n <= max)
            });
            let gas_fits = r.output_chemical.is_none_or(|(id, n)| output.accepts(id) && output.amount + n <= output.capacity);
            if item_fits
                && gas_fits
                && fluid.amount >= r.fluid_amount
                && input.amount >= r.chemical_amount
                && energy.consume(REACTION_USAGE + r.energy)
            {
                active = true;
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    fluid.extract(r.fluid_amount);
                    input.extract(r.chemical_amount);
                    let left = stack.count - r.item_count;
                    entity.items[0] = (left > 0).then(|| ItemStack { count: left, ..stack });
                    if let Some((id, n)) = r.output {
                        entity.items[2] = Some(match out {
                            Some(o) => ItemStack { count: o.count + n, ..o },
                            None => stack_of(id, n),
                        });
                    }
                    if let Some((id, n)) = r.output_chemical {
                        output.insert(id, n);
                    }
                }
            }
        }
        _ => progress = 0,
    }
    push_chemical(game, pos, &mut output, None, u64::MAX, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "fluid", &fluid);
    put_tank(&mut entity.nbt, "input", &input);
    put_tank(&mut entity.nbt, "output", &output);
    entity.data = vec![(energy.stored * 1000 / capacity.max(1)) as i32, 1000, progress as i32, ticks as i32];
    Some(active)
}

// --- Frequency selector (GuiFrequencySelector) ------------------------------------------------------

/// The teleporters' and entangloporters' frequency packets (`PacketSetTileFrequency`,
/// `PacketSetFrequencyColor`).
const FREQUENCY_CHANNEL: &str = "mekanism:frequency";

/// Which frequency list a block uses (`FrequencyTypes.TELEPORTER`, `FrequencyTypes.INVENTORY`).
fn freq_type(block: &str) -> Option<u8> {
    match kind(block)? {
        Kind::Teleporter => Some(0),
        Kind::Entangloporter => Some(1),
        Kind::QioDriveArray | Kind::QioDashboard | Kind::QioImporter | Kind::QioExporter | Kind::QioRedstoneAdapter => Some(2),
        _ => None,
    }
}

/// The QIO blocks that pick their frequency on the selector screen their tab opens
/// (`GuiQIOTileFrequencySelect`).
fn is_qio_tile(block: &str) -> bool {
    freq_type(block) == Some(2)
}

thread_local! {
    /// The open QIO block's screen shows its frequency selector (set by the tab, cleared by the back
    /// button and when a screen opens).
    static QIO_SELECT: std::cell::Cell<Option<([i32; 3], u64)>> = const { std::cell::Cell::new(None) };
}

fn qio_select_open(block: &str) -> bool {
    is_qio_tile(block) && QIO_SELECT.get().is_some()
}

/// `GuiFrequencySelector`'s y offset on the QIO screen (17) over the teleporter's (14).
const QIO_SELECT_DY: f32 = 3.0;
/// The tab (`GuiQIOFrequencyTab`: at -26, 6, a 26 px holder with an 18 px button) and the back button.
const QIO_TAB: [f32; 4] = [-26.0, 6.0, 26.0, 26.0];
const QIO_TAB_BUTTON: [f32; 4] = [-22.0, 10.0, 18.0, 18.0];
const QIO_BACK: [f32; 4] = [6.0, 6.0, 14.0, 14.0];

thread_local! {
    /// The screen asked for last: its block's position and the tick it was last asked.
    static OPEN_SEEN: std::cell::Cell<Option<([i32; 3], u64)>> = const { std::cell::Cell::new(None) };
}

/// Asked every frame a screen is open: the first time for a screen, the saved window places load and
/// the pinned window opens again (`GuiMekanism` restores the pinned windows).
fn screen_opened(game: &Game<'_>, block: &str) {
    let now = game.ticks as u64;
    let here = game.container.map(|(x, y, z)| [x, y, z]);
    let seen = OPEN_SEEN.get();
    OPEN_SEEN.set(here.map(|p| (p, now)));
    if here.is_none() || seen.is_some_and(|(p, t)| Some(p) == here && now.saturating_sub(t) <= 10) {
        return;
    }
    load_moves(game);
    if window(block).is_none() && !is_filter_screen(block) {
        let t = tabs(block);
        let (sides, upgrades, transporter) = MOVES.with_borrow(|m| (m.pinned[WINDOW_SIDES], m.pinned[WINDOW_UPGRADES], m.pinned[WINDOW_TRANSPORTER]));
        if sides && t & TAB_SIDES != 0 {
            set_window(block, Some(Window::Sides(0)));
        } else if upgrades && t & TAB_UPGRADE != 0 {
            set_window(block, Some(Window::Upgrades(None)));
        } else if transporter && t & TAB_SIDES != 0 {
            set_window(block, Some(Window::Transporter));
        }
    }
}

/// Whether the open QIO block shows its selector now; a selector left open on a screen that closed
/// (not asked for a few ticks) or on another block is dropped. Asked every frame.
pub(super) fn screen_alt(game: &Game<'_>, block: &str, alt: &mut bool) {
    screen_opened(game, block);
    let now = game.ticks as u64;
    if kind(block) == Some(Kind::FusionPort) {
        if let Some((pos, seen, _)) = FUSION_VIEW.get() {
            let here = game.container.map(|(x, y, z)| [x, y, z]);
            if here != Some(pos) || now.saturating_sub(seen) > 10 {
                FUSION_VIEW.set(None);
            } else {
                FUSION_VIEW.set(Some((pos, now, FUSION_VIEW.get().map_or(1, |v| v.2))));
                *alt = true;
            }
        }
        return;
    }
    if kind(block) == Some(Kind::DigitalMiner) {
        if let Some((x, y, z)) = game.container {
            MINER_OPEN.set([x, y, z]);
        }
        miner_screen_alt(game, alt);
        return;
    }
    if matches!(kind(block), Some(Kind::FissionPort | Kind::BoilerValve | Kind::TurbineValve)) {
        if let Some((pos, seen)) = STATS_VIEW.get() {
            let here = game.container.map(|(x, y, z)| [x, y, z]);
            if here != Some(pos) || now.saturating_sub(seen) > 10 {
                STATS_VIEW.set(None);
            } else {
                STATS_VIEW.set(Some((pos, now)));
                *alt = true;
            }
        }
        return;
    }
    if !is_qio_tile(block) {
        return;
    }
    if let Some((pos, seen)) = QIO_SELECT.get() {
        let here = game.container.map(|(x, y, z)| [x, y, z]);
        if here != Some(pos) || now.saturating_sub(seen) > 10 {
            QIO_SELECT.set(None);
        } else {
            QIO_SELECT.set(Some((pos, now)));
            *alt = true;
        }
    }
}

/// The selector's screen (`GuiQIOTileFrequencySelect`): 176 x 155, the back button, the title beside
/// it, and the frequency selector 3 px lower than the teleporter's.
fn draw_qio_select(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, block: &str, origin: [f32; 2]) {
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, 176.0, 155.0]);
    let title = game.native_screen_title().unwrap_or_default();
    let tw = game.native_text_width(&title);
    game.native_text(batch, &title, [ox + 17.0 + (176.0 - 17.0 - tw) / 2.0, oy + 3.0], 0x40_4040, false);
    let back = [ox + QIO_BACK[0], oy + QIO_BACK[1], QIO_BACK[2], QIO_BACK[3]];
    draw_mek_button(game, batch, back, true, inside(mouse, QIO_BACK));
    sprite(game, batch, "button/back", [0.0, 0.0, 14.0, 14.0], back);
    draw_freq_selector(game, batch, block, [ox, oy + QIO_SELECT_DY]);
}

/// The tab on a QIO block's screen: the holder in the tab colour (white), the button and its icon.
fn draw_qio_tab(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    let [x, y] = [ox + QIO_TAB[0], oy + QIO_TAB[1]];
    nine_slice_tinted(game, batch, "holder_left", [26.0, 9.0], 4.0, [x, y, 26.0, 26.0], 0xFFFF_FFFF);
    let hovered = inside([cursor[0] - ox, cursor[1] - oy], QIO_TAB_BUTTON);
    let button = if hovered { "button_focus" } else { "button" };
    let b = [ox + QIO_TAB_BUTTON[0] + 1.0, oy + QIO_TAB_BUTTON[1], 18.0, 18.0];
    nine_slice_tinted(game, batch, button, [200.0, 20.0], 2.0, b, 0xFFFF_FFFF);
    sprite(game, batch, "button/frequency", [0.0, 0.0, 18.0, 18.0], b);
}

/// A press on the QIO screen's tab or, in the selector, the back button and the selector.
fn qio_select_click(game: &Game<'_>, block: &str, at: [f32; 2], right: bool) {
    if qio_select_open(block) {
        if !right && inside(at, QIO_BACK) {
            QIO_SELECT.set(None);
        } else {
            frequency_click(game, block, [at[0], at[1] - QIO_SELECT_DY], right);
        }
    } else if !right && inside(at, QIO_TAB_BUTTON) && let Some((x, y, z)) = game.container {
        QIO_SELECT.set(Some(([x, y, z], game.ticks as u64)));
    }
}

/// The frequencies this server knows, by type. A block puts its own back on the list as it ticks, so
/// the list comes back after a restart once the blocks on it load.
static FREQ_LISTS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<u8, Vec<freq::Entry>>>> =
    std::sync::LazyLock::new(Default::default);

/// Frequencies deleted since the server started, by type and key: a block still on one leaves it.
static FREQ_DELETED: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<(u8, String)>>> =
    std::sync::LazyLock::new(Default::default);

fn get_hex(nbt: &Compound, key: &str) -> u128 {
    get_string(nbt, key).and_then(|s| u128::from_str_radix(&s, 16).ok()).unwrap_or(0)
}

/// The frequency a block is on, from its NBT. One a name tag set before the selector existed is a
/// public frequency of that name.
fn block_frequency(nbt: &Compound) -> Option<freq::Entry> {
    let key = get_string(nbt, "frequency")?;
    Some(freq::Entry {
        name: get_string(nbt, "freq_name").unwrap_or(key),
        security: freq::Security::from_ordinal(i64::from(get_int(nbt, "freq_security"))),
        owner: get_hex(nbt, "freq_owner"),
        owner_name: get_string(nbt, "freq_owner_name").unwrap_or_default(),
        color: nbt.get(&JavaString::from("freq_color")).map_or(freq::DEFAULT_COLOR, |_| get_int(nbt, "freq_color") as u8),
    })
}

/// Puts a block on frequency `entry` (or on none).
fn put_block_frequency(nbt: &mut Compound, entry: Option<&freq::Entry>) {
    let Some(e) = entry else {
        for k in ["frequency", "freq_name", "freq_security", "freq_owner", "freq_owner_name", "freq_color"] {
            nbt.remove(&JavaString::from(k));
        }
        return;
    };
    nbt.insert("frequency".into(), Tag::String(e.key().as_str().into()));
    nbt.insert("freq_name".into(), Tag::String(e.name.as_str().into()));
    put_int(nbt, "freq_security", u32::from(e.security.ordinal()));
    nbt.insert("freq_owner".into(), Tag::String(format!("{:032x}", e.owner).as_str().into()));
    nbt.insert("freq_owner_name".into(), Tag::String(e.owner_name.as_str().into()));
    put_int(nbt, "freq_color", u32::from(e.color));
}

/// An entangloporter joins the frequency under `key`: its copy of what the frequency holds is the
/// frequency's own from now on.
fn qe_join(nbt: &mut Compound, key: &str) {
    let f = FREQUENCIES.lock().ok().and_then(|all| all.get(key).cloned()).unwrap_or_default();
    put_long(nbt, "shared_energy", f.energy);
    put_tank(nbt, "shared_fluid", &f.fluid);
    put_tank(nbt, "shared_chemical", &f.chemical);
}

fn entry_tag(e: &freq::Entry) -> Tag {
    let mut c = Compound::new();
    c.insert("n".into(), Tag::String(e.name.as_str().into()));
    c.insert("s".into(), Tag::Int(i32::from(e.security.ordinal())));
    c.insert("o".into(), Tag::String(format!("{:032x}", e.owner).as_str().into()));
    c.insert("on".into(), Tag::String(e.owner_name.as_str().into()));
    c.insert("c".into(), Tag::Int(i32::from(e.color)));
    Tag::Compound(c)
}

fn entry_of(tag: &Tag) -> Option<freq::Entry> {
    let Tag::Compound(c) = tag else {
        return None;
    };
    Some(freq::Entry {
        name: get_string(c, "n")?,
        security: freq::Security::from_ordinal(i64::from(get_int(c, "s"))),
        owner: get_hex(c, "o"),
        owner_name: get_string(c, "on").unwrap_or_default(),
        color: get_int(c, "c") as u8,
    })
}

/// A teleporter's or entangloporter's frequency after its tick: it leaves a deleted frequency, puts
/// its own on the list (with the list's colour), and sends players what its selector shows (its
/// `client` compound: its frequency, the frequencies, the teleporter's status) when that changed.
fn frequency_tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let Some(t) = freq_type(&entity.id) else {
        return;
    };
    let mut own = block_frequency(&entity.nbt);
    if let Some(e) = &own
        && FREQ_DELETED.lock().is_ok_and(|d| d.contains(&(t, e.key())))
    {
        put_block_frequency(&mut entity.nbt, None);
        own = None;
    }
    let list = {
        let Ok(mut lists) = FREQ_LISTS.lock() else {
            return;
        };
        let list = lists.entry(t).or_default();
        if let Some(e) = own.as_mut() {
            match list.iter().find(|l| l.key() == e.key()) {
                Some(known) if known.color != e.color => {
                    e.color = known.color;
                    put_int(&mut entity.nbt, "freq_color", u32::from(e.color));
                }
                Some(_) => {}
                None => list.push(e.clone()),
            }
        }
        list.clone()
    };
    let mut client = Compound::new();
    if let Some(e) = &own {
        client.insert("freq".into(), entry_tag(e));
    }
    client.insert("list".into(), Tag::List(list.iter().map(entry_tag).collect()));
    put_int(&mut client, "status", get_int(&entity.nbt, "status"));
    put_client(game, pos, entity, &["freq", "list", "status"], client);
}

/// Merges what one part of the port tells the screens into the block entity's `client` compound: the
/// `owned` keys are replaced by `new`'s, the others stay (a QIO block has the frequency's, its filters'
/// and its own). The viewers hear of it when it changed.
fn put_client(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, owned: &[&str], new: Compound) {
    let before = match entity.nbt.get(&JavaString::from("client")) {
        Some(Tag::Compound(c)) => c.clone(),
        _ => Compound::new(),
    };
    let mut client = before.clone();
    for k in owned {
        client.remove(&JavaString::from(*k));
    }
    for (k, v) in new.iter() {
        client.insert(k.clone(), v.clone());
    }
    if client != before || !entity.nbt.contains_key(&JavaString::from("client")) {
        entity.nbt.insert("client".into(), Tag::Compound(client));
        game.native_sync_block_entity(pos);
    }
}

/// The frequency's name as the screens show it.
fn frequency_label(nbt: &Compound) -> Option<String> {
    get_string(nbt, "freq_name").or_else(|| get_string(nbt, "frequency"))
}

/// A player's frequency packet, on the server: set the block's frequency (making it when it is new),
/// delete one of theirs, or step its colour. Players act only on a block within reach, and only on
/// frequencies they may (`Frequency.ownerMatches`).
fn frequency_payload(game: &mut Game<'_>, data: &[u8]) {
    let (Some(r), Some(player)) = (freq::Request::decode(data), game.native_acting_player()) else {
        return;
    };
    let near = (0..3).map(|k| (player.position[k] - f64::from(r.pos[k]) - 0.5).powi(2)).sum::<f64>() <= 64.0;
    let Some(t) = freq_type(block_name(game, r.pos)).filter(|_| near) else {
        return;
    };
    let owner = if r.security == freq::Security::Public && r.op != freq::Op::Set { r.owner } else { player.uuid };
    let key = freq::key(&r.name, r.security, owner);
    let Ok(mut lists) = FREQ_LISTS.lock() else {
        return;
    };
    let list = lists.entry(t).or_default();
    match r.op {
        freq::Op::Set => {
            let entry = match list.iter().find(|e| e.key() == key) {
                Some(e) => e.clone(),
                None => {
                    let e = freq::Entry {
                        name: r.name.clone(),
                        security: r.security,
                        owner,
                        owner_name: player.name.clone(),
                        color: freq::DEFAULT_COLOR,
                    };
                    list.push(e.clone());
                    e
                }
            };
            drop(lists);
            if let Ok(mut d) = FREQ_DELETED.lock() {
                d.remove(&(t, key.clone()));
            }
            game.native_with_mod_block_entity(r.pos, |m| {
                put_block_frequency(&mut m.nbt, Some(&entry));
                if t == 1 {
                    qe_join(&mut m.nbt, &key);
                }
            });
        }
        freq::Op::Remove => {
            let Some(at) = list.iter().position(|e| e.key() == key && e.owner == player.uuid) else {
                return;
            };
            list.remove(at);
            drop(lists);
            if let Ok(mut d) = FREQ_DELETED.lock() {
                d.insert((t, key.clone()));
            }
            if let Ok(mut all) = FREQUENCIES.lock() {
                all.remove(&key);
            }
            game.native_with_mod_block_entity(r.pos, |m| {
                if get_string(&m.nbt, "frequency").as_deref() == Some(key.as_str()) {
                    put_block_frequency(&mut m.nbt, None);
                }
            });
        }
        freq::Op::ColorNext | freq::Op::ColorPrevious => {
            if let Some(e) = list.iter_mut().find(|e| e.key() == key && e.owner == player.uuid) {
                e.color = freq::step_color(e.color, r.op == freq::Op::ColorNext);
            }
        }
    }
}

/// `TeleporterStatus`, as the teleporter's tick finds it: 0 ready, 1 no frame, 2 no link, 3 needs
/// energy (`GuiTeleporterStatus` shows no frequency apart).
fn teleporter_status(game: &mut Game<'_>, pos: [i32; 3], entity: &ModBlockEntity) -> u32 {
    if get_pos(&entity.nbt, "dir").is_none() {
        return 1;
    }
    let Some(f) = get_string(&entity.nbt, "frequency") else {
        return 2;
    };
    let from = [f64::from(pos[0]) + 0.5, f64::from(pos[1]), f64::from(pos[2]) + 0.5];
    match teleport_target(game, &f, from, Some(pos)) {
        None => 2,
        Some((to, _)) if get_long(&entity.nbt, "energy") < teleporter::energy_cost(from, to, true) => 3,
        Some(_) => 0,
    }
}

/// What the client heard of the open block's frequencies: its own, the list, the teleporter's status.
struct FreqView {
    own: Option<freq::Entry>,
    list: Vec<freq::Entry>,
    status: u32,
}

fn freq_view(game: &Game<'_>) -> Option<([i32; 3], FreqView)> {
    let (x, y, z) = game.container?;
    let pos = [x, y, z];
    let (_, client) = game.native_client_block_entity(pos)?;
    let list = match client.get(&JavaString::from("list")) {
        Some(Tag::List(l)) => l.iter().filter_map(entry_of).collect(),
        _ => vec![],
    };
    let own = client.get(&JavaString::from("freq")).and_then(entry_of);
    Some((pos, FreqView { own, list, status: get_int(client, "status") }))
}

fn own_uuid(game: &Game<'_>) -> u128 {
    game.remote.as_ref().map_or(0, |r| r.profile())
}

/// The selector's own state on this client (its buttons, list and text box live in the screen).
#[derive(Default)]
struct FreqUi {
    pos: [i32; 3],
    mode: freq::Security,
    selected: Option<usize>,
    scroll: usize,
    text: freq::TextBox,
    /// The mode followed the block's frequency once (`GuiFrequencySelector.tick`'s `init`).
    init: bool,
}

thread_local! {
    static FREQ_UI: std::cell::RefCell<FreqUi> = std::cell::RefCell::new(FreqUi::default());
}

/// The selector's state for the block at `pos`, fresh when another block's screen opened.
fn with_freq_ui<R>(pos: [i32; 3], view: &FreqView, f: impl FnOnce(&mut FreqUi) -> R) -> R {
    FREQ_UI.with_borrow_mut(|ui| {
        if ui.pos != pos {
            *ui = FreqUi { pos, ..FreqUi::default() };
        }
        if !ui.init {
            ui.init = true;
            if let Some(e) = &view.own {
                ui.mode = e.security;
            }
        }
        f(ui)
    })
}

/// The selector's places on the screen (`GuiFrequencySelector` at 27, 14).
const FREQ_MODES: [[f32; 4]; 3] = [[27.0, 14.0, 38.0, 20.0], [111.0, 14.0, 38.0, 20.0], [69.0, 14.0, 38.0, 20.0]];
const FREQ_LIST: [f32; 4] = [27.0, 36.0, 122.0, 42.0];
const FREQ_FIELD: [f32; 4] = [50.0, 113.0, 98.0, 11.0];
const FREQ_CHECK: [f32; 4] = [137.0, 113.0, 11.0, 11.0];
const FREQ_ROWS: usize = 4;
const TELEPORTER_STATUS: [f32; 4] = [6.0, 6.0, 18.0, 18.0];
const FREQ_COLOR: [f32; 4] = [132.0, 128.0, 16.0, 16.0];

/// The Set and Delete buttons: 50 wide beside the teleporter's colour, 60 otherwise.
fn freq_buttons(block: &str) -> [[f32; 4]; 2] {
    let w = if kind(block) == Some(Kind::Teleporter) { 50.0 } else { 60.0 };
    [[27.0, 127.0, w, 18.0], [29.0 + w, 127.0, w, 18.0]]
}

fn is_freq_screen(block: &str) -> bool {
    matches!(freq_type(block), Some(0 | 1))
}

/// A selector is showing: the teleporters' and entangloporters' own screens, or a QIO block's.
fn freq_open(block: &str) -> bool {
    is_freq_screen(block) || qio_select_open(block)
}

/// The listed frequencies in the selector's mode.
fn freq_shown<'a>(game: &Game<'_>, view: &'a FreqView, mode: freq::Security) -> Vec<&'a freq::Entry> {
    freq::visible(&view.list, mode, own_uuid(game))
}

fn send_frequency(game: &Game<'_>, op: freq::Op, pos: [i32; 3], e: &freq::Entry) {
    let r = freq::Request { op, pos, security: e.security, owner: e.owner, name: e.name.clone() };
    game.native_send_to_server(FREQUENCY_CHANNEL, &r.encode());
}

/// `setFrequency(GuiTextField)`: the box's name as a new frequency in the selector's mode.
fn submit_frequency_field(game: &Game<'_>, pos: [i32; 3], ui: &mut FreqUi) {
    if ui.text.value.is_empty() {
        return;
    }
    let e = freq::Entry { name: std::mem::take(&mut ui.text.value), security: ui.mode, owner: own_uuid(game), ..freq::Entry::default() };
    ui.text.cursor = 0;
    send_frequency(game, freq::Op::Set, pos, &e);
}

/// A press on the selector, on the client: the mode buttons, the list (a row selects it, the bar
/// scrolls it), the text box and its checkmark, Set, Delete, and the teleporter's colour.
fn frequency_click(game: &Game<'_>, block: &str, at: [f32; 2], right: bool) -> bool {
    let Some((pos, view)) = freq_view(game).filter(|_| freq_open(block)) else {
        return false;
    };
    let shown: Vec<freq::Entry> = with_freq_ui(pos, &view, |ui| freq_shown(game, &view, ui.mode).into_iter().cloned().collect());
    FREQ_UI.with_borrow_mut(|ui| {
        ui.text.focused = inside(at, FREQ_FIELD) && !inside(at, FREQ_CHECK);
        if right {
            if kind(block) == Some(Kind::Teleporter) && inside(at, FREQ_COLOR) && let Some(e) = &view.own {
                send_frequency(game, freq::Op::ColorPrevious, pos, e);
                return true;
            }
            return false;
        }
        for (mode, r) in freq::Security::ALL.into_iter().zip(FREQ_MODES) {
            if inside(at, r) {
                if ui.mode != mode {
                    ui.mode = mode;
                    ui.selected = None;
                    ui.scroll = 0;
                }
                return true;
            }
        }
        let [lx, ly, lw, lh] = FREQ_LIST;
        // The scroll bar's track: the list jumps to where it was pressed.
        if inside(at, [lx + lw - 6.0, ly, 6.0, lh]) {
            let hidden = shown.len().saturating_sub(FREQ_ROWS);
            let f = ((at[1] - ly - 2.0) / (lh - 4.0)).clamp(0.0, 1.0);
            ui.scroll = ((hidden as f32 + 0.5) * f) as usize;
            return true;
        }
        if inside(at, [lx + 1.0, ly + 1.0, lw - 8.0, lh - 2.0]) {
            let row = ui.scroll + ((at[1] - ly - 1.0) / 10.0) as usize;
            ui.selected = (row < shown.len()).then_some(row);
            return true;
        }
        if inside(at, FREQ_CHECK) {
            if !ui.text.value.is_empty() {
                submit_frequency_field(game, pos, ui);
            }
            return true;
        }
        if inside(at, FREQ_FIELD) {
            return true;
        }
        let [set, delete] = freq_buttons(block);
        let chosen = ui.selected.and_then(|i| shown.get(i));
        if inside(at, set) {
            if let Some(e) = chosen.filter(|e| view.own.as_ref() != Some(*e)) {
                send_frequency(game, freq::Op::Set, pos, e);
            }
            return true;
        }
        if inside(at, delete) {
            if let Some(e) = chosen.filter(|e| e.owner == own_uuid(game)) {
                send_frequency(game, freq::Op::Remove, pos, e);
                ui.selected = None;
            }
            return true;
        }
        if kind(block) == Some(Kind::Teleporter) && inside(at, FREQ_COLOR) {
            if let Some(e) = &view.own {
                send_frequency(game, freq::Op::ColorNext, pos, e);
            }
            return true;
        }
        false
    })
}

/// Whether the selector's text box has focus (hook point `native_screen_typing`).
pub(super) fn screen_typing(block: &str, typing: &mut bool) {
    if miner_cfg_open(block) {
        *typing |= miner_cfg_typing();
        return;
    }
    if freq_open(block) {
        *typing |= FREQ_UI.with_borrow(|ui| ui.text.focused);
    }
    if !qio_select_open(block) {
        *typing |= qio_typing(block) || filter_typing(block) || adapter_typing(block);
    }
    if is_od(block) {
        *typing |= OD_EDIT.with_borrow(|e| e.as_ref().is_some_and(|e| e.text.focused));
    }
    if fission_stats_open(block) {
        *typing |= STATS_TEXT.with_borrow(|t| t.focused);
    }
    if fusion_view(block) == Some(2) {
        *typing |= FUSION_TEXT.with_borrow(|t| t.focused);
    }
}

/// A key on the selector's focused text box (hook point `native_screen_key`): Enter sets the name as
/// a new frequency (`setEnterHandler`).
pub(super) fn screen_key(game: &mut Game<'_>, block: &str, key: &str, text: Option<&str>) {
    if miner_cfg_open(block) {
        miner_cfg_key(game, key, text);
        return;
    }
    if fission_stats_open(block) {
        fission_stats_key(game, key, text);
        return;
    }
    if fusion_view(block) == Some(2) {
        fusion_key(game, key, text);
        return;
    }
    let select = qio_select_open(block);
    if !select && kind(block) == Some(Kind::QioDashboard) && game.native_on(MOD_ID) {
        qio_key(game, key, text);
        return;
    }
    if !select && is_filter_screen(block) {
        filter_key(key, text);
        return;
    }
    if is_od(block) {
        od_key(game, key, text);
        return;
    }
    if !select && kind(block) == Some(Kind::QioRedstoneAdapter) {
        adapter_key(game, key, text);
        return;
    }
    if !freq_open(block) || !game.native_on(MOD_ID) {
        return;
    }
    let Some((pos, _)) = freq_view(game) else {
        return;
    };
    let game = &*game;
    FREQ_UI.with_borrow_mut(|ui| {
        if ui.pos != pos || !ui.text.focused {
            return;
        }
        if ui.text.key(key, text, freq::valid_char, freq::MAX_FREQ_LENGTH) == freq::Typed::Enter {
            submit_frequency_field(game, pos, ui);
        }
    });
}

/// A Mekanism button (`MekanismButton`): its background as active, hovered or inactive.
fn draw_mek_button(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, r: [f32; 4], active: bool, hovered: bool) {
    let name = if !active {
        "button_inactive"
    } else if hovered {
        "button_focus"
    } else {
        "button"
    };
    nine_slice_tinted(game, batch, name, [200.0, 20.0], 2.0, r, 0xFFFF_FFFF);
}

/// The teleporter's and the entangloporter's screen (`GuiTeleporter`, `GuiQuantumEntangloporter`):
/// 240 high, the frequency selector, and the teleporter's status, energy slot and power bar.
fn draw_frequency_screen(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, block: &str, data: &[i32], origin: [f32; 2]) {
    let [ox, oy] = origin;
    let at = |r: [f32; 4]| [ox + r[0], oy + r[1], r[2], r[3]];
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, 176.0, 240.0]);
    for row in 0..4 {
        let y = if row < 3 { 158.0 + 18.0 * row as f32 } else { 216.0 };
        for col in 0..9 {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 7.0 + 18.0 * col as f32, oy + y - 1.0, 18.0, 18.0]);
        }
    }
    let teleporter = kind(block) == Some(Kind::Teleporter);
    let view = freq_view(game);
    let title = game.native_screen_title().unwrap_or_default();
    let tw = game.native_text_width(&title);
    // `renderTitleTextWithOffset`: the teleporter's title between its status and its energy slot.
    let (left, right) = if teleporter { (24.0, 153.0) } else { (0.0, 176.0) };
    game.native_text(batch, &title, [ox + left + (right - left - tw) / 2.0, oy + 4.0], 0x40_4040, false);
    game.native_text(batch, "Inventory", [ox + 8.0, oy + 147.0], 0x40_4040, false);
    if teleporter {
        draw_mek_button(game, batch, at(TELEPORTER_STATUS), true, false);
        let status = match view.as_ref().map(|(_, v)| (v.own.is_some(), v.status)) {
            Some((true, 0)) => "ready",
            Some((true, 1)) => "no_frame",
            Some((true, 3)) => "needs_energy",
            Some((true, _)) => "no_link",
            _ => "no_frequency",
        };
        sprite(game, batch, &format!("teleporter/{status}"), [0.0, 0.0, 18.0, 18.0], at(TELEPORTER_STATUS));
        sprite(game, batch, "slot/power", [0.0, 0.0, 18.0, 18.0], [ox + 152.0, oy + 6.0, 18.0, 18.0]);
        let [x, y, w, h] = power_rect(PowerBar::Vertical([158.0, 26.0]));
        nine_slice(game, batch, "bar/base", 5.0, 1.0, [ox + x, oy + y, w, h]);
        let capacity = read_wide(data, DATA_CAPACITY);
        let energy = if capacity > 0 { read_wide(data, DATA_STORED) as f32 / capacity as f32 } else { 0.0 };
        let filled = energy.clamp(0.0, 1.0) * (h - 2.0);
        if filled > 0.0 {
            sprite(game, batch, "bar/vertical_power", [0.0, 0.0, 2.0, 2.0], [ox + x + 1.0, oy + y + h - 1.0 - filled, w - 2.0, filled]);
        }
    } else {
        // `GuiEnergyTab` and `GuiHeatTab` on the left.
        sprite(game, batch, "tab/energy_info", [0.0, 0.0, 26.0, 26.0], [ox - 26.0, oy + 137.0, 26.0, 26.0]);
        sprite(game, batch, "tab/heat_info_k", [0.0, 0.0, 26.0, 26.0], [ox - 26.0, oy + 109.0, 26.0, 26.0]);
    }
    draw_freq_selector(game, batch, block, origin);
}

/// The frequency selector (`GuiFrequencySelector`) drawn from `origin`: the mode buttons, the list, the
/// frequency in use, the name box with its checkmark, Set and Delete.
fn draw_freq_selector(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, block: &str, origin: [f32; 2]) {
    let [ox, oy] = origin;
    let at = |r: [f32; 4]| [ox + r[0], oy + r[1], r[2], r[3]];
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    let teleporter = kind(block) == Some(Kind::Teleporter);
    let Some((pos, view)) = freq_view(game) else {
        return;
    };
    with_freq_ui(pos, &view, |ui| {
        let shown = freq_shown(game, &view, ui.mode);
        if ui.selected.is_some_and(|s| s >= shown.len()) {
            ui.selected = None;
        }
        let hidden = shown.len().saturating_sub(FREQ_ROWS);
        ui.scroll = ui.scroll.min(hidden);
        // The mode buttons: the one in use is inactive.
        for (mode, r) in freq::Security::ALL.into_iter().zip(FREQ_MODES) {
            let active = ui.mode != mode;
            draw_mek_button(game, batch, at(r), active, active && inside(mouse, r));
            let icon = format!("security/button_{}", mode.name().to_lowercase());
            sprite(game, batch, &icon, [0.0, 0.0, 40.0, 16.0], at(r));
        }
        // The list (`GuiTextScrollList`): the inner screen, the scroll bar, the selection, the lines.
        let [lx, ly, lw, lh] = FREQ_LIST;
        nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 32.0, at(FREQ_LIST), 0xFFFF_FFFF);
        let (bar_x, bar_y, bar_max) = (lx + lw - 6.0, ly + 2.0, lh - 4.0);
        nine_slice_tinted(game, batch, "scroll/list_border", [6.0, 6.0], 2.0, [ox + bar_x - 1.0, oy + bar_y - 1.0, 6.0, bar_max + 2.0], 0xFFFF_FFFF);
        let thumb = if hidden > 0 { (ui.scroll as f32 / hidden as f32 * (bar_max - 4.0)).round() } else { 0.0 };
        sprite(game, batch, "scroll/list_bar", [0.0, 0.0, 4.0, 4.0], [ox + bar_x, oy + bar_y + thumb, 4.0, 4.0]);
        if let Some(sel) = ui.selected.filter(|s| (ui.scroll..ui.scroll + FREQ_ROWS).contains(s)) {
            let y = ly + 1.0 + (sel - ui.scroll) as f32 * 10.0;
            sprite(game, batch, "scroll/list_selection", [0.0, 0.0, 2.0, 2.0], [ox + lx + 1.0, oy + y, lw - 8.0, 10.0]);
        }
        for (i, e) in shown.iter().skip(ui.scroll).take(FREQ_ROWS).enumerate() {
            let line = freq::list_line(e, ui.mode);
            game.native_text_scaled(batch, &line, [ox + lx + 2.0, oy + ly + 2.0 + 10.0 * i as f32], 0x3C_FE9A, 0.8);
        }
        // The frequency in use (`renderForeground`).
        let (name, owner, security) = match &view.own {
            Some(e) => (format!("\u{a7}8{}", e.name), e.owner_name.clone(), e.security.name()),
            None => ("\u{a7}4None".to_owned(), "\u{a7}4None".to_owned(), "\u{a7}4None"),
        };
        let label = |batch: &mut horizon_client::hud::HudBatch, text: String, y: f32| {
            game.native_text(batch, &text, [ox + 27.0, oy + y], 0x40_4040, false);
        };
        label(batch, format!("Frequency: {name}"), 81.0);
        label(batch, format!("Owner: {owner}"), 91.0);
        label(batch, format!("Security: {security}"), 101.0);
        let set_w = game.native_text_width("Set:");
        game.native_text(batch, "Set:", [ox + 45.0 - set_w, oy + 115.0], 0x40_4040, false);
        // The text box over an inner screen, with its caret, and its checkmark.
        let [fx, fy, fw, fh] = FREQ_FIELD;
        nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 32.0, [ox + fx - 1.0, oy + fy - 1.0, fw + 2.0, fh + 2.0], 0xFFFF_FFFF);
        let text_at = [ox + fx + 2.0, oy + fy + 2.0];
        game.native_text(batch, &ui.text.value, text_at, 0xE0_E0E0, true);
        let blink = (game.ticks / 6) % 2 == 0;
        if ui.text.focused && blink {
            let before: String = ui.text.value.chars().take(ui.text.cursor).collect();
            let cx = text_at[0] + game.native_text_width(&before);
            if ui.text.cursor >= ui.text.value.chars().count() {
                game.native_text(batch, "_", [cx, text_at[1]], 0xE0_E0E0, true);
            } else {
                game.native_fill(batch, [cx, text_at[1] - 1.0, 1.0, 10.0], 0xFFD0_D0D0);
            }
        }
        let typed = !ui.text.value.is_empty();
        draw_mek_button(game, batch, at(FREQ_CHECK), typed, typed && inside(mouse, FREQ_CHECK));
        sprite(game, batch, "button/checkmark", [0.0, 0.0, 12.0, 12.0], at(FREQ_CHECK));
        // Set (a listed frequency other than the one in use) and Delete (one of the player's).
        let chosen = ui.selected.and_then(|i| shown.get(i).copied());
        let [set, delete] = freq_buttons(block);
        let can_set = chosen.is_some_and(|e| view.own.as_ref() != Some(e));
        let can_delete = chosen.is_some_and(|e| e.owner == own_uuid(game));
        for (r, label, active) in [(set, "Set", can_set), (delete, "Delete", can_delete)] {
            draw_mek_button(game, batch, at(r), active, active && inside(mouse, r));
            let w = game.native_text_width(label);
            let rgb = if active { 0xFF_FFFF } else { 0xA0_A0A0 };
            game.native_text(batch, label, [ox + r[0] + (r[2] - w) / 2.0, oy + r[1] + (r[3] - 8.0) / 2.0], rgb, true);
        }
        // The teleporter's colour (`ColorButton` in a slot).
        if teleporter {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 131.0, oy + 127.0, 18.0, 18.0]);
            if let Some(e) = &view.own {
                let rgb = freq::COLORS.get(usize::from(e.color)).map_or(0, |c| c.1);
                game.native_fill(batch, at(FREQ_COLOR), 0xFF00_0000 | rgb);
            }
        }
    });
}

/// The selector's tooltips: the mode buttons, the teleporter's status and colour, the power bar, the
/// entangloporter's energy tab.
fn frequency_tooltip(block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) -> bool {
    if !is_freq_screen(block) {
        return false;
    }
    let modes = ["Public Mode", "Private Mode", "Trusted Mode"];
    if let Some(i) = FREQ_MODES.iter().position(|r| inside(at, *r)) {
        lines.push(modes[i].to_owned());
        return true;
    }
    let fe = mekanism_core::energy::format_fe;
    if kind(block) == Some(Kind::Teleporter) {
        if inside(at, power_rect(PowerBar::Vertical([158.0, 26.0]))) {
            lines.push(format!("{}/{}", fe(read_wide(data, DATA_STORED)), fe(read_wide(data, DATA_CAPACITY))));
            return true;
        }
    } else if inside(at, [-26.0, 137.0, 26.0, 26.0]) {
        lines.push(format!("Storing: {}", fe(read_wide(data, DATA_STORED))));
        return true;
    }
    false
}

/// Tooltips that need what the client heard of the block: the teleporter's status and colour.
pub(super) fn frequency_tooltip_view(game: &Game<'_>, block: &str, at: [f32; 2], lines: &mut Vec<String>) {
    if kind(block) != Some(Kind::Teleporter) || !lines.is_empty() {
        return;
    }
    let Some((_, view)) = freq_view(game) else {
        return;
    };
    if inside(at, TELEPORTER_STATUS) {
        let status = match (view.own.is_some(), view.status) {
            (false, _) => "No frequency",
            (_, 0) => "Ready",
            (_, 1) => "No frame",
            (_, 3) => "Needs energy",
            _ => "No destination",
        };
        lines.push(status.to_owned());
    } else if inside(at, FREQ_COLOR)
        && let Some(e) = &view.own
    {
        lines.push(freq::COLORS.get(usize::from(e.color)).map_or("", |c| c.0).to_owned());
    }
}

/// The public frequency a name tag names, its owner the player using it.
fn name_tag_frequency(game: &Game<'_>, name: &str) -> freq::Entry {
    let player = game.native_acting_player();
    freq::Entry {
        name: name.to_owned(),
        security: freq::Security::Public,
        owner: player.as_ref().map_or(0, |p| p.uuid),
        owner_name: player.map(|p| p.name).unwrap_or_default(),
        color: freq::DEFAULT_COLOR,
    }
}

// --- Logistical Sorter screen (GuiLogisticalSorter, GuiFilterHolder) ----------------------------------

const SORTER_CHANNEL: &str = "mekanism:sorter";

/// The sorter's rectangles on its 276 x 254 screen.
const SORTER_INFO: [f32; 4] = [9.0, 17.0, 85.0, 140.0];
const SORTER_HOLDER: [f32; 4] = [95.0, 17.0, 158.0, 118.0];
const SORTER_NEW_HOLDER: [f32; 4] = [95.0, 135.0, 158.0, 22.0];
const SORTER_NEW: [f32; 4] = [96.0, 136.0, 156.0, 20.0];
const SORTER_BAR: [f32; 4] = [253.0, 17.0, 14.0, 140.0];
const SORTER_COLOR: [f32; 4] = [13.0, 137.0, 16.0, 16.0];
const SORTER_TOGGLES: [[f32; 4]; 3] = [[12.0, 46.0, 14.0, 14.0], [12.0, 76.0, 14.0, 14.0], [12.0, 106.0, 14.0, 14.0]];
const SORTER_ROWS: usize = 4;

fn is_sorter(block: &str) -> bool {
    kind(block) == Some(Kind::Sorter)
}

/// The QIO importer and exporter, whose screens list filters (`GuiQIOFilterHandler`).
fn is_qio_filter(block: &str) -> bool {
    matches!(kind(block), Some(Kind::QioImporter | Kind::QioExporter))
}

/// The screens with a filter list and filter windows: the sorter's and the QIO filter handlers'.
fn is_filter_screen(block: &str) -> bool {
    is_sorter(block) || is_qio_filter(block)
}

fn sorter_row(i: usize) -> [f32; 4] {
    [96.0, 18.0 + 29.0 * i as f32, 156.0, 29.0]
}

/// A row's up arrow, down arrow and enable radio (`MovableFilterButton`).
fn sorter_row_parts(i: usize) -> ([f32; 4], [f32; 4], [f32; 4]) {
    let [x, y, w, h] = sorter_row(i);
    ([x + w - 14.0, y + h / 2.0 - 8.0, 11.0, 7.0], [x + w - 14.0, y + h / 2.0 + 1.0, 11.0, 7.0], [x + w - 25.0, y + h / 2.0 - 4.0, 8.0, 8.0])
}

thread_local! {
    /// Whether the open screen is a QIO filter handler's (its windows are laid out differently).
    static FILTER_QIO: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The first filter the sorter's list shows, and whose sorter it is.
    static SORTER_SCROLL: std::cell::Cell<([i32; 3], usize)> = const { std::cell::Cell::new(([0; 3], 0)) };
}

fn sorter_scroll(pos: [i32; 3]) -> usize {
    let (at, scroll) = SORTER_SCROLL.get();
    if at == pos { scroll } else { 0 }
}

/// What a sorter shows its players, in its `client` compound.
struct SorterView {
    filters: Vec<String>,
    disabled: Vec<String>,
    single: bool,
    eject: bool,
    round_robin: bool,
    opts: std::collections::BTreeMap<String, sorter::Opts>,
    /// A QIO importer or exporter: its frequency, and what that holds (items, capacity, types, capacity).
    qio: Option<QioInfo>,
}

#[derive(Default)]
struct QioInfo {
    frequency: Option<String>,
    totals: [u64; 4],
}

fn strings_of(nbt: &Compound, key: &str) -> Vec<String> {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::List(l)) => l.iter().filter_map(|t| if let Tag::String(s) = t { s.to_string().ok() } else { None }).collect(),
        _ => vec![],
    }
}

fn string_list(v: &[String]) -> Tag {
    Tag::List(v.iter().map(|s| Tag::String(s.as_str().into())).collect())
}

fn sorter_view(game: &Game<'_>) -> Option<([i32; 3], SorterView)> {
    let (x, y, z) = game.container?;
    let (_, c) = game.native_client_block_entity([x, y, z])?;
    let flag = |k: &str| get_int(c, k) == 1;
    let qio = flag("qio").then(|| QioInfo { frequency: get_string(c, "frequency"), totals: [get_long(c, "items"), get_long(c, "item_cap"), get_long(c, "types"), get_long(c, "type_cap")] });
    FILTER_QIO.set(qio.is_some());
    Some(([x, y, z], SorterView { filters: strings_of(c, "filters"), disabled: strings_of(c, "disabled"), single: flag("single"), eject: flag("eject"), round_robin: flag("round_robin"), opts: match c.get(&JavaString::from("filter_opts")) { Some(Tag::String(o)) => Sorter::parse_opts(&o.to_string().unwrap_or_default()), _ => Default::default() }, qio }))
}

/// Sends the sorter's settings to the players looking at it when they change.
fn sorter_sync(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if !is_filter_screen(&entity.id) {
        return;
    }
    let s = load_sorter(&entity.nbt);
    let mut client = Compound::new();
    if is_qio_filter(&entity.id) {
        put_int(&mut client, "qio", 1);
        if let Some(f) = get_string(&entity.nbt, "frequency") {
            let label = frequency_label(&entity.nbt).unwrap_or_else(|| f.clone());
            let (net, at) = load_network(game, &f);
            if !at.is_empty() {
                let (items, item_cap, types, type_cap) = net.totals();
                put_long(&mut client, "items", items);
                put_long(&mut client, "item_cap", item_cap);
                put_long(&mut client, "types", types as u64);
                put_long(&mut client, "type_cap", type_cap as u64);
            }
            client.insert("frequency".into(), Tag::String(label.as_str().into()));
        }
    }
    client.insert("filters".into(), string_list(&s.filters));
    client.insert("disabled".into(), string_list(&s.disabled));
    client.insert("filter_opts".into(), Tag::String(s.save_opts().as_str().into()));
    put_int(&mut client, "single", u32::from(s.single_item));
    put_int(&mut client, "eject", u32::from(s.auto_eject));
    put_int(&mut client, "round_robin", u32::from(s.round_robin));
    put_client(
        game,
        pos,
        entity,
        &["filters", "disabled", "filter_opts", "single", "eject", "round_robin", "qio", "frequency", "items", "item_cap", "types", "type_cap"],
        client,
    );
}

/// A player's sorter packet, on the server: a switch, a filter moved, enabled, deleted or added from
/// the item in their hand (or its first tag). Only within reach.
fn sorter_payload(game: &mut Game<'_>, data: &[u8]) {
    let (Some(r), Some(player)) = (sorter::Request::decode(data), game.native_acting_player()) else {
        return;
    };
    let near = (0..3).map(|k| (player.position[k] - f64::from(r.pos[k]) - 0.5).powi(2)).sum::<f64>() <= 64.0;
    if !near || !is_filter_screen(block_name(game, r.pos)) {
        return;
    }
    let held = game.inventory.get(game.selected).cloned().flatten();
    let filter = held.as_ref().and_then(|h| match r.op {
        sorter::Op::AddItem => Some(h.item.to_string()),
        sorter::Op::AddMod => h.item.split_once(':').map(|(ns, _)| format!("@{ns}")),
        sorter::Op::AddTag => {
            let items = world_items!(game);
            let mut tags: Vec<&String> = items.item_tags.iter().filter(|(_, t)| t.contains(&*h.item)).map(|(k, _)| k).collect();
            tags.sort();
            tags.first().map(|t| format!("#{t}"))
        }
        _ => None,
    });
    let index = usize::from(r.index);
    game.native_with_mod_block_entity(r.pos, |m| {
        let mut s = load_sorter(&m.nbt);
        match r.op {
            sorter::Op::Single => s.single_item = !s.single_item,
            sorter::Op::RoundRobin => s.round_robin = !s.round_robin,
            sorter::Op::AutoEject => s.auto_eject = !s.auto_eject,
            sorter::Op::Up | sorter::Op::Down | sorter::Op::Top | sorter::Op::Bottom => {
                s.move_filter(index, r.op);
            }
            sorter::Op::Toggle => {
                s.toggle_enabled(index);
            }
            sorter::Op::Delete => {
                s.remove(index);
            }
            sorter::Op::AddItem | sorter::Op::AddTag | sorter::Op::AddMod => {
                if let Some(f) = filter.filter(|f| !s.filters.contains(f)) {
                    s.filters.push(f);
                }
            }
            sorter::Op::AllowDefault => {
                s.edit(index, |o| o.allow_default = !o.allow_default);
            }
            sorter::Op::SizeMode => {
                s.edit(index, |o| o.size_mode = !o.size_mode);
            }
            sorter::Op::ColorNext | sorter::Op::ColorPrevious => {
                let next = r.op == sorter::Op::ColorNext;
                s.edit(index, |o| o.color = sorter::Opts::step_color(o.color, next));
            }
            sorter::Op::ColorClear => {
                s.edit(index, |o| o.color = None);
            }
            sorter::Op::AddNamed => {
                if let Some(f) = sorter::Sorter::named_filter(r.index, &r.name).filter(|f| !s.filters.contains(f)) {
                    s.filters.push(f);
                }
            }
            sorter::Op::SetColor if r.value <= 16 => {
                s.edit(index, |o| o.color = r.value.checked_sub(1).map(|c| c as u8));
            }
            sorter::Op::SetColor => {}
            sorter::Op::Range => {
                let (lo, hi) = (r.value >> 8, r.value & 0xFF);
                if sorter::Opts::valid_range(lo, hi) {
                    s.edit(index, |o| (o.min, o.max) = (lo, hi));
                }
            }
        }
        save_sorter(&mut m.nbt, &s);
    });
}

fn send_sorter(game: &Game<'_>, op: sorter::Op, pos: [i32; 3], index: usize) {
    send_sorter_value(game, op, pos, index, 0);
}

fn send_sorter_value(game: &Game<'_>, op: sorter::Op, pos: [i32; 3], index: usize, value: u16) {
    let r = sorter::Request { op, pos, index: index.min(usize::from(u16::MAX)) as u16, value, name: String::new() };
    game.native_send_to_server(SORTER_CHANNEL, &r.encode());
}

/// A filter as the list names it: an item's name, or a tag's id.
fn filter_name(f: &str) -> String {
    match f.strip_prefix('#') {
        Some(tag) => tag.to_owned(),
        None if f.starts_with('@') => f[1..].to_owned(),
        None => {
            let path = f.split_once(':').map_or(f, |(_, p)| p);
            path.split('_').map(|w| w.chars().next().map_or(String::new(), |c| c.to_uppercase().chain(w.chars().skip(1)).collect())).collect::<Vec<_>>().join(" ")
        }
    }
}

/// A press on the sorter's screen: its three switches, a filter's arrows (shift: to the end), radio
/// and body (right click deletes it), the New Filter button (right click: for the held item's tag),
/// and the scroll bar.
fn sorter_click(game: &Game<'_>, block: &str, at: [f32; 2], right: bool) -> bool {
    let Some((pos, view)) = sorter_view(game) else {
        return false;
    };
    let shift = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) || game.keys.contains(&winit::keyboard::KeyCode::ShiftRight);
    if windowed(block, at, |wat| filter_window_click(game, pos, &view, wat, right)) {
        return true;
    }
    use sorter::Op;
    for (r, op) in SORTER_TOGGLES.iter().zip([Op::Single, Op::RoundRobin, Op::AutoEject]) {
        if inside(at, *r) {
            send_sorter(game, op, pos, 0);
            return true;
        }
    }
    if inside(at, SORTER_NEW) {
        // The New Filter button opens the select window (`GuiFilterHolder`); the held item's filters
        // stay on a right click, and on sneaking.
        if right {
            send_sorter(game, Op::AddTag, pos, 0);
        } else if shift {
            send_sorter(game, Op::AddMod, pos, 0);
        } else {
            FILTER_SELECT.set(Some(pos));
        }
        return true;
    }
    let first = sorter_scroll(pos);
    let hidden = view.filters.len().saturating_sub(SORTER_ROWS);
    if inside(at, SORTER_BAR) {
        let f = ((at[1] - SORTER_BAR[1] - 8.0) / (SORTER_BAR[3] - 16.0)).clamp(0.0, 1.0);
        SORTER_SCROLL.set((pos, ((hidden as f32 + 0.5) * f) as usize));
        return true;
    }
    for i in 0..SORTER_ROWS.min(view.filters.len().saturating_sub(first)) {
        let index = first + i;
        let (up, down, radio) = sorter_row_parts(i);
        let row = sorter_row(i);
        if !inside(at, row) {
            continue;
        }
        if inside(at, up) && index > 0 {
            send_sorter(game, if shift { Op::Top } else { Op::Up }, pos, index);
        } else if inside(at, down) && index + 1 < view.filters.len() {
            send_sorter(game, if shift { Op::Bottom } else { Op::Down }, pos, index);
        } else if inside(at, radio) {
            send_sorter(game, Op::Toggle, pos, index);
        } else if right {
            send_sorter(game, Op::Delete, pos, index);
        } else {
            // A click on the filter opens its window (`GuiSorterItemStackFilter` and the others).
            open_filter_window(&view, pos, index);
        }
        return true;
    }
    inside(at, SORTER_HOLDER) || inside(at, SORTER_INFO)
}

fn draw_sorter_screen(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    let [ox, oy] = origin;
    let at = |r: [f32; 4]| [ox + r[0], oy + r[1], r[2], r[3]];
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, 276.0, 254.0]);
    for row in 0..4 {
        let y = if row < 3 { 172.0 + 18.0 * row as f32 } else { 230.0 };
        for col in 0..9 {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 57.0 + 18.0 * col as f32, oy + y - 1.0, 18.0, 18.0]);
        }
    }
    let title = game.native_screen_title().unwrap_or_default();
    let tw = game.native_text_width(&title);
    game.native_text(batch, &title, [ox + (276.0 - tw) / 2.0, oy + 4.0], 0x40_4040, false);
    game.native_text(batch, "Inventory", [ox + 58.0, oy + 161.0], 0x40_4040, false);
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 32.0, at(SORTER_INFO), 0xFFFF_FFFF);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(SORTER_HOLDER), 0xFFFF_FFFF);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(SORTER_NEW_HOLDER), 0xFFFF_FFFF);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(SORTER_BAR), 0xFFFF_FFFF);
    let view = sorter_view(game);
    let (pos, view) = match view {
        Some((p, v)) => (p, v),
        None => ([0; 3], SorterView { filters: vec![], disabled: vec![], single: false, eject: false, round_robin: false, opts: Default::default(), qio: None }),
    };
    let on = |b: bool| if b { "\u{a7}aON" } else { "\u{a7}cOFF" };
    let small = |batch: &mut horizon_client::hud::HudBatch, text: &str, x: f32, y: f32| {
        game.native_text_scaled(batch, text, [ox + 9.0 + x + 1.0, oy + 17.0 + y + 3.0], 0x3C_FE9A, 0.8);
    };
    small(batch, &format!("Filters: {}", view.filters.len()), 0.0, 4.0);
    small(batch, "Force single item:", 0.0, 19.0);
    small(batch, on(view.single), 14.0, 32.0);
    small(batch, "Round robin:", 0.0, 49.0);
    small(batch, on(view.round_robin), 14.0, 62.0);
    small(batch, "Eject unfiltered:", 0.0, 79.0);
    small(batch, on(view.eject), 14.0, 92.0);
    small(batch, "Default color:", 0.0, 109.0);
    for (r, icon) in SORTER_TOGGLES.iter().zip(["single", "round_robin", "auto_eject"]) {
        let hovered = inside(mouse, *r);
        draw_button(game, batch, at(*r), hovered, true, 0xFFFF_FFFF, Some((&format!("button/{icon}"), 14.0)));
    }
    sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 12.0, oy + 136.0, 18.0, 18.0]);
    let _ = SORTER_COLOR;
    draw_mek_button(game, batch, at(SORTER_NEW), true, inside(mouse, SORTER_NEW));
    let w = game.native_text_width("New Filter");
    game.native_text(batch, "New Filter", [ox + 96.0 + (156.0 - w) / 2.0, oy + 142.0], 0xFF_FFFF, true);
    // The list.
    let hidden = view.filters.len().saturating_sub(SORTER_ROWS);
    let first = sorter_scroll(pos).min(hidden);
    let thumb = if hidden > 0 { (first as f32 / hidden as f32 * (SORTER_BAR[3] - 2.0 - 15.0)).round() } else { 0.0 };
    let bar = if hidden > 0 { "scroll/bar" } else { "scroll/bar_inactive" };
    sprite(game, batch, bar, [0.0, 0.0, 12.0, 15.0], [ox + SORTER_BAR[0] + 1.0, oy + SORTER_BAR[1] + 1.0 + thumb, 12.0, 15.0]);
    for (i, f) in view.filters.iter().skip(first).take(SORTER_ROWS).enumerate() {
        let index = first + i;
        let row = sorter_row(i);
        let [rx, ry] = [ox + row[0], oy + row[1]];
        let hovered = inside(mouse, row);
        nine_slice_tinted(game, batch, if hovered { "list/filter/hovered" } else { "list/filter/base" }, [156.0, 29.0], 1.0, [rx, ry, 156.0, 29.0], 0xFFFF_FFFF);
        let tint = if f.starts_with('#') { 0x4D54_FF54 } else { 0x4D54_54FF };
        game.native_fill(batch, [rx, ry, 156.0, 29.0], tint);
        sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [rx + 2.0, ry + 2.0, 18.0, 18.0]);
        if !f.starts_with('#') {
            game.native_item(batch, f, [rx + 3.0, ry + 3.0], 16.0);
        }
        game.native_text(batch, &filter_name(f), [rx + 22.0, ry + 3.0], 0x40_4040, false);
        game.native_fill(batch, [rx + 22.0, ry + 13.0, 6.0, 1.0], 0xFF39_3939);
        game.native_fill(batch, [rx + 22.0, ry + 18.0, 6.0, 1.0], 0xFF39_3939);
        game.native_fill(batch, [rx + 22.0, ry + 13.0, 1.0, 6.0], 0xFF39_3939);
        game.native_fill(batch, [rx + 27.0, ry + 13.0, 1.0, 6.0], 0xFF39_3939);
        game.native_text_scaled(batch, "No color", [rx + 30.0, ry + 14.0], 0x40_4040, 0.7);
        let (up, down, radio) = sorter_row_parts(i);
        if index > 0 {
            let name = if inside(mouse, up) { "button/filter_arrow/up_hovered" } else { "button/filter_arrow/up" };
            sprite(game, batch, name, [0.0, 0.0, 11.0, 7.0], at(up));
        }
        if index + 1 < view.filters.len() {
            let name = if inside(mouse, down) { "button/filter_arrow/down_hovered" } else { "button/filter_arrow/down" };
            sprite(game, batch, name, [0.0, 0.0, 11.0, 7.0], at(down));
        }
        let enabled = !view.disabled.contains(f);
        let name = if enabled {
            "button/radio/selected"
        } else if inside(mouse, radio) {
            "button/radio/hovered"
        } else {
            "button/radio/base"
        };
        sprite(game, batch, name, [0.0, 0.0, 8.0, 8.0], at(radio));
    }
}

fn sorter_tooltip(game: &Game<'_>, at: [f32; 2], lines: &mut Vec<String>) {
    let tips = [
        "Sends a single item instead of a whole stack each time (overrides min and max set in ItemStack filters).",
        "Cycles between all connected inventories when sending items.",
        "Ejects unfiltered items automatically to connected inventories, using the default configuration.",
    ];
    if let Some(i) = SORTER_TOGGLES.iter().position(|r| inside(at, *r)) {
        lines.push(tips[i].to_owned());
        return;
    }
    if inside(at, SORTER_NEW) {
        lines.push("Left click: a filter for the item in your hand".to_owned());
        lines.push("Right click: for its first tag".to_owned());
        return;
    }
    let Some((pos, view)) = sorter_view(game) else {
        return;
    };
    let first = sorter_scroll(pos);
    for i in 0..SORTER_ROWS.min(view.filters.len().saturating_sub(first)) {
        let (up, down, radio) = sorter_row_parts(i);
        if inside(at, up) {
            lines.push("Move Up".to_owned());
            lines.push("Hold shift to move to top".to_owned());
        } else if inside(at, down) {
            lines.push("Move Down".to_owned());
            lines.push("Hold shift to move to bottom".to_owned());
        } else if inside(at, radio) {
            let enabled = !view.disabled.contains(&view.filters[first + i]);
            lines.push(if enabled { "Filter: \u{a7}aenabled".to_owned() } else { "Filter: \u{a7}cdisabled".to_owned() });
        } else if inside(at, sorter_row(i)) {
            lines.push(view.filters[first + i].clone());
            lines.push("Right click: delete".to_owned());
        }
    }
}

// --- QIO screens (GuiQIODriveArray, GuiQIODashboard) ---------------------------------------------------

const QIO_CHANNEL: &str = "mekanism:qio";

fn is_qio_screen(block: &str) -> bool {
    matches!(kind(block), Some(Kind::QioDriveArray | Kind::QioDashboard))
}

/// A dashboard's width: 16 + 9 columns of slots + 18 (`GuiQIOItemViewer.calcWidth`).
const QIO_WIDTH: f32 = 196.0;
const QIO_HEIGHT: f32 = 247.0;

/// What the screens show of the network, in the block entity's `client` compound.
fn qio_sync(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, net: Option<&qio::Network>) {
    let mut client = Compound::new();
    if let Some(f) = frequency_label(&entity.nbt) {
        client.insert("frequency".into(), Tag::String(f.as_str().into()));
    }
    if let Some(net) = net {
        let (items, item_cap, types, type_cap) = net.totals();
        put_long(&mut client, "items", items);
        put_long(&mut client, "item_cap", item_cap);
        put_long(&mut client, "types", types as u64);
        put_long(&mut client, "type_cap", type_cap as u64);
    }
    put_client(game, pos, entity, &["frequency", "items", "item_cap", "types", "type_cap"], client);
}

/// A dashboard's search, from its player: `pos` and the text.
fn qio_payload(game: &mut Game<'_>, data: &[u8]) {
    let (Some(pos), Some(player)) = (data.get(..12), game.native_acting_player()) else {
        return;
    };
    let int = |i: usize| i32::from_be_bytes([pos[i], pos[i + 1], pos[i + 2], pos[i + 3]]);
    let pos = [int(0), int(4), int(8)];
    let Ok(text) = std::str::from_utf8(&data[12..]) else {
        return;
    };
    let near = (0..3).map(|k| (player.position[k] - f64::from(pos[k]) - 0.5).powi(2)).sum::<f64>() <= 64.0;
    if !near || text.chars().count() > qio::MAX_SEARCH || !text.chars().all(qio::valid_search_char) || kind(block_name(game, pos)) != Some(Kind::QioDashboard) {
        return;
    }
    game.native_with_mod_block_entity(pos, |m| {
        m.nbt.insert("search".into(), Tag::String(text.into()));
    });
}

struct QioView {
    pos: [i32; 3],
    frequency: Option<String>,
    items: u64,
    item_cap: u64,
    types: u64,
    type_cap: u64,
}

fn qio_view(game: &Game<'_>) -> Option<QioView> {
    let (x, y, z) = game.container?;
    let (_, c) = game.native_client_block_entity([x, y, z])?;
    Some(QioView {
        pos: [x, y, z],
        frequency: get_string(c, "frequency"),
        items: get_long(c, "items"),
        item_cap: get_long(c, "item_cap"),
        types: get_long(c, "types"),
        type_cap: get_long(c, "type_cap"),
    })
}

thread_local! {
    static QIO_SEARCH: std::cell::RefCell<([i32; 3], freq::TextBox)> = std::cell::RefCell::new(([0; 3], freq::TextBox::default()));
}

const QIO_SEARCH_BOX: [f32; 4] = [50.0, 30.0, 136.0, 10.0];

fn qio_slots(block: &str) -> Vec<Option<[f32; 2]>> {
    match kind(block) {
        Some(Kind::QioDriveArray) => (0..18)
            .map(|i| (i < qio::DRIVE_SLOTS).then(|| [34.0 + 18.0 * (i % 6) as f32, 70.0 + 18.0 * (i / 6) as f32]))
            .collect(),
        _ => (0..qio::DASHBOARD_SLOTS).map(|i| Some([8.0 + 18.0 * (i % 9) as f32, 44.0 + 18.0 * (i / 9) as f32])).collect(),
    }
}

fn qio_screen(block: &str) -> super::native_hooks::NativeScreen {
    if kind(block) == Some(Kind::QioDriveArray) {
        super::native_hooks::NativeScreen { width: 176.0, height: 206.0, slots: qio_slots(block), inventory_y: 124.0, inventory_x: 8.0, own_labels: true, alt: None, ..Default::default() }
    } else {
        super::native_hooks::NativeScreen {
            width: QIO_WIDTH,
            height: QIO_HEIGHT,
            slots: qio_slots(block),
            inventory_y: QIO_HEIGHT - 82.0,
            inventory_x: 8.0 + (QIO_WIDTH - 176.0) / 2.0,
            own_labels: true,
            alt: None,
            ..Default::default()
        }
    }
}

fn draw_qio_screen(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, block: &str, origin: [f32; 2]) {
    let [ox, oy] = origin;
    let drive = kind(block) == Some(Kind::QioDriveArray);
    let (w, h) = if drive { (176.0, 206.0) } else { (QIO_WIDTH, QIO_HEIGHT) };
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, w, h]);
    let view = qio_view(game);
    let title = game.native_screen_title().unwrap_or_default();
    let tw = game.native_text_width(&title);
    game.native_text(batch, &title, [ox + (w - tw) / 2.0, oy + if drive { 4.0 } else { 5.0 }], 0x40_4040, false);
    let (inv_x, inv_y) = ((w - 176.0) / 2.0 + 7.0, h - 82.0);
    for row in 0..4 {
        let y = if row < 3 { inv_y + 18.0 * row as f32 } else { inv_y + 58.0 };
        for col in 0..9 {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + inv_x + 18.0 * col as f32, oy + y - 1.0, 18.0, 18.0]);
        }
    }
    game.native_text(batch, "Inventory", [ox + inv_x + 1.0, oy + inv_y - 11.0], 0x40_4040, false);
    for slot in qio_slots(block).into_iter().flatten() {
        sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + slot[0] - 1.0, oy + slot[1] - 1.0, 18.0, 18.0]);
    }
    let frequency = view.as_ref().and_then(|v| v.frequency.clone());
    if drive {
        // `GuiQIOFrequencyDataScreen`: the frequency, and digital bars for the items and types held.
        nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 32.0, [ox + 15.0, oy + 19.0, w - 32.0, 46.0], 0xFFFF_FFFF);
        if let Some(f) = &frequency {
            game.native_text_scaled(batch, &format!("Frequency: {f}"), [ox + 20.0, oy + 24.0], 0x3C_FE9A, 0.8);
        }
        let ratio = |a: u64, b: u64| if b == 0 { 0.0 } else { (a as f64 / b as f64).clamp(0.0, 1.0) as f32 };
        let (items, types) = view.as_ref().map_or((0.0, 0.0), |v| (ratio(v.items, v.item_cap), ratio(v.types, v.type_cap)));
        for (x, fill, label) in [(26.0, items, "Items"), (98.0, types, "Types")] {
            nine_slice_tinted(game, batch, "bar/digital", [5.0, 5.0], 1.0, [ox + x, oy + 39.0, 50.0, 8.0], 0xFFFF_FFFF);
            let filled = (48.0 * fill).floor();
            if filled > 0.0 {
                game.native_fill(batch, [ox + x + 1.0, oy + 40.0, filled, 6.0], 0xFF3C_FE9A);
            }
            let lw = game.native_text_width(label) * 0.8;
            game.native_text_scaled(batch, label, [ox + x + (50.0 - lw) / 2.0, oy + 51.0], 0x3C_FE9A, 0.8);
        }
    } else {
        nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 32.0, [ox + 7.0, oy + 15.0, w - 16.0, 12.0], 0xFFFF_FFFF);
        let line = frequency.map_or("No frequency".to_owned(), |f| format!("Frequency: {f}"));
        game.native_text_scaled(batch, &line, [ox + 10.0, oy + 18.0], 0x3C_FE9A, 0.8);
        let label_w = game.native_text_width("Search:");
        game.native_text(batch, "Search:", [ox + 4.0 + (46.0 - label_w) / 2.0 - 2.0, oy + 31.0], 0x40_4040, false);
        let [bx, by, bw, bh] = QIO_SEARCH_BOX;
        nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, [ox + bx - 1.0, oy + by - 1.0, bw + 2.0, bh + 2.0], 0xFFFF_FFFF);
        let pos = view.as_ref().map_or([0; 3], |v| v.pos);
        QIO_SEARCH.with_borrow(|(at, text)| {
            if *at != pos {
                return;
            }
            game.native_text(batch, &text.value, [ox + bx + 2.0, oy + by + 1.0], 0xFF_FFFF, true);
            if text.focused && (game.ticks / 6) % 2 == 0 {
                let before: String = text.value.chars().take(text.cursor).collect();
                game.native_text(batch, "_", [ox + bx + 2.0 + game.native_text_width(&before), oy + by + 1.0], 0xFF_FFFF, true);
            }
        });
    }
}

fn qio_click(game: &Game<'_>, block: &str, at: [f32; 2]) {
    if kind(block) != Some(Kind::QioDashboard) {
        return;
    }
    let Some(view) = qio_view(game) else {
        return;
    };
    QIO_SEARCH.with_borrow_mut(|(pos, text)| {
        if *pos != view.pos {
            *pos = view.pos;
            *text = freq::TextBox::default();
        }
        text.focused = inside(at, QIO_SEARCH_BOX);
    });
}

fn qio_typing(block: &str) -> bool {
    kind(block) == Some(Kind::QioDashboard) && QIO_SEARCH.with_borrow(|(_, t)| t.focused)
}

/// A key in the dashboard's search box: it sends the new text to the server as it changes.
fn qio_key(game: &Game<'_>, key: &str, text: Option<&str>) {
    let Some(view) = qio_view(game) else {
        return;
    };
    QIO_SEARCH.with_borrow_mut(|(pos, t)| {
        if *pos != view.pos || !t.focused {
            return;
        }
        if t.key(key, text, qio::valid_search_char, qio::MAX_SEARCH) == freq::Typed::Changed {
            let mut data = Vec::new();
            for c in view.pos {
                data.extend(c.to_be_bytes());
            }
            data.extend(t.value.as_bytes());
            game.native_send_to_server(QIO_CHANNEL, &data);
        }
    });
}

fn qio_tooltip(game: &Game<'_>, block: &str, at: [f32; 2], lines: &mut Vec<String>) {
    let Some(v) = qio_view(game).filter(|v| v.frequency.is_some()) else {
        return;
    };
    let area = if kind(block) == Some(Kind::QioDriveArray) { [15.0, 19.0, 144.0, 46.0] } else { [7.0, 15.0, 180.0, 12.0] };
    if inside(at, area) {
        lines.push(format!("\u{a7}7Items: \u{a7}9{}\u{a7}7 / \u{a7}9{}", v.items, v.item_cap));
        lines.push(format!("\u{a7}7Types: \u{a7}9{}\u{a7}7 / \u{a7}9{}", v.types, v.type_cap));
    }
}

// --- Sorter filter windows (GuiFilterSelect, GuiSorterItemStackFilter, GuiSorterTagFilter,
// GuiSorterModIDFilter) --------------------------------------------------------------------------------

/// The window over the sorter's screen: 200 × 90, centred, 30 px down (`GuiFilterHolder.addWindow`).
const FILTER_WINDOW: [f32; 4] = [38.0, 30.0, 200.0, 90.0];
const FILTER_CLOSE: [f32; 4] = [6.0, 6.0, 8.0, 8.0];
const FILTER_SCREEN: [f32; 4] = [29.0, 18.0, 116.0, 42.0];
const FILTER_SLOT: [f32; 4] = [7.0, 18.0, 18.0, 18.0];
const FILTER_NAME: [f32; 4] = [32.0, 46.0, 110.0, 11.0];
const FILTER_DELETE: [f32; 4] = [24.0, 62.0, 60.0, 20.0];
const FILTER_SAVE: [f32; 4] = [86.0, 62.0, 60.0, 20.0];
const FILTER_DEFAULT: [f32; 4] = [148.0, 18.0, 11.0, 11.0];
const FILTER_SIZE: [f32; 4] = [148.0, 56.0, 11.0, 11.0];
const FILTER_COLOR: [f32; 4] = [8.0, 44.0, 16.0, 16.0];
const FILTER_MIN: [f32; 4] = [174.0, 31.0, 20.0, 11.0];
const FILTER_MAX: [f32; 4] = [174.0, 43.0, 20.0, 11.0];
/// The select window (`GuiFilterSelect`): 152 wide, 30 + 20 a button high, centred 20 px down.
const SELECT_WINDOW: [f32; 4] = [62.0, 20.0, 152.0, 90.0];
/// The edit window (`GuiQIOItemStackFilter`: 185 wide, 15 down, centred on the 236 wide screen).
fn filter_window() -> [f32; 4] {
    if FILTER_OD.get() {
        return OD_WINDOW;
    }
    if FILTER_QIO.get() { [25.0, 15.0, 185.0, 90.0] } else { FILTER_WINDOW }
}

fn select_window() -> [f32; 4] {
    if FILTER_QIO.get() { [42.0, 20.0, 152.0, 90.0] } else { SELECT_WINDOW }
}

/// Delete (or Cancel) and Save: the QIO windows' buttons start at 29 (`getLeftButtonX`).
fn filter_buttons() -> ([f32; 4], [f32; 4]) {
    if FILTER_QIO.get() { ([29.0, 62.0, 60.0, 20.0], [91.0, 62.0, 60.0, 20.0]) } else { (FILTER_DELETE, FILTER_SAVE) }
}

const SELECT_NAMES: [&str; 3] = ["Item Filter", "Tag Filter", "Mod ID Filter"];

fn select_button(k: usize) -> [f32; 4] {
    [12.0, 19.0 + 20.0 * k as f32, 128.0, 20.0]
}

/// A filter window being edited: an existing filter (`index`), or a new one of `kind` (item, tag or
/// mod id) not sent until Save, with its `name` and options kept here.
struct FilterEdit {
    pos: [i32; 3],
    index: Option<usize>,
    kind: u16,
    name: freq::TextBox,
    draft: sorter::Opts,
    min: freq::TextBox,
    max: freq::TextBox,
}

thread_local! {
    static FILTER_EDIT: std::cell::RefCell<Option<FilterEdit>> = const { std::cell::RefCell::new(None) };
    /// The select window's sorter, when it is open.
    static FILTER_SELECT: std::cell::Cell<Option<[i32; 3]>> = const { std::cell::Cell::new(None) };
}

fn range_box(n: u16) -> freq::TextBox {
    freq::TextBox { value: n.to_string(), cursor: n.to_string().len(), focused: false }
}

fn open_filter_window(view: &SorterView, pos: [i32; 3], index: usize) {
    let Some(f) = view.filters.get(index) else {
        return;
    };
    let o = view.opts.get(f).copied().unwrap_or_default();
    let kind = match f.as_bytes().first() {
        Some(b'#') => 1,
        Some(b'@') => 2,
        _ => 0,
    };
    FILTER_EDIT.with_borrow_mut(|e| {
        *e = Some(FilterEdit { pos, index: Some(index), kind, name: freq::TextBox::default(), draft: o, min: range_box(o.min), max: range_box(o.max) })
    });
}

fn open_new_filter_window(pos: [i32; 3], kind: u16) {
    let o = sorter::Opts::default();
    FILTER_EDIT.with_borrow_mut(|e| {
        *e = Some(FilterEdit { pos, index: None, kind, name: freq::TextBox { focused: kind != 0, ..Default::default() }, draft: o, min: range_box(o.min), max: range_box(o.max) })
    });
}

fn close_filter_window() {
    FILTER_EDIT.with_borrow_mut(|e| *e = None);
}

fn window_rect(r: [f32; 4]) -> [f32; 4] {
    let [x, y, ..] = filter_window();
    [x + r[0], y + r[1], r[2], r[3]]
}

fn select_rect(r: [f32; 4]) -> [f32; 4] {
    let [x, y, ..] = select_window();
    [x + r[0], y + r[1], r[2], r[3]]
}

fn filter_edit_open(pos: [i32; 3]) -> bool {
    FILTER_EDIT.with_borrow(|e| e.as_ref().is_some_and(|e| e.pos == pos))
}

/// The windows' drawing (hook point `native_screen_overlay`).
fn draw_filter_window(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    let Some((pos, view)) = sorter_view(game) else {
        return;
    };
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    let at = [cursor[0] - ox, cursor[1] - oy];
    if FILTER_SELECT.get() == Some(pos) {
        let sc = |r: [f32; 4]| {
            let [x, y, w, h] = select_rect(r);
            [ox + x, oy + y, w, h]
        };
        let [x, y, w, h] = sc([0.0, 0.0, select_window()[2], select_window()[3]]);
        nine_slice_tinted(game, batch, "shadow", [9.0, 9.0], 3.0, [x - 3.0, y - 3.0, w + 6.0, h + 6.0], 0xBFFF_FFFF);
        nine_slice_tinted(game, batch, "base", [16.0, 16.0], 4.0, [x, y, w, h], 0xFFFF_FFFF);
        draw_button(game, batch, sc(FILTER_CLOSE), inside(at, select_rect(FILTER_CLOSE)), true, 0xFFFF_FFFF, Some(("button/close", 12.0)));
        centred(game, batch, "Create Filter", x, w, y + 6.0, 0x40_4040);
        nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, sc([11.0, 18.0, 130.0, 62.0]), 0xFFFF_FFFF);
        for (k, label) in SELECT_NAMES.iter().enumerate() {
            let b = sc(select_button(k));
            draw_button(game, batch, b, inside(at, select_rect(select_button(k))), true, 0xFFFF_FFFF, None);
            centred(game, batch, label, b[0], b[2], b[1] + 6.0, 0xFF_FFFF);
        }
        return;
    }
    FILTER_EDIT.with_borrow(|e| {
        let Some(e) = e.as_ref().filter(|e| e.pos == pos) else {
            return;
        };
        let filter = e.index.and_then(|i| view.filters.get(i)).cloned();
        if e.index.is_some() && filter.is_none() {
            return;
        }
        let o = filter.as_ref().map_or(e.draft, |f| view.opts.get(f).copied().unwrap_or_default());
        let screen = |r: [f32; 4]| {
            let [x, y, w, h] = window_rect(r);
            [ox + x, oy + y, w, h]
        };
        let hover = |r: [f32; 4]| inside(at, window_rect(r));
        let [x, y, w, h] = screen([0.0, 0.0, filter_window()[2], filter_window()[3]]);
        nine_slice_tinted(game, batch, "shadow", [9.0, 9.0], 3.0, [x - 3.0, y - 3.0, w + 6.0, h + 6.0], 0xBFFF_FFFF);
        nine_slice_tinted(game, batch, "base", [16.0, 16.0], 4.0, [x, y, w, h], 0xFFFF_FFFF);
        draw_button(game, batch, screen(FILTER_CLOSE), hover(FILTER_CLOSE), true, 0xFFFF_FFFF, Some(("button/close", 12.0)));
        let what = SELECT_NAMES[usize::from(e.kind.min(2))];
        let title = if e.index.is_none() { format!("New {what}") } else { what.to_owned() };
        centred(game, batch, &title, x, w, y + 5.0, 0x40_4040);
        // The slot (an item filter's item; a click with an item in hand sets it) and the inner screen.
        sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [x + 7.0, y + 18.0, 18.0, 18.0]);
        let named = filter.clone().unwrap_or_else(|| sorter::Sorter::named_filter(e.kind, &e.name.value).unwrap_or_default());
        if e.kind == 0 && !named.is_empty() {
            game.native_item_stack(batch, &stack_of(&named, 1), [x + 8.0, y + 19.0], 16.0);
        }
        let s = screen(FILTER_SCREEN);
        nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, s, 0xFFFF_FFFF);
        game.native_text(batch, what, [s[0] + 3.0, s[1] + 3.0], 0x3C_FE9A, false);
        if e.index.is_some() {
            let shown = match e.kind {
                0 => filter_name(&named),
                _ => named[1..].to_owned(),
            };
            game.native_text_scaled(batch, &shown, [s[0] + 3.0, s[1] + 14.0], 0x3C_FE9A, 0.8);
        } else if e.kind == 0 {
            game.native_text_scaled(batch, if named.is_empty() { "Click the slot with an item" } else { &named }, [s[0] + 3.0, s[1] + 14.0], 0x3C_FE9A, 0.8);
        } else {
            let b = screen(FILTER_NAME);
            game.native_fill(batch, [b[0] - 1.0, b[1] - 1.0, b[2] + 2.0, b[3] + 2.0], if e.name.focused { 0xFFFF_FFFF } else { 0xFFA0_A0A0 });
            game.native_fill(batch, b, 0xFF00_0000);
            game.native_text_scaled(batch, &e.name.value, [b[0] + 2.0, b[1] + 2.0], 0xE0_E0E0, 0.8);
            if e.name.focused && (game.ticks / 6) % 2 == 0 {
                let before: String = e.name.value.chars().take(e.name.cursor).collect();
                game.native_text_scaled(batch, "_", [b[0] + 2.0 + game.native_text_width(&before) * 0.8, b[1] + 2.0], 0xE0_E0E0, 0.8);
            }
            let hint = if e.kind == 1 { "Tag name:" } else { "Mod id:" };
            game.native_text_scaled(batch, hint, [s[0] + 3.0, s[1] + 14.0], 0x3C_FE9A, 0.8);
        }
        if view.qio.is_some() {
            // An item filter's fuzzy switch (`GuiQIOItemStackFilter`), the only option a QIO filter has.
            if e.kind == 0 {
                draw_button(game, batch, screen(FILTER_DEFAULT), hover(FILTER_DEFAULT), true, 0xFFFF_FFFF, Some(("button/fuzzy", 11.0)));
                game.native_text(batch, if o.allow_default { "On" } else { "Off" }, [x + 159.0, y + 20.0], 0x40_4040, false);
            }
        } else {
            // The colour slot and button (`ColorButton`).
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [x + 7.0, y + 43.0, 18.0, 18.0]);
            if let Some(c) = o.color {
                game.native_fill(batch, screen(FILTER_COLOR), 0xFF00_0000 | freq::COLORS[usize::from(c) % 16].1);
            }
            draw_button(game, batch, screen(FILTER_DEFAULT), hover(FILTER_DEFAULT), true, 0xFFFF_FFFF, Some(("button/default", 11.0)));
            game.native_text(batch, if o.allow_default { "On" } else { "Off" }, [x + 159.0, y + 20.0], 0x40_4040, false);
            draw_button(game, batch, screen(FILTER_SIZE), hover(FILTER_SIZE), true, 0xFFFF_FFFF, Some(("button/silk_touch", 11.0)));
            let size_text = if view.single && o.size_mode { "\u{a7}cOn" } else if o.size_mode { "On" } else { "Off" };
            game.native_text(batch, size_text, [x + 159.0, y + 58.0], 0x40_4040, false);
            game.native_text(batch, "Min:", [x + 146.0, y + 32.0], 0x40_4040, false);
            game.native_text(batch, "Max:", [x + 146.0, y + 44.0], 0x40_4040, false);
            for (r, t) in [(FILTER_MIN, &e.min), (FILTER_MAX, &e.max)] {
                let b = screen(r);
                game.native_fill(batch, [b[0] - 1.0, b[1] - 1.0, b[2] + 2.0, b[3] + 2.0], if t.focused { 0xFFFF_FFFF } else { 0xFFA0_A0A0 });
                game.native_fill(batch, b, 0xFF00_0000);
                game.native_text(batch, &t.value, [b[0] + 2.0, b[1] + 2.0], 0xE0_E0E0, false);
                if t.focused && (game.ticks / 6) % 2 == 0 {
                    let before: String = t.value.chars().take(t.cursor).collect();
                    game.native_text(batch, "_", [b[0] + 2.0 + game.native_text_width(&before), b[1] + 2.0], 0xE0_E0E0, false);
                }
            }
        }
        let (delete, save) = filter_buttons();
        for (r, label) in [(delete, if e.index.is_some() { "Delete" } else { "Cancel" }), (save, "Save")] {
            let b = screen(r);
            draw_button(game, batch, b, hover(r), true, 0xFFFF_FFFF, None);
            centred(game, batch, label, b[0], b[2], b[1] + 6.0, 0xFF_FFFF);
        }
    });
}

/// A press while a filter window is open: true when one is (it takes every click).
fn filter_window_click(game: &Game<'_>, pos: [i32; 3], view: &SorterView, at: [f32; 2], right: bool) -> bool {
    use sorter::Op;
    if FILTER_SELECT.get() == Some(pos) {
        let on = |r: [f32; 4]| inside(at, select_rect(r));
        if on(FILTER_CLOSE) {
            FILTER_SELECT.set(None);
        } else if let Some(k) = (0..3).find(|k| on(select_button(*k))) {
            FILTER_SELECT.set(None);
            open_new_filter_window(pos, k as u16);
        }
        return true;
    }
    if !filter_edit_open(pos) {
        return false;
    }
    let on = |r: [f32; 4]| inside(at, window_rect(r));
    let sorter = view.qio.is_none();
    let (delete, save) = filter_buttons();
    let shift = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) || game.keys.contains(&winit::keyboard::KeyCode::ShiftRight);
    let (index, kind) = FILTER_EDIT.with_borrow(|e| e.as_ref().map(|e| (e.index, e.kind))).unwrap_or((None, 0));
    FILTER_EDIT.with_borrow_mut(|e| {
        let Some(edit) = e.as_mut() else {
            return;
        };
        edit.min.focused = sorter && on(FILTER_MIN);
        edit.max.focused = sorter && on(FILTER_MAX);
        edit.name.focused = kind != 0 && on(FILTER_NAME);
    });
    let _ = view;
    // The switches act at once on an existing filter, and on the draft of a new one.
    let draft = |change: &dyn Fn(&mut sorter::Opts)| {
        FILTER_EDIT.with_borrow_mut(|e| {
            if let Some(e) = e {
                change(&mut e.draft);
            }
        });
    };
    if on(FILTER_CLOSE) {
        close_filter_window();
    } else if on(FILTER_SLOT) && kind == 0 && index.is_none() {
        // A click with an item in hand names it (`GuiItemStackFilter`'s slot).
        if let Some(h) = game.inventory.get(game.selected).cloned().flatten() {
            FILTER_EDIT.with_borrow_mut(|e| {
                if let Some(e) = e {
                    e.name = freq::TextBox { value: h.item.to_string(), cursor: h.item.len(), focused: false };
                }
            });
        }
    } else if on(FILTER_DEFAULT) && (sorter || kind == 0) {
        match index {
            Some(i) => send_sorter(game, Op::AllowDefault, pos, i),
            None => draft(&|o| o.allow_default = !o.allow_default),
        }
    } else if sorter && on(FILTER_SIZE) {
        match index {
            Some(i) => send_sorter(game, Op::SizeMode, pos, i),
            None => draft(&|o| o.size_mode = !o.size_mode),
        }
    } else if sorter && on(FILTER_COLOR) {
        match (index, right, shift) {
            (Some(i), true, _) => send_sorter(game, Op::ColorPrevious, pos, i),
            (Some(i), false, true) => send_sorter(game, Op::ColorClear, pos, i),
            (Some(i), false, false) => send_sorter(game, Op::ColorNext, pos, i),
            (None, _, _) => draft(&|o| o.color = if shift && !right { None } else { sorter::Opts::step_color(o.color, !right) }),
        }
    } else if on(delete) {
        if let Some(i) = index {
            send_sorter(game, Op::Delete, pos, i);
        }
        close_filter_window();
    } else if on(save) {
        filter_window_save(game, pos, view, index);
    }
    true
}

/// Save: an existing filter takes its new range; a new one is added, then given its options
/// (`validateAndSaveSorterFilter`). A range outside 0 <= min <= max <= 64 or a bad name keeps the
/// window open.
fn filter_window_save(game: &Game<'_>, pos: [i32; 3], view: &SorterView, index: Option<usize>) {
    use sorter::Op;
    if view.qio.is_some() {
        // A QIO filter has a name and the fuzzy switch only (`GuiQIOFilterHelper`).
        let saved = FILTER_EDIT.with_borrow(|e| e.as_ref().map(|e| (e.kind, e.name.value.clone(), e.draft)));
        let Some((kind, name, draft)) = saved else {
            return;
        };
        if index.is_none() {
            if sorter::Sorter::named_filter(kind, &name).is_none() {
                return;
            }
            let at = view.filters.len();
            let r = sorter::Request { op: Op::AddNamed, pos, index: kind, value: 0, name };
            game.native_send_to_server(SORTER_CHANNEL, &r.encode());
            if draft.allow_default && kind == 0 {
                send_sorter(game, Op::AllowDefault, pos, at);
            }
        }
        close_filter_window();
        return;
    }
    let saved = FILTER_EDIT.with_borrow(|e| {
        let e = e.as_ref()?;
        let num = |t: &freq::TextBox, empty: u16| if t.value.is_empty() { Some(empty) } else { t.value.parse::<u16>().ok() };
        let (lo, hi) = (num(&e.min, 0)?, num(&e.max, 64)?);
        sorter::Opts::valid_range(lo, hi).then(|| (lo, hi, e.kind, e.name.value.clone(), e.draft))
    });
    let Some((lo, hi, kind, name, draft)) = saved else {
        return;
    };
    match index {
        Some(i) => send_sorter_value(game, Op::Range, pos, i, lo << 8 | hi),
        None => {
            let Some(_) = sorter::Sorter::named_filter(kind, &name) else {
                return;
            };
            let at = view.filters.len();
            let r = sorter::Request { op: Op::AddNamed, pos, index: kind, value: 0, name };
            game.native_send_to_server(SORTER_CHANNEL, &r.encode());
            if draft.allow_default {
                send_sorter(game, Op::AllowDefault, pos, at);
            }
            if draft.size_mode {
                send_sorter(game, Op::SizeMode, pos, at);
            }
            if let Some(c) = draft.color {
                send_sorter_value(game, Op::SetColor, pos, at, u16::from(c) + 1);
            }
            if (lo, hi) != (0, 64) {
                send_sorter_value(game, Op::Range, pos, at, lo << 8 | hi);
            }
        }
    }
    close_filter_window();
}

fn filter_window_tooltip(game: &Game<'_>, at: [f32; 2], lines: &mut Vec<String>) -> bool {
    let Some((pos, view)) = sorter_view(game) else {
        return false;
    };
    if FILTER_SELECT.get() == Some(pos) {
        return inside(at, select_window());
    }
    if !filter_edit_open(pos) {
        return false;
    }
    let on = |r: [f32; 4]| inside(at, window_rect(r));
    let (index, draft) = FILTER_EDIT.with_borrow(|e| e.as_ref().map(|e| (e.index, e.draft))).unwrap_or((None, Default::default()));
    if view.qio.is_some() {
        if on(FILTER_DEFAULT) {
            lines.push("Fuzzy Mode".into());
        }
    } else if on(FILTER_DEFAULT) {
        lines.push("Allow Default".into());
    } else if on(FILTER_SIZE) {
        lines.push(if view.single { "\u{a7}cSize Mode conflicts with single item mode".into() } else { "Size Mode".into() });
    } else if on(FILTER_COLOR) {
        let color = match index {
            Some(i) => view.filters.get(i).and_then(|f| view.opts.get(f)).and_then(|o| o.color),
            None => draft.color,
        };
        lines.push(color.map_or("None", |c| freq::COLORS[usize::from(c) % 16].0).into());
    }
    inside(at, filter_window())
}

fn filter_covers(at: [f32; 2]) -> bool {
    (FILTER_EDIT.with_borrow(|e| e.is_some()) && inside(at, filter_window())) || (FILTER_SELECT.get().is_some() && inside(at, select_window()))
}

fn filter_typing(block: &str) -> bool {
    is_filter_screen(block) && FILTER_EDIT.with_borrow(|e| e.as_ref().is_some_and(|e| e.min.focused || e.max.focused || e.name.focused))
}

fn filter_key(key: &str, text: Option<&str>) {
    FILTER_EDIT.with_borrow_mut(|e| {
        let Some(e) = e else {
            return;
        };
        if e.name.focused {
            e.name.key(key, text, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./:#@".contains(c), 64);
        } else {
            let t = if e.min.focused { &mut e.min } else { &mut e.max };
            t.key(key, text, |c| c.is_ascii_digit(), 2);
        }
    });
}

// --- Digital Miner config screen (GuiDigitalMinerConfig, GuiMinerFilerSelect, GuiMinerItemStackFilter...) -----

const MCFG_BACK: [f32; 4] = [5.0, 5.0, 11.0, 11.0];
const MCFG_SWITCH: [f32; 4] = [10.0, 115.0, 30.0, 16.0];
const MCFG_SLOT: [f32; 4] = [13.0, 135.0, 18.0, 18.0];
const MCFG_REQUIRE: [f32; 4] = [35.0, 137.0, 14.0, 14.0];
/// The radius, min Y and max Y fields (`GuiTextField`, digital border).
const MCFG_FIELDS: [[f32; 4]; 3] = [[13.0, 45.0, 38.0, 11.0], [13.0, 71.0, 38.0, 11.0], [13.0, 98.0, 38.0, 11.0]];
/// The filter window (`MINER_FILTER_WIDTH` 173 wide, centred on 276, 30 down) and what is in it.
const MWIN: [f32; 4] = [51.0, 30.0, 173.0, 90.0];
const MWIN_DELETE: [f32; 4] = [25.0, 62.0, 60.0, 20.0];
const MWIN_SAVE: [f32; 4] = [87.0, 62.0, 60.0, 20.0];
const MWIN_REPLACE: [f32; 4] = [148.0, 18.0, 18.0, 18.0];
const MWIN_REQUIRE: [f32; 4] = [148.0, 45.0, 14.0, 14.0];

thread_local! {
    static MINER_VIEW: std::cell::Cell<Option<([i32; 3], u64)>> = const { std::cell::Cell::new(None) };
    static MINER_UI: std::cell::RefCell<MinerUi> = std::cell::RefCell::new(MinerUi::default());
}

#[derive(Default)]
struct MinerUi {
    scroll: usize,
    select: bool,
    edit: Option<MinerEdit>,
    fields: [freq::TextBox; 3],
}

/// A filter window being edited: an existing filter (`index`) or a new one not sent until Save.
struct MinerEdit {
    index: Option<usize>,
    filter: miner::Filter,
    name: freq::TextBox,
    status: String,
    ticker: u64,
}

struct MinerCfg {
    pos: [i32; 3],
    region: miner::Region,
    filters: Vec<miner::Filter>,
    inverse: bool,
    inv_replace: String,
    inv_require: bool,
}

fn miner_cfg_open(block: &str) -> bool {
    kind(block) == Some(Kind::DigitalMiner) && MINER_VIEW.get().is_some()
}

fn miner_cfg(game: &Game<'_>) -> Option<MinerCfg> {
    let (x, y, z) = game.container?;
    let (_, c) = game.native_client_block_entity([x, y, z])?;
    Some(MinerCfg {
        pos: [x, y, z],
        region: miner_region(c),
        filters: miner_filters(c),
        inverse: get_int(c, "inverse") == 1,
        inv_replace: get_string(c, "inv_replace").unwrap_or_default(),
        inv_require: get_int(c, "inv_require") == 1,
    })
}

fn send_miner(game: &Game<'_>, pos: [i32; 3], op: u8, value: i32, text: &str) {
    let mut data = Vec::new();
    for c in pos {
        data.extend(c.to_be_bytes());
    }
    data.push(op);
    data.extend(value.to_be_bytes());
    data.extend(text.as_bytes());
    game.native_send_to_server(MINER_CHANNEL, &data);
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

fn miner_toggle_visuals(game: &Game<'_>) {
    if let Some((x, y, z)) = game.container {
        MINER_VISUALS.with_borrow_mut(|v| {
            if !v.remove(&[x, y, z]) {
                v.insert([x, y, z]);
            }
        });
    }
}

/// The regions of the miners that show them (hook point `native_world_boxes`, API 41): one translucent box
/// each, 0.01 inside the region (`RenderDigitalMiner`), white at the faces' brightness. Only within 64.
pub(super) fn world_boxes(game: &Game<'_>, boxes: &mut Vec<super::native_hooks::NativeWorldBox>) {
    let shown: Vec<[i32; 3]> = MINER_VISUALS.with_borrow(|v| v.iter().copied().collect());
    for pos in shown {
        let Some((id, c)) = game.native_client_block_entity(pos) else {
            continue;
        };
        if kind(&id) != Some(Kind::DigitalMiner) {
            continue;
        }
        let r = miner_region(c);
        if r.radius > 64 {
            continue;
        }
        let d = f64::from(r.diameter()) - 0.02;
        let min = [f64::from(pos[0] - r.radius) + 0.01, f64::from(r.min_y) + 0.01, f64::from(pos[2] - r.radius) + 0.01];
        let max = [min[0] + d, min[1] + f64::from(r.max_y - r.min_y) - 0.02, min[2] + d];
        // `ARGB.white(alpha)`: east/west 0.78, up/down 0.82, north/south 0.8.
        let (east_west, up_down, north_south) = (0xC7FF_FFFF, 0xD1FF_FFFF, 0xCCFF_FFFF);
        boxes.push(super::native_hooks::NativeWorldBox { min, max, colors: [east_west, east_west, up_down, up_down, north_south, north_south] });
    }
}

/// Opens the config screen from the main one, and keeps it open while the screen is.
fn miner_screen_alt(game: &Game<'_>, alt: &mut bool) {
    if let Some((pos, seen)) = MINER_VIEW.get() {
        let now = game.ticks as u64;
        if game.container.map(|(x, y, z)| [x, y, z]) != Some(pos) || now.saturating_sub(seen) > 10 {
            MINER_VIEW.set(None);
            MINER_UI.with_borrow_mut(|u| *u = MinerUi::default());
        } else {
            MINER_VIEW.set(Some((pos, now)));
            *alt = true;
        }
    }
}

fn miner_item_name(id: &str) -> String {
    if id.is_empty() { "No item".to_owned() } else { filter_name(id) }
}

fn miner_filter_name(f: &miner::Filter) -> String {
    match f.kind {
        miner::FilterKind::Item => miner_item_name(&f.name),
        _ => f.name.clone(),
    }
}

fn draw_text_field(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, b: [f32; 4], t: &freq::TextBox) {
    let screen = 0xFF3C_FE9A_u32;
    game.native_fill(batch, [b[0] - 1.0, b[1] - 1.0, b[2] + 2.0, b[3] + 2.0], if t.focused { screen } else { 0xFF24_985C });
    game.native_fill(batch, b, 0xFF00_0000);
    game.native_text_scaled(batch, &t.value, [b[0] + 2.0, b[1] + 2.0], screen & 0xFF_FFFF, 0.8);
    if t.focused && (game.ticks / 6) % 2 == 0 {
        let before: String = t.value.chars().take(t.cursor).collect();
        game.native_text_scaled(batch, "_", [b[0] + 2.0 + game.native_text_width(&before) * 0.8, b[1] + 2.0], screen & 0xFF_FFFF, 0.8);
    }
}

fn draw_miner_config(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    let [ox, oy] = origin;
    let at = |r: [f32; 4]| [ox + r[0], oy + r[1], r[2], r[3]];
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, 276.0, 254.0]);
    for row in 0..4 {
        let y = if row < 3 { 172.0 + 18.0 * row as f32 } else { 230.0 };
        for col in 0..9 {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 57.0 + 18.0 * col as f32, oy + y - 1.0, 18.0, 18.0]);
        }
    }
    // `renderTitleTextWithOffset(14)`: centred right of the back button.
    let title = "Digital Miner Config";
    centred(game, batch, title, ox + 14.0, 262.0, oy + 4.0, 0x40_4040);
    game.native_text(batch, "Inventory", [ox + 58.0, oy + 161.0], 0x40_4040, false);
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 32.0, at(SORTER_INFO), 0xFFFF_FFFF);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(SORTER_HOLDER), 0xFFFF_FFFF);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(SORTER_NEW_HOLDER), 0xFFFF_FFFF);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(SORTER_BAR), 0xFFFF_FFFF);
    let Some(c) = miner_cfg(game) else {
        return;
    };
    let small = |batch: &mut horizon_client::hud::HudBatch, text: &str, y: f32| {
        game.native_text_scaled(batch, text, [ox + 9.0 + 1.0, oy + 17.0 + y + 3.0], 0x3C_FE9A, 0.8);
    };
    small(batch, &format!("Filters: {}", c.filters.len()), 5.0);
    small(batch, &format!("Block radius: {}", c.region.radius), 18.0);
    small(batch, &format!("Min Y level: {}", c.region.min_y), 44.0);
    small(batch, &format!("Max Y level: {}", c.region.max_y), 71.0);
    draw_button(game, batch, at(MCFG_BACK), inside(mouse, MCFG_BACK), true, 0xFFFF_FFFF, Some(("button/back", 14.0)));
    // The inverse switch (`GuiDigitalSwitch`, `LEFT_ICON`).
    let (sx, sy) = (ox + MCFG_SWITCH[0] + 15.0, oy + MCFG_SWITCH[1]);
    let (top, bottom) = if c.inverse { ("switch/selected", "switch/unselected") } else { ("switch/unselected", "switch/selected") };
    sprite(game, batch, top, [0.0, 0.0, 15.0, 8.0], [sx, sy, 15.0, 8.0]);
    sprite(game, batch, bottom, [0.0, 0.0, 15.0, 8.0], [sx, sy + 9.0, 15.0, 8.0]);
    sprite(game, batch, "switch/inverse", [0.0, 0.0, 5.0, 5.0], [ox + MCFG_SWITCH[0] + 5.0, oy + MCFG_SWITCH[1] + 6.0, 5.0, 5.0]);
    for (j, text) in ["On", "Off"].into_iter().enumerate() {
        let tw = game.native_text_width(text) * 0.6;
        game.native_text_scaled(batch, text, [sx + (15.0 - tw) / 2.0, sy + 9.0 * j as f32 + 1.0], 0xFF10_1010, 0.6);
    }
    // The inverse replace target slot, and whether inverse mode requires the replacement.
    sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], at(MCFG_SLOT));
    if !c.inv_replace.is_empty() {
        game.native_item(batch, &c.inv_replace, [ox + MCFG_SLOT[0] + 1.0, oy + MCFG_SLOT[1] + 1.0], 16.0);
    }
    if inside(mouse, MCFG_SLOT) {
        game.native_fill(batch, [ox + MCFG_SLOT[0] + 1.0, oy + MCFG_SLOT[1] + 1.0, 16.0, 16.0], 0x80FF_FFFF);
    }
    draw_button(game, batch, at(MCFG_REQUIRE), inside(mouse, MCFG_REQUIRE), true, 0xFFFF_FFFF, Some(("button/exclamation", 16.0)));
    MINER_UI.with_borrow(|ui| {
        for (i, r) in MCFG_FIELDS.iter().enumerate() {
            let b = at(*r);
            draw_text_field(game, batch, b, &ui.fields[i]);
            let ready = !ui.fields[i].value.is_empty();
            let mark = [b[0] + b[2] - 12.0, b[1] - 1.0, 12.0, 12.0];
            game.native_sprite_tinted(batch, &format!("{SPRITES}button/checkmark_digital"), [0.0, 0.0, 12.0, 12.0], mark, if ready { 0xFFFF_FFFF } else { 0x60FF_FFFF });
        }
        // The list.
        let hidden = c.filters.len().saturating_sub(SORTER_ROWS);
        let first = ui.scroll.min(hidden);
        let thumb = if hidden > 0 { (first as f32 / hidden as f32 * (SORTER_BAR[3] - 2.0 - 15.0)).round() } else { 0.0 };
        let bar = if hidden > 0 { "scroll/bar" } else { "scroll/bar_inactive" };
        sprite(game, batch, bar, [0.0, 0.0, 12.0, 15.0], [ox + SORTER_BAR[0] + 1.0, oy + SORTER_BAR[1] + 1.0 + thumb, 12.0, 15.0]);
        for (i, f) in c.filters.iter().skip(first).take(SORTER_ROWS).enumerate() {
            let index = first + i;
            let row = sorter_row(i);
            let [rx, ry] = [ox + row[0], oy + row[1]];
            nine_slice(game, batch, if inside(mouse, row) { "list/filter/hovered" } else { "list/filter/base" }, 156.0, 1.0, [rx, ry, 156.0, 29.0]);
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [rx + 2.0, ry + 2.0, 18.0, 18.0]);
            if f.kind == miner::FilterKind::Item && !f.name.is_empty() {
                game.native_item(batch, &f.name, [rx + 3.0, ry + 3.0], 16.0);
            }
            game.native_text(batch, &miner_filter_name(f), [rx + 22.0, ry + 3.0], 0x40_4040, false);
            let (up, down, radio) = sorter_row_parts(i);
            if index > 0 {
                let name = if inside(mouse, up) { "button/filter_arrow/up_hovered" } else { "button/filter_arrow/up" };
                sprite(game, batch, name, [0.0, 0.0, 11.0, 7.0], at(up));
            }
            if index + 1 < c.filters.len() {
                let name = if inside(mouse, down) { "button/filter_arrow/down_hovered" } else { "button/filter_arrow/down" };
                sprite(game, batch, name, [0.0, 0.0, 11.0, 7.0], at(down));
            }
            let name = if f.enabled {
                "button/radio/selected"
            } else if inside(mouse, radio) {
                "button/radio/hovered"
            } else {
                "button/radio/base"
            };
            sprite(game, batch, name, [0.0, 0.0, 8.0, 8.0], at(radio));
        }
    });
    draw_mek_button(game, batch, at(SORTER_NEW), true, inside(mouse, SORTER_NEW));
    let w = game.native_text_width("New Filter");
    game.native_text(batch, "New Filter", [ox + 96.0 + (156.0 - w) / 2.0, oy + 142.0], 0xFF_FFFF, true);
}

/// The windows over the config screen (hook point `native_screen_overlay`): the select window and the
/// item, tag and mod id filter windows.
fn draw_miner_windows(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    let at = [cursor[0] - ox, cursor[1] - oy];
    MINER_UI.with_borrow(|ui| {
        if ui.select {
            let sc = |r: [f32; 4]| {
                let [x, y, w, h] = select_rect(r);
                [ox + x, oy + y, w, h]
            };
            let [x, y, w, h] = sc([0.0, 0.0, SELECT_WINDOW[2], SELECT_WINDOW[3]]);
            nine_slice_tinted(game, batch, "shadow", [9.0, 9.0], 3.0, [x - 3.0, y - 3.0, w + 6.0, h + 6.0], 0xBFFF_FFFF);
            nine_slice_tinted(game, batch, "base", [16.0, 16.0], 4.0, [x, y, w, h], 0xFFFF_FFFF);
            draw_button(game, batch, sc(FILTER_CLOSE), inside(at, select_rect(FILTER_CLOSE)), true, 0xFFFF_FFFF, Some(("button/close", 12.0)));
            centred(game, batch, "Create New Filter", x, w, y + 6.0, 0x40_4040);
            nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, sc([11.0, 18.0, 130.0, 62.0]), 0xFFFF_FFFF);
            for (k, label) in ["ItemStack", "Tag", "Mod ID"].iter().enumerate() {
                let b = sc(select_button(k));
                draw_button(game, batch, b, inside(at, select_rect(select_button(k))), true, 0xFFFF_FFFF, None);
                centred(game, batch, label, b[0], b[2], b[1] + 6.0, 0xFF_FFFF);
            }
            return;
        }
        let Some(e) = ui.edit.as_ref() else {
            return;
        };
        let win = |r: [f32; 4]| [ox + MWIN[0] + r[0], oy + MWIN[1] + r[1], r[2], r[3]];
        let hover = |r: [f32; 4]| inside(at, [MWIN[0] + r[0], MWIN[1] + r[1], r[2], r[3]]);
        let [x, y, w, h] = win([0.0, 0.0, MWIN[2], MWIN[3]]);
        nine_slice_tinted(game, batch, "shadow", [9.0, 9.0], 3.0, [x - 3.0, y - 3.0, w + 6.0, h + 6.0], 0xBFFF_FFFF);
        nine_slice_tinted(game, batch, "base", [16.0, 16.0], 4.0, [x, y, w, h], 0xFFFF_FFFF);
        let what = match e.filter.kind {
            miner::FilterKind::Item => "Item Filter",
            miner::FilterKind::Tag => "Tag Filter",
            miner::FilterKind::Mod => "Mod ID Filter",
        };
        let new = e.index.is_none();
        if new {
            draw_button(game, batch, win(MCFG_BACK_WIN), hover(MCFG_BACK_WIN), true, 0xFFFF_FFFF, Some(("button/back", 14.0)));
        } else {
            draw_button(game, batch, win(FILTER_CLOSE), hover(FILTER_CLOSE), true, 0xFFFF_FFFF, Some(("button/close", 12.0)));
        }
        let title = if new { format!("New: {what}") } else { format!("Edit: {what}") };
        let pad = if new { 3.0 } else { 0.0 };
        centred(game, batch, &title, x + pad, w - pad, y + 5.0, 0x40_4040);
        sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], win(FILTER_SLOT));
        if e.filter.kind == miner::FilterKind::Item && !e.filter.name.is_empty() {
            game.native_item(batch, &e.filter.name, [x + 8.0, y + 19.0], 16.0);
        }
        let s = win(FILTER_SCREEN);
        nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, s, 0xFFFF_FFFF);
        let ok = e.status.is_empty() || game.ticks as u64 >= e.ticker;
        game.native_text(batch, &format!("Status: {}", if ok { "\u{a7}2OK" } else { &e.status }), [s[0] + 3.0, s[1] + 3.0], 0x3C_FE9A, false);
        match e.filter.kind {
            miner::FilterKind::Item => game.native_text_scaled(batch, &miner_item_name(&e.filter.name), [s[0] + 3.0, s[1] + 14.0], 0x3C_FE9A, 0.8),
            _ => {
                let b = win(FILTER_NAME);
                let t = &e.name;
                game.native_fill(batch, [b[0] - 1.0, b[1] - 1.0, b[2] + 2.0, b[3] + 2.0], if t.focused { 0xFFFF_FFFF } else { 0xFFA0_A0A0 });
                game.native_fill(batch, b, 0xFF00_0000);
                game.native_text_scaled(batch, &t.value, [b[0] + 2.0, b[1] + 2.0], 0xE0_E0E0, 0.8);
                if t.focused && (game.ticks / 6) % 2 == 0 {
                    let before: String = t.value.chars().take(t.cursor).collect();
                    game.native_text_scaled(batch, "_", [b[0] + 2.0 + game.native_text_width(&before) * 0.8, b[1] + 2.0], 0xE0_E0E0, 0.8);
                }
                let hint = if e.filter.kind == miner::FilterKind::Tag { "Tag: " } else { "ID: " };
                game.native_text_scaled(batch, &format!("{hint}{}", e.filter.name), [s[0] + 3.0, s[1] + 14.0], 0x3C_FE9A, 0.8);
            }
        }
        // `addMinerDefaults`: the replace target slot and the require-replacement toggle.
        sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], win(MWIN_REPLACE));
        if !e.filter.replace.is_empty() {
            game.native_item(batch, &e.filter.replace, [x + MWIN_REPLACE[0] + 1.0, y + MWIN_REPLACE[1] + 1.0], 16.0);
        }
        draw_button(game, batch, win(MWIN_REQUIRE), hover(MWIN_REQUIRE), true, 0xFFFF_FFFF, Some(("button/exclamation", 16.0)));
        for (r, label) in [(MWIN_DELETE, if new { "Cancel" } else { "Delete" }), (MWIN_SAVE, "Save")] {
            let b = win(r);
            draw_button(game, batch, b, hover(r), true, 0xFFFF_FFFF, None);
            centred(game, batch, label, b[0], b[2], b[1] + 6.0, 0xFF_FFFF);
        }
    });
}

const MCFG_BACK_WIN: [f32; 4] = [6.0, 6.0, 11.0, 11.0];

/// A press on the config screen (hook point `native_screen_click`).
fn miner_cfg_click(game: &Game<'_>, at: [f32; 2], right: bool) {
    let Some(c) = miner_cfg(game) else {
        return;
    };
    let shift = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) || game.keys.contains(&winit::keyboard::KeyCode::ShiftRight);
    let held = game.inventory.get(game.selected).cloned().flatten().map(|h| h.item.to_string());
    let is_block = |id: &str| game.pack.assets.registry.default_state(id).is_some();
    // The windows take every press while one is open.
    if MINER_UI.with_borrow(|ui| ui.select) {
        let on = |r: [f32; 4]| inside(at, select_rect(r));
        if on(FILTER_CLOSE) {
            MINER_UI.with_borrow_mut(|ui| ui.select = false);
        } else if let Some(k) = (0..3).find(|k| on(select_button(*k))) {
            MINER_UI.with_borrow_mut(|ui| {
                ui.select = false;
                let kind = miner::FilterKind::from_index(k as u32);
                ui.edit = Some(MinerEdit {
                    index: None,
                    filter: miner::Filter::new(kind, ""),
                    name: freq::TextBox { focused: kind != miner::FilterKind::Item, ..Default::default() },
                    status: String::new(),
                    ticker: 0,
                });
            });
        }
        return;
    }
    if MINER_UI.with_borrow(|ui| ui.edit.is_some()) {
        let on = |r: [f32; 4]| inside(at, [MWIN[0] + r[0], MWIN[1] + r[1], r[2], r[3]]);
        let (index, kind) = MINER_UI.with_borrow(|ui| ui.edit.as_ref().map(|e| (e.index, e.filter.kind))).unwrap_or((None, miner::FilterKind::Item));
        let edit = |change: &dyn Fn(&mut MinerEdit)| {
            MINER_UI.with_borrow_mut(|ui| {
                if let Some(e) = ui.edit.as_mut() {
                    change(e);
                }
            });
        };
        edit(&|e| e.name.focused = e.filter.kind != miner::FilterKind::Item && on(FILTER_NAME));
        if index.is_none() && on(MCFG_BACK_WIN) {
            MINER_UI.with_borrow_mut(|ui| {
                ui.edit = None;
                ui.select = true;
            });
        } else if index.is_some() && on(FILTER_CLOSE) {
            MINER_UI.with_borrow_mut(|ui| ui.edit = None);
        } else if on(FILTER_SLOT) && kind == miner::FilterKind::Item {
            if let Some(h) = held.filter(|h| is_block(h)) {
                edit(&|e| e.filter.name = h.clone());
            }
        } else if on(MWIN_REPLACE) {
            if shift {
                edit(&|e| e.filter.replace.clear());
            } else if let Some(h) = held.filter(|h| is_block(h)) {
                edit(&|e| e.filter.replace = h.clone());
            }
        } else if on(MWIN_REQUIRE) {
            edit(&|e| e.filter.requires_replacement = !e.filter.requires_replacement);
        } else if on(MWIN_DELETE) {
            if let Some(i) = index {
                send_miner(game, c.pos, 8, i as i32, "");
            }
            MINER_UI.with_borrow_mut(|ui| ui.edit = None);
        } else if on(MWIN_SAVE) {
            miner_save_filter(game, c.pos);
        }
        return;
    }
    if inside(at, MCFG_BACK) {
        MINER_VIEW.set(None);
        MINER_UI.with_borrow_mut(|ui| *ui = MinerUi::default());
        return;
    }
    if inside(at, MCFG_SWITCH) {
        send_miner(game, c.pos, 3, 0, "");
        return;
    }
    if inside(at, MCFG_REQUIRE) {
        send_miner(game, c.pos, 4, 0, "");
        return;
    }
    if inside(at, MCFG_SLOT) {
        if shift {
            send_miner(game, c.pos, 5, 0, "");
        } else if let Some(h) = held.filter(|h| is_block(h)) {
            send_miner(game, c.pos, 5, 0, &h);
        }
        return;
    }
    for (i, r) in MCFG_FIELDS.iter().enumerate() {
        let focus = inside(at, *r);
        let mark = inside(at, [r[0] + r[2] - 12.0, r[1] - 1.0, 12.0, 12.0]);
        MINER_UI.with_borrow_mut(|ui| ui.fields[i].focused = focus && !mark);
        if mark {
            miner_set_field(game, c.pos, i);
            return;
        }
        if focus {
            return;
        }
    }
    if inside(at, SORTER_NEW) {
        MINER_UI.with_borrow_mut(|ui| ui.select = true);
        return;
    }
    let (first, hidden) = (MINER_UI.with_borrow(|ui| ui.scroll), c.filters.len().saturating_sub(SORTER_ROWS));
    if inside(at, SORTER_BAR) {
        let f = ((at[1] - SORTER_BAR[1] - 8.0) / (SORTER_BAR[3] - 16.0)).clamp(0.0, 1.0);
        MINER_UI.with_borrow_mut(|ui| ui.scroll = ((hidden as f32 + 0.5) * f) as usize);
        return;
    }
    let first = first.min(hidden);
    for i in 0..SORTER_ROWS.min(c.filters.len().saturating_sub(first)) {
        let index = first + i;
        let (up, down, radio) = sorter_row_parts(i);
        if !inside(at, sorter_row(i)) {
            continue;
        }
        if inside(at, up) && index > 0 {
            send_miner(game, c.pos, if shift { 11 } else { 9 }, index as i32, "");
        } else if inside(at, down) && index + 1 < c.filters.len() {
            send_miner(game, c.pos, if shift { 12 } else { 10 }, index as i32, "");
        } else if inside(at, radio) {
            send_miner(game, c.pos, 13, index as i32, "");
        } else if !right {
            let filter = c.filters[index].clone();
            MINER_UI.with_borrow_mut(|ui| {
                let name = freq::TextBox { value: filter.name.clone(), cursor: filter.name.len(), focused: false };
                ui.edit = Some(MinerEdit { index: Some(index), filter, name, status: String::new(), ticker: 0 });
            });
        }
        return;
    }
}

/// The field's Enter (`setText`): the number goes to the server and the box empties.
fn miner_set_field(game: &Game<'_>, pos: [i32; 3], field: usize) {
    let value = MINER_UI.with_borrow_mut(|ui| std::mem::take(&mut ui.fields[field]).value);
    if let Ok(n) = value.parse::<i32>() {
        send_miner(game, pos, field as u8, n, "");
    }
}

/// Save (`validateAndSave`): a filter that names nothing keeps the window open with the reason.
fn miner_save_filter(game: &Game<'_>, pos: [i32; 3]) {
    let fail = MINER_UI.with_borrow_mut(|ui| {
        let e = ui.edit.as_mut()?;
        if e.filter.kind != miner::FilterKind::Item {
            e.filter.name = e.name.value.clone();
        }
        if e.filter.name.is_empty() {
            e.status = match e.filter.kind {
                miner::FilterKind::Item => "\u{a7}4No item",
                miner::FilterKind::Tag => "\u{a7}4No tag",
                miner::FilterKind::Mod => "\u{a7}4No ID",
            }
            .to_owned();
            e.ticker = game.ticks as u64 + 100;
            return Some(());
        }
        let (index, line) = (e.index, e.filter.save());
        ui.edit = None;
        match index {
            Some(i) => send_miner(game, pos, 7, i as i32, &line),
            None => send_miner(game, pos, 6, 0, &line),
        }
        None
    });
    let _ = fail;
}

fn miner_cfg_tooltip(game: &Game<'_>, at: [f32; 2], lines: &mut Vec<String>) {
    let Some(c) = miner_cfg(game) else {
        return;
    };
    if MINER_UI.with_borrow(|ui| ui.select) {
        return;
    }
    if let Some(e) = MINER_UI.with_borrow(|ui| ui.edit.as_ref().map(|e| (e.index.is_none(), e.filter.requires_replacement))) {
        let on = |r: [f32; 4]| inside(at, [MWIN[0] + r[0], MWIN[1] + r[1], r[2], r[3]]);
        if e.0 && on(MCFG_BACK_WIN) {
            lines.push("Back".to_owned());
        } else if on(MWIN_REQUIRE) {
            lines.push(format!("Require replace: {}", if e.1 { "Yes" } else { "No" }));
        }
        return;
    }
    if inside(at, MCFG_BACK) {
        lines.push("Back".to_owned());
    } else if inside(at, MCFG_SWITCH) {
        lines.push("Inverse mode".to_owned());
    } else if inside(at, MCFG_REQUIRE) {
        lines.push(format!("Inverse mode requires replacement: {}", if c.inv_require { "Yes" } else { "No" }));
    }
    let first = MINER_UI.with_borrow(|ui| ui.scroll).min(c.filters.len().saturating_sub(SORTER_ROWS));
    for i in 0..SORTER_ROWS.min(c.filters.len().saturating_sub(first)) {
        let (up, down, radio) = sorter_row_parts(i);
        if inside(at, up) {
            lines.push("Move Up".to_owned());
            lines.push("Hold shift to move to top".to_owned());
        } else if inside(at, down) {
            lines.push("Move Down".to_owned());
            lines.push("Hold shift to move to bottom".to_owned());
        } else if inside(at, radio) {
            lines.push(if c.filters[first + i].enabled { "Filter: \u{a7}aenabled".to_owned() } else { "Filter: \u{a7}cdisabled".to_owned() });
        }
    }
}

fn miner_cfg_covers(at: [f32; 2]) -> bool {
    MINER_UI.with_borrow(|ui| (ui.select && inside(at, select_window())) || (ui.edit.is_some() && inside(at, MWIN)))
}

fn miner_cfg_typing() -> bool {
    MINER_UI.with_borrow(|ui| ui.fields.iter().any(|f| f.focused) || ui.edit.as_ref().is_some_and(|e| e.name.focused))
}

/// A key in a focused field: digits (a minus for the Y fields); Enter sends a setting.
fn miner_cfg_key(game: &Game<'_>, key: &str, text: Option<&str>) {
    let Some(c) = miner_cfg(game) else {
        return;
    };
    MINER_UI.with_borrow_mut(|ui| {
        if let Some(e) = ui.edit.as_mut().filter(|e| e.name.focused) {
            e.name.key(key, text, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./:#@".contains(c), 64);
            return;
        }
        let Some(i) = ui.fields.iter().position(|f| f.focused) else {
            return;
        };
        let negative = i > 0;
        let max = if i == 0 { 2 } else { 4 };
        if ui.fields[i].key(key, text, |c| c.is_ascii_digit() || (negative && c == '-'), max) == freq::Typed::Enter {
            let value = std::mem::take(&mut ui.fields[i]).value;
            ui.fields[i].focused = true;
            if let Ok(n) = value.parse::<i32>() {
                send_miner(game, c.pos, i as u8, n, "");
            }
        }
    });
}

fn miner_cfg_escape(consumed: &mut bool) {
    let closed = MINER_UI.with_borrow_mut(|ui| {
        if ui.edit.is_some() {
            ui.edit = None;
            true
        } else if ui.select {
            ui.select = false;
            true
        } else {
            false
        }
    });
    *consumed |= closed;
}

// --- Transporter configuration window (GuiTransporterConfig) ---------------------------------------------

/// Where the ejector's data starts after the screen's redstone slot (after the factory's progress and
/// sorting slots).
const EJECTOR_AT: usize = 5 + 10;
const TRANSPORTER_WINDOW: [f32; 4] = [10.0, 15.0, 156.0, 119.0];
const TRANSPORTER_SCREEN: [f32; 4] = [38.0, 15.0, 80.0, 12.0];
const TRANSPORTER_STRICT: [f32; 4] = [136.0, 6.0, 14.0, 14.0];
const TRANSPORTER_COLOR: [f32; 4] = [112.0, 49.0, 16.0, 16.0];
/// The side buttons by relative side (front, left, right, back, top, bottom), 22 × 22.
const TRANSPORTER_SIDES: [[f32; 2]; 6] = [[41.0, 57.0], [18.0, 57.0], [64.0, 57.0], [18.0, 80.0], [41.0, 34.0], [41.0, 80.0]];
/// The menu buttons it sends: strict input, the output colour (+0 next, +1 clear, +2 previous), and each
/// side's input colour (310 + 3 × side, the same steps).
const BUTTON_STRICT: i32 = 300;
const BUTTON_OUT_COLOR: i32 = 301;
const BUTTON_IN_COLOR: i32 = 310;

fn in_transporter(r: [f32; 4]) -> [f32; 4] {
    [TRANSPORTER_WINDOW[0] + r[0], TRANSPORTER_WINDOW[1] + r[1], r[2], r[3]]
}

/// The ejector as the screen's data holds it: the output colour, the input colours, strict input.
fn ejector_of(data: &[i32], l: &Layout) -> (Option<u8>, [Option<u8>; 6], bool) {
    let at = redstone_slot(l) + EJECTOR_AT;
    let color = |i: usize| data.get(at + i).copied().filter(|c| (1..=16).contains(c)).map(|c| (c - 1) as u8);
    (color(0), std::array::from_fn(|s| color(1 + s)), data.get(at + 7).copied().unwrap_or(0) != 0)
}

fn dye_rgb(c: u8) -> u32 {
    freq::COLORS[usize::from(c) % 16].1
}

fn draw_transporter_window(game: &Game<'_>, block: &str, data: &[i32], batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    let Some(l) = layout(block) else {
        return;
    };
    let [ox, oy] = origin;
    let cursor = game.native_cursor();
    let at = [cursor[0] - ox, cursor[1] - oy];
    let screen = |r: [f32; 4]| {
        let [x, y, w, h] = in_transporter(r);
        [ox + x, oy + y, w, h]
    };
    let hover = |r: [f32; 4]| inside(at, in_transporter(r));
    let [x, y, w, h] = screen([0.0, 0.0, TRANSPORTER_WINDOW[2], TRANSPORTER_WINDOW[3]]);
    nine_slice_tinted(game, batch, "shadow", [9.0, 9.0], 3.0, [x - 3.0, y - 3.0, w + 6.0, h + 6.0], 0xBFFF_FFFF);
    nine_slice_tinted(game, batch, "base", [16.0, 16.0], 4.0, [x, y, w, h], 0xFFFF_FFFF);
    draw_button(game, batch, screen(WINDOW_CLOSE), hover(WINDOW_CLOSE), true, 0xFFFF_FFFF, Some(("button/close", 12.0)));
    let (out, ins, strict) = ejector_of(data, &l);
    // The title leaves room for the strict input button (`getTitlePadEnd`).
    centred(game, batch, "Transporter Config", x, w - 18.0, y + 5.0, 0x40_4040);
    let s = screen(TRANSPORTER_SCREEN);
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, s, 0xFFFF_FFFF);
    game.native_text_scaled(batch, &format!("Strict Input: {}", if strict { "On" } else { "Off" }), [s[0] + 3.0, s[1] + 3.0], 0x3C_FE9A, 0.8);
    draw_button(game, batch, screen(TRANSPORTER_STRICT), hover(TRANSPORTER_STRICT), true, 0xFFFF_FFFF, Some(("button/exclamation", 14.0)));
    sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [x + 111.0, y + 48.0, 18.0, 18.0]);
    if let Some(c) = out {
        game.native_fill(batch, screen(TRANSPORTER_COLOR), 0xFF00_0000 | dye_rgb(c));
    }
    let (packed, _) = window_sides(&l, data, Transmission::Item);
    let pos = game.container.map(|(x, y, z)| [x, y, z]);
    let facing = pos.and_then(|p| game.native_block_property(p, "facing")).and_then(|f| direction(&f)).map_or(2, face_index);
    for (side, [bx, by]) in TRANSPORTER_SIDES.iter().enumerate() {
        let r = [*bx, *by, 22.0, 22.0];
        let mode = Sides::relative_get(packed, side);
        let enabled = matches!(mode, SideMode::Input | SideMode::Default);
        let (_, rgb, _) = mode.data_type();
        draw_button(game, batch, screen(r), enabled && hover(r), enabled, button_tint(rgb), None);
        if let Some(c) = ins[side] {
            let b = screen(r);
            game.native_fill(batch, [b[0] + 5.0, b[1] + 5.0, 12.0, 12.0], 0xFF00_0000 | dye_rgb(c));
        } else if let Some(p) = pos {
            let d = DIRECTIONS[side_config::relative_face(facing, side)].1;
            let beside = game.native_block([p[0] + d[0], p[1] + d[1], p[2] + d[2]]);
            if !beside.ends_with(":air") {
                let b = screen(r);
                game.native_item_stack(batch, &stack_of(&beside, 1), [b[0] + 3.0, b[1] + 3.0], 16.0);
            }
        }
    }
    centred(game, batch, "Input", x + 18.0, 68.0, y + 105.0, 0x40_4040);
    centred(game, batch, "Output", x + 86.0, w - 86.0, y + 68.0, 0x40_4040);
}

fn transporter_window_click(block: &str, data: &[i32], at: [f32; 2], right: bool, button: &mut Option<i32>) -> bool {
    if !inside(at, TRANSPORTER_WINDOW) {
        return false;
    }
    let Some(l) = layout(block) else {
        return true;
    };
    let on = |r: [f32; 4]| inside(at, in_transporter(r));
    // Shift clears the colour; the key state is not here, so a middle step is the clear: left steps on,
    // right steps back (`MekClickType`), and the cycle passes through none.
    let step = i32::from(right) * 2;
    if on(WINDOW_CLOSE) {
        set_window(block, None);
    } else if on(TRANSPORTER_STRICT) {
        *button = Some(BUTTON_STRICT);
    } else if on(TRANSPORTER_COLOR) {
        *button = Some(BUTTON_OUT_COLOR + step);
    } else {
        let (packed, _) = window_sides(&l, data, Transmission::Item);
        if let Some(side) = TRANSPORTER_SIDES.iter().position(|[x, y]| on([*x, *y, 22.0, 22.0]))
            && matches!(Sides::relative_get(packed, side), SideMode::Input | SideMode::Default)
        {
            *button = Some(BUTTON_IN_COLOR + 3 * side as i32 + step);
        }
    }
    true
}

fn transporter_window_tooltip(block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) -> bool {
    if !inside(at, TRANSPORTER_WINDOW) {
        return false;
    }
    let Some(l) = layout(block) else {
        return true;
    };
    let on = |r: [f32; 4]| inside(at, in_transporter(r));
    let (out, ins, _) = ejector_of(data, &l);
    let name = |c: Option<u8>| c.map_or("None", |c| freq::COLORS[usize::from(c) % 16].0);
    if on(TRANSPORTER_STRICT) {
        lines.push("Strict Input".into());
    } else if on(TRANSPORTER_COLOR) {
        lines.push(name(out).into());
    } else if let Some(side) = TRANSPORTER_SIDES.iter().position(|[x, y]| on([*x, *y, 22.0, 22.0])) {
        lines.push(name(ins[side]).into());
    }
    true
}

/// A press of the transporter window, on the server: strict input, the output colour, a side's input
/// colour, stepping through none and the sixteen dyes.
fn transporter_button(entity: &mut ModBlockEntity, id: i32) -> bool {
    if tabs(&entity.id) & TAB_SIDES == 0 || !(BUTTON_STRICT..BUTTON_IN_COLOR + 18).contains(&id) {
        return false;
    }
    let step = |nbt: &mut Compound, key: &str, how: i32| {
        let now = get_int(nbt, key);
        let cur = (now > 0).then(|| (now - 1) as u8);
        // `TransporterUtils.increment` and `decrement`; a middle press clears.
        let next = match how {
            1 => None,
            h => sorter::Opts::step_color(cur, h == 0),
        };
        put_int(nbt, key, next.map_or(0, |c| u32::from(c) + 1));
    };
    if id == BUTTON_STRICT {
        let on = get_int(&entity.nbt, "ej_strict") == 0;
        put_int(&mut entity.nbt, "ej_strict", u32::from(on));
    } else if (BUTTON_OUT_COLOR..BUTTON_OUT_COLOR + 3).contains(&id) {
        step(&mut entity.nbt, "ej_out", id - BUTTON_OUT_COLOR);
    } else if id >= BUTTON_IN_COLOR {
        let (side, how) = ((id - BUTTON_IN_COLOR) / 3, (id - BUTTON_IN_COLOR) % 3);
        step(&mut entity.nbt, &format!("ej_in{side}"), how);
    }
    true
}

/// The personal chest's and barrel's screen (`GuiPersonalStorageTile`): the panel, its title, six rows of
/// slots, then the inventory.
fn draw_storage_screen(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    let [ox, oy] = origin;
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, 176.0, 222.0]);
    game.native_text(batch, &game.native_screen_title().unwrap_or_default(), [ox + 8.0, oy + 6.0], 0x40_4040, false);
    for row in 0..6 {
        for col in 0..9 {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 7.0 + 18.0 * col as f32, oy + 17.0 + 18.0 * row as f32, 18.0, 18.0]);
        }
    }
    game.native_text(batch, "Inventory", [ox + 8.0, oy + 129.0], 0x40_4040, false);
    for row in 0..4 {
        let y = if row < 3 { 140.0 + 18.0 * row as f32 } else { 198.0 };
        for col in 0..9 {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 7.0 + 18.0 * col as f32, oy + y - 1.0, 18.0, 18.0]);
        }
    }
}

// --- QIO Redstone Adapter screen (GuiQIORedstoneAdapter) -------------------------------------------------

const ADAPTER_CHANNEL: &str = "mekanism:qio_adapter";
const ADAPTER_FREQ: [f32; 4] = [7.0, 16.0, 161.0, 12.0];
const ADAPTER_SLOT: [f32; 4] = [7.0, 30.0, 18.0, 18.0];
const ADAPTER_INFO: [f32; 4] = [27.0, 30.0, 141.0, 64.0];
const ADAPTER_INVERT: [f32; 4] = [9.0, 64.0, 14.0, 14.0];
const ADAPTER_FUZZY: [f32; 4] = [9.0, 80.0, 14.0, 14.0];
const ADAPTER_TEXT: [f32; 4] = [29.0, 80.0, 137.0, 12.0];

thread_local! {
    static ADAPTER_TEXT_BOX: std::cell::RefCell<freq::TextBox> = std::cell::RefCell::new(freq::TextBox::default());
}

struct AdapterView {
    pos: [i32; 3],
    frequency: Option<String>,
    item: Option<String>,
    count: u64,
    inverted: bool,
    fuzzy: bool,
    stored: u64,
}

fn adapter_view(game: &Game<'_>) -> Option<AdapterView> {
    let (x, y, z) = game.container?;
    let (_, c) = game.native_client_block_entity([x, y, z])?;
    Some(AdapterView {
        pos: [x, y, z],
        frequency: get_string(c, "frequency"),
        item: get_string(c, "filter"),
        count: get_long(c, "count"),
        inverted: get_int(c, "inverted") == 1,
        fuzzy: get_int(c, "fuzzy") == 1,
        stored: get_long(c, "stored"),
    })
}

/// What the screen shows: the frequency, the item and count, the signal's sense, and what the
/// frequency stores of the item.
fn adapter_sync(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if kind(&entity.id) != Some(Kind::QioRedstoneAdapter) {
        return;
    }
    let mut client = Compound::new();
    let frequency = get_string(&entity.nbt, "frequency");
    let item = get_string(&entity.nbt, "filter");
    if let Some(f) = frequency_label(&entity.nbt) {
        client.insert("frequency".into(), Tag::String(f.as_str().into()));
    }
    if let Some(i) = &item {
        client.insert("filter".into(), Tag::String(i.as_str().into()));
        if let Some(f) = &frequency {
            put_long(&mut client, "stored", load_network(game, f).0.stored(i));
        }
    }
    put_long(&mut client, "count", get_long(&entity.nbt, "count"));
    put_int(&mut client, "inverted", get_int(&entity.nbt, "inverted"));
    put_int(&mut client, "fuzzy", get_int(&entity.nbt, "fuzzy"));
    put_client(game, pos, entity, &["frequency", "filter", "count", "inverted", "fuzzy", "stored"], client);
}

/// A player's press on the adapter's screen: `pos`, an op (0 the item in hand, 1 clear the item, 2 invert,
/// 3 fuzzy, 4 the count) and the count. Only within reach.
fn adapter_payload(game: &mut Game<'_>, data: &[u8]) {
    let (Some(pos), Some(&op), Some(count), Some(player)) = (data.get(..12), data.get(12), data.get(13..17), game.native_acting_player()) else {
        return;
    };
    let int = |i: usize| i32::from_be_bytes([pos[i], pos[i + 1], pos[i + 2], pos[i + 3]]);
    let pos = [int(0), int(4), int(8)];
    let near = (0..3).map(|k| (player.position[k] - f64::from(pos[k]) - 0.5).powi(2)).sum::<f64>() <= 64.0;
    if !near || kind(block_name(game, pos)) != Some(Kind::QioRedstoneAdapter) {
        return;
    }
    let held = game.inventory.get(game.selected).cloned().flatten().map(|h| h.item.to_string());
    let count = u32::from_be_bytes([count[0], count[1], count[2], count[3]]).min(i32::MAX as u32);
    game.native_with_mod_block_entity(pos, |m| match op {
        0 => {
            if let Some(item) = held {
                m.nbt.insert("filter".into(), Tag::String(item.as_str().into()));
            }
        }
        1 => {
            m.nbt.remove(&JavaString::from("filter"));
        }
        2 => {
            let flipped = u32::from(get_int(&m.nbt, "inverted") == 0);
            put_int(&mut m.nbt, "inverted", flipped)
        }
        3 => {
            let flipped = u32::from(get_int(&m.nbt, "fuzzy") == 0);
            put_int(&mut m.nbt, "fuzzy", flipped)
        }
        4 => put_long(&mut m.nbt, "count", u64::from(count)),
        _ => {}
    });
}

fn send_adapter(game: &Game<'_>, pos: [i32; 3], op: u8, count: u32) {
    let mut data = Vec::new();
    for c in pos {
        data.extend(c.to_be_bytes());
    }
    data.push(op);
    data.extend(count.to_be_bytes());
    game.native_send_to_server(ADAPTER_CHANNEL, &data);
}

fn draw_adapter_screen(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    let [ox, oy] = origin;
    let at = |r: [f32; 4]| [ox + r[0], oy + r[1], r[2], r[3]];
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, 176.0, 192.0]);
    let title = game.native_screen_title().unwrap_or_default();
    centred(game, batch, &title, ox, 176.0, oy + 4.0, 0x40_4040);
    for row in 0..4 {
        let y = if row < 3 { 110.0 + 18.0 * row as f32 } else { 168.0 };
        for col in 0..9 {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 7.0 + 18.0 * col as f32, oy + y - 1.0, 18.0, 18.0]);
        }
    }
    game.native_text(batch, "Inventory", [ox + 8.0, oy + 99.0], 0x40_4040, false);
    let Some(v) = adapter_view(game) else {
        return;
    };
    let s = at(ADAPTER_FREQ);
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, s, 0xFFFF_FFFF);
    let line = v.frequency.as_ref().map_or("No frequency".to_owned(), |f| format!("Frequency: {f}"));
    game.native_text_scaled(batch, &line, [s[0] + 3.0, s[1] + 3.0], 0x3C_FE9A, 0.8);
    // The item slot (a click with an item sets it, shift with an empty hand clears it).
    sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], at(ADAPTER_SLOT));
    if let Some(item) = &v.item {
        game.native_item_stack(batch, &stack_of(item, 1), [ox + 8.0, oy + 31.0], 16.0);
    }
    let s = at(ADAPTER_INFO);
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, s, 0xFFFF_FFFF);
    let mut lines = vec![v.item.as_deref().map_or("Undefined item".to_owned(), filter_name)];
    lines.push(format!("Trigger Count {} {}", if v.inverted { "<" } else { ">=" }, v.count));
    if v.item.is_some() && v.frequency.is_some() {
        lines.push(format!("Stored: {}", v.stored));
    }
    lines.push(format!("Fuzzy Mode: {}", if v.fuzzy { "On" } else { "Off" }));
    for (i, l) in lines.iter().enumerate() {
        game.native_text_scaled(batch, l, [s[0] + 3.0, s[1] + 3.0 + 10.0 * i as f32], 0x3C_FE9A, 0.8);
    }
    draw_button(game, batch, at(ADAPTER_INVERT), inside(mouse, ADAPTER_INVERT), true, 0xFFFF_FFFF, Some((if v.inverted { "button/toggle_flipped" } else { "button/toggle" }, 14.0)));
    draw_button(game, batch, at(ADAPTER_FUZZY), inside(mouse, ADAPTER_FUZZY), true, 0xFFFF_FFFF, Some(("button/fuzzy", 14.0)));
    // The count's text box (`GuiTextField`, digits).
    let b = at(ADAPTER_TEXT);
    ADAPTER_TEXT_BOX.with_borrow(|t| {
        game.native_fill(batch, [b[0] - 1.0, b[1] - 1.0, b[2] + 2.0, b[3] + 2.0], if t.focused { 0xFFFF_FFFF } else { 0xFFA0_A0A0 });
        game.native_fill(batch, b, 0xFF00_0000);
        game.native_text(batch, &t.value, [b[0] + 2.0, b[1] + 2.0], 0xE0_E0E0, false);
        if t.focused && (game.ticks / 6) % 2 == 0 {
            let before: String = t.value.chars().take(t.cursor).collect();
            game.native_text(batch, "_", [b[0] + 2.0 + game.native_text_width(&before), b[1] + 2.0], 0xE0_E0E0, false);
        }
    });
}

fn adapter_click(game: &Game<'_>, at: [f32; 2]) {
    let Some(v) = adapter_view(game) else {
        return;
    };
    ADAPTER_TEXT_BOX.with_borrow_mut(|t| t.focused = inside(at, ADAPTER_TEXT));
    let shift = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) || game.keys.contains(&winit::keyboard::KeyCode::ShiftRight);
    let holding = game.inventory.get(game.selected).is_some_and(|h| h.is_some());
    if inside(at, ADAPTER_SLOT) {
        // `stack.isEmpty() == event.hasShiftDown()`: set with an item and no shift, clear with none and shift.
        if holding && !shift {
            send_adapter(game, v.pos, 0, 0);
        } else if !holding && shift {
            send_adapter(game, v.pos, 1, 0);
        }
    } else if inside(at, ADAPTER_INVERT) {
        send_adapter(game, v.pos, 2, 0);
    } else if inside(at, ADAPTER_FUZZY) {
        send_adapter(game, v.pos, 3, 0);
    }
}

fn adapter_typing(block: &str) -> bool {
    kind(block) == Some(Kind::QioRedstoneAdapter) && ADAPTER_TEXT_BOX.with_borrow(|t| t.focused)
}

/// A key in the count box: digits (ten at most); Enter sends the count and clears the box
/// (`configureDigitalInput`).
fn adapter_key(game: &Game<'_>, key: &str, text: Option<&str>) {
    let Some(v) = adapter_view(game) else {
        return;
    };
    ADAPTER_TEXT_BOX.with_borrow_mut(|t| {
        if t.key(key, text, |c| c.is_ascii_digit(), 10) == freq::Typed::Enter {
            if let Ok(n) = t.value.parse::<u64>() {
                send_adapter(game, v.pos, 4, n.min(u64::from(i32::MAX as u32)) as u32);
            }
            *t = freq::TextBox { focused: true, ..Default::default() };
        }
    });
}

// --- QIO importer and exporter screens (GuiQIOFilterHandler, GuiQIOImporter, GuiQIOExporter) --------------

const QF_FREQ: [f32; 4] = [9.0, 16.0, 218.0, 12.0];
const QF_HOLDER: [f32; 4] = [9.0, 30.0, 204.0, 68.0];
const QF_NEW_HOLDER: [f32; 4] = [9.0, 98.0, 204.0, 22.0];
const QF_NEW: [f32; 4] = [10.0, 99.0, 202.0, 20.0];
const QF_BAR: [f32; 4] = [213.0, 30.0, 14.0, 90.0];
const QF_ROWS: usize = 3;
/// The exporter's two switches: export without a filter, and round robin (`GuiScreenSwitch`).
const QF_SWITCHES: [[f32; 4]; 2] = [[9.0, 122.0, 108.0, 22.0], [119.0, 122.0, 108.0, 22.0]];

fn qf_row(i: usize) -> [f32; 4] {
    [10.0, 31.0 + 22.0 * i as f32, 202.0, 22.0]
}

/// A row's up arrow, down arrow and enable radio (`MovableFilterButton`, toggle shifted 17 in).
fn qf_row_parts(i: usize) -> ([f32; 4], [f32; 4], [f32; 4]) {
    let [x, y, w, h] = qf_row(i);
    ([x + w - 14.0, y + h / 2.0 - 8.0, 11.0, 7.0], [x + w - 14.0, y + h / 2.0 + 1.0, 11.0, 7.0], [x + w - 8.0 - 17.0, y + h / 2.0 - 4.0, 8.0, 8.0])
}

/// A switch's toggle (the digital switch at the right of its inner screen).
fn qf_switch_toggle(i: usize) -> [f32; 4] {
    let [x, y, w, _] = QF_SWITCHES[i];
    [x + w - 17.0, y + 2.0, 15.0, 17.0]
}

fn qf_switches(block: &str) -> &'static [[f32; 4]] {
    if kind(block) == Some(Kind::QioExporter) { &QF_SWITCHES } else { &[] }
}

/// The first item of a tag filter, which stands for the tag in its slot (the original cycles the tag's items).
fn tag_item(game: &Game<'_>, filter: &str) -> Option<String> {
    let tag = filter.strip_prefix('#')?;
    let items = world_items!(game);
    items.item_tags.get(tag).and_then(|t| t.iter().min().map(|i| i.to_string()))
}

fn draw_qio_filter_screen(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, block: &str, origin: [f32; 2]) {
    let [ox, oy] = origin;
    let at = |r: [f32; 4]| [ox + r[0], oy + r[1], r[2], r[3]];
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, 236.0, 240.0]);
    for row in 0..4 {
        let y = if row < 3 { 158.0 + 18.0 * row as f32 } else { 216.0 };
        for col in 0..9 {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 37.0 + 18.0 * col as f32, oy + y - 1.0, 18.0, 18.0]);
        }
    }
    let title = game.native_screen_title().unwrap_or_default();
    centred(game, batch, &title, ox, 236.0, oy + 4.0, 0x40_4040);
    game.native_text(batch, "Inventory", [ox + 38.0, oy + 147.0], 0x40_4040, false);
    let Some((pos, view)) = sorter_view(game) else {
        return;
    };
    let qio = view.qio.as_ref();
    let s = at(QF_FREQ);
    nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, s, 0xFFFF_FFFF);
    let line = match qio.and_then(|q| q.frequency.as_ref()) {
        Some(f) => format!("Frequency: {f}"),
        None => "No frequency".to_owned(),
    };
    game.native_text_scaled(batch, &line, [s[0] + 3.0, s[1] + 3.0], 0x3C_FE9A, 0.8);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(QF_HOLDER), 0xFFFF_FFFF);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(QF_NEW_HOLDER), 0xFFFF_FFFF);
    draw_mek_button(game, batch, at(QF_NEW), true, inside(mouse, QF_NEW));
    let w = game.native_text_width("New Filter");
    game.native_text(batch, "New Filter", [ox + 10.0 + (202.0 - w) / 2.0, oy + 99.0 + 6.0], 0xFF_FFFF, true);
    // The list and its scroll bar.
    let hidden = view.filters.len().saturating_sub(QF_ROWS);
    let first = sorter_scroll(pos).min(hidden);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(QF_BAR), 0xFFFF_FFFF);
    let thumb = if hidden > 0 { (first as f32 / hidden as f32 * (QF_BAR[3] - 2.0 - 15.0)).round() } else { 0.0 };
    let bar = if hidden > 0 { "scroll/bar" } else { "scroll/bar_inactive" };
    sprite(game, batch, bar, [0.0, 0.0, 12.0, 15.0], [ox + QF_BAR[0] + 1.0, oy + QF_BAR[1] + 1.0 + thumb, 12.0, 15.0]);
    for (i, f) in view.filters.iter().skip(first).take(QF_ROWS).enumerate() {
        let index = first + i;
        let row = qf_row(i);
        let [rx, ry] = [ox + row[0], oy + row[1]];
        nine_slice(game, batch, if inside(mouse, row) { "list/filter/hovered" } else { "list/filter/base" }, 202.0, 1.0, [rx, ry, 202.0, 22.0]);
        // Item filters indigo, tag filters bright green, mod filters red (`MovableFilterButton`).
        let tint = match f.as_bytes().first() {
            Some(b'#') => 0x4D54_FF54,
            Some(b'@') => 0x4DFF_5555,
            _ => 0x4D54_54FF,
        };
        game.native_fill(batch, [rx, ry, 202.0, 22.0], tint);
        sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [rx + 2.0, ry + 2.0, 18.0, 18.0]);
        let shown = if f.starts_with('#') { tag_item(game, f) } else if f.starts_with('@') { None } else { Some(f.clone()) };
        if let Some(item) = shown {
            game.native_item(batch, &item, [rx + 3.0, ry + 3.0], 16.0);
        }
        let (up, down, radio) = qf_row_parts(i);
        if index > 0 {
            let name = if inside(mouse, up) { "button/filter_arrow/up_hovered" } else { "button/filter_arrow/up" };
            sprite(game, batch, name, [0.0, 0.0, 11.0, 7.0], at(up));
        }
        if index + 1 < view.filters.len() {
            let name = if inside(mouse, down) { "button/filter_arrow/down_hovered" } else { "button/filter_arrow/down" };
            sprite(game, batch, name, [0.0, 0.0, 11.0, 7.0], at(down));
        }
        let name = if !view.disabled.contains(f) {
            "button/radio/selected"
        } else if inside(mouse, radio) {
            "button/radio/hovered"
        } else {
            "button/radio/base"
        };
        sprite(game, batch, name, [0.0, 0.0, 8.0, 8.0], at(radio));
    }
    for (i, r) in qf_switches(block).iter().enumerate() {
        let (label, on) = if i == 0 { ("Export without filter", view.eject) } else { ("Round Robin", view.round_robin) };
        let s = at(*r);
        nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, s, 0xFFFF_FFFF);
        game.native_text_scaled(batch, label, [s[0] + 4.0, s[1] + 4.0], 0x3C_FE9A, 0.8);
        let t = at(qf_switch_toggle(i));
        sprite(game, batch, if on { "switch/selected" } else { "switch/unselected" }, [0.0, 0.0, 15.0, 8.0], [t[0], t[1], 15.0, 8.0]);
        sprite(game, batch, if on { "switch/unselected" } else { "switch/selected" }, [0.0, 0.0, 15.0, 8.0], [t[0], t[1] + 9.0, 15.0, 8.0]);
        game.native_text_scaled(batch, "On", [t[0] + (15.0 - game.native_text_width("On") * 0.6) / 2.0, t[1] + 1.0], 0x10_1010, 0.6);
        game.native_text_scaled(batch, "Off", [t[0] + (15.0 - game.native_text_width("Off") * 0.6) / 2.0, t[1] + 10.0], 0x10_1010, 0.6);
    }
}

fn qio_filter_click(game: &Game<'_>, block: &str, at: [f32; 2], right: bool) -> bool {
    use sorter::Op;
    let Some((pos, view)) = sorter_view(game) else {
        return false;
    };
    let shift = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) || game.keys.contains(&winit::keyboard::KeyCode::ShiftRight);
    if windowed(block, at, |wat| filter_window_click(game, pos, &view, wat, right)) {
        return true;
    }
    for i in 0..qf_switches(block).len() {
        if inside(at, qf_switch_toggle(i)) {
            send_sorter(game, if i == 0 { Op::AutoEject } else { Op::RoundRobin }, pos, 0);
            return true;
        }
    }
    if inside(at, QF_NEW) {
        FILTER_SELECT.set(Some(pos));
        return true;
    }
    let first = sorter_scroll(pos);
    let hidden = view.filters.len().saturating_sub(QF_ROWS);
    if inside(at, QF_BAR) {
        let f = ((at[1] - QF_BAR[1] - 8.0) / (QF_BAR[3] - 16.0)).clamp(0.0, 1.0);
        SORTER_SCROLL.set((pos, ((hidden as f32 + 0.5) * f) as usize));
        return true;
    }
    for i in 0..QF_ROWS.min(view.filters.len().saturating_sub(first)) {
        let index = first + i;
        let (up, down, radio) = qf_row_parts(i);
        if !inside(at, qf_row(i)) {
            continue;
        }
        if inside(at, up) && index > 0 {
            send_sorter(game, if shift { Op::Top } else { Op::Up }, pos, index);
        } else if inside(at, down) && index + 1 < view.filters.len() {
            send_sorter(game, if shift { Op::Bottom } else { Op::Down }, pos, index);
        } else if inside(at, radio) {
            send_sorter(game, Op::Toggle, pos, index);
        } else if right {
            send_sorter(game, Op::Delete, pos, index);
        } else {
            open_filter_window(&view, pos, index);
        }
        return true;
    }
    inside(at, QF_HOLDER)
}

fn qio_filter_tooltip(game: &Game<'_>, block: &str, at: [f32; 2], lines: &mut Vec<String>) {
    let Some((pos, view)) = sorter_view(game) else {
        return;
    };
    if inside(at, QF_FREQ) {
        if let Some(q) = view.qio.as_ref().filter(|q| q.frequency.is_some()) {
            lines.push(format!("\u{a7}7Items: \u{a7}9{}\u{a7}7 / \u{a7}9{}", q.totals[0], q.totals[1]));
            lines.push(format!("\u{a7}7Types: \u{a7}9{}\u{a7}7 / \u{a7}9{}", q.totals[2], q.totals[3]));
        }
        return;
    }
    if qf_switches(block).len() == 2 && inside(at, QF_SWITCHES[1]) {
        lines.push("Cycles through the filters, one item type at a time.".to_owned());
        return;
    }
    let first = sorter_scroll(pos);
    for i in 0..QF_ROWS.min(view.filters.len().saturating_sub(first)) {
        let (up, down, radio) = qf_row_parts(i);
        if inside(at, up) {
            lines.push("Move Up".to_owned());
            lines.push("Hold shift to move to top".to_owned());
        } else if inside(at, down) {
            lines.push("Move Down".to_owned());
            lines.push("Hold shift to move to bottom".to_owned());
        } else if inside(at, radio) {
            let enabled = !view.disabled.contains(&view.filters[first + i]);
            lines.push(if enabled { "Filter: \u{a7}aenabled".to_owned() } else { "Filter: \u{a7}cdisabled".to_owned() });
        } else if inside(at, qf_row(i)) {
            lines.push(view.filters[first + i].clone());
            lines.push("Right click: delete".to_owned());
        }
    }
}

// --- Oredictionificator (TileEntityOredictionificator, GuiOredictionificator, GuiOredictionificatorFilter) --

const OD_CHANNEL: &str = "mekanism:oredictionificator";
/// The filters that fit in the list at once (`FILTER_COUNT`).
const OD_ROWS: usize = 3;
const OD_HOLDER: [f32; 4] = [9.0, 17.0, 204.0, 68.0];
const OD_NEW_HOLDER: [f32; 4] = [9.0, 85.0, 204.0, 22.0];
const OD_NEW: [f32; 4] = [10.0, 86.0, 202.0, 20.0];
const OD_BAR: [f32; 4] = [213.0, 17.0, 14.0, 90.0];
/// The filter window (`GuiOredictionificatorFilter`: 162 wide, centred on the 236 wide screen, 15 down) and its parts.
const OD_WINDOW: [f32; 4] = [37.0, 15.0, 162.0, 100.0];
const OD_SCREEN: [f32; 4] = [29.0, 18.0, 116.0, 52.0];
const OD_TEXT: [f32; 4] = [31.0, 56.0, 112.0, 12.0];
const OD_LAST: [f32; 4] = [3.0, 38.0, 12.0, 12.0];
const OD_NEXT: [f32; 4] = [16.0, 38.0, 12.0, 12.0];
const OD_DELETE: [f32; 4] = [20.0, 72.0, 60.0, 20.0];
const OD_SAVE: [f32; 4] = [82.0, 72.0, 60.0, 20.0];

fn is_od(block: &str) -> bool {
    kind(block) == Some(Kind::Oredictionificator)
}

fn od_row(i: usize) -> [f32; 4] {
    [10.0, 18.0 + 22.0 * i as f32, 202.0, 22.0]
}

/// A row's enable radio button (`RadioButton`, 8 px, 4 in from the right).
fn od_radio(i: usize) -> [f32; 4] {
    let [x, y, w, h] = od_row(i);
    [x + w - 12.0, y + h / 2.0 - 4.0, 8.0, 8.0]
}

fn od_filters(nbt: &Compound) -> Vec<od::Filter> {
    strings_of(nbt, "od_filters").iter().filter_map(|l| od::Filter::load(l)).collect()
}

/// The items of a tag, in a fixed order.
fn od_tag_items(items: &horizon_client::items::Items, tag: &str) -> Vec<String> {
    let mut all: Vec<String> = items.item_tags.get(tag).map(|t| t.iter().map(|i| i.to_string()).collect()).unwrap_or_default();
    all.sort();
    all
}

/// The tags an item is in that a filter may name.
fn od_item_tags(items: &horizon_client::items::Items, item: &str) -> Vec<String> {
    let mut tags: Vec<String> = items.item_tags.iter().filter(|(k, t)| od::valid_tag(k) && t.contains(item)).map(|(k, _)| k.to_string()).collect();
    tags.sort();
    tags
}

/// The Oredictionificator, every tick: an item of a tag a filter names becomes the filter's item, as
/// much as the output slot has room for.
fn tick_oredictionificator(game: &mut Game<'_>, entity: &mut ModBlockEntity) -> Option<bool> {
    let filters = od_filters(&entity.nbt);
    let mut did = false;
    if let Some(Some(input)) = entity.items.first().cloned()
        && !filters.is_empty()
    {
        let items = world_items!(game);
        let tags = od_item_tags(items, &input.item);
        if let Some(result) = od::result(&filters, &tags, &|tag| od_tag_items(items, tag)) {
            let max = items.get(&result).map_or(64, |i| i.max_stack);
            let out = entity.items.get(2).cloned().flatten();
            let room = match &out {
                None => max,
                Some(o) if *o.item == *result => max.saturating_sub(o.count),
                Some(_) => 0,
            };
            let n = input.count.min(room);
            if n > 0 {
                entity.items[0] = (input.count > n).then(|| ItemStack { count: input.count - n, ..input });
                entity.items[2] = Some(match out {
                    Some(o) => ItemStack { count: o.count + n, ..o },
                    None => stack_of(&result, n),
                });
                did = true;
            }
        }
    }
    put_int(&mut entity.nbt, "did", u32::from(did));
    Some(did)
}

/// Sends the filters to the players looking at the machine.
fn od_sync(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if !is_od(&entity.id) {
        return;
    }
    let mut client = Compound::new();
    client.insert("od_filters".into(), Tag::List(strings_of(&entity.nbt, "od_filters").iter().map(|s| Tag::String(s.as_str().into())).collect()));
    put_client(game, pos, entity, &["od_filters"], client);
}

/// A player's packet, on the server: a radio button, Delete or Save. Only within reach.
fn od_payload(game: &mut Game<'_>, data: &[u8]) {
    let (Some(r), Some(player)) = (od::Request::decode(data), game.native_acting_player()) else {
        return;
    };
    let near = (0..3).map(|k| (player.position[k] - f64::from(r.pos[k]) - 0.5).powi(2)).sum::<f64>() <= 64.0;
    if !near || !is_od(block_name(game, r.pos)) {
        return;
    }
    let exists = world_items!(game).item_tags.contains_key(r.tag.as_str());
    let index = (r.index != od::NEW).then_some(usize::from(r.index));
    game.native_with_mod_block_entity(r.pos, |m| {
        let mut filters = od_filters(&m.nbt);
        match r.op {
            od::Op::Toggle => {
                if let Some(f) = index.and_then(|i| filters.get_mut(i)) {
                    f.enabled = !f.enabled;
                }
            }
            od::Op::Delete => {
                if let Some(i) = index.filter(|i| *i < filters.len()) {
                    filters.remove(i);
                }
            }
            od::Op::Save => {
                if od::valid_target(&r.tag, exists) {
                    let enabled = index.and_then(|i| filters.get(i)).is_none_or(|f| f.enabled);
                    od::save(&mut filters, index, od::Filter { enabled, tag: r.tag.clone(), selected: r.selected.clone() });
                }
            }
        }
        let lines: Vec<String> = filters.iter().map(od::Filter::save).collect();
        m.nbt.insert("od_filters".into(), string_list(&lines));
    });
}

fn od_send(game: &Game<'_>, op: od::Op, pos: [i32; 3], index: Option<usize>, filter: &od::Filter) {
    let r = od::Request { op, pos, index: index.map_or(od::NEW, |i| i.min(usize::from(u16::MAX - 1)) as u16), tag: filter.tag.clone(), selected: filter.selected.clone() };
    game.native_send_to_server(OD_CHANNEL, &r.encode());
}

/// The filters the open machine has, and where it is.
fn od_view(game: &Game<'_>) -> Option<([i32; 3], Vec<od::Filter>)> {
    let (x, y, z) = game.container?;
    let (_, c) = game.native_client_block_entity([x, y, z])?;
    Some(([x, y, z], strings_of(c, "od_filters").iter().filter_map(|l| od::Filter::load(l)).collect()))
}

/// A filter window being edited: a copy of the filter (`index`) or a new one, its text box and status line.
struct OdEdit {
    index: Option<usize>,
    filter: od::Filter,
    text: freq::TextBox,
    /// The status line (`GuiFilter.status`) and the tick it turns back to OK.
    error: Option<(String, u64)>,
}

thread_local! {
    static OD_EDIT: std::cell::RefCell<Option<OdEdit>> = const { std::cell::RefCell::new(None) };
    /// Whether the open screen is the Oredictionificator's (the filter window is laid out differently).
    static FILTER_OD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static OD_SCROLL: std::cell::Cell<([i32; 3], usize)> = const { std::cell::Cell::new(([0; 3], 0)) };
}

fn od_scroll(pos: [i32; 3]) -> usize {
    let (at, scroll) = OD_SCROLL.get();
    if at == pos { scroll } else { 0 }
}

fn od_edit_open() -> bool {
    OD_EDIT.with_borrow(|e| e.is_some())
}

fn od_window_rect(r: [f32; 4]) -> [f32; 4] {
    [OD_WINDOW[0] + r[0], OD_WINDOW[1] + r[1], r[2], r[3]]
}

/// A filter's text under its tag: the item it gives and that item's mod (`FilterButton`).
fn od_result_text(items: &horizon_client::items::Items, f: &od::Filter) -> Option<(String, String)> {
    let result = f.result(&od_tag_items(items, &f.tag))?;
    let namespace = result.split_once(':').map_or("minecraft", |(n, _)| n).to_owned();
    Some((result, namespace))
}

/// The list, its holders and the New Filter button, over the machine's screen.
fn draw_od_list(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    let [ox, oy] = origin;
    let at = |r: [f32; 4]| [ox + r[0], oy + r[1], r[2], r[3]];
    let cursor = game.native_cursor();
    let mouse = [cursor[0] - ox, cursor[1] - oy];
    let (pos, filters) = od_view(game).unwrap_or(([0; 3], vec![]));
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(OD_HOLDER), 0xFFFF_FFFF);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(OD_NEW_HOLDER), 0xFFFF_FFFF);
    nine_slice_tinted(game, batch, "element_holder", [16.0, 16.0], 2.0, at(OD_BAR), 0xFFFF_FFFF);
    draw_mek_button(game, batch, at(OD_NEW), true, inside(mouse, OD_NEW) && !od_edit_open());
    centred(game, batch, "New Filter", ox + OD_NEW[0], OD_NEW[2], oy + OD_NEW[1] + 6.0, 0xFF_FFFF);
    let hidden = filters.len().saturating_sub(OD_ROWS);
    let first = od_scroll(pos).min(hidden);
    let thumb = if hidden > 0 { (first as f32 / hidden as f32 * (OD_BAR[3] - 2.0 - 15.0)).round() } else { 0.0 };
    let bar = if hidden > 0 { "scroll/bar" } else { "scroll/bar_inactive" };
    sprite(game, batch, bar, [0.0, 0.0, 12.0, 15.0], [ox + OD_BAR[0] + 1.0, oy + OD_BAR[1] + 1.0 + thumb, 12.0, 15.0]);
    let items = world_items!(game);
    for (i, f) in filters.iter().skip(first).take(OD_ROWS).enumerate() {
        let row = od_row(i);
        let [rx, ry] = [ox + row[0], oy + row[1]];
        let hovered = inside(mouse, row) && !od_edit_open();
        nine_slice_tinted(game, batch, if hovered { "list/filter/hovered" } else { "list/filter/base" }, [156.0, 29.0], 1.0, [rx, ry, row[2], row[3]], 0xFFFF_FFFF);
        let result = od_result_text(items, f);
        sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [rx + 2.0, ry + 2.0, 18.0, 18.0]);
        // `WarningType.INVALID_OREDICTIONIFICATOR_FILTER` on the slot of an enabled filter that names no usable tag.
        if f.enabled && !f.has_filter() {
            sprite(game, batch, "warning/slot", [0.0, 0.0, 18.0, 18.0], [rx + 2.0, ry + 2.0, 18.0, 18.0]);
        }
        if let Some((item, _)) = &result {
            game.native_item(batch, item, [rx + 3.0, ry + 3.0], 16.0);
        }
        game.native_text(batch, &f.tag, [rx + 19.0, ry + 3.0], 0x40_4040, false);
        if let Some((item, namespace)) = &result {
            game.native_text_scaled(batch, &format!("{} ({namespace})", filter_name(item)), [rx + 19.0, ry + 12.0], 0x40_4040, 0.7);
        }
        let radio = od_radio(i);
        let name = if f.enabled {
            "button/radio/selected"
        } else if inside(mouse, radio) && !od_edit_open() {
            "button/radio/hovered"
        } else {
            "button/radio/base"
        };
        sprite(game, batch, name, [0.0, 0.0, 8.0, 8.0], at(radio));
    }
}

fn od_tooltip(game: &Game<'_>, at: [f32; 2], lines: &mut Vec<String>) -> bool {
    if od_edit_open() {
        let on = |r: [f32; 4]| inside(at, od_window_rect(r));
        if on(OD_LAST) {
            lines.push("Last Item".to_owned());
        } else if on(OD_NEXT) {
            lines.push("Next Item".to_owned());
        }
        return true;
    }
    let Some((pos, filters)) = od_view(game) else {
        return false;
    };
    let first = od_scroll(pos);
    for i in 0..OD_ROWS.min(filters.len().saturating_sub(first)) {
        if inside(at, od_radio(i)) {
            lines.push(if filters[first + i].enabled { "Filter: \u{a7}aenabled".to_owned() } else { "Filter: \u{a7}cdisabled".to_owned() });
            return true;
        }
    }
    false
}

/// A press on the list: a radio button, a row (opens its window), the New Filter button, the scroll bar.
fn od_click(game: &Game<'_>, block: &str, at: [f32; 2]) -> bool {
    let Some((pos, filters)) = od_view(game) else {
        return false;
    };
    // Another window (the sides', the upgrades') is the standard path's.
    if !od_edit_open() && open_window_id(block).is_some() {
        return false;
    }
    if windowed(block, at, |wat| od_window_click(game, pos, wat)) {
        return true;
    }
    if inside(at, OD_NEW) {
        OD_EDIT.with_borrow_mut(|e| {
            *e = Some(OdEdit { index: None, filter: od::Filter::new(""), text: freq::TextBox { focused: true, ..Default::default() }, error: None });
        });
        return true;
    }
    let first = od_scroll(pos);
    let hidden = filters.len().saturating_sub(OD_ROWS);
    if inside(at, OD_BAR) {
        let f = ((at[1] - OD_BAR[1] - 8.0) / (OD_BAR[3] - 16.0)).clamp(0.0, 1.0);
        OD_SCROLL.set((pos, ((hidden as f32 + 0.5) * f) as usize));
        return true;
    }
    for i in 0..OD_ROWS.min(filters.len().saturating_sub(first)) {
        if !inside(at, od_row(i)) {
            continue;
        }
        let index = first + i;
        if inside(at, od_radio(i)) {
            od_send(game, od::Op::Toggle, pos, Some(index), &filters[index]);
        } else {
            let filter = filters[index].clone();
            OD_EDIT.with_borrow_mut(|e| *e = Some(OdEdit { index: Some(index), filter, text: freq::TextBox { focused: true, ..Default::default() }, error: None }));
        }
        return true;
    }
    inside(at, OD_HOLDER)
}

/// The window's text (`GuiOredictionificatorFilter.setText`): the tag typed becomes the filter's, if a
/// filter may name it. Sets the status line when not.
fn od_set_text(game: &Game<'_>, e: &mut OdEdit) -> bool {
    let fail = |e: &mut OdEdit, why: &str| {
        e.error = Some((why.to_owned(), game.ticks as u64 + 100));
        false
    };
    let tag = match od::parse_tag(&e.text.value) {
        Ok(t) => t,
        Err("Tag filter must have a tag") => return fail(e, "No tag"),
        Err(why) if why.starts_with("Invalid path") => return fail(e, "Invalid tag path"),
        Err(_) => return fail(e, "Invalid tag namespace"),
    };
    if e.filter.has_filter() && e.filter.tag == tag {
        fail(e, "Same tag")
    } else if od::valid_target(&tag, world_items!(game).item_tags.contains_key(tag.as_str())) {
        e.filter.tag = tag;
        e.text = freq::TextBox { focused: true, ..Default::default() };
        true
    } else {
        fail(e, "Unsupported tag")
    }
}

/// A press while the window is open: it takes every click.
fn od_window_click(game: &Game<'_>, pos: [i32; 3], at: [f32; 2]) -> bool {
    if !od_edit_open() {
        return false;
    }
    let on = |r: [f32; 4]| inside(at, od_window_rect(r));
    let mut edit = OD_EDIT.with_borrow_mut(|e| e.take());
    let Some(e) = edit.as_mut() else {
        return true;
    };
    e.text.focused = on(OD_TEXT);
    let mut keep = true;
    if on([6.0, 6.0, 8.0, 8.0]) {
        keep = false;
    } else if on(OD_LAST) || on(OD_NEXT) {
        if e.filter.has_filter() {
            let items = od_tag_items(world_items!(game), &e.filter.tag);
            e.filter.step(&items, on(OD_NEXT));
        }
    } else if on(OD_DELETE) {
        if let Some(i) = e.index {
            od_send(game, od::Op::Delete, pos, Some(i), &e.filter);
        }
        keep = false;
    } else if on(OD_SAVE) {
        keep = !od_save(game, pos, e);
    }
    if keep {
        OD_EDIT.with_borrow_mut(|slot| *slot = edit);
    }
    true
}

/// Save (`GuiTextFilter.validateAndSave`): a typed tag is set first; the filter must name a tag.
fn od_save(game: &Game<'_>, pos: [i32; 3], e: &mut OdEdit) -> bool {
    if !e.text.value.is_empty() && !od_set_text(game, e) {
        return false;
    }
    if !e.filter.has_filter() {
        e.error = Some(("No tag".to_owned(), game.ticks as u64 + 100));
        return false;
    }
    od_send(game, od::Op::Save, pos, e.index, &e.filter);
    true
}

/// The window's drawing (hook point `native_screen_overlay`).
fn draw_od_window(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    OD_EDIT.with_borrow(|e| {
        let Some(e) = e.as_ref() else {
            return;
        };
        let [ox, oy] = origin;
        let cursor = game.native_cursor();
        let at = [cursor[0] - ox, cursor[1] - oy];
        let screen = |r: [f32; 4]| {
            let [x, y, w, h] = od_window_rect(r);
            [ox + x, oy + y, w, h]
        };
        let hover = |r: [f32; 4]| inside(at, od_window_rect(r));
        let [x, y, w, h] = screen([0.0, 0.0, OD_WINDOW[2], OD_WINDOW[3]]);
        nine_slice_tinted(game, batch, "shadow", [9.0, 9.0], 3.0, [x - 3.0, y - 3.0, w + 6.0, h + 6.0], 0xBFFF_FFFF);
        nine_slice_tinted(game, batch, "base", [16.0, 16.0], 4.0, [x, y, w, h], 0xFFFF_FFFF);
        draw_button(game, batch, screen([6.0, 6.0, 8.0, 8.0]), hover([6.0, 6.0, 8.0, 8.0]), true, 0xFFFF_FFFF, Some(("button/close", 12.0)));
        let title = format!("{}: Oredictionificator Filter", if e.index.is_none() { "New" } else { "Edit" });
        centred(game, batch, &title, x, w, y + 6.0, 0x40_4040);
        sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [x + 7.0, y + 18.0, 18.0, 18.0]);
        let items = world_items!(game);
        let result = e.filter.has_filter().then(|| od_result_text(items, &e.filter)).flatten();
        if let Some((item, _)) = &result {
            game.native_item(batch, item, [x + 8.0, y + 19.0], 16.0);
        }
        let s = screen(OD_SCREEN);
        nine_slice_tinted(game, batch, "inner_screen", [256.0, 256.0], 6.0, s, 0xFFFF_FFFF);
        let status = match &e.error {
            Some((why, until)) if game.ticks as u64 <= *until => format!("Status: \u{a7}4{why}"),
            _ => "Status: \u{a7}2All OK".to_owned(),
        };
        game.native_text(batch, &status, [s[0] + 3.0, s[1] + 3.0], 0x3C_FE9A, false);
        if e.filter.has_filter() {
            if let Some((item, namespace)) = &result {
                game.native_text(batch, &format!("{} ({namespace})", filter_name(item)), [s[0] + 3.0, s[1] + 14.0], 0x3C_FE9A, false);
            }
            game.native_text(batch, &e.filter.tag, [s[0] + 3.0, s[1] + 25.0], 0x3C_FE9A, false);
        }
        let b = screen(OD_TEXT);
        game.native_fill(batch, [b[0] - 1.0, b[1] - 1.0, b[2] + 2.0, b[3] + 2.0], if e.text.focused { 0xFFFF_FFFF } else { 0xFFA0_A0A0 });
        game.native_fill(batch, b, 0xFF00_0000);
        game.native_text(batch, &e.text.value, [b[0] + 2.0, b[1] + 2.0], 0xE0_E0E0, false);
        if e.text.focused && (game.ticks / 6) % 2 == 0 {
            let before: String = e.text.value.chars().take(e.text.cursor).collect();
            game.native_text(batch, "_", [b[0] + 2.0 + game.native_text_width(&before), b[1] + 2.0], 0xE0_E0E0, false);
        }
        draw_button(game, batch, screen(OD_LAST), hover(OD_LAST), true, 0xFFFF_FFFF, Some(("button/left", 12.0)));
        draw_button(game, batch, screen(OD_NEXT), hover(OD_NEXT), true, 0xFFFF_FFFF, Some(("button/right", 12.0)));
        for (r, label) in [(OD_DELETE, if e.index.is_some() { "Delete" } else { "Cancel" }), (OD_SAVE, "Save")] {
            let b = screen(r);
            draw_button(game, batch, b, hover(r), true, 0xFFFF_FFFF, None);
            centred(game, batch, label, b[0], b[2], b[1] + 6.0, 0xFF_FFFF);
        }
    });
}

fn od_key(game: &Game<'_>, key: &str, text: Option<&str>) {
    let mut edit = OD_EDIT.with_borrow_mut(|e| e.take());
    if let Some(e) = edit.as_mut()
        && e.text.focused
    {
        // The text is forced to lower case (`getInputTransformer`) and limited to resource locations.
        let lower = text.map(str::to_lowercase);
        let typed = e.text.key(key, lower.as_deref(), |c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./:".contains(c), 100);
        if typed == freq::Typed::Enter {
            od_set_text(game, e);
        }
    }
    OD_EDIT.with_borrow_mut(|slot| *slot = edit);
}
