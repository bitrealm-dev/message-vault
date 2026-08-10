//! Fastmail-style four-seed theme: mode + presets, derived palette for Slint.

use message_vault_io_core::AppearanceSection;
use slint::{Brush, Color, ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{AppWindow, AppearanceAdapter, Theme};

/// Appearance mode. `System` follows the OS color scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

impl ThemeMode {
    pub const ALL: [Self; 3] = [Self::Light, Self::Dark, Self::System];

    pub fn as_ini(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::System => "System",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            "system" => Some(Self::System),
            _ => None,
        }
    }

    pub fn index(self) -> i32 {
        match self {
            Self::Light => 0,
            Self::Dark => 1,
            Self::System => 2,
        }
    }

    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Self::Light,
            2 => Self::System,
            _ => Self::Dark,
        }
    }
}

/// Resolved light/dark after applying system preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedTheme {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeSeeds {
    pub light_header: Rgb,
    pub light_accent: Rgb,
    pub dark_header: Rgb,
    pub dark_accent: Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePreset {
    pub id: &'static str,
    pub label: &'static str,
    pub seeds: ThemeSeeds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xff) as u8,
            g: ((hex >> 8) & 0xff) as u8,
            b: (hex & 0xff) as u8,
        }
    }

    pub fn color(self) -> Color {
        Color::from_rgb_u8(self.r, self.g, self.b)
    }

    pub fn brush(self) -> Brush {
        Brush::SolidColor(self.color())
    }

    pub fn with_alpha(self, a: u8) -> Brush {
        Brush::SolidColor(Color::from_argb_u8(a, self.r, self.g, self.b))
    }
}

/// Ocean Depths — theme-factory default.
pub const DEFAULT_SEEDS: ThemeSeeds = ThemeSeeds {
    light_header: Rgb::from_hex(0xf1faee),
    light_accent: Rgb::from_hex(0x2d8b8b),
    dark_header: Rgb::from_hex(0x1a2332),
    dark_accent: Rgb::from_hex(0xa8dadc),
};

pub const DEFAULT_MODE: ThemeMode = ThemeMode::Dark;

pub const THEME_PRESETS: &[ThemePreset] = &[
    ThemePreset {
        id: "ocean-depths",
        label: "Ocean Depths",
        seeds: DEFAULT_SEEDS,
    },
    ThemePreset {
        id: "graphite-blue",
        label: "Graphite Blue",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xe6e9ee),
            light_accent: Rgb::from_hex(0x2b7fff),
            dark_header: Rgb::from_hex(0x222426),
            dark_accent: Rgb::from_hex(0x5ea1ff),
        },
    },
    ThemePreset {
        id: "light",
        label: "Light",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xf0f2f5),
            light_accent: Rgb::from_hex(0x2563eb),
            dark_header: Rgb::from_hex(0x2c3036),
            dark_accent: Rgb::from_hex(0x6ba3ff),
        },
    },
    ThemePreset {
        id: "dark",
        label: "Dark",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xe8eaed),
            light_accent: Rgb::from_hex(0x3b82f6),
            dark_header: Rgb::from_hex(0x141618),
            dark_accent: Rgb::from_hex(0x5ea1ff),
        },
    },
    ThemePreset {
        id: "sunset-boulevard",
        label: "Sunset Boulevard",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xe9c46a),
            light_accent: Rgb::from_hex(0xe76f51),
            dark_header: Rgb::from_hex(0x264653),
            dark_accent: Rgb::from_hex(0xf4a261),
        },
    },
    ThemePreset {
        id: "forest-canopy",
        label: "Forest Canopy",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xfaf9f6),
            light_accent: Rgb::from_hex(0x7d8471),
            dark_header: Rgb::from_hex(0x2d4a2b),
            dark_accent: Rgb::from_hex(0xa4ac86),
        },
    },
    ThemePreset {
        id: "modern-minimalist",
        label: "Modern Minimalist",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xd3d3d3),
            light_accent: Rgb::from_hex(0x708090),
            dark_header: Rgb::from_hex(0x36454f),
            // Light Gray darkened so white sent-text stays readable
            dark_accent: Rgb::from_hex(0x9aa8b5),
        },
    },
    ThemePreset {
        id: "golden-hour",
        label: "Golden Hour",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xd4b896),
            light_accent: Rgb::from_hex(0xc1666b),
            dark_header: Rgb::from_hex(0x4a403a),
            dark_accent: Rgb::from_hex(0xf4a900),
        },
    },
    ThemePreset {
        id: "arctic-frost",
        label: "Arctic Frost",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xfafafa),
            light_accent: Rgb::from_hex(0x4a6fa5),
            dark_header: Rgb::from_hex(0x4a6fa5),
            // Ice Blue darkened toward Steel Blue for sent-text contrast
            dark_accent: Rgb::from_hex(0x5a7fb5),
        },
    },
    ThemePreset {
        id: "desert-rose",
        label: "Desert Rose",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xe8d5c4),
            light_accent: Rgb::from_hex(0xb87d6d),
            dark_header: Rgb::from_hex(0x5d2e46),
            dark_accent: Rgb::from_hex(0xd4a5a5),
        },
    },
    ThemePreset {
        id: "tech-innovation",
        label: "Tech Innovation",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xffffff),
            light_accent: Rgb::from_hex(0x0066ff),
            dark_header: Rgb::from_hex(0x1e1e1e),
            // Neon Cyan mixed toward Electric Blue for sent-text contrast
            dark_accent: Rgb::from_hex(0x0088bb),
        },
    },
    ThemePreset {
        id: "botanical-garden",
        label: "Botanical Garden",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xf5f3ed),
            light_accent: Rgb::from_hex(0xb7472a),
            dark_header: Rgb::from_hex(0x4a7c59),
            dark_accent: Rgb::from_hex(0xf9a620),
        },
    },
    ThemePreset {
        id: "midnight-galaxy",
        label: "Midnight Galaxy",
        seeds: ThemeSeeds {
            light_header: Rgb::from_hex(0xe6e6fa),
            light_accent: Rgb::from_hex(0x4a4e8f),
            dark_header: Rgb::from_hex(0x2b1e3e),
            dark_accent: Rgb::from_hex(0xa490c2),
        },
    },
];

