// The rêve mark from assets/reve.svg, with the smoke of Paper's GemSmoke shader (the
// "Songe" study) drifting through its body.
//
// GPUI runs no custom shaders, so a background thread computes the smoke into a
// 128-texel texture the GPU upscales; the smoke is soft enough that texels never show.
// Everything else holds still, so it is drawn once per size, off the main thread and
// pixel for pixel: the window background around the body, the details, and the outline
// in the colors its own smoke averages to. A frame is then two sprites.

use std::f32::consts::{FRAC_1_PI, TAU};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::{ActiveTheme as _, oklch};
use gpui_kit::*;
use image::{Frame, RgbaImage};
use resvg::tiny_skia::{
    self, BlendMode, FillRule, FilterQuality, Pixmap, Shader, SpreadMode, Transform,
};
use resvg::usvg::{self, Node, Paint, Tree};

const SVG: &str = include_str!("../assets/reve.svg");

// The SVG's 1039-unit canvas, shifted so the box centers on the dragon's optical
// center: halfway between its bounding-box center (506, 511) and its centroid (558, 524).
const VIEW_X: f32 = 12.;
const VIEW_Y: f32 = -2.;
const VIEW_SIZE: f32 = 1039.;

// GemSmoke as the Songe study sets it.
const SPEED: f32 = 0.22;
const DISTORTION: f32 = 0.5;
const GLOW: f32 = 0.8;
const SIZE: f32 = 0.8;

// Smoke texels per side.
const N: usize = 128;
// Paper solves the shape's depth at 512 px with only 32 relaxation passes, so depth
// rises within a short band of the edge; solving the same way keeps that band.
const DEPTH_N: usize = 512;
const DEPTH_PASSES: usize = 32;
// About 30 frames a second. At 15, the smoke's quickest texels change twice as much a
// frame, up to 22 of 255 levels.
const FRAME: Duration = Duration::from_millis(33);
const REVEAL_SECS: f32 = 1.2;
// How long a resize has to rest before the still image is redrawn at the new size.
const SETTLE: Duration = Duration::from_millis(150);

// The mark and its smoke, sized by the window. It animates only while it is drawn:
// each render schedules the next frame, so a hidden window or an open binary stops it.
pub struct Logo {
    mark: Option<Arc<Mark>>,
    started: Instant,
    // The smoke on screen, and the frame computed to replace it.
    smoke: Option<Arc<RenderImage>>,
    next: Option<RenderImage>,
    // Computes and paces the next frame.
    tick: Option<Task<()>>,
    // The still image on screen and what it was drawn for, and the one being drawn
    // once the size or the background changes.
    still: Option<(Still, Arc<RenderImage>)>,
    drawing: Option<(Still, Task<()>)>,
    revealed: Option<Instant>,
}

// What the still image is drawn for: its side in device pixels, and the window
// background it covers the box with.
#[derive(Clone, Copy, PartialEq)]
struct Still {
    side: u32,
    background: Hsla,
}

