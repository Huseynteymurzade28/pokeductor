//! Retro color theme — a PICO-8-inspired palette of warm yellows and matte
//! blues, plus per-type accent colors.
//!
//! Centralizing colors here keeps the rendering code readable and makes it
//! trivial to swap palettes later.

use ratatui::style::Color;

// --- PICO-8 retro base palette -------------------------------------------
/// Raw components of [`BASE`], used when alpha-blending sprite pixels onto the
/// panel background. PICO-8 "dark-blue" — a deep, matte navy.
pub const BASE_RGB: (u8, u8, u8) = (29, 43, 83);
pub const BASE: Color = Color::Rgb(BASE_RGB.0, BASE_RGB.1, BASE_RGB.2);
/// A slightly lifted blue for bars and secondary surfaces.
pub const SURFACE: Color = Color::Rgb(41, 54, 111);
/// PICO-8 "indigo" — dim borders and de-emphasised text.
pub const OVERLAY: Color = Color::Rgb(131, 118, 156);
/// PICO-8 "white" — a warm cream, easy on the eyes against the navy.
pub const TEXT: Color = Color::Rgb(255, 241, 232);
/// PICO-8 "light-grey" — muted labels.
pub const SUBTEXT: Color = Color::Rgb(194, 195, 199);

/// Primary accent: PICO-8 "yellow", a warm retro highlight used for titles,
/// focused borders and the list selection bar.
pub const MAUVE: Color = Color::Rgb(255, 236, 39);
pub const BLUE: Color = Color::Rgb(41, 173, 255);
pub const SAPPHIRE: Color = Color::Rgb(41, 173, 255);
pub const GREEN: Color = Color::Rgb(0, 228, 54);
pub const YELLOW: Color = Color::Rgb(255, 236, 39);
/// PICO-8 "orange".
pub const PEACH: Color = Color::Rgb(255, 163, 0);
pub const RED: Color = Color::Rgb(255, 0, 77);
pub const TEAL: Color = Color::Rgb(43, 210, 200);
pub const PINK: Color = Color::Rgb(255, 119, 168);
/// PICO-8 "peach" — a soft warm cream accent (totals, etc.).
pub const LAVENDER: Color = Color::Rgb(255, 204, 170);

/// Color used to draw a stat bar, scaled by how high the value is so that
/// weak stats read "warm/alarming" and strong stats read "cool/calm".
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
        "normal" => Color::Rgb(194, 195, 199),
        "fire" => PEACH,
        "water" => BLUE,
        "electric" => YELLOW,
        "grass" => GREEN,
        "ice" => Color::Rgb(130, 220, 255),
        "fighting" => RED,
        "poison" => Color::Rgb(199, 87, 197),
        "ground" => Color::Rgb(171, 82, 54),
        "flying" => Color::Rgb(160, 200, 255),
        "psychic" => PINK,
        "bug" => Color::Rgb(140, 200, 60),
        "rock" => Color::Rgb(171, 140, 100),
        "ghost" => Color::Rgb(131, 118, 156),
        "dragon" => Color::Rgb(120, 130, 240),
        "dark" => Color::Rgb(130, 120, 140),
        "steel" => Color::Rgb(160, 178, 196),
        "fairy" => PINK,
        _ => SUBTEXT,
    }
}
