use crate::calc::{
    calculate_misclose, check_deflection_sum, dd_to_dms_string, dd_to_dms_string_prec,
    detect_blunders, dms_to_dd, BlunderCandidate,
};
use eframe::egui::{self};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::HashSet;

fn split_unit(s: &str) -> (&str, &str) {
    let s = s.trim_end();
    let unit_start = s
        .rfind(|c: char| !c.is_alphabetic())
        .map(|i| i + 1)
        .unwrap_or(0);
    let (expr, unit) = s.split_at(unit_start);
    if unit.chars().all(|c| c.is_alphabetic()) {
        (expr.trim_end(), unit)
    } else {
        (s, "")
    }
}

/// Evaluate a simple arithmetic expression: +, -, *, / with correct precedence.
fn eval_arithmetic(s: &str) -> Option<f64> {
    let s = s.trim();
    parse_expr(s, &mut 0)
}

fn parse_expr(s: &str, pos: &mut usize) -> Option<f64> {
    let mut val = parse_term(s, pos)?;
    loop {
        skip_ws(s, pos);
        if *pos >= s.len() {
            break;
        }
        let ch = s.as_bytes()[*pos] as char;
        if ch == '+' || ch == '-' {
            *pos += 1;
            let rhs = parse_term(s, pos)?;
            if ch == '+' {
                val += rhs;
            } else {
                val -= rhs;
            }
        } else {
            break;
        }
    }
    Some(val)
}

fn parse_term(s: &str, pos: &mut usize) -> Option<f64> {
    let mut val = parse_unary(s, pos)?;
    loop {
        skip_ws(s, pos);
        if *pos >= s.len() {
            break;
        }
        let ch = s.as_bytes()[*pos] as char;
        if ch == '*' || ch == '/' {
            *pos += 1;
            let rhs = parse_unary(s, pos)?;
            if ch == '*' {
                val *= rhs;
            } else {
                if rhs == 0.0 {
                    return None;
                }
                val /= rhs;
            }
        } else {
            break;
        }
    }
    Some(val)
}

fn parse_unary(s: &str, pos: &mut usize) -> Option<f64> {
    skip_ws(s, pos);
    if *pos < s.len() && s.as_bytes()[*pos] as char == '-' {
        *pos += 1;
        return Some(-parse_atom(s, pos)?);
    }
    parse_atom(s, pos)
}

fn parse_atom(s: &str, pos: &mut usize) -> Option<f64> {
    skip_ws(s, pos);
    if *pos >= s.len() {
        return None;
    }
    if s.as_bytes()[*pos] as char == '(' {
        *pos += 1;
        let val = parse_expr(s, pos)?;
        skip_ws(s, pos);
        if *pos < s.len() && s.as_bytes()[*pos] as char == ')' {
            *pos += 1;
        }
        return Some(val);
    }
    let start = *pos;
    while *pos < s.len() {
        let c = s.as_bytes()[*pos] as char;
        if c.is_ascii_digit() || c == '.' {
            *pos += 1;
        } else {
            break;
        }
    }
    if *pos == start {
        return None;
    }
    s[start..*pos].parse().ok()
}

fn skip_ws(s: &str, pos: &mut usize) {
    while *pos < s.len() && (s.as_bytes()[*pos] as char).is_whitespace() {
        *pos += 1;
    }
}

/// Parse a distance string, optionally with an arithmetic expression and a unit suffix.
/// Supported units: m (default), ft, ch (Gunter's chain), lk (link).
/// All values are returned in metres.
fn parse_distance(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (expr, unit) = split_unit(s);
    if expr.is_empty() {
        return None;
    }
    let val = eval_arithmetic(expr)?;
    if val < 0.0 {
        return None;
    }
    let factor = match unit.to_lowercase().as_str() {
        "" | "m" => 1.0,
        "ft" => 0.3048,
        "ch" => 20.1168,  // Gunter's chain
        "lk" => 0.201168, // link (1/100 chain)
        _ => return None,
    };
    Some(val * factor)
}

#[derive(Default, Serialize, Deserialize)]
struct Leg {
    bearing: String,
    distance: String,
}

impl Leg {
    fn is_reversed(&self) -> bool {
        self.bearing.trim().ends_with('*')
    }

    /// Bearing string with trailing `*` and surrounding whitespace stripped.
    fn bearing_base(&self) -> &str {
        self.bearing.trim().trim_end_matches('*').trim()
    }

    fn parse(&self) -> Option<(f64, f64)> {
        let base = self.bearing_base();
        if base.is_empty() {
            return None;
        }
        let b = base.parse::<f64>().ok()?;
        let b = if self.is_reversed() {
            (b + 180.0) % 360.0
        } else {
            b
        };
        let d = parse_distance(&self.distance)?;
        Some((b, d))
    }

    fn bearing_valid(&self) -> bool {
        let s = self.bearing_base();
        s.is_empty() || s.parse::<f64>().is_ok()
    }

    fn distance_valid(&self) -> bool {
        self.distance.is_empty() || parse_distance(&self.distance).is_some()
    }

