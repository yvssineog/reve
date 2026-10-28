use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::header;
use crate::palette::Palette;

// A frame at 120Hz.
const BUDGET: Duration = Duration::from_micros(8_333);
const SECOND: Duration = Duration::from_secs(1);

// Frame stats on the top bar: frames drawn in the last second, their mean time, and the
// slowest. A frame is timed from the root view's render to the end of its paint. While shown,
// the window draws on every display refresh, so the count reads the rate the display gets
// rather than how often something changed.
#[derive(Default)]
pub struct Hud {
    frames: Rc<RefCell<Frames>>,
}

#[derive(Default)]
struct Frames {
    started: Option<Instant>,
    // Each recent frame's start and duration, oldest first.
    recent: VecDeque<(Instant, Duration)>,
}

impl Hud {
    // Starts timing this frame and shows the stats of the ones before it. Paint it last, so
    // its end mark comes after the rest of the frame.
    pub fn frame<V: 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> impl IntoElement + use<V> {
        let now = Instant::now();
        let mut frames = self.frames.borrow_mut();
        frames.started = Some(now);
        while frames
            .recent
            .front()
            .is_some_and(|&(at, _)| now - at >= SECOND)
        {
            frames.recent.pop_front();
        }
        let count = frames.recent.len();
        let total: Duration = frames.recent.iter().map(|&(_, took)| took).sum();
        let max = frames.recent.iter().map(|&(_, took)| took).max();
        drop(frames);
        window.request_animation_frame();

        let ms = |took: Duration| format!("{:.1}", took.as_secs_f64() * 1e3);
        let frames = self.frames.clone();
        let theme = cx.theme();
        // Left of the header's right-hand buttons.
        h_flex()
            .absolute()
            .top_0()
            .right(px(78.))
            .h(header::HEIGHT)
            .gap_2()
            .whitespace_nowrap()
            .font_family(theme.mono_font_family.clone())
            .text_size(px(11.))
            .text_color(Palette::of(cx).fg_4)
            .map(|hud| match max {
                None => hud.child("idle"),
                Some(max) => hud
                    .child(format!("{count} fps"))
                    .child(format!("{} ms", ms(total / count as u32)))
                    .child(
                        div()
                            .when(max > BUDGET, |slowest| slowest.text_color(theme.warning))
                            .child(format!("{} max", ms(max))),
                    ),
            })
            .child(canvas(
                |_, _, _| {},
                move |_, _, _, _| {
                    let mut frames = frames.borrow_mut();
                    if let Some(started) = frames.started.take() {
                        frames.recent.push_back((started, started.elapsed()));
                    }
                },
            ))
    }
}
