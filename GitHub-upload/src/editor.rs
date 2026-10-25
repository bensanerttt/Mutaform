//! Custom painted egui editor.

use crate::params::{DetectRange, FxKind, Key, MidiMode, MutaformParams, Scale, NUM_FX_SLOTS};
use crate::presets::{self, PRESETS};
use crate::SharedState;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Mesh, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui,
};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub const WIDTH: u32 = 880;
pub const HEIGHT: u32 = 660;

// ---------------------------------------------------------------- palette / themes

pub struct Theme {
    pub name: &'static str,
    pub light: bool,
    pub swatch: Color32,
    bg_top: Color32,
    bg_bottom: Color32,
    panel: Color32,
    panel_edge: Color32,
    knob_dark: Color32,
    knob_light: Color32,
    knob_ring: Color32,
    knob_inner: Color32,
    accent: Color32,
    accent_dim: Color32,
    text: Color32,
    text_dim: Color32,
    warm: Color32,
    hi: Color32,
    field: Color32,
    sub_panel: Color32,
    sub_edge: Color32,
    btn_on: Color32,
    btn_off: Color32,
    texture: Color32,
    header: Color32,
}

const fn ch_scale(v: u8, k: f32) -> u8 {
    let x = v as f32 * k;
    if x > 255.0 {
        255
    } else {
        x as u8
    }
}

const fn ch_mix(x: u8, y: u8, t: f32) -> u8 {
    (x as f32 + (y as f32 - x as f32) * t) as u8
}

const fn sc(c: [u8; 3], k: f32) -> Color32 {
    Color32::from_rgb(ch_scale(c[0], k), ch_scale(c[1], k), ch_scale(c[2], k))
}

const fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> Color32 {
    Color32::from_rgb(ch_mix(a[0], b[0], t), ch_mix(a[1], b[1], t), ch_mix(a[2], b[2], t))
}

/// Dark theme derived from a mid "body" colour and an accent.
const fn dark(name: &'static str, m: [u8; 3], accent: [u8; 3], warm: [u8; 3]) -> Theme {
    const W: [u8; 3] = [255, 255, 255];
    Theme {
        name,
        light: false,
        swatch: Color32::from_rgb(accent[0], accent[1], accent[2]),
        bg_top: sc(m, 0.72),
        bg_bottom: sc(m, 0.32),
        panel: sc(m, 0.4),
        panel_edge: sc(m, 1.2),
        knob_dark: sc(m, 0.28),
        knob_light: sc(m, 1.0),
        knob_ring: sc(m, 1.35),
        knob_inner: Color32::from_rgba_premultiplied(
            (m[0] as f32 * 0.19 * 0.78) as u8,
            (m[1] as f32 * 0.19 * 0.78) as u8,
            (m[2] as f32 * 0.19 * 0.78) as u8,
            200,
        ),
        accent: Color32::from_rgb(accent[0], accent[1], accent[2]),
        accent_dim: mix([(m[0] as f32 * 0.8) as u8, (m[1] as f32 * 0.8) as u8, (m[2] as f32 * 0.8) as u8], accent, 0.12),
        text: mix(m, W, 0.85),
        text_dim: mix(m, W, 0.45),
        warm: Color32::from_rgb(warm[0], warm[1], warm[2]),
        hi: Color32::WHITE,
        field: sc(m, 0.2),
        sub_panel: sc(m, 0.52),
        sub_edge: sc(m, 0.88),
        btn_on: mix([(m[0] as f32 * 0.7) as u8, (m[1] as f32 * 0.7) as u8, (m[2] as f32 * 0.7) as u8], accent, 0.3),
        btn_off: sc(m, 0.43),
        texture: Color32::from_rgba_premultiplied(4, 4, 4, 4),
        header: Color32::from_rgba_premultiplied(0, 0, 0, 60),
    }
}

pub const THEMES: [Theme; 7] = [
    // Blue (default) – hand tuned.
    Theme {
        name: "Blue",
        light: false,
        swatch: Color32::from_rgb(110, 200, 255),
        bg_top: Color32::from_rgb(38, 50, 84),
        bg_bottom: Color32::from_rgb(16, 22, 42),
        panel: Color32::from_rgb(20, 28, 52),
        panel_edge: Color32::from_rgb(64, 82, 128),
        knob_dark: Color32::from_rgb(14, 22, 46),
        knob_light: Color32::from_rgb(52, 72, 122),
        knob_ring: Color32::from_rgb(70, 90, 140),
        knob_inner: Color32::from_rgba_premultiplied(8, 12, 26, 200),
        accent: Color32::from_rgb(110, 200, 255),
        accent_dim: Color32::from_rgb(44, 70, 112),
        text: Color32::from_rgb(214, 226, 246),
        text_dim: Color32::from_rgb(130, 148, 186),
        warm: Color32::from_rgb(255, 140, 120),
        hi: Color32::WHITE,
        field: Color32::from_rgb(10, 16, 34),
        sub_panel: Color32::from_rgb(26, 36, 66),
        sub_edge: Color32::from_rgb(48, 64, 104),
        btn_on: Color32::from_rgb(36, 74, 128),
        btn_off: Color32::from_rgb(22, 30, 56),
        texture: Color32::from_rgba_premultiplied(4, 4, 4, 4),
        header: Color32::from_rgba_premultiplied(0, 0, 0, 60),
    },
    dark("Green", [38, 96, 66], [110, 240, 160], [255, 150, 120]),
    dark("Purple", [80, 56, 128], [205, 150, 255], [255, 150, 140]),
    dark("Yellow", [102, 86, 38], [255, 214, 90], [255, 140, 110]),
    dark("Red", [116, 42, 48], [255, 112, 112], [255, 205, 140]),
    dark("Dark", [72, 72, 78], [215, 215, 222], [255, 150, 120]),
    // White / light.
    Theme {
        name: "White",
        light: true,
        swatch: Color32::from_rgb(245, 245, 248),
        bg_top: Color32::from_rgb(238, 241, 246),
        bg_bottom: Color32::from_rgb(212, 218, 228),
        panel: Color32::from_rgb(247, 248, 251),
        panel_edge: Color32::from_rgb(178, 186, 202),
        knob_dark: Color32::from_rgb(196, 203, 216),
        knob_light: Color32::from_rgb(253, 254, 255),
        knob_ring: Color32::from_rgb(166, 175, 194),
        knob_inner: Color32::from_rgba_premultiplied(206, 209, 214, 220),
        accent: Color32::from_rgb(28, 108, 220),
        accent_dim: Color32::from_rgb(194, 204, 222),
        text: Color32::from_rgb(36, 42, 58),
        text_dim: Color32::from_rgb(112, 120, 138),
        warm: Color32::from_rgb(214, 84, 64),
        hi: Color32::from_rgb(10, 14, 24),
        field: Color32::from_rgb(255, 255, 255),
        sub_panel: Color32::from_rgb(234, 237, 243),
        sub_edge: Color32::from_rgb(196, 203, 216),
        btn_on: Color32::from_rgb(198, 220, 252),
        btn_off: Color32::from_rgb(234, 238, 244),
        texture: Color32::from_rgba_premultiplied(0, 0, 0, 5),
        header: Color32::from_rgba_premultiplied(0, 0, 0, 10),
    },
];