impl Logo {
    // Parses the SVG and solves the depth off the main thread, then starts the smoke.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let parse = cx.background_spawn(async { Mark::new() });
        cx.spawn(async move |this, cx| {
            let mark = Arc::new(parse.await);
            this.update(cx, |logo, cx| {
                logo.mark = Some(mark);
                cx.notify();
            })
            .ok();
        })
        .detach();
        // The atlas keeps an image until told otherwise.
        cx.on_release(|logo, cx| {
            let still = logo.still.take().map(|(_, image)| image);
            for image in [logo.smoke.take(), still].into_iter().flatten() {
                cx.drop_image(image, None);
            }
        })
        .detach();
        Self {
            mark: None,
            started: Instant::now(),
            smoke: None,
            next: None,
            tick: None,
            still: None,
            drawing: None,
            revealed: None,
        }
    }

    // Computes the frame after this one in the background, ready a frame from now.
    // With reduced motion, the first frame stays.
    fn animate(&mut self, cx: &mut Context<Self>) {
        let Some(mark) = self.mark.clone() else {
            return;
        };
        let still = cx.reduce_motion();
        if self.tick.is_some() || (still && self.smoke.is_some()) {
            return;
        }
        let due = Instant::now() + FRAME;
        let time = if still {
            0.
        } else {
            (due - self.started).as_secs_f32() * SPEED
        };
        self.tick = Some(cx.spawn(async move |this, cx| {
            let executor = cx.background_executor().clone();
            let smoke = executor.spawn(async move { mark.smoke(time) }).await;
            executor
                .timer(due.saturating_duration_since(Instant::now()))
                .await;
            this.update(cx, |logo, cx| {
                logo.next = Some(smoke);
                logo.tick = None;
                cx.notify();
            })
            .ok();
        }));
    }

    // Draws the still image for `still` off the main thread, unless it is on screen or
    // on its way. A redraw waits for a resize to settle; meanwhile the image on screen
    // stretches to fit.
    fn draw(&mut self, still: Still, cx: &mut Context<Self>) {
        let Some(mark) = self.mark.clone() else {
            return;
        };
        if self
            .still
            .as_ref()
            .is_some_and(|(drawn, _)| *drawn == still)
        {
            self.drawing = None;
            return;
        }
        if self
            .drawing
            .as_ref()
            .is_some_and(|(drawing, _)| *drawing == still)
        {
            return;
        }
        let settle = if self.still.is_some() {
            SETTLE
        } else {
            Duration::ZERO
        };
        let task = cx.spawn(async move |this, cx| {
            let executor = cx.background_executor().clone();
            executor.timer(settle).await;
            let image = executor.spawn(async move { mark.still(still) }).await;
            this.update(cx, |logo, cx| {
                logo.drawing = None;
                if let Some((_, old)) = logo.still.replace((still, Arc::new(image))) {
                    cx.drop_image(old, None);
                }
                cx.notify();
            })
            .ok();
        });
        self.drawing = Some((still, task));
    }
}

impl Render for Logo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let retired = self
            .next
            .take()
            .and_then(|next| self.smoke.replace(Arc::new(next)));
        self.animate(cx);

        // A little over half the window's short side, in whole device pixels so the
        // still image lands on the screen's own.
        let scale = window.scale_factor();
        let viewport = window.viewport_size();
        let short = f32::from(viewport.width).min(f32::from(viewport.height));
        let pixels = ((short * 0.56).clamp(240., 560.) * scale).round();
        let background = cx.theme().background;
        self.draw(
            Still {
                side: pixels as u32,
                background,
            },
            cx,
        );

        let layers = self
            .smoke
            .clone()
            .zip(self.still.as_ref().map(|(_, image)| image.clone()));
        if layers.is_some() && self.revealed.is_none() {
            self.revealed = Some(Instant::now());
        }
        let reveal = match self.revealed {
            Some(at) if !cx.reduce_motion() => {
                ease_out_quint()((at.elapsed().as_secs_f32() / REVEAL_SECS).min(1.))
            }
            _ => 1.,
        };
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                if let Some((smoke, still)) = layers {
                    for image in [smoke, still] {
                        window
                            .paint_image(bounds, bounds, Corners::default(), image, 0, false)
                            .ok();
                    }
                    // Fades in from the background. Fading the sprites instead would turn
                    // the still image translucent and show the smoke's square.
                    if reveal < 1. {
                        window.paint_quad(fill(bounds, background.opacity(1. - reveal)));
                    }
                }
                // Freed once its replacement is in the atlas. Freed first, it would leave
                // its atlas texture empty, and GPUI would release that texture only to
                // allocate a new one for the next frame, every frame.
                if let Some(old) = retired {
                    window.drop_image(old).ok();
                }
            },
        )
        .size(px(pixels / scale))
    }
}

