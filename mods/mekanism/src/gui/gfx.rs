//! The old runtime drew through `game.native_*(batch, ..)`; these keep those names over the upstream
//! `DrawContext` and `ClientHost`, so the screens move across with their bodies unchanged.
use mod_api::client::hud_layout::PoseOp;
use mod_api::{ClientHost, DrawContext, GuiDraw, ItemStack, Text};
use std::cell::RefCell;
use std::borrow::Cow;

/// Where the old `HudBatch` went: the draw context lives in [`Gfx`], so this is only a marker.
pub struct Batch;

/// One call of a screen hook: the host, and the draw context when the hook draws.
pub struct Gfx<'a> {
    pub host: &'a dyn ClientHost,
    /// The block of the open screen.
    pub container: Option<(i32, i32, i32)>,
    draw: RefCell<Option<&'a mut dyn DrawContext>>,
}

/// Colours of the old API had no alpha byte; the new one hides a transparent text.
fn opaque(argb: u32) -> u32 {
    if argb >> 24 == 0 { argb | 0xFF00_0000 } else { argb }
}

impl<'a> Gfx<'a> {
    pub fn drawing(host: &'a dyn ClientHost, draw: &'a mut dyn DrawContext) -> Self {
        Self { host, container: host.screen_pos().map(|p| (p[0], p[1], p[2])), draw: RefCell::new(Some(draw)) }
    }

    pub fn reading(host: &'a dyn ClientHost) -> Self {
        Self { host, container: host.screen_pos().map(|p| (p[0], p[1], p[2])), draw: RefCell::new(None) }
    }

    fn with<R>(&self, f: impl FnOnce(&mut dyn DrawContext) -> R) -> Option<R> {
        self.draw.borrow_mut().as_mut().map(|d| f(&mut **d))
    }

    pub fn native_fill(&self, _: &mut Batch, rect: [f32; 4], argb: u32) {
        self.with(|d| d.fill(rect, argb));
    }

    pub fn native_sprite(&self, _: &mut Batch, name: &str, src: [f32; 4], dst: [f32; 4]) -> bool {
        self.with(|d| d.sprite(name, src, dst, None)).unwrap_or(false)
    }

    pub fn native_sprite_tinted(&self, _: &mut Batch, name: &str, src: [f32; 4], dst: [f32; 4], argb: u32) -> bool {
        self.with(|d| d.sprite(name, src, dst, Some(argb))).unwrap_or(false)
    }

    pub fn native_text(&self, _: &mut Batch, text: &str, at: [f32; 2], rgb: u32, shadow: bool) {
        self.with(|d| d.text(text, at, opaque(rgb), shadow));
    }

    /// `scale` times the normal size, its top left at `at`.
    pub fn native_text_scaled(&self, _: &mut Batch, text: &str, at: [f32; 2], rgb: u32, scale: f32) {
        self.with(|d| {
            let _ = d.gui_paint(&[
                GuiDraw::Push(vec![PoseOp::Translate(at[0], at[1]), PoseOp::Scale(scale, scale)]),
                GuiDraw::Text { text: Text::literal(text), x: 0, y: 0, color: opaque(rgb), shadow: false },
                GuiDraw::Pop,
            ]);
        });
    }

    pub fn native_text_width(&self, text: &str) -> f32 {
        self.with(|d| d.text_width(text)).unwrap_or(0.0)
    }

    pub fn native_item_stack(&self, _: &mut Batch, stack: &ItemStack, at: [f32; 2], size: f32) -> bool {
        self.with(|d| d.item_stack(stack, at, size)).unwrap_or(false)
    }

    pub fn native_cursor(&self) -> [f32; 2] {
        self.host.cursor()
    }

    pub fn native_on(&self, mod_id: &str) -> bool {
        self.host.on(mod_id)
    }

    pub fn native_screen_title(&self) -> Option<String> {
        self.host.screen_title()
    }
}

#[allow(dead_code)]
fn _unused(_: Cow<'static, str>) {}
