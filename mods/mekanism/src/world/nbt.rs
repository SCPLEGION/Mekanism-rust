//! Reading and writing the numbers a block entity keeps in its compound, as `runtime.rs` did.
use crate::gear::data::get;
use mod_api::{Compound, Tag};

pub(crate) fn get_long(nbt: &Compound, key: &str) -> u64 {
    match get(nbt, key) {
        Some(Tag::Long(v)) => (*v).max(0) as u64,
        Some(Tag::Int(v)) => (*v).max(0) as u64,
        _ => 0,
    }
}

pub(crate) fn get_int(nbt: &Compound, key: &str) -> u32 {
    get_long(nbt, key).min(u32::MAX as u64) as u32
}

pub(crate) fn put_long(nbt: &mut Compound, key: &str, v: u64) {
    nbt.insert(key.into(), Tag::Long(v.min(i64::MAX as u64) as i64));
}

pub(crate) fn put_int(nbt: &mut Compound, key: &str, v: u32) {
    nbt.insert(key.into(), Tag::Int(v.min(i32::MAX as u32) as i32));
}

/// A 64-bit value as four 15-bit data slots, low first (data slots are 16 bits).
pub(crate) fn wide(v: u64) -> [i32; 4] {
    let v = v.min((1 << 60) - 1);
    [0, 1, 2, 3].map(|k| ((v >> (15 * k)) & 0x7FFF) as i32)
}