    /// False when the MM or SS field in the DMS input is ≥ 60 (e.g. 45.6230 = 45°62′30″).
    fn bearing_dms_sane(&self) -> bool {
        let base = self.bearing_base().trim().trim_start_matches('-');
        let Some((_, frac)) = base.split_once('.') else {
            return true;
        };
        if frac.len() < 2 {
            return true;
        }
        let mm: u32 = frac[..2].parse().unwrap_or(0);
        let ss: u32 = if frac.len() >= 4 {
            frac[2..4].parse().unwrap_or(0)
        } else {
            0
        };
        mm < 60 && ss < 60
    }

    /// DMS hint for the resolved bearing (always non-empty when valid, "→" prefix when reversed).
    fn bearing_hint(&self) -> String {
        let base = self.bearing_base();
        if base.is_empty() {
            return String::new();
        }
        let b = match base.parse::<f64>() {
            Ok(v) => v,
            Err(_) => return String::new(),
        };
        let resolved = if self.is_reversed() {
            (b + 180.0) % 360.0
        } else {
            b
        };
        let prefix = if self.is_reversed() { "→ " } else { "" };
        format!("{}{}", prefix, dd_to_dms_string(dms_to_dd(resolved)))
    }

    /// Metres hint shown when arithmetic or unit conversion is present.
    fn distance_hint(&self) -> String {
        let s = self.distance.trim();
        if s.is_empty() {
            return String::new();
        }
        let (expr, unit) = split_unit(s);
        let has_op = expr.contains(['+', '-', '*', '/', '(', ')']);
        let has_conversion = !unit.is_empty() && unit.to_lowercase() != "m";
        if !has_op && !has_conversion {
            return String::new();
        }
        match parse_distance(s) {
            Some(v) => format!("= {:.4} m", v),
            None => String::new(),
        }
    }
}

// --- Coordinate helpers ---

fn dms_to_rad_f64(bearing_dms: f64) -> f64 {
    dms_to_dd(bearing_dms).to_radians()
}

/// Compute (E, N) after each leg; None for invalid legs (cursor still advances for valid ones).
fn compute_leg_coords(
    legs: &[Leg],
    start_e: f64,
    start_n: f64,
    scale: f64,
) -> Vec<Option<(f64, f64)>> {
    let mut ce = start_e;
    let mut cn = start_n;
    legs.iter()
        .map(|leg| {
            let (b_dms, dist) = leg.parse()?;
            let rad = dms_to_rad_f64(b_dms);
            ce += dist * scale * rad.sin();
            cn += dist * scale * rad.cos();
            Some((ce, cn))
        })
        .collect()
}

// --- Diagram helpers ---

fn traverse_points(legs: &[(f64, f64)]) -> Vec<[f64; 2]> {
    let mut pts = vec![[0.0f64, 0.0f64]];
    for &(bearing_dms, dist) in legs {
        let rad = dms_to_rad_f64(bearing_dms);
        let [le, ln] = *pts.last().unwrap();
        pts.push([le + dist * rad.sin(), ln + dist * rad.cos()]);
    }
    pts
}

fn arrow_tip(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, color: egui::Color32) {
    let d = to - from;
    if d.length() < 2.0 {
        return;
    }
    let dir = d.normalized();
    let perp = egui::vec2(-dir.y, dir.x);
    let sz = 9.0f32;
    painter.add(egui::Shape::convex_polygon(
        vec![
            to,
            to - dir * sz + perp * (sz * 0.45),
            to - dir * sz - perp * (sz * 0.45),
        ],
        color,
        egui::Stroke::NONE,
    ));
}

/// Persistent pan/zoom for the diagram, applied on top of the auto-fit transform.
/// `zoom` 1.0 and `offset` zero reproduce the auto-fit view.
#[derive(Clone, Copy)]
struct DiagramView {
    offset: egui::Vec2,
    zoom: f32,
}

