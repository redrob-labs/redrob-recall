// SPDX-License-Identifier: Apache-2.0
//! The window's look: the Redrob Canvas tokens (its Pro dark and Pro light palettes), one style set
//! by the shell at start-up and re-applied only when the theme changes, and the fonts built once.
//!
//! The token values are the ones Redrob Canvas draws with, so the two windows sit side by side as
//! one product family; the structure is this app's own (two palettes, no bevels, no tab strips).
//! Fonts: Inter (UI) and JetBrains Mono (paths, numbers) bundled from `assets/fonts` (SIL OFL 1.1,
//! THIRD_PARTY_NOTICES.md), egui's defaults after them for symbols, and the first CJK-capable
//! system font found as the last fallback, read once -- a library is full of Hangul, Han and Kana.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};
use std::sync::Arc;

/// Light or dark: what the window is drawn in this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    Dark,
    Light,
}

/// What the person asked for: follow the system, or one of the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePref {
    pub const ALL: [ThemePref; 3] = [ThemePref::System, ThemePref::Light, ThemePref::Dark];

    /// The English label; translated through `i18n::t`.
    pub fn label(self) -> &'static str {
        match self {
            ThemePref::System => "System",
            ThemePref::Light => "Light",
            ThemePref::Dark => "Dark",
        }
    }

    /// The theme this preference means on a system that reports `system` (`None`: unknown, dark).
    pub fn resolve(self, system: Option<egui::Theme>) -> ThemeKind {
        match self {
            ThemePref::Light => ThemeKind::Light,
            ThemePref::Dark => ThemeKind::Dark,
            ThemePref::System => match system {
                Some(egui::Theme::Light) => ThemeKind::Light,
                _ => ThemeKind::Dark,
            },
        }
    }
}

/// Colour and shape tokens: the Redrob Canvas Pro palette, light or dark.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub kind: ThemeKind,
    /// Window chrome: the top bar, the side panels, the status line.
    pub chrome: Color32,
    /// The central area behind the view.
    pub canvas: Color32,
    /// Cards and lists.
    pub card: Color32,
    pub card_border: Color32,
    /// Inputs and buttons.
    pub field: Color32,
    pub field_border: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_border: Color32,
    pub accent_text: Color32,
    pub separator: Color32,
    pub shadow: Color32,
    pub primary_bg: Color32,
    pub primary_text: Color32,
    pub danger: Color32,
    pub warning: Color32,
    pub success: Color32,
    pub row_selected: Color32,
    pub radius_sm: f32,
    pub radius: f32,
    pub radius_lg: f32,
}

