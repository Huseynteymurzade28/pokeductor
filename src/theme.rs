//! Soft pastel color theme (Catppuccin Mocha) plus per-type accent colors.
//!
//! Centralizing colors here keeps the rendering code readable and makes it
//! trivial to swap palettes later.

use ratatui::style::Color;

// --- Catppuccin Mocha base palette ---------------------------------------
/// Raw components of [`BASE`], used when alpha-blending sprite pixels onto the
/// panel background.
pub const BASE_RGB: (u8, u8, u8) = (30, 30, 46);
pub const BASE: Color = Color::Rgb(BASE_RGB.0, BASE_RGB.1, BASE_RGB.2);
pub const SURFACE: Color = Color::Rgb(49, 50, 68);
pub const OVERLAY: Color = Color::Rgb(108, 112, 134);
pub const TEXT: Color = Color::Rgb(205, 214, 244);
pub const SUBTEXT: Color = Color::Rgb(166, 173, 200);

pub const MAUVE: Color = Color::Rgb(203, 166, 247);
pub const BLUE: Color = Color::Rgb(137, 180, 250);
pub const SAPPHIRE: Color = Color::Rgb(116, 199, 236);
pub const GREEN: Color = Color::Rgb(166, 227, 161);
pub const YELLOW: Color = Color::Rgb(249, 226, 175);
pub const PEACH: Color = Color::Rgb(250, 179, 135);
pub const RED: Color = Color::Rgb(243, 139, 168);
pub const TEAL: Color = Color::Rgb(148, 226, 213);
pub const PINK: Color = Color::Rgb(245, 194, 231);
pub const LAVENDER: Color = Color::Rgb(180, 190, 254);

/// Color used to draw a stat bar, scaled by how high the value is so that
/// weak stats read "cool" and strong stats read "warm".
pub fn stat_color(base: u16) -> Color {
    match base {
        0..=49 => RED,
        50..=89 => PEACH,
        90..=119 => YELLOW,
        120..=149 => GREEN,
        _ => TEAL,
    }
}

/// Accent color for a Pokemon type slug (e.g. `"fire"`).
pub fn type_color(type_name: &str) -> Color {
    match type_name {
        "normal" => Color::Rgb(186, 188, 200),
        "fire" => PEACH,
        "water" => BLUE,
        "electric" => YELLOW,
        "grass" => GREEN,
        "ice" => SAPPHIRE,
        "fighting" => RED,
        "poison" => MAUVE,
        "ground" => Color::Rgb(229, 192, 123),
        "flying" => LAVENDER,
        "psychic" => PINK,
        "bug" => Color::Rgb(166, 209, 137),
        "rock" => Color::Rgb(180, 162, 122),
        "ghost" => Color::Rgb(150, 130, 200),
        "dragon" => Color::Rgb(120, 130, 240),
        "dark" => OVERLAY,
        "steel" => Color::Rgb(160, 178, 196),
        "fairy" => PINK,
        _ => SUBTEXT,
    }
}
