//! Text helpers of the gear tooltips: `EnumColor` (mekanism.api.text.EnumColor) and `ILangEntry.translateColored`.
use mod_api::Text;

/// `EnumColor` rgb values.
pub(crate) mod color {
    pub const DARK_RED: u32 = 0xC9071F;
    pub const GRAY: u32 = 0xCFCFCF;
    pub const INDIGO: u32 = 0x559EFF;
    pub const BRIGHT_GREEN: u32 = 0x75FF89;
    pub const AQUA: u32 = 0x30FFF9;
}

/// A line colour as the tooltip hook packs it (`rgba` of the game's HUD: red in the low byte, opaque).
pub(crate) fn line_color(rgb: u32) -> u32 {
    0xFF00_0000 | ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

/// `text` in `rgb`.
pub(crate) fn colored(text: impl Into<String>, rgb: u32) -> Text {
    let mut out = Text::literal(text);
    out.style.color = Some(format!("#{:06X}", rgb & 0xFF_FFFF));
    out
}

/// `ILangEntry.translateColored(color, arg_color, args)`: the line in `color`, its arguments in `arg_color`.
pub(crate) fn translate_colored(key: &str, color: u32, arg_color: u32, args: &[&str]) -> (Text, u32) {
    let args = args.iter().map(|a| colored(*a, arg_color)).collect();
    (Text::translate(key, args), line_color(color))
}

/// `ILangEntry.translateColored(color)`: a line with no arguments.
pub(crate) fn line(key: &str, color: u32) -> (Text, u32) {
    (Text::translate(key, Vec::new()), line_color(color))
}