/// CSS `color-mix(in srgb, a percent%, b)` → `percent` of `a` + rest of `b`.
fn mix(a: Rgb, b: Rgb, a_percent: f32) -> Rgb {
    let t = (a_percent / 100.0).clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Rgb {
        r: (a.r as f32 * t + b.r as f32 * inv).round() as u8,
        g: (a.g as f32 * t + b.g as f32 * inv).round() as u8,
        b: (a.b as f32 * t + b.b as f32 * inv).round() as u8,
    }
}

const BLACK: Rgb = Rgb::new(0, 0, 0);
const WHITE: Rgb = Rgb::new(255, 255, 255);

#[derive(Debug, Clone, Copy)]
pub struct DerivedPalette {
    pub is_dark: bool,
    pub header: Rgb,
    pub accent: Rgb,
    pub bg: Rgb,
    pub panel: Rgb,
    pub elevated: Rgb,
    pub border: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    /// Hover overlay alpha (ARGB alpha channel).
    pub hover_alpha: u8,
    pub danger: Rgb,
    pub danger_bg: Rgb,
    pub danger_text: Rgb,
    pub link: Rgb,
    pub sent: Rgb,
    pub received: Rgb,
    pub sent_text: Rgb,
    pub selection: Rgb,
    pub selection_text: Rgb,
    pub tab_bar: Rgb,
    pub tab_inactive: Rgb,
    pub tab_active: Rgb,
    /// Always a dark terminal surface (independent of light/dark UI mode).
    pub log_bg: Rgb,
    /// Always light text for [`log_bg`] readability.
    pub log_text: Rgb,
    pub glyph_bg: Rgb,
    pub glyph_fg: Rgb,
    pub chrome: Rgb,
    pub separator: Rgb,
}

pub fn prefers_dark_scheme() -> bool {
    match dark_light::detect() {
        Ok(dark_light::Mode::Light) => false,
        Ok(dark_light::Mode::Dark) | Ok(dark_light::Mode::Unspecified) | Err(_) => true,
    }
}