impl Default for DiagramView {
    fn default() -> Self {
        Self {
            offset: egui::Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

/// Round up to a "nice" 1/2/5 × 10ⁿ value, for scale-bar lengths.
fn nice_round(x: f64) -> f64 {
    if x <= 0.0 || !x.is_finite() {
        return 1.0;
    }
    let pow = 10f64.powf(x.log10().floor());
    let frac = x / pow;
    let nice = if frac < 1.5 {
        1.0
    } else if frac < 3.0 {
        2.0
    } else if frac < 7.0 {
        5.0
    } else {
        10.0
    };
    nice * pow
}

fn draw_traverse_diagram(ui: &mut egui::Ui, legs: &[(f64, f64)]) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        let rect = ui.max_rect();
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(16, 18, 26));

        if legs.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No valid legs to display",
                egui::FontId::proportional(16.0),
                egui::Color32::from_gray(90),
            );
            return;
        }

        let pts = traverse_points(legs);

        let padding = 56.0f32;
        let draw_rect = rect.shrink(padding);

        let min_e = pts.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let max_e = pts.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
        let min_n = pts.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let max_n = pts.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);

        let span_e = (max_e - min_e) as f32;
        let span_n = (max_n - min_n) as f32;

        let fit_scale = {
            let sx = if span_e > 1e-4 {
                draw_rect.width() / span_e
            } else {
                f32::MAX
            };
            let sy = if span_n > 1e-4 {
                draw_rect.height() / span_n
            } else {
                f32::MAX
            };
            sx.min(sy).min(1e6f32)
        };

        let center_e = ((min_e + max_e) / 2.0) as f32;
        let center_n = ((min_n + max_n) / 2.0) as f32;
        let canvas_center = draw_rect.center();

        // --- Pan / zoom interaction ---
        let view_id = ui.id().with("diagram_view");
        let mut view = ui
            .ctx()
            .data(|d| d.get_temp::<DiagramView>(view_id))
            .unwrap_or_default();

        let response = ui.interact(
            rect,
            ui.id().with("diagram_canvas"),
            egui::Sense::click_and_drag(),
        );
        if response.dragged() {
            view.offset += response.drag_delta();
        }
        if response.double_clicked() {
            view = DiagramView::default();
        }
        if response.hovered() {
            let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_y != 0.0 {
                let factor = (scroll_y * 0.0015).exp();
                let new_zoom = (view.zoom * factor).clamp(0.05, 50.0);
                // Zoom about the pointer so the point under the cursor stays put.
                if let Some(p) = ui.input(|i| i.pointer.hover_pos()) {
                    let anchor = p - canvas_center - view.offset;
                    view.offset -= anchor * (new_zoom / view.zoom - 1.0);
                }
                view.zoom = new_zoom;
            }
        }
        ui.ctx().data_mut(|d| d.insert_temp(view_id, view));

        // Effective pixels-per-metre after pan/zoom.
        let ppm = fit_scale * view.zoom;

        let to_screen = |e: f64, n: f64| -> egui::Pos2 {
            let bx = canvas_center.x + (e as f32 - center_e) * fit_scale;
            let by = canvas_center.y - (n as f32 - center_n) * fit_scale;
            egui::pos2(
                canvas_center.x + view.offset.x + (bx - canvas_center.x) * view.zoom,
                canvas_center.y + view.offset.y + (by - canvas_center.y) * view.zoom,
            )
        };

        // Legs
        let leg_color = egui::Color32::from_rgb(94, 179, 255);
        let label_color = egui::Color32::from_rgb(165, 185, 215);
        for i in 0..pts.len() - 1 {
            let p1 = to_screen(pts[i][0], pts[i][1]);
            let p2 = to_screen(pts[i + 1][0], pts[i + 1][1]);
            painter.line_segment([p1, p2], egui::Stroke::new(2.0, leg_color));

            // Direction arrow at midpoint
            let mid = egui::pos2((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
            arrow_tip(&painter, p1, mid, leg_color);

            // Leg label, offset perpendicular to the leg. Show bearing + distance
            // when there's room, otherwise just the leg number.
            let seg_len = p1.distance(p2);
            if seg_len > 28.0 {
                let dir = (p2 - p1).normalized();
                let perp = egui::vec2(-dir.y, dir.x);
                let (bearing_dms, dist) = legs[i];
                let text = if seg_len > 64.0 {
                    format!(
                        "{}\n{}\n{:.2} m",
                        i + 1,
                        dd_to_dms_string(dms_to_dd(bearing_dms)),
                        dist
                    )
                } else {
                    format!("{}", i + 1)
                };
                painter.text(
                    mid + perp * 18.0,
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(11.0),
                    label_color,
                );
            }
        }

        // Misclose vector (last point → origin), shown in red
        let sp_origin = to_screen(0.0, 0.0);
        let sp_last = to_screen(pts.last().unwrap()[0], pts.last().unwrap()[1]);
        let has_misclose = sp_last.distance(sp_origin) > 4.0;
        if has_misclose {
            let mc_color = egui::Color32::from_rgb(255, 80, 60);
            painter.line_segment([sp_last, sp_origin], egui::Stroke::new(1.5, mc_color));
            arrow_tip(&painter, sp_last, sp_origin, mc_color);
        }

        // Vertex dots
        for (i, &[e, n]) in pts.iter().enumerate() {
            let sp = to_screen(e, n);
            let is_start = i == 0;
            let is_last = i == pts.len() - 1;
            let fill = if is_start || (is_last && !has_misclose) {
                egui::Color32::from_rgb(70, 210, 105)
            } else if is_last {
                egui::Color32::from_rgb(255, 80, 60)
            } else {
                egui::Color32::from_rgb(175, 185, 205)
            };
            let r = if is_start || is_last { 6.0f32 } else { 4.0f32 };
            painter.circle_filled(sp, r, fill);
            painter.circle_stroke(
                sp,
                r,
                egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 160),
                ),
            );
        }

        // "Start" label at origin
        painter.text(
            sp_origin + egui::vec2(10.0, -10.0),
            egui::Align2::LEFT_BOTTOM,
            "Start",
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(70, 210, 105),
        );

        // North indicator — top-right corner
        let ni_x = rect.right() - 24.0;
        let ni_top = rect.top() + 20.0;
        let ni_bottom = rect.top() + 52.0;
        let ni_color = egui::Color32::from_gray(155);
        painter.line_segment(
            [egui::pos2(ni_x, ni_bottom), egui::pos2(ni_x, ni_top + 9.0)],
            egui::Stroke::new(1.5, ni_color),
        );
        arrow_tip(
            &painter,
            egui::pos2(ni_x, ni_bottom),
            egui::pos2(ni_x, ni_top),
            ni_color,
        );
        painter.text(
            egui::pos2(ni_x, ni_top - 4.0),
            egui::Align2::CENTER_BOTTOM,
            "N",
            egui::FontId::proportional(11.0),
            ni_color,
        );

        // Misclose magnitude callout — top-left (the red vector is often sub-pixel).
        let last = pts.last().unwrap();
        let mc_dist = (last[0] * last[0] + last[1] * last[1]).sqrt();
        let (mc_text, mc_text_color) = if mc_dist < 1e-6 {
            (
                "Misclose  0 (perfect)".to_string(),
                egui::Color32::from_rgb(70, 210, 105),
            )
        } else {
            (
                format!("Misclose  {:.3} m", mc_dist),
                egui::Color32::from_rgb(255, 110, 90),
            )
        };
        painter.text(
            egui::pos2(rect.left() + 14.0, rect.top() + 14.0),
            egui::Align2::LEFT_TOP,
            mc_text,
            egui::FontId::proportional(12.0),
            mc_text_color,
        );

        // Scale bar — bottom-left, a "nice" 1/2/5 length near 110 px.
        if ppm.is_finite() && ppm > 0.0 {
            let bar_m = nice_round(110.0 / ppm as f64);
            let bar_px = (bar_m * ppm as f64) as f32;
            let x0 = rect.left() + 16.0;
            let y = rect.bottom() - 22.0;
            let bar_color = egui::Color32::from_gray(170);
            painter.line_segment(
                [egui::pos2(x0, y), egui::pos2(x0 + bar_px, y)],
                egui::Stroke::new(2.0, bar_color),
            );
            for tx in [x0, x0 + bar_px] {
                painter.line_segment(
                    [egui::pos2(tx, y - 4.0), egui::pos2(tx, y + 4.0)],
                    egui::Stroke::new(2.0, bar_color),
                );
            }
            let bar_label = if bar_m >= 1.0 {
                format!("{:.0} m", bar_m)
            } else {
                format!("{} m", bar_m)
            };
            painter.text(
                egui::pos2(x0 + bar_px / 2.0, y - 6.0),
                egui::Align2::CENTER_BOTTOM,
                bar_label,
                egui::FontId::proportional(11.0),
                bar_color,
            );
        }

        // Interaction hint — bottom-right.
        painter.text(
            egui::pos2(rect.right() - 12.0, rect.bottom() - 10.0),
            egui::Align2::RIGHT_BOTTOM,
            "drag · scroll · double-click to reset",
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(95),
        );
    });
}

