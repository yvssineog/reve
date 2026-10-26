use gpui_kit::component::{ActiveTheme as _, oklch};
use gpui_kit::*;

// The Electron build's palette. Text sits on contrast steps against the pane: fg 15:1
// primary, fg_3 4.7:1 labels and addresses, fg_4 2.7:1 decoration only. Hues carry one meaning each, in code and in icons alike.
pub struct Palette {
    pub fg: Hsla,
    pub fg_3: Hsla,
    pub fg_4: Hsla,
    pub hairline: Hsla,
    pub cursor: Hsla,
    pub mnemonic: Hsla,
    pub symbol: Hsla,
    pub external: Hsla,
    pub string: Hsla,
    pub number: Hsla,
    pub comment: Hsla,
}

impl Palette {
    // Dark is Electron's, in OKLCH. Light keeps each hue and chroma and lowers only the
    // lightness, so every hue reads at 4.8:1 on white; its ink steps hit the same ratios.
    pub fn of(cx: &App) -> Self {
        let dark = cx.theme().mode.is_dark();
        let (ink, steps) = if dark {
            (white(), [0.9, 0.46, 0.3, 0.07])
        } else {
            (black(), [0.85, 0.55, 0.38, 0.08])
        };
        let hue = |on_dark, on_light, chroma, hue| {
            oklch(if dark { on_dark } else { on_light }, chroma, hue)
        };
        Self {
            fg: ink.opacity(steps[0]),
            fg_3: ink.opacity(steps[1]),
            fg_4: ink.opacity(steps[2]),
            hairline: ink.opacity(steps[3]),
            cursor: oklch(0.682, 0.178, 246.6).opacity(0.2),
            mnemonic: hue(0.768, 0.55, 0.121, 254.1),
            symbol: hue(0.784, 0.56, 0.127, 298.4),
            external: hue(0.805, 0.56, 0.136, 60.),
            string: hue(0.794, 0.535, 0.15, 149.),
            number: hue(0.802, 0.545, 0.108, 224.6),
            comment: hue(0.826, 0.55, 0.098, 86.3),
        }
    }
}