pub fn resolve_mode(mode: ThemeMode) -> ResolvedTheme {
    match mode {
        ThemeMode::Light => ResolvedTheme::Light,
        ThemeMode::Dark => ResolvedTheme::Dark,
        ThemeMode::System => {
            if prefers_dark_scheme() {
                ResolvedTheme::Dark
            } else {
                ResolvedTheme::Light
            }
        }
    }
}

pub fn preset_by_id(id: &str) -> &'static ThemePreset {
    THEME_PRESETS
        .iter()
        .find(|p| p.id == id)
        .unwrap_or(&THEME_PRESETS[0])
}

pub fn preset_index(id: &str) -> i32 {
    THEME_PRESETS
        .iter()
        .position(|p| p.id == id)
        .unwrap_or(0) as i32
}

pub fn preset_from_index(index: i32) -> &'static ThemePreset {
    THEME_PRESETS
        .get(index as usize)
        .unwrap_or(&THEME_PRESETS[0])
}

pub fn derive_palette(seeds: ThemeSeeds, resolved: ResolvedTheme) -> DerivedPalette {
    let (header, accent) = match resolved {
        ResolvedTheme::Dark => (seeds.dark_header, seeds.dark_accent),
        ResolvedTheme::Light => (seeds.light_header, seeds.light_accent),
    };
    // Log pane stays terminal-style in every theme (dark bg + light text).
    let log_bg = Rgb::from_hex(0x121416);
    let log_text = Rgb::from_hex(0xe8eaed);

    match resolved {
        ResolvedTheme::Dark => {
            let bg = mix(header, BLACK, 55.0);
            let panel = mix(header, BLACK, 78.0);
            let elevated = mix(header, WHITE, 82.0);
            let border = mix(header, WHITE, 65.0);
            let text = mix(WHITE, header, 92.0);
            let muted = mix(WHITE, header, 55.0);
            let received = mix(header, WHITE, 70.0);
            let danger = Rgb::from_hex(0xf87171);
            DerivedPalette {
                is_dark: true,
                header,
                accent,
                bg,
                panel,
                elevated,
                border,
                text,
                muted,
                hover_alpha: 31, // ~12% white
                danger,
                danger_bg: mix(danger, header, 28.0),
                danger_text: mix(danger, WHITE, 70.0),
                link: accent,
                sent: accent,
                received,
                sent_text: WHITE,
                selection: accent,
                selection_text: WHITE,
                tab_bar: panel,
                tab_inactive: mix(header, BLACK, 90.0),
                tab_active: elevated,
                log_bg,
                log_text,
                glyph_bg: mix(accent, header, 22.0),
                glyph_fg: mix(accent, WHITE, 55.0),
                chrome: header,
                separator: border,
            }
        }
        ResolvedTheme::Light => {
            let bg = mix(header, WHITE, 45.0);
            let panel = WHITE;
            let elevated = mix(header, WHITE, 35.0);
            let border = mix(header, BLACK, 55.0);
            let text = mix(BLACK, header, 88.0);
            let muted = mix(BLACK, header, 45.0);
            let received = mix(header, WHITE, 55.0);
            let danger = Rgb::from_hex(0xdc2626);
            DerivedPalette {
                is_dark: false,
                header,
                accent,
                bg,
                panel,
                elevated,
                border,
                text,
                muted,
                hover_alpha: 15, // ~6% black
                danger,
                danger_bg: mix(danger, WHITE, 12.0),
                danger_text: mix(danger, BLACK, 45.0),
                link: accent,
                sent: accent,
                received,
                sent_text: WHITE,
                selection: accent,
                selection_text: WHITE,
                tab_bar: mix(header, WHITE, 70.0),
                tab_inactive: mix(header, WHITE, 55.0),
                tab_active: panel,
                log_bg,
                log_text,
                glyph_bg: mix(accent, WHITE, 18.0),
                glyph_fg: mix(accent, BLACK, 55.0),
                chrome: header,
                separator: mix(header, BLACK, 40.0),
            }
        }
    }
}

pub fn mode_options() -> ModelRc<SharedString> {
    let items: Vec<SharedString> = ThemeMode::ALL
        .iter()
        .map(|m| SharedString::from(m.label()))
        .collect();
    ModelRc::new(VecModel::from(items))
}