// What the mark needs that never changes: its shapes, its smoke's field, and the
// outline's colors.
struct Mark {
    body: tiny_skia::Path,
    line: tiny_skia::Path,
    // Horn sheen, belly and highlight, at half strength over the smoke.
    details: Vec<(tiny_skia::Path, Shader<'static>)>,
    body_tones: Tones,
    field: Field,
    // The outline's colors texel by texel: its own smoke averaged over a full turn of
    // the shader's clock, so it holds still and keeps the smoke's tint.
    outline: Pixmap,
}

impl Mark {
    fn new() -> Self {
        let tree = Tree::from_str(SVG, &usvg::Options::default()).expect("assets/reve.svg parses");
        let path = |id| match tree.node_by_id(id) {
            Some(Node::Path(path)) => path,
            _ => panic!("assets/reve.svg has no #{id} path"),
        };
        let fill = |id| match path(id).fill().map(|fill| fill.paint()) {
            Some(Paint::Color(color)) => Shader::SolidColor(color_of(*color, 1.)),
            Some(Paint::LinearGradient(gradient)) => {
                let stops = gradient
                    .stops()
                    .iter()
                    .map(|stop| {
                        let color = color_of(stop.color(), stop.opacity().get());
                        tiny_skia::GradientStop::new(stop.offset().get(), color)
                    })
                    .collect();
                tiny_skia::LinearGradient::new(
                    tiny_skia::Point::from_xy(gradient.x1(), gradient.y1()),
                    tiny_skia::Point::from_xy(gradient.x2(), gradient.y2()),
                    stops,
                    SpreadMode::Pad,
                    gradient.transform(),
                )
                .expect("assets/reve.svg's gradients have stops")
            }
            _ => panic!("assets/reve.svg's #{id} has no flat or linear fill"),
        };
        let details = ["horn-sheen", "belly", "highlight"]
            .map(|id| {
                let mut shader = fill(id);
                shader.apply_opacity(0.5);
                (path(id).data().clone(), shader)
            })
            .into();
        let clear = |id| match path(id).fill().map(|fill| fill.paint()) {
            Some(Paint::Color(color)) => {
                [color.red, color.green, color.blue].map(|c| c as f32 / 255.)
            }
            _ => panic!("assets/reve.svg's #{id} isn't a flat color"),
        };

        let body = path("body").data().clone();
        let line = path("line").data().clone();
        let field = Field::new(&[&body, &line]);

        // The Songe palette: the SVG's own fills where no smoke reaches.
        let tone = |l, c, h| {
            let rgb: Rgba = oklch(l, c, h).into();
            [rgb.r, rgb.g, rgb.b]
        };
        let line_tones = Tones {
            clear: clear("line"),
            thin: tone(0.55, 0.165, 279.),
            thick: tone(0.72, 0.12, 302.),
        };
        const TURN: usize = 16;
        let mut average = vec![[0.; 3]; N * N];
        for step in 0..TURN {
            let mixer = field.mixer(TAU * step as f32 / TURN as f32);
            for (sum, mixer) in average.iter_mut().zip(mixer) {
                let color = line_tones.at(mixer);
                for channel in 0..3 {
                    sum[channel] += color[channel] / TURN as f32;
                }
            }
        }
        let mut outline = Pixmap::new(N as u32, N as u32).unwrap();
        for (pixel, color) in outline.pixels_mut().iter_mut().zip(average) {
            let [r, g, b] = color.map(|c| (c * 255. + 0.5) as u8);
            *pixel = tiny_skia::ColorU8::from_rgba(r, g, b, 255).premultiply();
        }

        Self {
            body_tones: Tones {
                clear: clear("belly"),
                thin: tone(0.81, 0.085, 282.),
                thick: tone(0.86, 0.06, 318.),
            },
            body,
            line,
            details,
            field,
            outline,
        }
    }