// --- App ---

/// Background tint for an invalid input field, kept readable against the active theme.
fn error_bg(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(80, 20, 20)
    } else {
        egui::Color32::from_rgb(255, 205, 205)
    }
}

/// Background tint for a suspected-blunder input field, kept readable against the active theme.
fn suspect_bg(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(90, 65, 10)
    } else {
        egui::Color32::from_rgb(255, 230, 160)
    }
}

/// User-tunable display and appearance settings, persisted with the app.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Decimal places for distances (misclose, total).
    distance_decimals: usize,
    /// Decimal places for E/N coordinates.
    coord_decimals: usize,
    /// Max decimal places of seconds in bearing readouts (trailing zeros trimmed to 2).
    seconds_decimals: usize,
    dark_mode: bool,
    /// egui zoom factor applied over native DPI.
    ui_scale: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            distance_decimals: 4,
            coord_decimals: 3,
            seconds_decimals: 4,
            dark_mode: true,
            ui_scale: 1.0,
        }
    }
}

/// Cached result of the misclose pipeline. The big-float trig in
/// [`calculate_misclose`]/[`detect_blunders`] is far too costly to redo on every
/// repaint (e.g. while the window is being resized), so it is recomputed on a
/// timer and reused on intervening frames.
#[derive(Default)]
struct ComputeCache {
    computed_at: Option<std::time::Instant>,
    valid_legs: Vec<(f64, f64)>,
    valid_leg_orig_idx: Vec<usize>,
    misclose: Option<crate::calc::MiscloseResult>,
    deflection: Option<crate::calc::DeflectionCheck>,
    blunder_candidates: Vec<BlunderCandidate>,
    suspect_orig_indices: HashSet<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct NullfieldCalcApp {
    legs: Vec<Leg>,
    show_ppm: bool,
    threshold: f64,
    #[serde(skip)]
    show_diagram: bool,
    start_e: String,
    start_n: String,
    scale_factor: String,
    threshold_str: String,
    #[serde(default)]
    settings: Settings,
    #[serde(skip)]
    show_settings: bool,
    #[serde(skip)]
    cache: ComputeCache,
}

impl Default for NullfieldCalcApp {
    fn default() -> Self {
        Self {
            legs: vec![Leg::default(), Leg::default()],
            show_ppm: false,
            threshold: 10000.0,
            show_diagram: false,
            start_e: String::new(),
            start_n: String::new(),
            scale_factor: String::new(),
            threshold_str: "10000".to_string(),
            settings: Settings::default(),
            show_settings: false,
            cache: ComputeCache::default(),
        }
    }
}

impl NullfieldCalcApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            if let Some(mut app) = eframe::get_value::<NullfieldCalcApp>(storage, eframe::APP_KEY) {
                app.threshold_str = format!("{:.0}", app.threshold);
                return app;
            }
        }
        Self::default()
    }
}

