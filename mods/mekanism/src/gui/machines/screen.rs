//! The generic machine screen (`GuiMekanismTile`): panel, slot frames, gauges, power bar, progress arrow,
//! the energy tab and the side tabs. Trimmed from the fork's `screen_draw_base`, `screen_click_base` and
//! `screen_tooltip_base`: the windows (side config, upgrades), the special screens and the warning
//! machinery are ported in later steps.
use super::*;

/// The `Screen` of a block: the size and slots of its panel, or none for a block with no screen here.
pub(in crate::gui) fn layout_of(block: &str) -> Option<mod_api::Screen> {
    let l = layout(block)?;
    let (w, h) = (screen_width(block), screen_height(block));
    let mut slots: Vec<Option<[f32; 2]>> = l.slots.iter().map(|s| s.map(|(at, _)| at)).collect();
    let extra = extra_slots(block);
    if !extra.is_empty() {
        slots.resize((3 + extra.len()).max(5), None);
        for (i, (at, _)) in extra.iter().enumerate() {
            slots[3 + i] = Some(*at);
        }
    }
    Some(mod_api::Screen {
        width: w,
        height: h,
        slots,
        inventory_y: h - 82.0,
        inventory_x: 8.0 + ((w - 176.0) / 2.0).floor(),
        // The panel is Mekanism's own (`base` nine-slice); the title and "Inventory" are vanilla's.
        own_labels: false,
        own_background: true,
        recipe_book: None,
        recipe_book_toggle: None,
    })
}

pub(in crate::gui) fn draw(game: &Gfx<'_>, block: &str, data: &[i32], origin: [f32; 2]) {
    let batch = &mut Batch;
    let Some(l) = layout(block) else {
        return;
    };
    let [ox, oy] = origin;
    let get = |i: usize| data.get(i).copied().unwrap_or(0).max(0) as f32;
    let width = screen_width(block);
    let inv_dx = ((width - 176.0) / 2.0).floor();
    let height = screen_height(block);
    nine_slice(game, batch, "base", 16.0, 4.0, [ox, oy, width, height]);
    let extra = extra_slots(block);
    let slots = l.slots.iter().filter_map(|s| s.as_ref()).chain(extra.iter());
    for (at, name) in slots {
        sprite(game, batch, name, [0.0, 0.0, 18.0, 18.0], [ox + at[0] - 1.0, oy + at[1] - 1.0, 18.0, 18.0]);
    }
    // The player's inventory and hotbar frames.
    for row in 0..4 {
        let y = if row < 3 { height - 82.0 + 18.0 * row as f32 } else { height - 24.0 };
        for col in 0..9 {
            sprite(game, batch, "slot/normal", [0.0, 0.0, 18.0, 18.0], [ox + 7.0 + inv_dx + 18.0 * col as f32, oy + y - 1.0, 18.0, 18.0]);
        }
    }
    let gauge = |batch: &mut Batch, at: [f32; 2], size: [f32; 2], fill: f32, (texture, argb): (&str, u32)| {
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
            [34.0, _] => "medium",
            _ => "standard",
        };
        sprite(game, batch, &format!("gauge/overlay/{overlay}"), [0.0, 0.0, size[0] - 2.0, size[1] - 2.0], [x + 1.0, y + 1.0, size[0] - 2.0, size[1] - 2.0]);
    };
    for (i, g) in l.gauges.iter().enumerate().filter(|(_, g)| !g.key.starts_with("stats_")) {
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
    let energy_texture = format!("{SPRITES_ROOT}gauge_fluid/energy");
    match l.power {
        Some(p @ (PowerBar::Vertical(_) | PowerBar::Tall(..))) => {
            let [x, y, w, h] = power_rect(p);
            nine_slice(game, batch, "bar/base", 5.0, 1.0, [ox + x, oy + y, w, h]);
            let filled = energy * (h - 2.0);
            if filled > 0.0 {
                sprite(game, batch, "bar/vertical_power", [0.0, 0.0, 2.0, 2.0], [ox + x + 1.0, oy + y + h - 1.0 - filled, w - 2.0, filled]);
            }
        }
        Some(p @ PowerBar::Horizontal(_)) => {
            let [x, y, w, h] = power_rect(p);
            nine_slice(game, batch, "bar/base", 5.0, 1.0, [ox + x, oy + y, w, h]);
            let filled = energy * (w - 2.0);
            if filled > 0.0 {
                sprite(game, batch, "bar/horizontal_power", [0.0, 0.0, 2.0, 2.0], [ox + x + 1.0, oy + y + 1.0, filled, h - 2.0]);
            }
        }
        Some(PowerBar::Gauge(at)) => gauge(batch, at, WIDE, energy, (&energy_texture, 0xFFFF_FFFF)),
        Some(PowerBar::SmallGauge(at)) => gauge(batch, at, SMALL_MED, energy, (&energy_texture, 0xFFFF_FFFF)),
        Some(PowerBar::Sized(at, size)) => gauge(batch, at, size, energy, (&energy_texture, 0xFFFF_FFFF)),
        None => {}
    }
    if let Some((at, name, [w, h])) = l.progress {
        sprite(game, batch, &format!("{name}/empty"), [0.0, 0.0, w, h], [ox + at[0], oy + at[1], w, h]);
        let done = ((get(2) / get(3).max(1.0)).clamp(0.0, 1.0) * w).floor();
        if done > 0.0 {
            sprite(game, batch, &format!("{name}/full"), [0.0, 0.0, done, h], [ox + at[0], oy + at[1], done, h]);
        }
    }
    if l.power.is_some() {
        sprite(game, batch, "tab/energy_info", [0.0, 0.0, 26.0, 26.0], [ox - 26.0, oy + 137.0, 26.0, 26.0]);
    }
    draw_tabs(game, batch, block, &l, data, origin);
}

/// A press at `at` (from the panel's top left): the menu button of the tab under it.
pub(in crate::gui) fn click(block: &str, data: &[i32], at: [f32; 2], right: bool) -> Option<i32> {
    let l = layout(block)?;
    let tab = screen_tabs(block, &l, data).into_iter().find(|t| inside(at, tab_button(t)))?;
    match tab.what {
        TabKind::Redstone => Some(if right { BUTTON_REDSTONE_PREVIOUS } else { BUTTON_REDSTONE_NEXT }),
        TabKind::EditMode => Some(if right { BUTTON_EDIT_PREVIOUS } else { BUTTON_EDIT_NEXT }),
        // The windows come with their own step.
        _ => None,
    }
}

/// The tooltip lines at `at`: a tab, the power bar and the gauges.
pub(in crate::gui) fn tooltip(block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) {
    let Some(l) = layout(block) else {
        return;
    };
    if tab_tooltip(block, &l, data, at, lines) {
        return;
    }
    if l.power.is_some() && inside(at, [-26.0, 137.0, 26.0, 26.0]) {
        let (stored, capacity) = (read_wide(data, DATA_STORED), read_wide(data, DATA_CAPACITY));
        lines.push(format!("Stored Energy: {stored} J"));
        lines.push(format!("Capacity: {capacity} J"));
        return;
    }
    if let Some(p) = l.power {
        if inside(at, power_rect(p)) {
            let (stored, capacity) = (read_wide(data, DATA_STORED), read_wide(data, DATA_CAPACITY));
            lines.push(format!("{stored} J / {capacity} J"));
        }
    }
}