    // One frame of the body's smoke at shader time `time`, as the BGRA GPUI draws.
    fn smoke(&self, time: f32) -> RenderImage {
        let bgra = self
            .field
            .mixer(time)
            .into_iter()
            .flat_map(|mixer| {
                let [r, g, b] = self.body_tones.at(mixer).map(|c| (c * 255. + 0.5) as u8);
                [b, g, r, 255]
            })
            .collect();
        image(N as u32, bgra)
    }

    // The still image, `side` pixels square, in the straight-alpha BGRA GPUI draws: the
    // background with the body cut out, the details, and the outline over the cut's edge
    // so its antialiasing never shows the smoke's square.
    fn still(&self, Still { side, background }: Still) -> RenderImage {
        let mut pixmap = Pixmap::new(side, side).unwrap();
        let Rgba { r, g, b, a } = background.into();
        pixmap.fill(tiny_skia::Color::from_rgba(r, g, b, a).unwrap());
        let scale = side as f32 / VIEW_SIZE;
        let transform = Transform::from_row(scale, 0., 0., scale, -VIEW_X * scale, -VIEW_Y * scale);
        let cut = tiny_skia::Paint {
            blend_mode: BlendMode::Clear,
            ..paint(Shader::SolidColor(tiny_skia::Color::BLACK))
        };
        pixmap.fill_path(&self.body, &cut, FillRule::Winding, transform, None);
        for (detail, shader) in &self.details {
            pixmap.fill_path(
                detail,
                &paint(shader.clone()),
                FillRule::Winding,
                transform,
                None,
            );
        }
        let texel = VIEW_SIZE / N as f32;
        let outline = tiny_skia::Pattern::new(
            self.outline.as_ref(),
            SpreadMode::Pad,
            FilterQuality::Bilinear,
            1.,
            Transform::from_row(texel, 0., 0., texel, VIEW_X, VIEW_Y),
        );
        pixmap.fill_path(
            &self.line,
            &paint(outline),
            FillRule::Winding,
            transform,
            None,
        );
        let bgra = pixmap
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let color = pixel.demultiply();
                [color.blue(), color.green(), color.red(), color.alpha()]
            })
            .collect();
        image(side, bgra)
    }
}

// GemSmoke's smoke over the texture.
struct Field {
    // The texels worth computing: the silhouette and a 2-texel margin, so the
    // upscaled texture stays smooth under the outline.
    texels: Vec<usize>,
    // Where each texel's smoke starts before it swirls, and how hard it swirls: the
    // distortion times GemSmoke's "roundness", 0 at the silhouette's edge and 1
    // deepest inside.
    start: (Vec<f32>, Vec<f32>),
    swirl: Vec<f32>,
}

impl Field {
    fn new(silhouette: &[&tiny_skia::Path]) -> Self {
        let depth = depth(silhouette);
        let inside = raster(silhouette, N);
        let texels: Vec<usize> = (0..N * N)
            .filter(|&i| {
                let (x, y) = (i % N, i / N);
                (y.saturating_sub(2)..(y + 3).min(N))
                    .any(|y| (x.saturating_sub(2)..(x + 3).min(N)).any(|x| inside[y * N + x] > 0))
            })
            .collect();
        // GemSmoke's smoke coordinates, lifted as the shader lifts them before the swirl.
        let scale = 4. - 3. * SIZE;
        let start = texels
            .iter()
            .map(|&i| {
                let (x, y) = uv(i);
                let (u, v) = (x * scale, y * scale);
                (
                    u,
                    v + DISTORTION * (1. - smoothstep(0.4 * u.hypot(v))) - 0.4 * DISTORTION,
                )
            })
            .unzip();
        let swirl = texels.iter().map(|&i| DISTORTION * depth[i]).collect();
        Self {
            texels,
            start,
            swirl,
        }
    }