thread_local! {
    static CURRENT_THEME: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn shadow(alpha: u8) -> Color32 {
    Color32::from_black_alpha(if th().light { alpha / 3 } else { alpha })
}

fn apply_visuals(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = if th().light { egui::Visuals::light() } else { egui::Visuals::dark() };
    style.visuals.window_fill = th().panel;
    style.visuals.panel_fill = th().bg_bottom;
    style.visuals.selection.bg_fill = th().btn_on;
    ctx.set_style(style);
}

/// The theme of the editor currently being drawn.
fn th() -> &'static Theme {
    &THEMES[CURRENT_THEME.with(|c| c.get()).min(THEMES.len() - 1)]
}

struct UiState {
    last_kinds: [FxKind; NUM_FX_SLOTS],
    applied_theme: usize,
    user_presets: Option<Vec<presets::UserPreset>>,
    save_name: String,
    save_focus: bool,
    /// Short status message and seconds left to show it.
    status: Option<(String, f32)>,
}

pub fn create(params: Arc<MutaformParams>, shared: Arc<SharedState>) -> Option<Box<dyn Editor>> {
    let egui_state: Arc<EguiState> = params.editor_state.clone();
    create_egui_editor(
        egui_state,
        UiState {
            last_kinds: [FxKind::Compressor, FxKind::Eq, FxKind::DeEsser, FxKind::Chorus, FxKind::Delay, FxKind::Reverb],
            applied_theme: usize::MAX,
            user_presets: None,
            save_name: String::new(),
            save_focus: false,
            status: None,
        },
        |_, _| {},
        move |ctx, setter, state| {
            let theme = params.theme.load(Ordering::Relaxed).min(THEMES.len() - 1);
            CURRENT_THEME.with(|c| c.set(theme));
            if state.applied_theme != theme {
                apply_visuals(ctx);
                state.applied_theme = theme;
            }
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                draw(ui, &params, &shared, setter, state);
            });
            ctx.request_repaint();
        },
    )
}

// ---------------------------------------------------------------- helpers

fn vertical_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    painter.add(Shape::mesh(mesh));
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn arc_points(center: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let steps = (((a1 - a0).abs() * r / 4.0) as usize).clamp(2, 128);
    (0..=steps)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / steps as f32;
            pos2(center.x + r * a.cos(), center.y - r * a.sin())
        })
        .collect()
}

fn spaced_text(painter: &egui::Painter, center: Pos2, text: &str, size: f32, spacing: f32, color: Color32) {
    let font = FontId::proportional(size);
    let widths: Vec<f32> = text
        .chars()
        .map(|c| painter.layout_no_wrap(c.to_string(), font.clone(), color).size().x)
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + spacing * (widths.len().saturating_sub(1)) as f32;
    let mut x = center.x - total / 2.0;
    for (c, w) in text.chars().zip(widths) {
        painter.text(pos2(x, center.y), Align2::LEFT_CENTER, c, font.clone(), color);
        x += w + spacing;
    }
}

fn panel(painter: &egui::Painter, rect: Rect, radius: u8) {
    painter.rect_filled(rect.translate(vec2(0.0, 3.0)), CornerRadius::same(radius), shadow(70));
    painter.rect_filled(rect, CornerRadius::same(radius), th().panel);
    painter.rect_stroke(rect, CornerRadius::same(radius), Stroke::new(1.0_f32, th().panel_edge), StrokeKind::Inside);
}

/// Shared drag logic for any float parameter. `pixels` = drag distance for the full range.
fn drag_param(ui: &Ui, resp: &Response, param: &FloatParam, setter: &ParamSetter, pixels: f32, horizontal: bool) {
    if resp.drag_started() {
        setter.begin_set_parameter(param);
    }
    if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) { 0.15 } else { 1.0 };
        let d = resp.drag_delta();
        let delta = if horizontal { d.x } else { -d.y };
        let n = (param.unmodulated_normalized_value() + delta / pixels * fine).clamp(0.0, 1.0);
        setter.set_parameter_normalized(param, n);
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if resp.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, param.default_plain_value());
        setter.end_set_parameter(param);
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let n = (param.unmodulated_normalized_value() + scroll / 2000.0).clamp(0.0, 1.0);
            setter.begin_set_parameter(param);
            setter.set_parameter_normalized(param, n);
            setter.end_set_parameter(param);
        }
    }
}

fn set_now<P: Param>(setter: &ParamSetter, param: &P, v: P::Plain) {
    setter.begin_set_parameter(param);
    setter.set_parameter(param, v);
    setter.end_set_parameter(param);
}


// ---------------------------------------------------------------- typed value entry

fn entry_id(ui: &Ui, param: &FloatParam) -> egui::Id {
    ui.id().with(("entry", param.name()))
}