impl eframe::App for NullfieldCalcApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        ctx.set_zoom_factor(self.settings.ui_scale);
        ctx.set_visuals(if self.settings.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });

        // Settings overlay — rendered before the panels borrow `self`.
        let mut show_settings = self.show_settings;
        egui::Window::new("⚙ Settings")
            .open(&mut show_settings)
            .resizable(false)
            .collapsible(false)
            .show(&ctx, |ui| {
                let s = &mut self.settings;
                ui.label(egui::RichText::new("Display precision").strong());
                ui.add(
                    egui::Slider::new(&mut s.distance_decimals, 0..=6).text("Distance decimals"),
                );
                ui.add(egui::Slider::new(&mut s.coord_decimals, 0..=6).text("Coordinate decimals"));
                ui.add(
                    egui::Slider::new(&mut s.seconds_decimals, 0..=6).text("Bearing seconds (max)"),
                );
                ui.separator();
                ui.label(egui::RichText::new("Appearance").strong());
                ui.checkbox(&mut s.dark_mode, "Dark mode");
                ui.add(
                    egui::Slider::new(&mut s.ui_scale, 0.8..=1.6)
                        .step_by(0.05)
                        .text("UI scale"),
                );
                ui.separator();
                if ui.button("Reset to defaults").clicked() {
                    *s = Settings::default();
                }
            });
        self.show_settings = show_settings;

        // The misclose pipeline uses 256-bit big-float trig, which is too costly
        // to run on every repaint (a window resize alone can drive 60+ fps). Cap
        // it to RECOMPUTE_INTERVAL and reuse the previous result in between.
        const RECOMPUTE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
        let now = std::time::Instant::now();
        match self.cache.computed_at {
            Some(t) if now.duration_since(t) < RECOMPUTE_INTERVAL => {
                // Still fresh — make sure a repaint lands once it goes stale so
                // edits made during the interval show up promptly.
                ctx.request_repaint_after(RECOMPUTE_INTERVAL - now.duration_since(t));
            }
            _ => {
                let (valid_legs, valid_leg_orig_idx): (Vec<(f64, f64)>, Vec<usize>) = self
                    .legs
                    .iter()
                    .enumerate()
                    .filter_map(|(i, l)| l.parse().map(|p| (p, i)))
                    .unzip();
                let misclose = calculate_misclose(&valid_legs);
                let deflection = check_deflection_sum(&valid_legs);
                let blunder_candidates: Vec<BlunderCandidate> = match &misclose {
                    Some(m)
                        if valid_legs.len() >= 4
                            && !m.ratio.is_infinite()
                            && m.ratio < self.threshold =>
                    {
                        detect_blunders(&valid_legs, m.ratio)
                    }
                    _ => vec![],
                };
                let suspect_orig_indices: HashSet<usize> = blunder_candidates
                    .iter()
                    .map(|c| valid_leg_orig_idx[c.leg_index])
                    .collect();
                self.cache = ComputeCache {
                    computed_at: Some(now),
                    valid_legs,
                    valid_leg_orig_idx,
                    misclose,
                    deflection,
                    blunder_candidates,
                    suspect_orig_indices,
                };
            }
        }

        let valid_legs = self.cache.valid_legs.clone();
        let valid_leg_orig_idx = self.cache.valid_leg_orig_idx.clone();
        let misclose = self.cache.misclose.clone();
        let deflection = self.cache.deflection.clone();
        let blunder_candidates = self.cache.blunder_candidates.clone();
        let suspect_orig_indices = self.cache.suspect_orig_indices.clone();

        let start_e_val = self.start_e.trim().parse::<f64>().ok();
        let start_n_val = self.start_n.trim().parse::<f64>().ok();
        let scale_val = self
            .scale_factor
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|&v| v > 0.0)
            .unwrap_or(1.0);
        let start_coord = start_e_val.zip(start_n_val);
        let has_coords = start_coord.is_some();

        let leg_coords: Vec<Option<(f64, f64)>> = if let Some((se, sn)) = start_coord {
            compute_leg_coords(&self.legs, se, sn, scale_val)
        } else {
            vec![]
        };
        let final_coord: Option<(f64, f64)> = leg_coords.iter().rev().find_map(|c| *c);

        // Diagram viewport — shown as a separate OS window
        let close_diagram = Cell::new(false);
        if self.show_diagram {
            let snap = valid_legs.clone();
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("traverse_diagram"),
                egui::ViewportBuilder::default()
                    .with_title("Traverse Diagram")
                    .with_min_inner_size([300.0, 300.0])
                    .with_inner_size([600.0, 600.0]),
                |ui, _class| {
                    if ui.ctx().input(|i| i.viewport().close_requested()) {
                        close_diagram.set(true);
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    draw_traverse_diagram(ui, &snap);
                },
            );
        }
        if close_diagram.get() {
            self.show_diagram = false;
        }

        egui::Panel::bottom("misclose_panel")
            .min_size(180.0)
            .show_inside(ui, |ui| {
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    let lbl = if self.show_diagram {
                        "Hide Diagram"
                    } else {
                        "Diagram"
                    };
                    if ui.button(lbl).clicked() {
                        self.show_diagram = !self.show_diagram;
                    }
                });
                ui.add_space(4.0);

                ui.group(|ui| match &misclose {
                    None => {
                        ui.label(
                            egui::RichText::new("Enter bearing and distance for each leg above")
                                .italics()
                                .color(egui::Color32::GRAY),
                        );
                    }
                    Some(m) => {
                        let coord_color = egui::Color32::from_rgb(140, 200, 140);
                        let show_coord_col = final_coord.is_some();
                        egui::Grid::new("mc_grid")
                            .num_columns(if show_coord_col { 4 } else { 2 })
                            .spacing([24.0, 6.0])
                            .show(ui, |ui| {
                                ui.strong("Bearing");
                                ui.label(
                                    egui::RichText::new(dd_to_dms_string_prec(
                                        m.bearing_dd,
                                        self.settings.seconds_decimals,
                                    ))
                                    .size(15.0)
                                    .monospace(),
                                );
                                if let Some((fe, _)) = final_coord {
                                    ui.strong("Final E");
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{:.*}",
                                            self.settings.coord_decimals, fe
                                        ))
                                        .monospace()
                                        .color(coord_color),
                                    );
                                }
                                ui.end_row();

                                ui.strong("Distance");
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{:.*} m",
                                        self.settings.distance_decimals, m.distance
                                    ))
                                    .monospace(),
                                );
                                if let Some((_, fn_)) = final_coord {
                                    ui.strong("Final N");
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{:.*}",
                                            self.settings.coord_decimals, fn_
                                        ))
                                        .monospace()
                                        .color(coord_color),
                                    );
                                }
                                ui.end_row();

                                ui.strong("Accuracy");
                                let pass = m.ratio.is_infinite() || m.ratio >= self.threshold;
                                let color = if pass {
                                    egui::Color32::from_rgb(100, 200, 100)
                                } else {
                                    egui::Color32::from_rgb(220, 80, 80)
                                };
                                let text = if self.show_ppm {
                                    format!("{:.0} ppm", m.ppm)
                                } else if m.ratio.is_infinite() {
                                    "perfect closure".to_string()
                                } else {
                                    format!("1:{:.0}", m.ratio)
                                };
                                ui.label(
                                    egui::RichText::new(text).size(15.0).strong().color(color),
                                );
                                if show_coord_col {
                                    ui.label("");
                                    ui.label("");
                                }
                                ui.end_row();

                                if let Some(d) = &deflection {
                                    ui.strong("Angle sum");
                                    let abs_err = d.error_deg.abs();
                                    let dir = if d.sum_deg >= 0.0 { "CW" } else { "CCW" };
                                    let (def_text, def_color) = if abs_err < 0.001 {
                                        (
                                            format!("360°00'00\" {dir}"),
                                            egui::Color32::from_rgb(100, 200, 100),
                                        )
                                    } else if abs_err < 0.1 {
                                        (
                                            format!("off {} {dir}", dd_to_dms_string(abs_err)),
                                            egui::Color32::from_rgb(230, 160, 40),
                                        )
                                    } else {
                                        (
                                            format!("off {} {dir}", dd_to_dms_string(abs_err)),
                                            egui::Color32::from_rgb(220, 80, 80),
                                        )
                                    };
                                    ui.label(egui::RichText::new(def_text).monospace().color(def_color));
                                    if show_coord_col {
                                        ui.label("");
                                        ui.label("");
                                    }
                                    ui.end_row();
                                }
                            });

                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "Traverse length  {:.*} m",
                                self.settings.distance_decimals, m.total_distance
                            ))
                            .color(egui::Color32::GRAY)
                            .monospace(),
                        );

                        if !blunder_candidates.is_empty() {
                            ui.add_space(6.0);
                            ui.separator();
                            ui.label(
                                egui::RichText::new("Possible blunder")
                                    .color(egui::Color32::from_rgb(230, 160, 40))
                                    .strong(),
                            );
                            egui::Grid::new("blunder_grid")
                                .num_columns(4)
                                .spacing([16.0, 4.0])
                                .show(ui, |ui| {
                                    ui.strong("Leg");
                                    ui.strong("Bearing");
                                    ui.strong("Distance");
                                    ui.strong("Ratio without");
                                    ui.end_row();
                                    for c in &blunder_candidates {
                                        let orig_i = valid_leg_orig_idx[c.leg_index];
                                        let leg = &self.legs[orig_i];
                                        ui.label(
                                            egui::RichText::new(format!("{}", orig_i + 1))
                                                .monospace()
                                                .color(egui::Color32::from_rgb(230, 160, 40)),
                                        );
                                        ui.label(egui::RichText::new(&leg.bearing).monospace());
                                        ui.label(egui::RichText::new(&leg.distance).monospace());
                                        let ratio_text = if c.ratio_without.is_infinite() {
                                            "perfect closure".to_string()
                                        } else {
                                            format!("1:{:.0}", c.ratio_without)
                                        };
                                        ui.label(
                                            egui::RichText::new(ratio_text)
                                                .monospace()
                                                .color(egui::Color32::from_rgb(100, 200, 100)),
                                        );
                                        ui.end_row();
                                    }
                                });
                        }
                    }
                });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Accuracy:");
                    ui.radio_value(&mut self.show_ppm, false, "Ratio");
                    ui.radio_value(&mut self.show_ppm, true, "PPM");

                    ui.separator();
                    ui.label("Pass threshold  1:");
                    let thr_ok = self.threshold_str.trim().parse::<f64>().map(|v| v > 0.0).unwrap_or(false);
                    ui.scope(|ui| {
                        if !thr_ok {
                            ui.visuals_mut().extreme_bg_color = error_bg(ui);
                        }
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.threshold_str).desired_width(80.0),
                        );
                        if resp.changed() {
                            if let Ok(v) = self.threshold_str.trim().parse::<f64>() {
                                if v > 0.0 {
                                    self.threshold = v;
                                }
                            }
                        }
                    });

                    if let Some(m) = &misclose {
                        ui.separator();
                        let copy_text = format!(
                            "Bearing:    {}\nDistance:   {:.*} m\nTraverse:   {:.*} m\nAccuracy:   {}",
                            dd_to_dms_string_prec(m.bearing_dd, self.settings.seconds_decimals),
                            self.settings.distance_decimals,
                            m.distance,
                            self.settings.distance_decimals,
                            m.total_distance,
                            if m.ratio.is_infinite() {
                                "perfect closure".to_string()
                            } else if self.show_ppm {
                                format!("{:.0} ppm", m.ppm)
                            } else {
                                format!("1:{:.0}", m.ratio)
                            },
                        );
                        if ui.button("Copy").clicked() {
                            ctx.copy_text(copy_text);
                        }
                    }
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Misclose Calculator");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear All").clicked() {
                        self.legs = vec![Leg::default(), Leg::default()];
                        self.start_e = String::new();
                        self.start_n = String::new();
                        self.scale_factor = String::new();
                    }
                    if ui.button("⚙ Settings").clicked() {
                        self.show_settings = !self.show_settings;
                    }
                });
            });
            ui.add_space(6.0);

            // Shared column geometry for the start row and the legs table, so the
            // Easting lines up under the Bearing column and Northing under Distance.
            const NUM_W: f32 = 34.0;
            const BEARING_W: f32 = 120.0;
            const DIST_W: f32 = 104.0;
            const COORD_W: f32 = 130.0;
            const COL_GAP: f32 = 10.0;
            // Nudge frameless cells (labels, coordinates) down so their text lines
            // up with the text inside the framed inputs.
            const INPUT_TOP_PAD: f32 = 4.0;
            // Tighter than the default `symmetric(4, 2)` to keep the boxes short.
            let input_margin = egui::Margin::symmetric(4, 1);

            // Starting coordinate; the scale factor (CSF) sits under the Easting.
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = COL_GAP;

                ui.allocate_ui_with_layout(
                    egui::vec2(NUM_W, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.add_space(INPUT_TOP_PAD);
                        ui.strong("Start");
                    },
                );

                // Easting, with the scale factor directly below it.
                ui.vertical(|ui| {
                    let se_ok = self.start_e.trim().is_empty()
                        || self.start_e.trim().parse::<f64>().is_ok();
                    ui.scope(|ui| {
                        if !se_ok {
                            ui.visuals_mut().extreme_bg_color = error_bg(ui);
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut self.start_e)
                                .desired_width(BEARING_W)
                                .margin(input_margin)
                                .hint_text("Easting"),
                        );
                    });

                    let sf_ok = self.scale_factor.trim().is_empty()
                        || self
                            .scale_factor
                            .trim()
                            .parse::<f64>()
                            .map(|v| v > 0.0)
                            .unwrap_or(false);
                    ui.scope(|ui| {
                        if !sf_ok {
                            ui.visuals_mut().extreme_bg_color = error_bg(ui);
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut self.scale_factor)
                                .desired_width(BEARING_W)
                                .margin(input_margin)
                                .hint_text("Scale 1.000000"),
                        );
                    });
                });

                // Northing
                ui.vertical(|ui| {
                    let sn_ok = self.start_n.trim().is_empty()
                        || self.start_n.trim().parse::<f64>().is_ok();
                    ui.scope(|ui| {
                        if !sn_ok {
                            ui.visuals_mut().extreme_bg_color = error_bg(ui);
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut self.start_n)
                                .desired_width(DIST_W)
                                .margin(input_margin)
                                .hint_text("Northing"),
                        );
                    });
                });
            });
            ui.add_space(4.0);

            let mut to_remove: Option<usize> = None;
            let mut to_insert: Option<usize> = None;
            let mut next_focus: Option<egui::Id> = None;
            let mut add_leg = false;
            let num_legs = self.legs.len();

            // Rows use `horizontal_top` (not a Grid) so single-line cells top-align
            // with the text inputs instead of being vertically centered against the
            // taller input + hint cells. Column widths/margins are the shared values
            // declared above the start row, so header, start, and legs all line up.
            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 40.0)
                .show(ui, |ui| {
                    // Header row
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = COL_GAP;
                        ui.allocate_ui_with_layout(
                            egui::vec2(NUM_W, 0.0),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                ui.strong("#");
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(BEARING_W, 0.0),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                ui.strong("Bearing  (* reverses)");
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(DIST_W, 0.0),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                ui.strong("Distance  (m/ft/ch/lk)");
                            },
                        );
                        if has_coords {
                            ui.allocate_ui_with_layout(
                                egui::vec2(COORD_W, 0.0),
                                egui::Layout::left_to_right(egui::Align::Min),
                                |ui| {
                                    ui.strong("Coordinate (E / N)");
                                },
                            );
                        }
                    });
                    ui.add_space(2.0);

                    for i in 0..num_legs {
                        let bearing_id = egui::Id::new(("bearing", i));
                        let distance_id = egui::Id::new(("distance", i));
                        let b_valid = self.legs[i].bearing_valid();
                        let d_valid = self.legs[i].distance_valid();
                        let is_suspect = suspect_orig_indices.contains(&i);

                        let (b_resp, d_resp) = ui
                            .horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = COL_GAP;

                                // Leg number
                                ui.allocate_ui_with_layout(
                                    egui::vec2(NUM_W, 0.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.add_space(INPUT_TOP_PAD);
                                        ui.monospace(format!("{:2}.", i + 1));
                                    },
                                );

                                // Bearing cell: input above, DMS hint below (always rendered)
                                let b_resp = ui
                                    .vertical(|ui| {
                                        let resp = ui
                                            .scope(|ui| {
                                                if !b_valid {
                                                    ui.visuals_mut().extreme_bg_color =
                                                        error_bg(ui);
                                                } else if is_suspect {
                                                    ui.visuals_mut().extreme_bg_color =
                                                        suspect_bg(ui);
                                                }
                                                ui.add(
                                                    egui::TextEdit::singleline(
                                                        &mut self.legs[i].bearing,
                                                    )
                                                    .id(bearing_id)
                                                    .desired_width(BEARING_W)
                                                    .margin(input_margin)
                                                    .hint_text("e.g. 298.0347 or 118.0347*"),
                                                )
                                            })
                                            .inner;
                                        if b_valid && !self.legs[i].bearing_dms_sane() {
                                            ui.label(
                                                egui::RichText::new("MM or SS ≥ 60")
                                                    .color(egui::Color32::from_rgb(230, 160, 40))
                                                    .size(11.0)
                                                    .monospace(),
                                            );
                                        } else {
                                            ui.label(
                                                egui::RichText::new(self.legs[i].bearing_hint())
                                                    .color(egui::Color32::GRAY)
                                                    .size(11.0)
                                                    .monospace(),
                                            );
                                        }
                                        resp
                                    })
                                    .inner;

                                // Distance cell: input above, evaluated-metres hint below
                                let d_resp = ui
                                    .vertical(|ui| {
                                        let resp = ui
                                            .scope(|ui| {
                                                if !d_valid {
                                                    ui.visuals_mut().extreme_bg_color =
                                                        error_bg(ui);
                                                } else if is_suspect {
                                                    ui.visuals_mut().extreme_bg_color =
                                                        suspect_bg(ui);
                                                }
                                                ui.add(
                                                    egui::TextEdit::singleline(
                                                        &mut self.legs[i].distance,
                                                    )
                                                    .id(distance_id)
                                                    .desired_width(DIST_W)
                                                    .margin(input_margin)
                                                    .hint_text("e.g. 100 or 328.084 ft"),
                                                )
                                            })
                                            .inner;
                                        ui.label(
                                            egui::RichText::new(self.legs[i].distance_hint())
                                                .color(egui::Color32::GRAY)
                                                .size(11.0)
                                                .monospace(),
                                        );
                                        resp
                                    })
                                    .inner;

                                if has_coords {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(COORD_W, 0.0),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            ui.add_space(INPUT_TOP_PAD);
                                            if let Some((e, n)) =
                                                leg_coords.get(i).and_then(|c| *c)
                                            {
                                                let coord_color =
                                                    egui::Color32::from_rgb(140, 200, 140);
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "E {:.*}",
                                                        self.settings.coord_decimals, e
                                                    ))
                                                    .monospace()
                                                    .size(11.0)
                                                    .color(coord_color),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "N {:.*}",
                                                        self.settings.coord_decimals, n
                                                    ))
                                                    .monospace()
                                                    .size(11.0)
                                                    .color(coord_color),
                                                );
                                            }
                                        },
                                    );
                                }

                                ui.menu_button("···", |ui| {
                                    if ui.button("Add above").clicked() {
                                        to_insert = Some(i);
                                        ui.close();
                                    }
                                    if ui.button("Add below").clicked() {
                                        to_insert = Some(i + 1);
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(num_legs > 1, egui::Button::new("Delete"))
                                        .clicked()
                                    {
                                        to_remove = Some(i);
                                        ui.close();
                                    }
                                });

                                (b_resp, d_resp)
                            })
                            .inner;

                        if b_resp.lost_focus()
                            && ctx.input(|inp| inp.key_pressed(egui::Key::Enter))
                        {
                            next_focus = Some(distance_id);
                        }
                        if d_resp.lost_focus()
                            && ctx.input(|inp| inp.key_pressed(egui::Key::Enter))
                        {
                            if i + 1 < num_legs {
                                next_focus = Some(egui::Id::new(("bearing", i + 1)));
                            } else {
                                add_leg = true;
                                next_focus = Some(egui::Id::new(("bearing", num_legs)));
                            }
                        }
                    }
                });

            if let Some(i) = to_remove {
                self.legs.remove(i);
            }
            if let Some(i) = to_insert {
                self.legs.insert(i, Leg::default());
                next_focus = Some(egui::Id::new(("bearing", i)));
            }
            if add_leg {
                self.legs.push(Leg::default());
            }
            if let Some(id) = next_focus {
                ctx.memory_mut(|mem| mem.request_focus(id));
            }

            ui.add_space(4.0);
            if ui.button("+ Add Leg").clicked() {
                self.legs.push(Leg::default());
            }
        });

        // Keep the event loop alive so the WM's liveness ping is answered
        // even when the app is idle and unfocused.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
