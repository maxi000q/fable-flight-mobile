//! Touch controls for phones and tablets: a virtual stick that grows
//! wherever the left thumb lands (pitch and roll), a rudder slider along
//! the bottom right, a throttle lever on the left edge, and action buttons
//! under the right thumb. A second finger on the upper screen turns the
//! view; a pinch zooms it.
//!
//! The fingers are tracked from the raw touch events (winit's
//! `WindowEvent::Touch`, one stream per finger), so the stick and the
//! rudder work at the same time; egui only paints the controls and senses
//! the buttons, whose hit-testing is done here on the same layout.

use egui::{pos2, vec2, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke};

/// The flight axes and buttons the touch UI is holding.
#[derive(Default)]
pub struct Touch {
    /// -1..1 each; the virtual stick
    pub pitch: f64,
    pub roll: f64,
    /// -1..1, the rudder slider (right is +)
    pub yaw: f64,
    /// 0..1, the throttle lever's setting
    pub throttle: f64,
    /// whether the lever was moved this frame (it sets the power outright)
    pub throttle_set: bool,
    /// the wheel brakes held
    pub brake: bool,
    /// the starter held (a piston engine cranks while it is)
    pub starter: bool,
    /// the look pad, -1..1 each (a second finger on the upper screen)
    pub look_x: f64,
    pub look_y: f64,
    /// pinch zoom: the change in the pinch's width this frame, in points
    pub zoom: f32,
    // -- one-shot requests, latched until the game takes them --
    gear: bool,
    flaps_up: bool,
    flaps_down: bool,
    view: bool,
    panel: bool,
    pause: bool,
    parking: bool,
    // -- the fingers, by what they are holding down (points) --
    /// the id of the finger on the stick, and where it took hold
    stick: Option<(u64, Pos2)>,
    /// where the stick's finger is now
    stick_pos: Pos2,
    rudder: Option<u64>,
    lever: Option<u64>,
    look: Option<(u64, Pos2)>,
    /// a finger on one of the buttons or tabs: which, and its id
    button: Option<(u64, usize)>,
    /// the throttle lever's shown position (it stays where it is let go)
    lever_at: f64,
    /// the last pinch width, while two fingers are down
    pinch: Option<f32>,
    /// where the controls were last laid out (points), for the paint and
    /// the hit-testing
    lay: Option<Layout>,
    /// which buttons and tabs are down (for the paint)
    down: [bool; 9],
}

/// Where everything sits, in points. The hit-testing (in `touch()`) and
/// the drawing (in `paint()`) share it, so a button is where it looks.
struct Layout {
    stick_zone: Rect,
    look_zone: Rect,
    rudder_zone: Rect,
    lever_zone: Rect,
    /// (pause, view, panel, flaps up, flaps down, gear, parking) and the
    /// two tabs (brake, starter)
    btns: [Rect; 9],
    /// one button's radius
    br: f32,
    landscape: bool,
}

impl Layout {
    fn new(scr: Rect, landscape: bool) -> Self {
        let s = if landscape { 1.0 } else { 0.86 };
        let m = 10.0;
        let mut btns = [Rect::NOTHING; 9];
        // Seven round buttons up the right edge, then the brake and starter
        // tabs under the rudder.
        let bs = 54.0 * s;
        let top = scr.top() + 64.0 * s + bs;
        for (i, r) in btns.iter_mut().take(7).enumerate() {
            *r = Rect::from_center_size(pos2(scr.right() - m - bs * 0.5 - 2.0, top + i as f32 * 62.0 * s), vec2(bs, bs));
        }
        let rudder_zone = Rect::from_min_size(pos2(scr.right() - m - 220.0 * s, scr.bottom() - m - 150.0 * s), vec2(220.0 * s, 92.0 * s));
        let tw = (rudder_zone.width() - 8.0) * 0.5;
        btns[7] = Rect::from_min_size(pos2(rudder_zone.left(), rudder_zone.bottom() + 8.0), vec2(tw, 42.0 * s));
        btns[8] = Rect::from_min_size(pos2(rudder_zone.left() + tw + 8.0, rudder_zone.bottom() + 8.0), vec2(tw, 42.0 * s));
        Layout {
            stick_zone: Rect::from_min_max(pos2(scr.left() + 4.0, scr.top() + scr.height() * 0.30), pos2(scr.left() + scr.width() * 0.44, scr.bottom() - 96.0 * s)),
            look_zone: Rect::from_min_max(pos2(scr.left() + scr.width() * 0.36, scr.top() + 4.0), pos2(scr.right() - 70.0 * s, scr.top() + scr.height() * 0.58)),
            rudder_zone,
            lever_zone: Rect::from_min_size(pos2(scr.left() + m, scr.top() + scr.height() * 0.22), vec2(74.0 * s, scr.height() * 0.55)),
            btns,
            br: bs * 0.5,
            landscape,
        }
    }
}