/// Right-click (or Ctrl+click) on a control opens a text box for typing an exact value.
/// Returns true while the box is open.
fn entry_begin(ui: &mut Ui, resp: &Response, param: &FloatParam) -> bool {
    let id = entry_id(ui, param);
    let open_now = resp.secondary_clicked() || (resp.clicked() && ui.input(|i| i.modifiers.command));
    if open_now {
        let text = param.normalized_value_to_string(param.unmodulated_normalized_value(), false);
        ui.data_mut(|d| {
            d.insert_temp(id, text.trim().to_string());
            d.insert_temp(id.with("focus"), true);
        });
    }
    ui.data(|d| d.get_temp::<String>(id)).is_some()
}

fn entry_show(ui: &mut Ui, rect: Rect, param: &FloatParam, setter: &ParamSetter) {
    let id = entry_id(ui, param);
    let Some(mut text) = ui.data(|d| d.get_temp::<String>(id)) else {
        return;
    };
    let unit = param.unit().trim().to_string();
    let edit = ui.put(
        rect,
        egui::TextEdit::singleline(&mut text)
            .font(FontId::monospace(13.0))
            .horizontal_align(egui::Align::Center)
            .text_color(th().hi)
            .background_color(th().field),
    );
    if ui.data(|d| d.get_temp::<bool>(id.with("focus")).unwrap_or(false)) {
        edit.request_focus();
        ui.data_mut(|d| d.remove::<bool>(id.with("focus")));
    }
    if !unit.is_empty() {
        ui.painter().text(
            rect.right_center() + vec2(4.0, 0.0),
            Align2::LEFT_CENTER,
            unit,
            FontId::proportional(11.0),
            th().text_dim,
        );
    }
    let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if edit.lost_focus() || cancel {
        if !cancel {
            let cleaned = text.trim().replace(',', ".");
            if let Some(n) = param.string_to_normalized_value(&cleaned) {
                setter.begin_set_parameter(param);
                setter.set_parameter_normalized(param, n.clamp(0.0, 1.0));
                setter.end_set_parameter(param);
            }
        }
        ui.data_mut(|d| d.remove::<String>(id));
    } else {
        ui.data_mut(|d| d.insert_temp(id, text));
    }
}

// ---------------------------------------------------------------- widgets

#[allow(clippy::too_many_arguments)]
fn knob(
    ui: &mut Ui,
    center: Pos2,
    radius: f32,
    param: &FloatParam,
    setter: &ParamSetter,
    label: &str,
    value_text: String,
    enabled: bool,
) {
    let rect = Rect::from_center_size(center, vec2(radius * 2.0, radius * 2.0));
    let resp = ui.interact(rect, ui.id().with(param.name()), Sense::click_and_drag());
    drag_param(ui, &resp, param, setter, 220.0, false);
    let editing = entry_begin(ui, &resp, param);
    let painter = ui.painter();

    // Housing: soft shadow, outer ring, and a "domed" body built from stacked circles.
    painter.circle_filled(center + vec2(0.0, radius * 0.08), radius * 1.06, shadow(90));
    painter.circle_filled(center, radius * 1.04, th().knob_ring);
    painter.circle_filled(center, radius, th().knob_dark);
    let layers = 14;
    for i in 0..layers {
        let t = i as f32 / layers as f32;
        let r = radius * (0.96 - t * 0.5);
        let c = lerp_color(th().knob_dark, th().knob_light, t * 0.9);
        painter.circle_filled(center - vec2(radius * 0.18 * t, radius * 0.22 * t), r, c);
    }
    painter.circle_filled(center, radius * 0.62, th().knob_inner);

    // Arc track + value arc (270 degree sweep, starting bottom-left).
    let start = 225f32.to_radians();
    let sweep = 270f32.to_radians();
    let track_r = radius * 0.78;
    let norm = param.unmodulated_normalized_value();
    let bipolar = param.preview_plain(0.0) < 0.0 && param.preview_plain(1.0) > 0.0;
    let zero = if bipolar { param.preview_normalized(0.0) } else { 0.0 };
    painter.add(Shape::line(arc_points(center, track_r, start, start - sweep), Stroke::new(3.0_f32, th().accent_dim)));
    let col = if enabled { th().accent } else { th().text_dim };
    let (a0, a1) = (start - sweep * zero, start - sweep * norm);
    if (a0 - a1).abs() > 0.005 {
        painter.add(Shape::line(arc_points(center, track_r, a0, a1), Stroke::new(3.5_f32, col)));
    }
    // Tick dots around the rim.
    for i in 0..=8 {
        let a = start - sweep * i as f32 / 8.0;
        let p = pos2(center.x + radius * 0.9 * a.cos(), center.y - radius * 0.9 * a.sin());
        painter.circle_filled(p, 1.4, th().text_dim);
    }
    // Pointer dot.
    let pa = start - sweep * norm;
    let pp = pos2(center.x + track_r * pa.cos(), center.y - track_r * pa.sin());
    painter.circle_filled(pp, 4.0, if enabled { th().hi } else { th().text_dim });

    let mut vsize = (radius * 0.3).clamp(11.0, 19.0);
    let chars = value_text.chars().count() as f32;
    if radius < 20.0 && chars > 3.0 {
        vsize *= 3.4 / chars;
    }
    let vcol = if resp.hovered() || resp.dragged() { th().hi } else if enabled { th().accent } else { th().text_dim };
    if !editing {
        painter.text(center, Align2::CENTER_CENTER, value_text, FontId::monospace(vsize), vcol);
    }
    if radius < 20.0 {
        spaced_text(painter, pos2(center.x, center.y + radius + 13.0), label, 9.0, 0.6, th().text);
    } else {
        spaced_text(painter, pos2(center.x, center.y + radius + 16.0), label, 12.0, 1.5, th().text);
    }
    if editing {
        let w = (radius * 1.3).max(52.0);
        entry_show(ui, Rect::from_center_size(center, vec2(w, 22.0)), param, setter);
    }
}

fn toggle_button(ui: &mut Ui, rect: Rect, text: &str, active: bool) -> bool {
    let resp = ui.interact(rect, ui.id().with(("btn", text, rect.min.x as i32, rect.min.y as i32)), Sense::click());
    let painter = ui.painter();
    let fill = if active { th().btn_on } else { th().btn_off };
    painter.rect_filled(rect, CornerRadius::same(5), fill);
    let edge = if active { th().accent } else if resp.hovered() { th().text_dim } else { th().panel_edge };
    painter.rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.0),
        if active { th().hi } else { th().text_dim },
    );
    resp.clicked()
}