impl Tokens {
    pub fn for_kind(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Dark => Tokens {
                kind,
                chrome: Color32::from_rgb(50, 50, 50),
                canvas: Color32::from_rgb(40, 40, 40),
                card: Color32::from_rgb(50, 50, 50),
                card_border: Color32::from_rgb(30, 30, 30),
                field: Color32::from_rgb(36, 36, 36),
                field_border: Color32::from_rgb(74, 74, 74),
                hover: Color32::from_rgb(66, 66, 66),
                pressed: Color32::from_rgb(78, 78, 78),
                text: Color32::from_rgb(222, 222, 222),
                text_dim: Color32::from_rgb(178, 178, 178),
                text_faint: Color32::from_rgb(128, 128, 128),
                accent: Color32::from_rgb(55, 142, 240),
                accent_border: Color32::from_rgb(55, 142, 240),
                accent_text: Color32::WHITE,
                separator: Color32::from_rgb(30, 30, 30),
                shadow: Color32::from_black_alpha(150),
                primary_bg: Color32::from_rgb(55, 142, 240),
                primary_text: Color32::WHITE,
                danger: Color32::from_rgb(236, 91, 98),
                warning: Color32::from_rgb(232, 176, 70),
                success: Color32::from_rgb(96, 190, 120),
                row_selected: Color32::from_rgb(82, 82, 82),
                radius_sm: 3.0,
                radius: 4.0,
                radius_lg: 6.0,
            },
            ThemeKind::Light => Tokens {
                kind,
                chrome: Color32::from_rgb(240, 240, 240),
                canvas: Color32::from_rgb(214, 214, 214),
                card: Color32::from_rgb(240, 240, 240),
                card_border: Color32::from_rgb(214, 214, 214),
                field: Color32::WHITE,
                field_border: Color32::from_rgb(186, 186, 186),
                hover: Color32::from_rgb(226, 226, 226),
                pressed: Color32::from_rgb(206, 206, 206),
                text: Color32::from_rgb(20, 20, 20),
                text_dim: Color32::from_rgb(64, 64, 64),
                text_faint: Color32::from_rgb(120, 120, 120),
                accent: Color32::from_rgb(16, 98, 196),
                accent_border: Color32::from_rgb(55, 142, 240),
                accent_text: Color32::WHITE,
                separator: Color32::from_rgb(214, 214, 214),
                shadow: Color32::from_black_alpha(50),
                primary_bg: Color32::from_rgb(55, 142, 240),
                primary_text: Color32::WHITE,
                danger: Color32::from_rgb(200, 48, 56),
                warning: Color32::from_rgb(176, 120, 20),
                success: Color32::from_rgb(40, 140, 70),
                row_selected: Color32::from_rgb(204, 204, 204),
                radius_sm: 3.0,
                radius: 4.0,
                radius_lg: 6.0,
            },
        }
    }

    pub fn dark(&self) -> bool {
        self.kind == ThemeKind::Dark
    }

    /// The tokens the shell applied last ([`apply`]); dark when none were.
    pub fn get(ctx: &egui::Context) -> Tokens {
        ctx.data(|d| d.get_temp::<Tokens>(tokens_id()))
            .unwrap_or_else(|| Tokens::for_kind(ThemeKind::Dark))
    }
}

fn tokens_id() -> egui::Id {
    egui::Id::new("redrob-recall-theme")
}

/// The frame every panel draws with: the chrome fill and a 1 px border in the separator colour.
pub fn panel_frame(ui: &egui::Ui) -> egui::Frame {
    let visuals = ui.visuals();
    egui::Frame::NONE
        .fill(visuals.panel_fill)
        .stroke(Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color))
}

/// A card: the card fill, its border, the small radius and an 10 px margin.
pub fn card_frame(ui: &egui::Ui) -> egui::Frame {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.card_border))
        .corner_radius(CornerRadius::same(t.radius as u8))
        .inner_margin(egui::Margin::same(10))
}