impl Touch {
    /// A touch began, moved or ended: track the fingers against the layout.
    /// `id` is the finger's, `pos` is in points. Returns whether the touch
    /// was used by the controls (the rest of the game should ignore it then).
    pub fn touch(&mut self, id: u64, phase: TouchPhase, pos: Pos2) -> bool {
        let Some(l) = &self.lay else { return false };
        match phase {
            TouchPhase::Start => {
                // What this finger took hold of. The stick and the look pad
                // catch anywhere in their zones; the rest have to be hit.
                if self.stick.is_none() && l.stick_zone.contains(pos) {
                    self.stick = Some((id, pos));
                    self.stick_pos = pos;
                    return true;
                }
                if self.look.is_none() && l.look_zone.contains(pos) {
                    self.look = Some((id, pos));
                    return true;
                }
                if self.rudder.is_none() && l.rudder_zone.contains(pos) {
                    self.rudder = Some(id);
                    return true;
                }
                if self.lever.is_none() && l.lever_zone.contains(pos) {
                    self.lever = Some(id);
                    return true;
                }
                if let Some(i) = l.btns.iter().position(|r| r.contains(pos)) {
                    if self.button.is_none() {
                        self.button = Some((id, i));
                        self.down[i] = true;
                        // The one-shot requests latch here; a held tab (brake,
                        // starter) is read live from `down`.
                        match i {
                            0 => self.pause = true,
                            1 => self.view = true,
                            2 => self.panel = true,
                            3 => self.flaps_up = true,
                            4 => self.flaps_down = true,
                            5 => self.gear = true,
                            6 => self.parking = true,
                            _ => {}
                        }
                        return true;
                    }
                }
                false
            }
            TouchPhase::Move => {
                if let Some((sid, home)) = self.stick {
                    if sid == id {
                        self.stick_pos = pos;
                        let d = (pos - home) / (l.br * 1.2);
                        (self.roll, self.pitch) = (d.x.clamp(-1.0, 1.0) as f64, (-d.y).clamp(-1.0, 1.0) as f64);
                        return true;
                    }
                }
                if let Some((lid, home)) = self.look {
                    if lid == id {
                        let d = (pos - home) / (115.0 * if l.landscape { 1.0 } else { 0.86 });
                        (self.look_x, self.look_y) = (d.x.clamp(-1.0, 1.0) as f64, d.y.clamp(-1.0, 1.0) as f64);
                        return true;
                    }
                }
                if self.rudder == Some(id) {
                    let rc = l.rudder_zone.center();
                    let x = ((pos.x - rc.x) / (l.rudder_zone.width() * 0.42)).clamp(-1.0, 1.0) as f64;
                    // A dead zone at the centre, so it settles there on its own.
                    self.yaw = if x.abs() < 0.14 { 0.0 } else { x - x.signum() * 0.14 };
                    return true;
                }
                if self.lever == Some(id) {
                    let t = 1.0 - ((pos.y - l.lever_zone.top()) / l.lever_zone.height()).clamp(0.0, 1.0) as f64;
                    (self.throttle, self.lever_at, self.throttle_set) = (t, t, true);
                    return true;
                }
                if let Some((bid, i)) = self.button {
                    if bid == id {
                        // A finger sliding off a tab lets go of it.
                        if i >= 7 && !l.btns[i].contains(pos) {
                            self.down[i] = false;
                            self.button = None;
                        }
                        return true;
                    }
                }
                false
            }
            TouchPhase::End => {
                let mut used = false;
                if let Some((sid, _)) = self.stick {
                    if sid == id {
                        self.stick = None;
                        (self.roll, self.pitch) = (0.0, 0.0);
                        used = true;
                    }
                }
                if let Some((lid, _)) = self.look {
                    if lid == id {
                        self.look = None;
                        (self.look_x, self.look_y) = (0.0, 0.0);
                        used = true;
                    }
                }
                if self.rudder == Some(id) {
                    self.rudder = None;
                    self.yaw = 0.0;
                    used = true;
                }
                if self.lever == Some(id) {
                    self.lever = None;
                    self.throttle_set = false;
                    used = true;
                }
                if let Some((bid, i)) = self.button {
                    if bid == id {
                        self.button = None;
                        self.down[i] = false;
                        used = true;
                    }
                }
                used
            }
        }
    }