/// Horizontal slider drawn as a field of slanted lines that light up.
fn line_slider(ui: &mut Ui, rect: Rect, param: &FloatParam, setter: &ParamSetter, label: &str) {
    let resp = ui.interact(rect, ui.id().with(param.name()), Sense::click_and_drag());
    let ctrl = ui.input(|i| i.modifiers.command);
    if (resp.clicked() && !ctrl) || resp.drag_started() {
        if let Some(p) = resp.interact_pointer_pos() {
            let n = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            setter.begin_set_parameter(param);
            setter.set_parameter_normalized(param, n);
            if resp.clicked() {
                setter.end_set_parameter(param);
            }
        }
    }
    if resp.dragged() {
        if let Some(p) = resp.interact_pointer_pos() {
            let n = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            setter.set_parameter_normalized(param, n);
        }
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if resp.double_clicked() {
        set_now(setter, param, param.default_plain_value());
    }
    let editing = entry_begin(ui, &resp, param);
    let painter = ui.painter();
    let norm = param.unmodulated_normalized_value();
    let n = 26;
    for i in 0..n {
        let t = (i as f32 + 0.5) / n as f32;
        let x = rect.left() + t * rect.width();
        // Lines grow taller towards the right, like a ramp.
        let h = rect.height() * (0.35 + 0.65 * t);
        let lit = t <= norm;
        let col = if lit { lerp_color(th().accent, th().hi, t * 0.3) } else { th().accent_dim };
        painter.line_segment(
            [pos2(x - 3.0, rect.bottom()), pos2(x + 3.0, rect.bottom() - h)],
            Stroke::new(if lit { 2.2_f32 } else { 1.6 }, col),
        );
    }
    let col = if resp.hovered() { th().hi } else { th().text };
    painter.text(
        pos2(rect.left() - 12.0, rect.center().y + 4.0),
        Align2::RIGHT_CENTER,
        label,
        FontId::proportional(12.0),
        col,
    );
    if editing {
        let r = Rect::from_min_size(pos2(rect.right() + 6.0, rect.center().y - 7.0), vec2(36.0, 22.0));
        entry_show(ui, r, param, setter);
    } else {
        painter.text(
            pos2(rect.right() + 10.0, rect.center().y + 4.0),
            Align2::LEFT_CENTER,
            format!("{:.0}%", norm * 100.0),
            FontId::monospace(11.0),
            th().accent,
        );
    }
}

fn vertical_meter_slider(ui: &mut Ui, rect: Rect, param: &FloatParam, setter: &ParamSetter) {
    let resp = ui.interact(rect, ui.id().with(param.name()), Sense::click_and_drag());
    let ctrl = ui.input(|i| i.modifiers.command);
    let set_from = |p: Pos2| ((rect.bottom() - p.y) / rect.height()).clamp(0.0, 1.0);
    if resp.drag_started() || (resp.clicked() && !ctrl) {
        setter.begin_set_parameter(param);
        if let Some(p) = resp.interact_pointer_pos() {
            setter.set_parameter_normalized(param, set_from(p));
        }
        if resp.clicked() {
            setter.end_set_parameter(param);
        }
    }
    if resp.dragged() {
        if let Some(p) = resp.interact_pointer_pos() {
            setter.set_parameter_normalized(param, set_from(p));
        }
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if resp.double_clicked() {
        set_now(setter, param, param.default_plain_value());
    }
    let editing = entry_begin(ui, &resp, param);
    let painter = ui.painter();
    painter.rect_filled(rect.expand(4.0), CornerRadius::same(6), th().field);
    painter.rect_stroke(rect.expand(4.0), CornerRadius::same(6), Stroke::new(1.0_f32, th().panel_edge), StrokeKind::Inside);
    let norm = param.unmodulated_normalized_value();
    let segs = 24;
    let gap = 2.0;
    let sh = (rect.height() - gap * (segs - 1) as f32) / segs as f32;
    for i in 0..segs {
        let t = (i as f32 + 0.5) / segs as f32;
        let y = rect.bottom() - (i as f32) * (sh + gap) - sh;
        let r = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), sh));
        painter.rect_filled(r, CornerRadius::same(1), if t <= norm { th().accent } else { th().accent_dim });
    }
    let text_pos = rect.center_bottom() + vec2(0.0, 20.0);
    if editing {
        entry_show(ui, Rect::from_center_size(text_pos, vec2(48.0, 22.0)), param, setter);
    } else {
        ui.painter().text(
            text_pos,
            Align2::CENTER_CENTER,
            format!("{:.0}%", param.value() * 100.0),
            FontId::monospace(14.0),
            th().accent,
        );
    }
}

fn dropdown_box(ui: &mut Ui, rect: Rect, text: &str, active: bool, id: &str) -> Response {
    let resp = ui.interact(rect, ui.id().with(id), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(5), th().field);
    let edge = if resp.hovered() { th().text_dim } else { th().panel_edge };
    painter.rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    let col = if resp.hovered() { th().hi } else if active { th().accent } else { th().text_dim };
    painter.text(rect.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, text, FontId::proportional(12.0), col);
    let ac = rect.right_center() - vec2(10.0, -1.0);
    painter.add(Shape::convex_polygon(
        vec![ac + vec2(-4.0, -2.5), ac + vec2(4.0, -2.5), ac + vec2(0.0, 2.5)],
        col,
        Stroke::NONE,
    ));
    resp
}

fn power_icon(painter: &egui::Painter, c: Pos2, on: bool) {
    let col = if on { th().accent } else { th().text_dim };
    painter.add(Shape::line(arc_points(c, 6.0, 60f32.to_radians(), -240f32.to_radians()), Stroke::new(1.6_f32, col)));
    painter.line_segment([c + vec2(0.0, -8.0), c + vec2(0.0, -1.0)], Stroke::new(1.6_f32, col));
}