    // GemSmoke's mixer, the smoke's amount, at every texel at shader time `time`.
    fn mixer(&self, time: f32) -> Vec<f32> {
        // The swirl goes a pass at a time over every texel: a texel's eight cosines wait
        // on one another, but texels don't, so each pass vectorizes. The shader also
        // damps the swirl where the field stretches across a pixel; at logo sizes that
        // factor stays above 0.98, so it is left out.
        let (mut u, mut v) = self.start.clone();
        for i in 1..5 {
            let i = i as f32;
            for ((u, v), swirl) in u.iter_mut().zip(&v).zip(&self.swirl) {
                *u += swirl / i * cos(time + i * 2.9 * v);
            }
            for ((v, u), swirl) in v.iter_mut().zip(&u).zip(&self.swirl) {
                *v += swirl / i * cos(time + i * 1.5 * u);
            }
        }
        // Its inner shape, doubled.
        let mut mixer = vec![0.; N * N];
        for ((&i, u), v) in self.texels.iter().zip(&u).zip(&v) {
            mixer[i] = 2. * (0.01 + 0.99 * GLOW) * (-1.5 * (u * u + v * v)).exp();
        }
        mixer
    }
}

// GemSmoke's colors: `clear` where no smoke reaches, `thin` where it starts, `thick`
// where it gathers. In sRGB, mixed as the shader mixes them.
struct Tones {
    clear: [f32; 3],
    thin: [f32; 3],
    thick: [f32; 3],
}

impl Tones {
    // GemSmoke's two-color gradient over its clear color, at smoke amount `mixer`.
    fn at(&self, mixer: f32) -> [f32; 3] {
        let smoke = mix(self.thin, self.thick, smoothstep(mixer - 1.));
        mix(self.clear, smoke, smoothstep(mixer))
    }
}

// Cosine to within 0.001, a few times cheaper than libm's: a parabola per half turn,
// then a second one to correct it. Eight-bit color can't show the difference.
fn cos(x: f32) -> f32 {
    let mut t = x * FRAC_1_PI / 2.;
    t -= 0.25 + (t + 0.25).floor();
    t *= 16. * (t.abs() - 0.5);
    t + 0.225 * t * (t.abs() - 1.)
}

// Paper's depth for GemSmoke: Poisson's equation over the silhouette, zero on its
// edge, relaxed as Paper relaxes it and scaled to 0..1, then averaged down to N.
fn depth(silhouette: &[&tiny_skia::Path]) -> Vec<f32> {
    const W: usize = DEPTH_N;
    let inside: Vec<bool> = raster(silhouette, W)
        .into_iter()
        .map(|alpha| alpha > 0)
        .collect();
    let interior: Vec<usize> = (0..W * W)
        .filter(|&i| {
            let (x, y) = (i % W, i / W);
            inside[i]
                && (1..W - 1).contains(&x)
                && (1..W - 1).contains(&y)
                && [
                    i - W - 1,
                    i - W,
                    i - W + 1,
                    i - 1,
                    i + 1,
                    i + W - 1,
                    i + W,
                    i + W + 1,
                ]
                .iter()
                .all(|&n| inside[n])
        })
        .collect();
    let mut u = vec![0f32; W * W];
    for _ in 0..DEPTH_PASSES {
        for parity in [0, 1] {
            for &i in &interior {
                if (i % W + i / W) % 2 != parity {
                    continue;
                }
                let around = u[i - 1] + u[i + 1] + u[i - W] + u[i + W];
                u[i] = 1.9 * (0.01 + around) / 4. - 0.9 * u[i];
            }
        }
    }
    let deepest = interior.iter().map(|&i| u[i]).fold(f32::EPSILON, f32::max);
    let step = W / N;
    let u = &u;
    (0..N * N)
        .map(|i| {
            let (x, y) = (i % N * step, i / N * step);
            let sum: f32 = (y..y + step)
                .flat_map(|y| (x..x + step).map(move |x| u[y * W + x]))
                .sum();
            sum / (step * step) as f32 / deepest
        })
        .collect()
}

