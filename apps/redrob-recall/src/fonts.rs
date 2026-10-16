// SPDX-License-Identifier: Apache-2.0
//! egui's default fonts carry no Hangul, Han or Kana, and a library is full of file names and
//! passages in them. Add the first CJK-capable font found on the system as a fallback, once, at
//! start-up. Nothing is bundled: a missing font only means those glyphs draw as boxes, as before.

use std::sync::Arc;

const CANDIDATES: &[&str] = &[
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

pub fn install(ctx: &egui::Context) {
    let Some((path, bytes)) = CANDIDATES
        .iter()
        .find_map(|path| std::fs::read(path).ok().map(|bytes| (*path, bytes)))
    else {
        tracing::info!("no CJK system font found; Hangul, Han and Kana draw as boxes");
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "system-cjk".into(),
        Arc::new(egui::FontData::from_owned(bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(list) = fonts.families.get_mut(&family) {
            list.push("system-cjk".into());
        }
    }
    ctx.set_fonts(fonts);
    tracing::info!(path, "CJK fallback font installed");
}