    /// The pinch, from the two farthest fingers anywhere on the screen
    /// (called once a frame with their positions, in points).
    pub fn pinch(&mut self, touches: &[Pos2]) {
        self.zoom = 0.0;
        if touches.len() < 2 {
            self.pinch = None;
            return;
        }
        let mut far = (touches[0], touches[1]);
        let mut d2 = 0.0;
        for a in touches {
            for b in touches {
                if a.distance_sq(*b) > d2 {
                    d2 = a.distance_sq(*b);
                    far = (*a, *b);
                }
            }
        }
        let w = far.0.distance(far.1);
        if let Some(prev) = self.pinch {
            self.zoom = w - prev;
        }
        self.pinch = Some(w);
    }

    /// Lay the controls out for the screen (called each frame before the
    /// paint; `touch()` uses the same layout in between).
    pub fn layout(&mut self, scr: Rect, landscape: bool) {
        self.lay = Some(Layout::new(scr, landscape));
    }

    /// The controls' own drawing, over the 3D view.
    pub fn paint(&self, p: &Painter) {
        let Some(l) = &self.lay else { return };
        let ink = Color32::from_rgba_unmultiplied(235, 240, 245, 235);
        let dim = Color32::from_rgba_unmultiplied(235, 240, 245, 120);
        let body = Color32::from_rgba_unmultiplied(30, 36, 44, 150);
        let line = Color32::from_rgba_unmultiplied(235, 240, 245, 60);
        let accent = Color32::from_rgb(255, 176, 32);

        // -- the stick: a ring where it took hold, the knob where the finger is --
        if let Some((_, home)) = self.stick {
            let r = l.br * 1.2;
            p.circle_stroke(home, r, Stroke::new(2.5, line));
            p.circle_stroke(home, r * 0.55, Stroke::new(1.5, line));
            p.line_segment([home, self.stick_pos], Stroke::new(2.0, dim));
            p.circle_filled(self.stick_pos, l.br * 0.42, body);
            p.circle_stroke(self.stick_pos, l.br * 0.42, Stroke::new(2.0, ink));
        } else {
            // Idle: a faint hint of where the thumb goes.
            let c = pos2(l.stick_zone.left() + l.br * 1.8, l.stick_zone.bottom() - l.br * 1.8);
            p.circle_stroke(c, l.br * 1.2, Stroke::new(1.5, dim));
            p.circle_filled(c, l.br * 0.2, dim);
        }

        // -- the rudder slider --
        let rc = l.rudder_zone.center();
        let track = Rect::from_center_size(rc, vec2(l.rudder_zone.width() * 0.84, 10.0));
        p.rect_filled(track, CornerRadius::same(5), body);
        let notch = pos2(rc.x, track.top() - 4.0);
        p.line_segment([notch, pos2(rc.x, track.bottom() + 4.0)], Stroke::new(2.0, line));
        let kx = rc.x + self.yaw as f32 * track.width() * 0.5;
        p.circle_filled(pos2(kx, rc.y), 22.0, body);
        p.circle_stroke(pos2(kx, rc.y), 22.0, Stroke::new(2.0, ink));
        p.text(pos2(kx, rc.y), Align2::CENTER_CENTER, if self.yaw > 0.02 { "▶" } else if self.yaw < -0.02 { "◀" } else { "●" }, FontId::proportional(13.0), dim);

        // -- the throttle lever --
        let tr = Rect::from_center_size(pos2(l.lever_zone.center().x + 18.0, l.lever_zone.center().y), vec2(16.0, l.lever_zone.height() * 0.92));
        p.rect_filled(tr, CornerRadius::same(8), body);
        // The scale's marks: full, three quarters, half, a quarter, idle.
        for i in 0..=4 {
            let y = tr.bottom() - (i as f32 / 4.0) * tr.height();
            p.line_segment([pos2(tr.left() - 8.0, y), pos2(tr.left() - 2.0, y)], Stroke::new(1.5, dim));
        }
        let y = tr.bottom() - (self.lever_at as f32) * tr.height();
        let knob = Rect::from_center_size(pos2(tr.center().x, y), vec2(38.0, 22.0));
        p.rect_filled(knob, CornerRadius::same(6), body);
        p.rect_stroke(knob, CornerRadius::same(6), Stroke::new(2.0, if self.throttle_set { accent } else { ink }), egui::StrokeKind::Middle);
        p.text(pos2(tr.center().x, tr.top() - 16.0), Align2::CENTER_CENTER, "THR", FontId::proportional(12.0), dim);

        // -- the round buttons --
        let labels = ["II", "CAM", "MENU", "F+", "F-", "GEAR", "PARK"];
        for (i, r) in l.btns.iter().take(7).enumerate() {
            p.circle_filled(r.center(), l.br, body);
            p.circle_stroke(r.center(), l.br, Stroke::new(2.0, if self.down[i] { accent } else { ink }));
            p.text(r.center(), Align2::CENTER_CENTER, labels[i], FontId::proportional(11.5), ink);
        }

        // -- the brake and starter tabs --
        for (i, label) in ["BRAKE", "START"].iter().enumerate() {
            let r = l.btns[7 + i];
            p.rect_filled(r, CornerRadius::same(8), body);
            p.rect_stroke(r, CornerRadius::same(8), Stroke::new(2.0, if self.down[7 + i] { accent } else { line }), egui::StrokeKind::Middle);
            p.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.0), if self.down[7 + i] { ink } else { dim });
        }
    }

    /// The one-shot requests: each returns whether it was asked for since the
    /// last call, then latches off.
    pub fn take_gear(&mut self) -> bool {
        std::mem::take(&mut self.gear)
    }
    pub fn take_flaps_up(&mut self) -> bool {
        std::mem::take(&mut self.flaps_up)
    }
    pub fn take_flaps_down(&mut self) -> bool {
        std::mem::take(&mut self.flaps_down)
    }
    pub fn take_view(&mut self) -> bool {
        std::mem::take(&mut self.view)
    }
    pub fn take_panel(&mut self) -> bool {
        std::mem::take(&mut self.panel)
    }
    pub fn take_pause(&mut self) -> bool {
        std::mem::take(&mut self.pause)
    }
    pub fn take_parking(&mut self) -> bool {
        std::mem::take(&mut self.parking)
    }

    /// The throttle lever's position, for the HUD to show.
    #[allow(dead_code)]
    pub fn lever(&self) -> f64 {
        self.lever_at
    }
}

/// The part of a touch's life a `touch()` call reports.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TouchPhase {
    Start,
    Move,
    End,
}

impl From<winit::event::TouchPhase> for TouchPhase {
    fn from(p: winit::event::TouchPhase) -> Self {
        match p {
            winit::event::TouchPhase::Started => TouchPhase::Start,
            winit::event::TouchPhase::Moved => TouchPhase::Move,
            winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => TouchPhase::End,
        }
    }
}

use std::sync::atomic::{AtomicBool, Ordering};

/// Touch controls are in use (a phone or tablet): set once at startup on
/// Android, and the menus show bigger widgets.
pub static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Whether touch controls are on.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

#[allow(dead_code)]
pub fn set_active(on: bool) {
    ACTIVE.store(on, Ordering::Relaxed);
}
