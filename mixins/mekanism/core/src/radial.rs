//! The radial selector (`GuiRadialSelector`): the slices a Meka-Tool offers, what each one sets, and the
//! geometry of the ring (which slice the cursor is over, which pixels a slice covers).

/// One slice of a radial.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slice {
    pub name: &'static str,
    /// Sprite under `radial/`.
    pub icon: &'static str,
    /// `EnumColor` as 0xRRGGBB.
    pub color: u32,
    /// For a nested slice: the module whose options it opens. For a mode: the value it sets.
    pub value: u32,
}

/// A root slice: a module that handles the radial and the options it opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nested {
    pub module: &'static str,
    /// The setting the options write (`module_config` key).
    pub key: &'static str,
    pub slice: Slice,
}

const WHITE: u32 = 0xFFFFFF;
const PINK: u32 = 0xD55ECB;
const GREEN: u32 = 0x75FF89;
const YELLOW: u32 = 0xFFDD4F;
const ORANGE: u32 = 0xFFA160;
const RED: u32 = 0xFF383C;
const AQUA: u32 = 0x30FFF9;
const DARK_BLUE: u32 = 0x366BD0;

const fn s(name: &'static str, icon: &'static str, color: u32, value: u32) -> Slice {
    Slice { name, icon, color, value }
}

const SPEED: [Slice; 6] = [
    s("Off", "speed_off", WHITE, 0),
    s("Slow", "speed_slow", PINK, 1),
    s("Normal", "speed_normal", GREEN, 2),
    s("Fast", "speed_fast", YELLOW, 3),
    s("Super Fast", "speed_super", ORANGE, 4),
    s("Extreme", "speed_extreme", RED, 5),
];

const BLAST: [Slice; 5] = [
    s("Off", "blasting_off", WHITE, 0),
    s("Low", "blasting_low", GREEN, 1),
    s("Medium", "blasting_med", YELLOW, 2),
    s("High", "blasting_high", ORANGE, 3),
    s("Extreme", "blasting_extreme", RED, 4),
];

const VEIN: [Slice; 2] = [s("Vein", "vein_normal", AQUA, 0), s("Extended Vein", "vein_extended", PINK, 1)];

pub const EXCAVATION: &str = "mekanism:excavation_escalation_unit";
pub const BLASTING: &str = "mekanism:blasting_unit";
pub const VEIN_MINING: &str = "mekanism:vein_mining_unit";
/// The Atomic Disassembler's own radial (`IRadialModeItem`): its modes, standing in for a module of the same id.
pub const DISASSEMBLER: &str = "mekanism:atomic_disassembler";

/// `DisassemblerMode` as radial slices (`value` is the index in `gear::DisassemblerMode::ALL`).
const DISASSEMBLER_MODES: [Slice; 4] = [
    s("Normal", "speed_normal", GREEN, 0),
    s("Slow", "speed_slow", PINK, 1),
    s("Fast", "speed_extreme", RED, 2),
    s("Off", "speed_off", WHITE, 3),
];

/// The slices a module offers at the root (`addRadialModes`), if it handles the radial.
pub fn nested(module: &str) -> Option<Nested> {
    Some(match module {
        EXCAVATION => Nested { module: EXCAVATION, key: "excavation_mode", slice: s("Excavation Speed", "speed_normal", YELLOW, 0) },
        BLASTING => Nested { module: BLASTING, key: "blast_radius", slice: s("Blasting Power", "blasting_low", DARK_BLUE, 0) },
        VEIN_MINING => Nested { module: VEIN_MINING, key: "extended_mode", slice: s("Vein Mining", "vein_normal", AQUA, 0) },
        DISASSEMBLER => Nested { module: DISASSEMBLER, key: "mode", slice: s("Disassembler", "speed_normal", GREEN, 0) },
        _ => return None,
    })
}

/// A module's options with `installed` of it (`dataForTruncated`: the enum values up to what is accessible).
pub fn modes(module: &str, installed: u32) -> Vec<Slice> {
    match module {
        EXCAVATION => SPEED[..SPEED.len().min(installed as usize + 2)].to_vec(),
        BLASTING => BLAST[..BLAST.len().min(installed as usize + 1)].to_vec(),
        VEIN_MINING => VEIN.to_vec(),
        DISASSEMBLER => DISASSEMBLER_MODES.to_vec(),
        _ => vec![],
    }
}

/// Which slice of `count` the cursor offset `(dx, dy)` is over (y down, slice 0 on top, clockwise), or None
/// inside the hole (`select_radius`).
pub fn slice_at(dx: f32, dy: f32, count: usize, select_radius: f32) -> Option<usize> {
    if count == 0 || dx.hypot(dy) <= select_radius {
        return None;
    }
    let angle = dy.atan2(dx).to_degrees();
    let mode = 180.0 / count as f32;
    let wrapped = (angle + mode + 90.0).rem_euclid(360.0);
    Some(((wrapped * (count as f32 / 360.0)) as usize).min(count - 1))
}

/// Start angle of slice `i` (degrees; 0 = right, clockwise on screen).
pub fn start_angle(i: usize, count: usize) -> f32 {
    -90.0 + 360.0 * (-0.5 + i as f32) / count as f32
}

/// Where the icon of slice `i` sits, relative to the centre (`MIDDLE_DISTANCE` out).
pub fn icon_centre(i: usize, count: usize, inner: f32, outer: f32) -> [f32; 2] {
    let a = (270.0 + 360.0 * i as f32 / count as f32).to_radians();
    let d = (inner + outer) / 2.0;
    [a.cos() * d, a.sin() * d]
}

/// Whether a point at offset `(dx, dy)` lies in the ring section from `start` over `size` degrees.
pub fn in_section(dx: f32, dy: f32, start: f32, size: f32, inner: f32, outer: f32) -> bool {
    let r = dx.hypot(dy);
    if r < inner || r > outer {
        return false;
    }
    if size >= 360.0 {
        return true;
    }
    let a = (dy.atan2(dx).to_degrees() - start).rem_euclid(360.0);
    a <= size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_run_clockwise_from_the_top() {
        assert_eq!(slice_at(0.0, -60.0, 4, 10.0), Some(0));
        assert_eq!(slice_at(60.0, 0.0, 4, 10.0), Some(1));
        assert_eq!(slice_at(0.0, 60.0, 4, 10.0), Some(2));
        assert_eq!(slice_at(-60.0, 0.0, 4, 10.0), Some(3));
        assert_eq!(slice_at(5.0, 5.0, 4, 10.0), None);
    }

    #[test]
    fn modes_are_truncated_by_installed_count() {
        assert_eq!(modes(EXCAVATION, 1).len(), 3);
        assert_eq!(modes(EXCAVATION, 4).len(), 6);
        assert_eq!(modes(BLASTING, 2).len(), 3);
        assert_eq!(modes(VEIN_MINING, 1).len(), 2);
    }

    #[test]
    fn a_section_covers_its_slice() {
        // Slice 0 of 4 spans -135..-45 degrees: straight up is in it, straight right is not.
        let st = start_angle(0, 4);
        assert!(in_section(0.0, -70.0, st, 90.0, 40.0, 100.0));
        assert!(!in_section(70.0, 0.0, st, 90.0, 40.0, 100.0));
        assert!(!in_section(0.0, -20.0, st, 90.0, 40.0, 100.0));
    }
}