// The paths' coverage of an n×n grid over the box, 0..255 per cell.
fn raster(paths: &[&tiny_skia::Path], n: usize) -> Vec<u8> {
    let mut pixmap = Pixmap::new(n as u32, n as u32).unwrap();
    let scale = n as f32 / VIEW_SIZE;
    let transform = Transform::from_row(scale, 0., 0., scale, -VIEW_X * scale, -VIEW_Y * scale);
    for path in paths {
        pixmap.fill_path(
            path,
            &tiny_skia::Paint::default(),
            FillRule::Winding,
            transform,
            None,
        );
    }
    pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect()
}

// Shows the mark as the app's Dock icon. A bundled build would carry it in its .icns.
#[cfg(target_os = "macos")]
pub fn set_dock_icon() {
    use objc2::{AllocAnyThread as _, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    // AppKit draws SVG itself, so the icon stays vector. It caches a bitmap at the
    // image's size, so the size is the Dock's largest, 256 points; at the viewBox's
    // size that cache was 100 MB. Widening the viewBox around the optical center keeps
    // the artwork a tenth in from each edge, as Apple's icon grid does.
    let pad = VIEW_SIZE / 8.;
    let side = VIEW_SIZE + 2. * pad;
    let view_box = format!(
        r#"width="256" height="256" viewBox="{} {} {side} {side}""#,
        VIEW_X - pad,
        VIEW_Y - pad
    );
    let svg = SVG.replacen(r#"viewBox="0 0 1039 1039""#, &view_box, 1);
    debug_assert_ne!(svg, SVG, "assets/reve.svg keeps its viewBox");
    let icon = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(svg.as_bytes()))
        .expect("the icon decodes");
    let app =
        NSApplication::sharedApplication(MainThreadMarker::new().expect("on the main thread"));
    unsafe { app.setApplicationIconImage(Some(&icon)) };
}

fn image(side: u32, bgra: Vec<u8>) -> RenderImage {
    RenderImage::new(vec![Frame::new(
        RgbaImage::from_raw(side, side, bgra).unwrap(),
    )])
}

fn paint(shader: Shader) -> tiny_skia::Paint {
    tiny_skia::Paint {
        shader,
        anti_alias: true,
        ..Default::default()
    }
}

fn color_of(color: usvg::Color, opacity: f32) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(
        color.red,
        color.green,
        color.blue,
        (opacity * 255. + 0.5) as u8,
    )
}

// Texel `i`'s center in object coordinates: -0.5..0.5, y up, as GemSmoke's are.
fn uv(i: usize) -> (f32, f32) {
    let x = (i % N) as f32 + 0.5;
    let y = (i / N) as f32 + 0.5;
    (x / N as f32 - 0.5, 0.5 - y / N as f32)
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

#[cfg(test)]
mod tests {
    use gpui_kit::white;

    use super::{DISTORTION, Mark, N, Still};

    #[test]
    fn the_mark_parses_and_its_depth_peaks_inside() {
        let mark = Mark::new();
        let deepest = mark.field.swirl.iter().copied().fold(0., f32::max) / DISTORTION;
        assert!((0.9..=1.).contains(&deepest), "depth peaks at {deepest}");
        assert!(
            mark.field.texels.len() < N * N / 2,
            "only the silhouette is computed"
        );
    }

    #[test]
    fn the_still_image_covers_the_box_but_the_body() {
        let side = 256;
        let image = Mark::new().still(Still {
            side,
            background: white(),
        });
        let bgra = image.as_bytes(0).unwrap();
        assert_eq!(bgra[..4], [255; 4], "the background fills the corners");
        let open = bgra.chunks(4).filter(|pixel| pixel[3] == 0).count();
        let share = open as f32 / (side * side) as f32;
        assert!(
            (0.1..0.5).contains(&share),
            "{share} of the box shows the smoke"
        );
    }
}