/// Apply a theme to the one global style (both of egui's branches) and publish its tokens.
pub fn apply(ctx: &egui::Context, kind: ThemeKind) {
    // Stay on System so the OS keeps reporting appearance changes; the palette below is what is
    // drawn whichever branch egui picks.
    ctx.set_theme(egui::ThemePreference::System);
    let t = Tokens::for_kind(kind);
    ctx.data_mut(|d| d.insert_temp(tokens_id(), t));
    let mut v = if t.dark() {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = t.chrome;
    v.window_fill = t.card;
    v.window_stroke = Stroke::new(1.0, t.card_border);
    v.extreme_bg_color = t.field;
    v.faint_bg_color = t.card;
    v.code_bg_color = t.field;
    v.override_text_color = Some(t.text);
    v.hyperlink_color = t.accent;
    v.warn_fg_color = t.warning;
    v.error_fg_color = t.danger;
    v.window_corner_radius = CornerRadius::same(t.radius_lg as u8);
    v.menu_corner_radius = CornerRadius::same(t.radius as u8);
    v.window_shadow = egui::Shadow {
        offset: [0, 10],
        blur: 32,
        spread: 0,
        color: t.shadow,
    };
    v.popup_shadow = egui::Shadow {
        offset: [0, 6],
        blur: 20,
        spread: 0,
        color: t.shadow,
    };
    v.selection.bg_fill = t.accent;
    v.selection.stroke = Stroke::new(1.0, t.accent_text);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.striped = false;
    let r = CornerRadius::same(t.radius_sm as u8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.card;
    w.noninteractive.weak_bg_fill = t.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.separator);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text_dim);
    w.noninteractive.corner_radius = r;
    for (wv, bg, stroke) in [
        (&mut w.inactive, t.field, t.field_border),
        (&mut w.hovered, t.hover, t.field_border),
        (&mut w.active, t.pressed, t.accent_border),
        (&mut w.open, t.hover, t.field_border),
    ] {
        wv.bg_fill = bg;
        wv.weak_bg_fill = bg;
        wv.bg_stroke = Stroke::new(1.0, stroke);
        wv.fg_stroke = Stroke::new(1.0, t.text);
        wv.corner_radius = r;
        wv.expansion = 0.0;
    }
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(10.5)),
            (TextStyle::Body, FontId::proportional(12.5)),
            (TextStyle::Button, FontId::proportional(12.5)),
            (TextStyle::Heading, semibold(15.0)),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.interact_size = egui::vec2(24.0, 24.0);
        s.spacing.slider_width = 150.0;
        s.spacing.combo_width = 120.0;
        s.spacing.menu_margin = egui::Margin::same(6);
        s.spacing.window_margin = egui::Margin::same(16);
        s.spacing.icon_width = 14.0;
        s.visuals.indent_has_left_vline = false;
        s.interaction.tooltip_delay = TOOLTIP_DELAY;
        let mut scroll = egui::style::ScrollStyle::thin();
        if scroll.floating {
            scroll.interact_handle_opacity = 1.0;
            scroll.interact_background_opacity = 0.4;
            scroll.floating_width = 4.0;
        }
        s.spacing.scroll = scroll;
        s.spacing.tooltip_width = 320.0;
    });
    // egui picks its style branch from the OS appearance; both branches carry the chosen palette.
    let style = ctx.global_style();
    ctx.set_style_of(egui::Theme::Light, Arc::clone(&style));
    ctx.set_style_of(egui::Theme::Dark, style);
}

/// Seconds the pointer rests on a control before its tooltip shows.
pub const TOOLTIP_DELAY: f32 = 0.35;

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

// ---- fonts ------------------------------------------------------------------------------------

const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

/// System fonts with Hangul, Han and Kana, most preferred first per platform.
const CJK_CANDIDATES: &[&str] = &[
    // Linux
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/nanum/NanumGothic.ttf",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    // macOS
    "/System/Library/Fonts/AppleSDGothicNeo.ttc",
    "/System/Library/Fonts/Supplemental/AppleGothic.ttf",
    // Windows
    "C:\\Windows\\Fonts\\malgun.ttf",
    "C:\\Windows\\Fonts\\msyh.ttc",
    "C:\\Windows\\Fonts\\YuGothR.ttc",
];

/// The font set: Inter first for proportional text, JetBrains Mono first for monospace, the named
/// `medium` and `semibold` weights, egui's defaults for the symbols Inter lacks, and the system's
/// CJK font last in every family.
pub fn font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
        name.to_owned()
    };
    let inter = add(&mut fonts, "Inter", INTER_REGULAR);
    let medium = add(&mut fonts, "Inter-Medium", INTER_MEDIUM);
    let semibold = add(&mut fonts, "Inter-SemiBold", INTER_SEMIBOLD);
    let mono = add(&mut fonts, "JetBrainsMono", JETBRAINS_MONO);
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, inter);
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, mono);
    let fallback: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (family, primary) in [("medium", medium), ("semibold", semibold)] {
        let mut stack = vec![primary];
        stack.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(family.into()), stack);
    }
    match CJK_CANDIDATES
        .iter()
        .find_map(|path| std::fs::read(path).ok().map(|bytes| (*path, bytes)))
    {
        Some((path, bytes)) => {
            fonts
                .font_data
                .insert("system-cjk".into(), Arc::new(FontData::from_owned(bytes)));
            for list in fonts.families.values_mut() {
                list.push("system-cjk".into());
            }
            tracing::info!(path, "CJK fallback font installed");
        }
        None => tracing::info!("no CJK system font found; Hangul, Han and Kana draw as boxes"),
    }
    fonts
}

/// Install the fonts once, at start-up (the shell never re-installs them).
pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
}