fn note_name(hz: f32) -> String {
    if hz <= 0.0 {
        return "--".into();
    }
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let midi = 69.0 + 12.0 * (hz / 440.0).log2();
    let m = midi.round() as i32;
    let cents = ((midi - m as f32) * 100.0).round() as i32;
    format!("{}{} {:+}c", NAMES[m.rem_euclid(12) as usize], m.div_euclid(12) - 1, cents)
}

// ---------------------------------------------------------------- main layout

fn draw(ui: &mut Ui, params: &MutaformParams, shared: &SharedState, setter: &ParamSetter, state: &mut UiState) {
    let full = ui.max_rect();
    let o = full.min.to_vec2();
    let at = |x: f32, y: f32| pos2(x, y) + o;
    let rect = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(pos2(x, y) + o, vec2(w, h));

    {
        let painter = ui.painter();
        vertical_gradient(painter, full, th().bg_top, th().bg_bottom);
        // Subtle diagonal texture.
        let mut x = -full.height();
        while x < full.width() {
            painter.line_segment(
                [at(x, full.height()), at(x + full.height(), 0.0)],
                Stroke::new(1.0_f32, th().texture),
            );
            x += 9.0;
        }

        // Header bar.
        painter.rect_filled(rect(0.0, 0.0, WIDTH as f32, 64.0), CornerRadius::ZERO, th().header);
        painter.line_segment([at(0.0, 64.0), at(WIDTH as f32, 64.0)], Stroke::new(1.0_f32, th().panel_edge));
        spaced_text(painter, at(128.0, 26.0), "MUTAFORM", 26.0, 6.0, th().hi);
        spaced_text(painter, at(128.0, 48.0), "VOICE MANGLER", 10.0, 3.0, th().accent);
    }

    // Bypass (top-left power button).
    {
        let byp = params.bypass.value();
        let r = Rect::from_center_size(at(16.0, 32.0), vec2(22.0, 22.0));
        let resp = ui.interact(r, ui.id().with("bypass"), Sense::click());
        if resp.clicked() {
            set_now(setter, &params.bypass, !byp);
        }
        let painter = ui.painter();
        power_icon(painter, r.center() + vec2(0.0, 1.0), !byp);
        if byp {
            painter.text(at(120.0, 58.0), Align2::CENTER_CENTER, "BYPASSED", FontId::proportional(9.0), th().warm);
        }
        if resp.hovered() {
            painter.circle_stroke(r.center(), 11.0, Stroke::new(1.0_f32, th().text_dim));
        }
    }

    // Preset selector (factory + user presets) and SAVE button.
    {
        let n_factory = PRESETS.len();
        if state.user_presets.is_none() {
            state.user_presets = Some(presets::list_user());
        }
        let n_user = state.user_presets.as_ref().map(|u| u.len()).unwrap_or(0);
        let total = n_factory + n_user;

        let bar = rect(244.0, 16.0, 200.0, 32.0);
        let painter = ui.painter();
        painter.rect_filled(bar, CornerRadius::same(8), th().field);
        painter.rect_stroke(bar, CornerRadius::same(8), Stroke::new(1.0_f32, th().panel_edge), StrokeKind::Inside);
        let idx = params.preset_index.load(Ordering::Relaxed).min(total.max(1) - 1);
        let left = Rect::from_min_size(bar.min, vec2(28.0, 32.0));
        let right = Rect::from_min_size(pos2(bar.max.x - 28.0, bar.min.y), vec2(28.0, 32.0));
        let mid = Rect::from_min_max(pos2(left.max.x, bar.min.y), pos2(right.min.x, bar.max.y));
        let l_resp = ui.interact(left, ui.id().with("prev"), Sense::click());
        let r_resp = ui.interact(right, ui.id().with("next"), Sense::click());
        let m_resp = ui.interact(mid, ui.id().with("presetname"), Sense::click());
        let painter = ui.painter();
        for (r, resp, tri) in [(left, &l_resp, -1.0f32), (right, &r_resp, 1.0)] {
            let c = r.center();
            let col = if resp.hovered() { th().hi } else { th().accent };
            painter.add(Shape::convex_polygon(
                vec![c + vec2(5.0 * tri, 0.0), c + vec2(-4.0 * tri, -6.0), c + vec2(-4.0 * tri, 6.0)],
                col,
                Stroke::NONE,
            ));
        }
        let shown = {
            let n = params.preset_name.read().map(|s| s.clone()).unwrap_or_default();
            if n.is_empty() { PRESETS[idx.min(n_factory - 1)].name.to_string() } else { n }
        };
        let font = FontId::proportional(if shown.chars().count() > 18 { 12.0 } else { 14.0 });
        painter.text(
            mid.center(),
            Align2::CENTER_CENTER,
            &shown,
            font,
            if m_resp.hovered() { th().hi } else { th().text },
        );

        // What to load this frame: Ok(factory index) or Err(user index).
        let mut load: Option<usize> = None;
        if l_resp.clicked() {
            load = Some((idx + total - 1) % total);
        }
        if r_resp.clicked() {
            load = Some((idx + 1) % total);
        }

        let popup_id = ui.id().with("preset_popup");
        if m_resp.clicked() {
            state.user_presets = Some(presets::list_user());
            ui.memory_mut(|m| m.toggle_popup(popup_id));
        }
        let mut delete: Option<usize> = None;
        let mut open_folder = false;
        egui::popup::popup_below_widget(ui, popup_id, &m_resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            ui.set_min_width(240.0);
            egui::ScrollArea::vertical().max_height(470.0).show(ui, |ui| {
                let mut last_cat = None;
                for (i, p) in PRESETS.iter().enumerate() {
                    if last_cat != Some(p.category) {
                        last_cat = Some(p.category);
                        if !p.category.label().is_empty() {
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(p.category.label()).size(10.0).color(th().text_dim));
                        }
                    }
                    if ui.selectable_label(i == idx, p.name).clicked() {
                        load = Some(i);
                    }
                }
                ui.add_space(4.0);
                ui.label(egui::RichText::new("MY PRESETS").size(10.0).color(th().text_dim));
                if let Some(user) = state.user_presets.as_ref() {
                    if user.is_empty() {
                        ui.label(egui::RichText::new("(none yet - use SAVE)").size(11.0).color(th().text_dim));
                    }
                    for (j, u) in user.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(idx == n_factory + j, &u.name).clicked() {
                                load = Some(n_factory + j);
                            }
                            if ui.small_button("x").on_hover_text("Delete this preset").clicked() {
                                delete = Some(j);
                            }
                        });
                    }
                }
                ui.add_space(6.0);
                if ui.button("Open presets folder").clicked() {
                    open_folder = true;
                }
            });
        });
        if let Some(j) = delete {
            if let Some(u) = state.user_presets.as_ref().and_then(|u| u.get(j)) {
                let _ = presets::delete_user(&u.path);
            }
            state.user_presets = Some(presets::list_user());
        }
        if open_folder {
            let dir = presets::user_dir();
            let _ = std::fs::create_dir_all(&dir);
            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            #[cfg(target_os = "linux")]
            let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open").arg(&dir).spawn();
            ui.memory_mut(|m| m.close_popup());
        }
        if let Some(i) = load {
            if i < n_factory {
                presets::apply(&PRESETS[i], params, setter);
                if let Ok(mut n) = params.preset_name.write() {
                    *n = PRESETS[i].name.to_string();
                }
                params.preset_index.store(i, Ordering::Relaxed);
            } else if let Some(u) = state.user_presets.as_ref().and_then(|u| u.get(i - n_factory)) {
                if let Ok(name) = presets::load_user(&u.path, params, setter) {
                    if let Ok(mut n) = params.preset_name.write() {
                        *n = name;
                    }
                    params.preset_index.store(i, Ordering::Relaxed);
                }
            }
            ui.memory_mut(|m| m.close_popup());
        }

        // SAVE button + name dialog.
        let sb = rect(450.0, 16.0, 50.0, 32.0);
        let save_resp = ui.interact(sb, ui.id().with("savebtn"), Sense::click());
        {
            let painter = ui.painter();
            painter.rect_filled(sb, CornerRadius::same(8), if save_resp.hovered() { th().btn_on } else { th().btn_off });
            painter.rect_stroke(sb, CornerRadius::same(8), Stroke::new(1.0_f32, th().panel_edge), StrokeKind::Inside);
            painter.text(sb.center(), Align2::CENTER_CENTER, "SAVE", FontId::proportional(11.0), th().accent);
        }
        let save_popup = ui.id().with("save_popup");
        if save_resp.clicked() {
            let current = if idx >= n_factory { shown.clone() } else { String::new() };
            state.save_name = current;
            state.save_focus = true;
            ui.memory_mut(|m| m.toggle_popup(save_popup));
        }
        let mut do_save = false;
        egui::popup::popup_below_widget(ui, save_popup, &save_resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            ui.set_min_width(220.0);
            ui.label("Save current settings as:");
            let te = ui.add(egui::TextEdit::singleline(&mut state.save_name).hint_text("Preset name").desired_width(200.0));
            if state.save_focus {
                te.request_focus();
                state.save_focus = false;
            }
            if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                do_save = true;
            }
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    do_save = true;
                }
                if ui.button("Cancel").clicked() {
                    ui.memory_mut(|m| m.close_popup());
                }
            });
            let exists = state
                .user_presets
                .as_ref()
                .map(|u| u.iter().any(|p| p.name.eq_ignore_ascii_case(state.save_name.trim())))
                .unwrap_or(false);
            if exists {
                ui.label(egui::RichText::new("A preset with this name will be overwritten.").size(10.0).color(th().warm));
            }
        });
        if do_save && !state.save_name.trim().is_empty() {
            let name = state.save_name.trim().to_string();
            match presets::save_user(&name, params) {
                Ok(_) => {
                    let list = presets::list_user();
                    if let Some(j) = list.iter().position(|p| p.name == name) {
                        params.preset_index.store(n_factory + j, Ordering::Relaxed);
                    }
                    state.user_presets = Some(list);
                    if let Ok(mut n) = params.preset_name.write() {
                        *n = name;
                    }
                    state.status = Some(("Preset saved".into(), 2.0));
                }
                Err(e) => state.status = Some((format!("Could not save: {e}"), 4.0)),
            }
            ui.memory_mut(|m| m.close_popup());
        }
    }

    // Key lock: key + scale.
    {
        let lock = params.key_lock.value();
        let midi_off = params.midi_mode.value() == MidiMode::Off;
        spaced_text(
            ui.painter(),
            at(621.0, 13.0),
            if midi_off { "KEY LOCK" } else { "KEY LOCK (MIDI OFF ONLY)" },
            9.0,
            1.5,
            if lock { th().accent } else { th().text_dim },
        );
        if toggle_button(ui, rect(512.0, 24.0, 44.0, 24.0), if lock { "ON" } else { "OFF" }, lock) {
            set_now(setter, &params.key_lock, !lock);
        }
        let key = params.key.value();
        let k_resp = dropdown_box(ui, rect(562.0, 24.0, 52.0, 24.0), key.label(), lock, "keybox");
        let key_popup = ui.id().with("key_popup");
        if k_resp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(key_popup));
        }
        egui::popup::popup_below_widget(ui, key_popup, &k_resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(150.0);
            egui::Grid::new("keygrid").num_columns(4).show(ui, |ui| {
                for (i, k) in Key::ALL.iter().enumerate() {
                    if ui.selectable_label(*k == key, k.label()).clicked() {
                        set_now(setter, &params.key, *k);
                    }
                    if i % 4 == 3 {
                        ui.end_row();
                    }
                }
            });
        });
        let scale = params.scale.value();
        let s_resp = dropdown_box(ui, rect(620.0, 24.0, 112.0, 24.0), scale.label(), lock, "scalebox");
        let scale_popup = ui.id().with("scale_popup");
        if s_resp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(scale_popup));
        }
        egui::popup::popup_below_widget(ui, scale_popup, &s_resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(130.0);
            for sc in Scale::ALL {
                if ui.selectable_label(sc == scale, sc.label()).clicked() {
                    set_now(setter, &params.scale, sc);
                }
            }
        });
    }

    // Detected pitch read-out (+ key-lock target).
    {
        let painter = ui.painter();
        let hz = shared.input_hz.load(Ordering::Relaxed);
        painter.text(at(808.0, 13.0), Align2::CENTER_CENTER, "INPUT PITCH", FontId::proportional(9.0), th().text_dim);
        let txt = if hz > 0.0 { format!("{} {:.0}Hz", note_name(hz), hz) } else { "-- no pitch --".into() };
        painter.text(at(808.0, 31.0), Align2::CENTER_CENTER, txt, FontId::monospace(12.0), th().accent);
        let target = shared.target_midi.load(Ordering::Relaxed);
        if target >= 0.0 && hz > 0.0 {
            let name = note_name(440.0 * ((target - 69.0) / 12.0).exp2());
            let name = name.split(' ').next().unwrap_or("").to_string();
            painter.text(
                at(808.0, 49.0),
                Align2::CENTER_CENTER,
                format!("LOCK \u{2192} {name}"),
                FontId::monospace(11.0),
                th().hi,
            );
        }
    }

    // ------------------------------------------------ left column: MIDI / detect / glide
    {
        let painter = ui.painter();
        panel(painter, rect(16.0, 82.0, 104.0, 380.0), 10);
        spaced_text(painter, at(68.0, 100.0), "MIDI", 11.0, 2.0, th().text_dim);
    }
    let mode = params.midi_mode.value();
    for (i, (m, name)) in [(MidiMode::Off, "OFF"), (MidiMode::Mono, "MONO"), (MidiMode::Poly, "POLY")].iter().enumerate() {
        if toggle_button(ui, rect(30.0, 114.0 + i as f32 * 30.0, 76.0, 24.0), name, mode == *m) {
            set_now(setter, &params.midi_mode, *m);
        }
    }
    spaced_text(ui.painter(), at(68.0, 222.0), "DETECT", 11.0, 2.0, th().text_dim);
    let det = params.detect.value();
    for (i, (d, name)) in [(DetectRange::Low, "LOW"), (DetectRange::Mid, "MID"), (DetectRange::High, "HIGH")].iter().enumerate() {
        if toggle_button(ui, rect(30.0, 236.0 + i as f32 * 30.0, 76.0, 24.0), name, det == *d) {
            set_now(setter, &params.detect, *d);
        }
    }
    knob(
        ui,
        at(68.0, 375.0),
        26.0,
        &params.glide,
        setter,
        "GLIDE",
        format!("{:.0}", params.glide.value()),
        true,
    );

    // ------------------------------------------------ main knob section
    {
        let painter = ui.painter();
        panel(painter, rect(136.0, 82.0, 608.0, 380.0), 14);
        // Harmonics sub-group backdrop.
        painter.rect_filled(rect(312.0, 96.0, 256.0, 352.0), CornerRadius::same(12), th().sub_panel);
        painter.rect_stroke(
            rect(312.0, 96.0, 256.0, 352.0),
            CornerRadius::same(12),
            Stroke::new(1.0_f32, th().sub_edge),
            StrokeKind::Inside,
        );
    }
    knob(ui, at(224.0, 178.0), 58.0, &params.pitch, setter, "PITCH", format!("{:.2}", params.pitch.value()), true);
    knob(
        ui,
        at(224.0, 354.0),
        48.0,
        &params.formant,
        setter,
        "FORMANT",
        format!("{:.2}", params.formant.value()),
        true,
    );
    knob(
        ui,
        at(440.0, 178.0),
        62.0,
        &params.harmonics,
        setter,
        "HARMONICS",
        format!("{:.3}", params.harmonics.value()),
        true,
    );
    knob(ui, at(372.0, 360.0), 38.0, &params.ratio, setter, "RATIO", format!("{:.3}", params.ratio.value()), true);
    knob(ui, at(508.0, 360.0), 38.0, &params.fm, setter, "FM", format!("{:.2}", params.fm.value()), true);
    // Snap toggle between Ratio and FM.
    {
        let snap = params.snap.value();
        let r = rect(426.0, 300.0, 28.0, 44.0);
        let resp = ui.interact(r, ui.id().with("snap"), Sense::click());
        if resp.clicked() {
            set_now(setter, &params.snap, !snap);
        }
        let painter = ui.painter();
        painter.rect_filled(r, CornerRadius::same(14), th().field);
        painter.rect_stroke(r, CornerRadius::same(14), Stroke::new(1.0_f32, th().panel_edge), StrokeKind::Inside);
        let knob_c = if snap { r.center_top() + vec2(0.0, 14.0) } else { r.center_bottom() - vec2(0.0, 14.0) };
        painter.circle_filled(knob_c, 9.0, if snap { th().accent } else { th().text_dim });
        spaced_text(painter, at(440.0, 356.0), "SNAP", 10.0, 1.5, if snap { th().accent } else { th().text_dim });
    }
    knob(
        ui,
        at(656.0, 178.0),
        58.0,
        &params.alternator,
        setter,
        "ALTERNATOR",
        format!("{:.2}", params.alternator.value()),
        true,
    );
    knob(ui, at(656.0, 354.0), 48.0, &params.octave, setter, "OCTAVE", format!("{:.3}", params.octave.value()), true);

    // ------------------------------------------------ right column: dry/wet + output
    {
        let painter = ui.painter();
        panel(painter, rect(760.0, 82.0, 104.0, 380.0), 10);
        spaced_text(painter, at(812.0, 100.0), "DRY/WET", 11.0, 1.5, th().text_dim);
    }
    vertical_meter_slider(ui, rect(796.0, 118.0, 32.0, 180.0), &params.dry_wet, setter);
    knob(
        ui,
        at(812.0, 382.0),
        26.0,
        &params.output,
        setter,
        "OUTPUT",
        format!("{:+.1}", util::gain_to_db(params.output.value())),
        true,
    );

    // ------------------------------------------------ smear / stereo / detune strip
    {
        let painter = ui.painter();
        painter.rect_filled(rect(16.0, 474.0, 848.0, 0.0).expand2(vec2(0.0, 0.5)), CornerRadius::ZERO, th().panel_edge);
    }
    line_slider(ui, rect(76.0, 486.0, 160.0, 26.0), &params.smear, setter, "SMEAR");
    line_slider(ui, rect(372.0, 486.0, 160.0, 26.0), &params.stereo, setter, "STEREO");
    line_slider(ui, rect(664.0, 486.0, 160.0, 26.0), &params.detune, setter, "DETUNE");

    // ------------------------------------------------ FX slots
    let tempo = shared.tempo.load(Ordering::Relaxed);
    let slot_w = (848.0 - 8.0 * (NUM_FX_SLOTS as f32 - 1.0)) / NUM_FX_SLOTS as f32;
    for i in 0..NUM_FX_SLOTS {
        let sp = &params.fx[i];
        let x = 16.0 + i as f32 * (slot_w + 8.0);
        let r = rect(x, 530.0, slot_w, 112.0);
        let kind = sp.kind.value();
        let on = kind != FxKind::Off;
        if on {
            state.last_kinds[i] = kind;
        }
        panel(ui.painter(), r, 8);

        // Power button.
        let pw = Rect::from_center_size(r.min + vec2(15.0, 16.0), vec2(20.0, 20.0));
        let pw_resp = ui.interact(pw, ui.id().with(("fxpow", i)), Sense::click());
        power_icon(ui.painter(), pw.center() + vec2(0.0, 1.0), on);
        if pw_resp.clicked() {
            set_now(setter, &sp.kind, if on { FxKind::Off } else { state.last_kinds[i] });
        }

        // Type selector.
        let tr = Rect::from_min_size(r.min + vec2(28.0, 6.0), vec2(slot_w - 32.0, 22.0));
        let t_resp = ui.interact(tr, ui.id().with(("fxtype", i)), Sense::click());
        let tcol = if t_resp.hovered() { th().hi } else if on { th().accent } else { th().warm };
        let galley_w = ui
            .painter()
            .text(tr.left_center(), Align2::LEFT_CENTER, kind.label(), FontId::proportional(12.0), tcol)
            .width();
        let ac = tr.left_center() + vec2(galley_w + 9.0, 1.0);
        ui.painter().add(Shape::convex_polygon(
            vec![ac + vec2(-4.0, -2.5), ac + vec2(4.0, -2.5), ac + vec2(0.0, 2.5)],
            tcol,
            Stroke::NONE,
        ));
        let popup_id = ui.id().with(("fxpopup", i));
        if t_resp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup_id));
        }
        egui::popup::popup_below_widget(ui, popup_id, &t_resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(140.0);
            for k in FxKind::ALL {
                if ui.selectable_label(k == kind, k.label()).clicked() {
                    set_now(setter, &sp.kind, k);
                }
            }
        });

        let (la, lb) = kind.knob_labels();
        let cy = r.min.y + 62.0 - o.y;
        let xs = [x + 25.0, x + slot_w / 2.0, x + slot_w - 25.0];
        let a_text = if kind == FxKind::Delay {
            if tempo > 20.0 {
                crate::params::delay_div(sp.a.value()).1.to_string()
            } else {
                format!("{:.0}", 20.0 * 50f32.powf(sp.a.value()))
            }
        } else {
            format!("{:.0}", sp.a.value() * 100.0)
        };
        knob(ui, at(xs[0], cy), 17.0, &sp.a, setter, la, a_text, on);
        knob(ui, at(xs[1], cy), 17.0, &sp.b, setter, lb, format!("{:.0}", sp.b.value() * 100.0), on);
        knob(ui, at(xs[2], cy), 17.0, &sp.mix, setter, "MIX", format!("{:.0}", sp.mix.value() * 100.0), on);
    }

    // Status message (e.g. "Preset saved").
    if let Some((msg, t)) = state.status.as_mut() {
        *t -= ui.input(|i| i.stable_dt).min(0.1);
        let alpha = (*t).clamp(0.0, 1.0);
        ui.painter().text(
            at(472.0, 56.0),
            Align2::CENTER_CENTER,
            msg.as_str(),
            FontId::proportional(10.0),
            th().accent.gamma_multiply(alpha),
        );
        if *t <= 0.0 {
            state.status = None;
        }
    }

    // Theme swatches.
    {
        let cur = params.theme.load(Ordering::Relaxed);
        ui.painter().text(at(588.0, 652.0), Align2::RIGHT_CENTER, "THEME", FontId::proportional(9.0), th().text_dim);
        for (i, theme) in THEMES.iter().enumerate() {
            let c = at(600.0 + i as f32 * 18.0, 652.0);
            let r = Rect::from_center_size(c, vec2(14.0, 14.0));
            let resp = ui.interact(r, ui.id().with(("theme", i)), Sense::click()).on_hover_text(theme.name);
            if resp.clicked() {
                params.theme.store(i, Ordering::Relaxed);
            }
            let painter = ui.painter();
            painter.circle_filled(c, 5.5, theme.swatch);
            painter.circle_stroke(c, 5.5, Stroke::new(1.0_f32, Color32::from_black_alpha(120)));
            if i == cur {
                painter.circle_stroke(c, 8.0, Stroke::new(1.5_f32, th().hi));
            } else if resp.hovered() {
                painter.circle_stroke(c, 8.0, Stroke::new(1.0_f32, th().text_dim));
            }
        }
    }

    // Footer.
    let painter = ui.painter();
    painter.text(
        at(16.0, 652.0),
        Align2::LEFT_CENTER,
        "Drag  ·  Shift = fine  ·  Right-click = type value  ·  Double-click = reset",
        FontId::proportional(10.0),
        th().text_dim,
    );
    // Credit.
    {
        let font_small = FontId::proportional(10.0);
        let font_name = FontId::proportional(11.0);
        let name = "Bendik Sandvold-Ertsås";
        let name_w = painter.layout_no_wrap(name.to_string(), font_name.clone(), th().accent).size().x;
        let right = at(546.0, 652.0);
        painter.text(right, Align2::RIGHT_CENTER, name, font_name, th().accent);
        painter.text(right - vec2(name_w + 5.0, 0.0), Align2::RIGHT_CENTER, "made by", font_small, th().text_dim);
    }
    painter.text(
        at(864.0, 652.0),
        Align2::RIGHT_CENTER,
        concat!("v", env!("CARGO_PKG_VERSION")),
        FontId::proportional(10.0),
        th().text_dim,
    );
}
