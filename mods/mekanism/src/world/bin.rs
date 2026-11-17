//! Bins (`TileEntityBin`) and personal chests and barrels. Port of `tick_bin`, `settle_bin` and the containers of
//! `runtime.rs`. The right click on a bin (put in, take out, lock) is a block-use hook of its own.
use super::common::{key_of, max_stack, put_client};
use super::nbt::{get_long, get_int, put_int, put_long};
use crate::gear::data::get_string;
use mekanism_core::bin::{self, Bin};
use mekanism_core::tier::BaseTier;
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent, BlockPart, Compound, Host,
    ItemStack, ModBlockEntityView, ModContainer, Registrar, ServerHost, Tag,
};

/// The id of the block entity type of the bins (`mekanism:bin`).
pub(crate) const TYPE: &str = "bin";
/// The id of the block entity type of personal chests and barrels (`mekanism:personal_storage`).
pub(crate) const PERSONAL_TYPE: &str = "personal_storage";

fn bin_blocks() -> Vec<String> {
    BaseTier::ALL.into_iter().map(|t| format!("mekanism:{}_bin", t.name())).collect()
}

pub(crate) fn register(r: &mut dyn Registrar) {
    let ids = bin_blocks();
    let ids: Vec<&str> = ids.iter().map(String::as_str).filter(|id| bin::tier_of(id).is_some()).collect();
    let entity = r.block_entity_type(TYPE).blocks(&ids).build();
    r.block_entity_part(&entity, BinPart).expect("native block entity type");
    for id in ids {
        let block = r.block_key(id);
        // Hoppers fill its input slot from above and the sides and take from its output slot below; it has no menu
        // (its contents show on a right click).
        r.block_container(
            &block,
            ModContainer {
                slots: 2,
                faces: [vec![1], vec![0], vec![0], vec![0], vec![0], vec![0]],
                inputs: vec![0],
                outputs: vec![1],
                menu: None,
            },
        );
    }
    // Personal storage: six rows that no hopper reaches.
    let personal = [bin::PERSONAL_CHEST, bin::PERSONAL_BARREL];
    let entity = r.block_entity_type(PERSONAL_TYPE).blocks(&personal).build();
    r.block_entity_part(&entity, PersonalPart).expect("native block entity type");
    for id in personal {
        let block = r.block_key(id);
        r.block_container(
            &block,
            ModContainer {
                slots: bin::PERSONAL_SLOTS,
                faces: Default::default(),
                inputs: vec![],
                outputs: vec![],
                menu: Some("minecraft:generic_9x6".into()),
            },
        );
    }
}

struct PersonalPart;

#[mod_api::horizon_part]
impl BlockPart for PersonalPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for PersonalPart {}

struct BinPart;

#[mod_api::horizon_part]
impl BlockPart for BinPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for BinPart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        super::common::client_tag(event.entity)
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let Some(tier) = bin::tier_of(event.entity.id()) else {
            return;
        };
        tick_bin(&*host, &mut event.entity, tier);
    }
}

pub(crate) fn bin_of(nbt: &Compound) -> Bin {
    let count = get_long(nbt, "count");
    Bin { item: get_string(nbt, "item").filter(|_| count > 0), count, lock: get_string(nbt, "lock") }
}

pub(crate) fn put_bin(nbt: &mut Compound, b: &Bin) {
    nbt.insert("item".into(), Tag::String(b.item.as_deref().unwrap_or("").into()));
    put_long(nbt, "count", b.count);
    nbt.insert("lock".into(), Tag::String(b.lock.as_deref().unwrap_or("").into()));
}

/// A stack a bin can hold: no damage and no components (a bin keeps only the item id).
pub(crate) fn plain(stack: &ItemStack) -> bool {
    !stack.is_damaged() && stack.damage_component().is_none() && stack.components().is_none()
}

/// What hoppers left in the output slot goes back into the bin; the bin as the slots and the compound make it.
pub(crate) fn settle_bin(host: &dyn Host, entity: &mut ModBlockEntityView<'_>, tier: BaseTier) -> Bin {
    let mut b = bin_of(&entity.nbt);
    if entity.items.len() < 2 {
        entity.items.resize(2, None);
    }
    if let Some(s) = entity.items[1].take() {
        let id = super::item_id(host, &s);
        if tier == BaseTier::Creative || b.item.as_deref() == Some(id.as_str()) {
            if tier != BaseTier::Creative {
                b.count += u64::from(s.count());
            }
        } else if b.insert(tier, &id, u64::from(s.count()), false) < u64::from(s.count()) {
            entity.items[1] = Some(s);
        }
    }
    b
}

/// A bin's tick: what hoppers put in the input slot goes in, and the output slot shows a stack of it for hoppers below
/// to take (`BinInventorySlot` through the item handler).
fn tick_bin(host: &dyn Host, entity: &mut ModBlockEntityView<'_>, tier: BaseTier) {
    let mut b = settle_bin(host, entity, tier);
    if let Some(s) = entity.items[0].clone().filter(plain) {
        let id = super::item_id(host, &s);
        let took = b.insert(tier, &id, u64::from(s.count()), false) as u32;
        if took > 0 {
            entity.items[0] = s.shrink(took);
        }
    }
    if entity.items[1].is_none()
        && let Some(item) = b.item.clone()
    {
        let max = max_stack(host, &item);
        if let Some((item, n)) = b.extract(tier, u64::from(max)) {
            entity.items[1] = key_of(host, &item).and_then(|k| ItemStack::new(k, n as u32));
        }
    }
    let ticks = get_int(&entity.nbt, "add_ticks");
    put_int(&mut entity.nbt, "add_ticks", ticks.saturating_sub(1));
    put_bin(&mut entity.nbt, &b);
    // What `RenderBin` shows on the front: the item (the locked one when empty), how many, and the lock.
    let mut shown = Compound::new();
    shown.insert("bin_item".into(), Tag::String(b.item_type().unwrap_or_default().into()));
    put_int(&mut shown, "bin_count", b.count.min(u64::from(u32::MAX)) as u32);
    put_int(&mut shown, "bin_lock", u32::from(b.lock.is_some()));
    put_client(entity, &["bin_item", "bin_count", "bin_lock"], shown);
}