pub fn preset_options() -> ModelRc<SharedString> {
    let items: Vec<SharedString> = THEME_PRESETS
        .iter()
        .map(|p| SharedString::from(p.label))
        .collect();
    ModelRc::new(VecModel::from(items))
}

pub fn appearance_from_section(section: &AppearanceSection) -> (ThemeMode, &'static ThemePreset) {
    let mode = ThemeMode::parse(&section.mode).unwrap_or(DEFAULT_MODE);
    let preset = preset_by_id(&section.preset);
    (mode, preset)
}

pub fn apply_to_ui(ui: &AppWindow, mode: ThemeMode, preset: &ThemePreset) {
    let resolved = resolve_mode(mode);
    let palette = derive_palette(preset.seeds, resolved);
    let theme = ui.global::<Theme>();

    theme.set_is_dark(palette.is_dark);
    theme.set_header(palette.header.brush());
    theme.set_accent(palette.accent.brush());
    theme.set_bg(palette.bg.brush());
    theme.set_panel(palette.panel.brush());
    theme.set_elevated(palette.elevated.brush());
    theme.set_border(palette.border.brush());
    theme.set_text(palette.text.brush());
    theme.set_muted(palette.muted.brush());
    let hover_rgb = if palette.is_dark { WHITE } else { BLACK };
    theme.set_hover(hover_rgb.with_alpha(palette.hover_alpha));
    theme.set_danger(palette.danger.brush());
    theme.set_danger_bg(palette.danger_bg.brush());
    theme.set_danger_text(palette.danger_text.brush());
    theme.set_link(palette.link.brush());
    theme.set_sent(palette.sent.brush());
    theme.set_received(palette.received.brush());
    theme.set_sent_text(palette.sent_text.brush());
    theme.set_selection(palette.selection.brush());
    theme.set_selection_text(palette.selection_text.brush());
    theme.set_tab_bar(palette.tab_bar.brush());
    theme.set_tab_inactive(palette.tab_inactive.brush());
    theme.set_tab_active(palette.tab_active.brush());
    theme.set_log_bg(palette.log_bg.brush());
    theme.set_log_text(palette.log_text.brush());
    theme.set_glyph_bg(palette.glyph_bg.brush());
    theme.set_glyph_fg(palette.glyph_fg.brush());
    theme.set_chrome(palette.chrome.brush());
    theme.set_separator(palette.separator.brush());

    let appearance = ui.global::<AppearanceAdapter>();
    appearance.set_mode_index(mode.index());
    appearance.set_preset_index(preset_index(preset.id));
}

pub fn push_option_models(ui: &AppWindow) {
    let appearance = ui.global::<AppearanceAdapter>();
    appearance.set_mode_options(mode_options());
    appearance.set_preset_options(preset_options());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocean_depths_dark_bg_is_darker_than_header() {
        let p = derive_palette(DEFAULT_SEEDS, ResolvedTheme::Dark);
        let header_luma = p.header.r as u16 + p.header.g as u16 + p.header.b as u16;
        let bg_luma = p.bg.r as u16 + p.bg.g as u16 + p.bg.b as u16;
        assert!(bg_luma < header_luma);
        assert!(p.is_dark);
    }

    #[test]
    fn light_panel_is_white() {
        let p = derive_palette(DEFAULT_SEEDS, ResolvedTheme::Light);
        assert_eq!(p.panel, WHITE);
        assert!(!p.is_dark);
    }

    #[test]
    fn log_pane_stays_dark_with_light_text_in_light_theme() {
        let p = derive_palette(DEFAULT_SEEDS, ResolvedTheme::Light);
        let bg_luma = p.log_bg.r as u16 + p.log_bg.g as u16 + p.log_bg.b as u16;
        let text_luma = p.log_text.r as u16 + p.log_text.g as u16 + p.log_text.b as u16;
        assert!(bg_luma < 80 * 3, "log background must stay dark");
        assert!(text_luma > 180 * 3, "log text must stay light");
        assert!(text_luma > bg_luma);
    }

    #[test]
    fn preset_lookup_falls_back() {
        assert_eq!(preset_by_id("missing").id, "ocean-depths");
        assert_eq!(preset_by_id("midnight-galaxy").id, "midnight-galaxy");
    }
}
