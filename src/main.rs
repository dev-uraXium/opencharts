// openCharts for PC — Copyright (C) 2026 uraXium
// SPDX-License-Identifier: AGPL-3.0-only
#![cfg_attr(windows, windows_subsystem = "windows")]

mod bundle;
mod render;
mod update;

use std::collections::BTreeMap;
use std::sync::Arc;

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Vec2};

use bundle::{Airport, Bundle, ChartRef};
use render::{Done, Job, Renderer};

const ACCENT: Color32 = Color32::from_rgb(0x2f, 0x7d, 0xd1);

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Ground,
    Sids,
    Stars,
    Info,
}

struct Shown {
    id: String,
    scale: f32,
    tex: egui::TextureHandle,
    natural: [f32; 2],
}

struct App {
    bundle: Arc<Bundle>,
    renderer: Renderer,
    search: String,
    airport: Option<String>,
    tab: Tab,
    chart: Option<String>,
    shown: Option<Shown>,
    want: Option<(String, f32)>,
    seq: u64,
    error: Option<String>,
    zoom: f32,
    pan: Vec2,
    status: String,
    about: bool,
    logo: egui::TextureHandle,
    upd_rx: Option<std::sync::mpsc::Receiver<update::Msg>>,
    upd: UpdState,
    upd_manual: bool,
}

#[derive(Clone)]
enum UpdState {
    Idle,
    Checking,
    UpToDate,
    Available(update::Release),
    Downloading(update::Release, f32),
    Done,
    Error(String),
    Dismissed,
}

trait TapSort {
    fn tap_sort(self) -> Self;
}
impl TapSort for Vec<ChartRef> {
    fn tap_sort(mut self) -> Self {
        self.sort_by_key(|c| (c.rw.clone().unwrap_or_default(), c.label != "All"));
        self
    }
}

static LOGO_SVG: &[u8] = include_bytes!("../assets/logo.svg");

/// Renders the openCharts logo to straight RGBA at `size`×`size`.
fn logo_rgba(size: u32) -> Vec<u8> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(LOGO_SVG, &usvg::Options::default()).expect("logo");
    let mut pm = tiny_skia::Pixmap::new(size, size).expect("pixmap");
    let s = size as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(s, s), &mut pm.as_mut());
    // tiny-skia is premultiplied; egui/winit want straight alpha
    pm.pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

const STEPS: [f32; 6] = [0.75, 1.0, 1.5, 2.0, 3.0, 4.0];

impl App {
    fn new(cc: &eframe::CreationContext, bundle: Arc<Bundle>) -> Self {
        let renderer = Renderer::new(bundle.clone(), cc.egui_ctx.clone());
        let logo = cc.egui_ctx.load_texture(
            "logo",
            egui::ColorImage::from_rgba_unmultiplied([128, 128], &logo_rgba(128)),
            egui::TextureOptions::LINEAR,
        );
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.visuals.selection.bg_fill = ACCENT;
        style.spacing.item_spacing = egui::vec2(6.0, 5.0);
        cc.egui_ctx.set_global_style(style);
        let mut a = Self {
            bundle,
            renderer,
            search: String::new(),
            airport: None,
            tab: Tab::Ground,
            chart: None,
            shown: None,
            want: None,
            seq: 0,
            error: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            status: String::new(),
            about: false,
            logo,
            upd_rx: None,
            upd: UpdState::Checking,
            upd_manual: false,
        };
        update::cleanup();
        a.upd_rx = Some(update::check(cc.egui_ctx.clone()));
        a.open_chart("area-world".into());
        a
    }

    fn chart_ref(&self, id: &str) -> Option<&ChartRef> {
        self.bundle.charts.iter().find(|c| c.id == id)
    }

    fn airport_ref(&self, icao: &str) -> Option<&Airport> {
        self.bundle.meta.airports.iter().find(|a| a.icao == icao)
    }

    fn open_chart(&mut self, id: String) {
        if self.chart.as_deref() != Some(id.as_str()) {
            self.chart = Some(id);
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
            self.error = None;
        }
    }

    fn open_airport(&mut self, icao: &str) {
        self.airport = Some(icao.to_string());
        let ground = format!("{}-ground", icao.to_lowercase());
        if self.chart_ref(&ground).is_some() {
            self.tab = Tab::Ground;
            self.open_chart(ground);
        } else {
            self.tab = Tab::Info;
        }
    }

    fn open_area(&mut self, region: &str) {
        self.airport = None;
        self.open_chart(format!("area-{region}"));
    }

    fn charts_for(&self, icao: &str, kind: &str) -> Vec<ChartRef> {
        self.bundle
            .charts
            .iter()
            .filter(|c| c.kind == kind && c.icao.as_deref() == Some(icao))
            .cloned()
            .collect::<Vec<_>>()
            .tap_sort()
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(d) = self.renderer.rx.try_recv() {
            match d {
                Done::View { id, scale, seq, size, rgba, natural } => {
                    if seq != self.seq {
                        continue;
                    }
                    let img = egui::ColorImage::from_rgba_premultiplied(size, &rgba);
                    let opts = egui::TextureOptions {
                        magnification: egui::TextureFilter::Linear,
                        minification: egui::TextureFilter::Linear,
                        wrap_mode: egui::TextureWrapMode::ClampToEdge,
                        mipmap_mode: Some(egui::TextureFilter::Linear),
                    };
                    let tex = ctx.load_texture(format!("chart-{id}"), img, opts);
                    self.shown = Some(Shown { id, scale, tex, natural });
                    self.want = None;
                }
                Done::Failed { id, seq, err } => {
                    if seq == self.seq {
                        self.error = Some(format!("Couldn't open {id}: {err}"));
                        self.want = None;
                    }
                }
                Done::Exported(r) => {
                    self.status = match r {
                        Ok(p) => format!("Saved {}", p.display()),
                        Err(e) => format!("Export failed: {e}"),
                    };
                }
            }
        }
    }

    /// Ask the renderer for a sharper/softer copy when the zoom changes enough.
    fn request(&mut self, id: &str, scale: f32) {
        if let Some(s) = &self.shown {
            if s.id == id && (s.scale - scale).abs() < 0.01 {
                return;
            }
        }
        if let Some((wid, ws)) = &self.want {
            if wid == id && (ws - scale).abs() < 0.01 {
                return;
            }
        }
        self.seq += 1;
        self.want = Some((id.to_string(), scale));
        self.renderer.send(Job::View { id: id.to_string(), scale, seq: self.seq });
    }

    fn export(&mut self) {
        let Some(id) = self.chart.clone() else { return };
        let name = format!("opencharts-{id}.png");
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Export chart as PNG")
            .set_file_name(name)
            .add_filter("PNG image", &["png"])
            .save_file()
        {
            self.status = "Exporting…".into();
            self.renderer.send(Job::Export { id, scale: 3.0, path });
        }
    }

    fn poll_update(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.upd_rx else { return };
        while let Ok(m) = rx.try_recv() {
            self.upd = match (m, &self.upd) {
                (update::Msg::Available(r), _) => UpdState::Available(r),
                (update::Msg::UpToDate, _) => UpdState::UpToDate,
                (update::Msg::Progress(p), UpdState::Downloading(r, _)) => UpdState::Downloading(r.clone(), p),
                (update::Msg::Progress(_), s) => s.clone(),
                (update::Msg::Installed, _) => UpdState::Done,
                (update::Msg::Failed(e), _) => UpdState::Error(e),
            };
        }
        if matches!(self.upd, UpdState::Done) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn update_banner(&mut self, ui: &mut egui::Ui) {
        let show = match &self.upd {
            UpdState::Available(_) | UpdState::Downloading(..) => true,
            UpdState::Error(_) => self.upd_manual,
            _ => false,
        };
        if !show {
            return;
        }
        egui::Frame::NONE
            .fill(Color32::from_rgb(0x1b, 0x4f, 0x8a))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| match self.upd.clone() {
                    UpdState::Available(r) => {
                        ui.label(RichText::new(format!("openCharts {} is available", r.version)).strong().color(Color32::WHITE));
                        ui.label(RichText::new(format!("(you have {})", env!("CARGO_PKG_VERSION"))).color(Color32::from_gray(200)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Later").clicked() {
                                self.upd = UpdState::Dismissed;
                            }
                            if ui.button("What's new").clicked() {
                                update::open_url(&r.page);
                            }
                            if ui.button(RichText::new("Update now").strong()).clicked() {
                                self.upd = UpdState::Downloading(r.clone(), 0.0);
                                self.upd_rx = Some(update::install(r, ui.ctx().clone()));
                            }
                        });
                    }
                    UpdState::Downloading(r, p) => {
                        ui.label(RichText::new(format!("Downloading openCharts {}…", r.version)).strong().color(Color32::WHITE));
                        ui.add(egui::ProgressBar::new(p).desired_width(260.0).show_percentage());
                        ui.label(RichText::new("The app restarts when it's done").color(Color32::from_gray(200)));
                    }
                    UpdState::Error(e) => {
                        ui.label(RichText::new(format!("Update failed: {e}")).color(Color32::WHITE));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                self.upd = UpdState::Dismissed;
                            }
                            if ui.button("Download from GitHub").clicked() {
                                update::open_url(update::RELEASES_URL);
                            }
                        });
                    }
                    _ => {}
                });
            });
    }

    // ───────────────────────────── sidebar ─────────────────────────────
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::splat(30.0)));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("openCharts").size(19.0).strong());
                ui.label(RichText::new("for Project Flight").size(10.5).color(ACCENT));
            });
        });
        ui.label(
            RichText::new(format!("Project Flight charts · AIRAC {}", self.bundle.meta.airac))
                .small()
                .weak(),
        );
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search airport, ICAO or procedure")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(4.0);
        ui.separator();

        let q = norm(&self.search);
        let mut click_airport: Option<String> = None;
        let mut click_area: Option<String> = None;
        let mut click_proc: Option<(String, String, Tab)> = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if !q.is_empty() {
                let mut any = false;
                for a in &self.bundle.meta.airports {
                    if norm(&a.icao).contains(&q) || norm(&a.name).contains(&q) {
                        any = true;
                        if ui
                            .selectable_label(
                                self.airport.as_deref() == Some(&a.icao),
                                format!("{}   {}", a.icao, a.name),
                            )
                            .clicked()
                        {
                            click_airport = Some(a.icao.clone());
                        }
                    }
                }
                let procs: Vec<&ChartRef> = self
                    .bundle
                    .charts
                    .iter()
                    .filter(|c| {
                        (c.kind == "SID" || c.kind == "STAR")
                            && c.label != "All"
                            && (norm(&c.label).contains(&q)
                                || c.title.as_deref().map_or(false, |t| norm(t).contains(&q)))
                    })
                    .take(60)
                    .collect();
                if !procs.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new("PROCEDURES").small().weak());
                    for c in procs {
                        any = true;
                        let icao = c.icao.clone().unwrap_or_default();
                        let txt = format!(
                            "{}  {} · {} RWY {}",
                            c.label,
                            icao,
                            c.kind,
                            c.rw.clone().unwrap_or_default()
                        );
                        if ui
                            .selectable_label(self.chart.as_deref() == Some(&c.id), txt)
                            .clicked()
                        {
                            let tab = if c.kind == "SID" { Tab::Sids } else { Tab::Stars };
                            click_proc = Some((icao, c.id.clone(), tab));
                        }
                    }
                }
                if !any {
                    ui.label(RichText::new("No matches").weak());
                }
            } else {
                if ui
                    .selectable_label(
                        self.chart.as_deref() == Some("area-world") && self.airport.is_none(),
                        "All of Project Flight",
                    )
                    .clicked()
                {
                    click_area = Some("world".into());
                }
                ui.add_space(2.0);
                for r in self.bundle.meta.regions.iter().filter(|r| r.id != "world") {
                    let airports: Vec<&Airport> = self
                        .bundle
                        .meta
                        .airports
                        .iter()
                        .filter(|a| a.region == r.id)
                        .collect();
                    egui::CollapsingHeader::new(RichText::new(&r.name).strong())
                        .id_salt(&r.id)
                        .default_open(true)
                        .show(ui, |ui| {
                            let area = format!("area-{}", r.id);
                            if self.bundle.charts.iter().any(|c| c.id == area)
                                && ui
                                    .selectable_label(
                                        self.chart.as_deref() == Some(&area) && self.airport.is_none(),
                                        "Area chart",
                                    )
                                    .clicked()
                            {
                                click_area = Some(r.id.clone());
                            }
                            for a in airports {
                                let sel = self.airport.as_deref() == Some(&a.icao);
                                let txt = RichText::new(format!("{}   {}", a.icao, a.name));
                                if ui.selectable_label(sel, txt).clicked() {
                                    click_airport = Some(a.icao.clone());
                                }
                            }
                        });
                }
            }
        });

        if let Some(i) = click_airport {
            self.open_airport(&i);
        }
        if let Some(r) = click_area {
            self.open_area(&r);
        }
        if let Some((icao, id, tab)) = click_proc {
            self.airport = Some(icao);
            self.tab = tab;
            self.open_chart(id);
        }
    }

    // ───────────────────────────── top bar ─────────────────────────────
    fn header(&mut self, ui: &mut egui::Ui) {
        let title = match self.airport.as_deref().and_then(|i| self.airport_ref(i)) {
            Some(a) => format!("{} — {}", a.icao, a.name),
            None => self
                .chart
                .as_deref()
                .and_then(|id| self.chart_ref(id))
                .and_then(|c| c.title.clone())
                .unwrap_or_else(|| "openCharts".into()),
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).size(17.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("About").clicked() {
                    self.about = true;
                }
                let can_export = self.chart.is_some() && self.tab != Tab::Info || self.airport.is_none();
                if ui
                    .add_enabled(can_export && self.chart.is_some(), egui::Button::new("Export PNG"))
                    .clicked()
                {
                    self.export();
                }
                if ui.button("Fit").clicked() {
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                }
                if ui.button(" + ").clicked() {
                    self.zoom = (self.zoom * 1.25).min(8.0);
                }
                if ui.button(" - ").clicked() {
                    self.zoom = (self.zoom / 1.25).max(0.5);
                }
                ui.label(RichText::new(format!("{:.0}%", self.zoom * 100.0)).weak());
            });
        });

        let Some(icao) = self.airport.clone() else { return };
        ui.add_space(4.0);
        let has_ground = self.chart_ref(&format!("{}-ground", icao.to_lowercase())).is_some();
        let sids = self.charts_for(&icao, "SID");
        let stars = self.charts_for(&icao, "STAR");
        ui.horizontal(|ui| {
            let mut tab = self.tab;
            if has_ground {
                ui.selectable_value(&mut tab, Tab::Ground, "Ground");
            }
            if !sids.is_empty() {
                ui.selectable_value(&mut tab, Tab::Sids, format!("SIDs ({})", sids.iter().filter(|c| c.label != "All").count()));
            }
            if !stars.is_empty() {
                ui.selectable_value(&mut tab, Tab::Stars, format!("STARs ({})", stars.iter().filter(|c| c.label != "All").count()));
            }
            ui.selectable_value(&mut tab, Tab::Info, "Info");
            if tab != self.tab {
                self.tab = tab;
                match tab {
                    Tab::Ground => self.open_chart(format!("{}-ground", icao.to_lowercase())),
                    Tab::Sids => {
                        if let Some(c) = sids.first() {
                            self.open_chart(c.id.clone())
                        }
                    }
                    Tab::Stars => {
                        if let Some(c) = stars.first() {
                            self.open_chart(c.id.clone())
                        }
                    }
                    Tab::Info => {}
                }
            }
        });

        let list = match self.tab {
            Tab::Sids => sids,
            Tab::Stars => stars,
            _ => return,
        };
        let mut by_rw: BTreeMap<String, Vec<ChartRef>> = BTreeMap::new();
        for c in list {
            by_rw.entry(c.rw.clone().unwrap_or_else(|| "—".into())).or_default().push(c);
        }
        ui.add_space(2.0);
        let mut pick = None;
        egui::ScrollArea::vertical().max_height(92.0).id_salt("chips").show(ui, |ui| {
            for (rw, cs) in &by_rw {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("RWY {rw}")).strong().color(ACCENT));
                    for c in cs {
                        let label = if c.label == "All" { "All (overview)".to_string() } else { c.label.clone() };
                        if ui.selectable_label(self.chart.as_deref() == Some(&c.id), label).clicked() {
                            pick = Some(c.id.clone());
                        }
                    }
                });
            }
        });
        if let Some(id) = pick {
            self.open_chart(id);
        }
    }

    // ───────────────────────────── info tab ─────────────────────────────
    fn info(&self, ui: &mut egui::Ui, a: &Airport) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(6.0);
            egui::Grid::new("ainfo").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
                ui.label(RichText::new("ICAO").weak());
                ui.label(RichText::new(&a.icao).strong());
                ui.end_row();
                ui.label(RichText::new("Name").weak());
                ui.label(&a.name);
                ui.end_row();
                if let Some(e) = a.elev_ft {
                    ui.label(RichText::new("Elevation").weak());
                    ui.label(format!("{e:.0} ft"));
                    ui.end_row();
                }
                if !a.freqs.is_empty() {
                    ui.label(RichText::new("Frequencies").weak());
                    ui.label(&a.freqs);
                    ui.end_row();
                }
                if let Some(p) = &a.proc_src {
                    ui.label(RichText::new("Procedures").weak());
                    ui.label(if p == "community" {
                        "Community-designed for Project Flight (not real-world)".to_string()
                    } else {
                        p.clone()
                    });
                    ui.end_row();
                }
            });
            ui.add_space(12.0);
            ui.label(RichText::new("RUNWAYS").small().strong().color(ACCENT));
            egui::Grid::new("rwys").striped(true).spacing([24.0, 4.0]).show(ui, |ui| {
                for h in ["RWY", "Course", "Length", "Width", "Threshold elev"] {
                    ui.label(RichText::new(h).weak());
                }
                ui.end_row();
                for r in &a.runways {
                    ui.label(RichText::new(&r.ident).strong());
                    ui.label(format!("{}°", r.course));
                    ui.label(r.len_ft.map(|v| format!("{v:.0} ft")).unwrap_or("—".into()));
                    ui.label(r.wid_ft.map(|v| format!("{v:.0} ft")).unwrap_or("—".into()));
                    ui.label(r.thr_ft.map(|v| format!("{v:.0} ft")).unwrap_or("—".into()));
                    ui.end_row();
                }
            });
            for kind in ["SID", "STAR"] {
                let ps: Vec<_> = a.procs.iter().filter(|p| p.kind == kind).collect();
                if ps.is_empty() {
                    continue;
                }
                ui.add_space(12.0);
                ui.label(RichText::new(format!("{kind}S")).small().strong().color(ACCENT));
                egui::Grid::new(kind).striped(true).spacing([18.0, 4.0]).show(ui, |ui| {
                    for p in ps {
                        ui.label(RichText::new(&p.name).strong().monospace());
                        ui.label(format!("RWY {}", p.rw));
                        ui.label(&p.route);
                        ui.end_row();
                    }
                });
            }
            ui.add_space(16.0);
            ui.label(
                RichText::new("For Project Flight (Roblox) simulation only — not for real-world navigation.")
                    .small()
                    .weak(),
            );
        });
    }

    // ───────────────────────────── chart view ─────────────────────────────
    fn viewer(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.chart.clone() else { return };
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(38));

        let ppp = ui.ctx().pixels_per_point();
        let natural = match &self.shown {
            Some(s) if s.id == id => Some(s.natural),
            _ => None,
        };

        if let Some(nat) = natural {
            let pad = 16.0;
            let fit = ((rect.width() - pad * 2.0) / nat[0])
                .min((rect.height() - pad * 2.0) / nat[1])
                .max(0.05);

            // mouse wheel / pinch zoom around the cursor
            if resp.hovered() {
                let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
                let mut f = pinch;
                if scroll.abs() > 0.0 {
                    f *= (scroll * 0.0015).exp();
                }
                if (f - 1.0).abs() > 1e-4 {
                    let new_zoom = (self.zoom * f).clamp(0.5, 8.0);
                    let f = new_zoom / self.zoom;
                    if let Some(p) = resp.hover_pos() {
                        let size = Vec2::new(nat[0], nat[1]) * fit * self.zoom;
                        let origin = rect.center() - size / 2.0 + self.pan;
                        let new_origin = p - (p - origin) * f;
                        let new_size = size * f;
                        self.pan = new_origin - (rect.center() - new_size / 2.0);
                    }
                    self.zoom = new_zoom;
                }
            }
            if resp.dragged() {
                self.pan += resp.drag_delta();
            }
            if resp.double_clicked() {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }

            let scale = fit * self.zoom;
            let size = Vec2::new(nat[0], nat[1]) * scale;
            let img_rect = Rect::from_min_size(rect.center() - size / 2.0 + self.pan, size);

            // pick a render resolution matching the on-screen size
            let need = scale * ppp;
            let step = STEPS.iter().copied().find(|s| *s >= need * 0.95).unwrap_or(4.0);
            self.request(&id, step);

            if let Some(s) = &self.shown {
                painter.rect_filled(img_rect.expand(1.0), 0.0, Color32::from_gray(20));
                painter.image(
                    s.tex.id(),
                    img_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            if resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
        } else {
            self.request(&id, 1.5);
            // keep showing the previous chart dimmed while the next one loads
            if let Some(s) = &self.shown {
                let nat = s.natural;
                let fit = ((rect.width() - 32.0) / nat[0]).min((rect.height() - 32.0) / nat[1]);
                let size = Vec2::new(nat[0], nat[1]) * fit;
                let r = Rect::from_center_size(rect.center(), size);
                painter.image(
                    s.tex.id(),
                    r,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::from_white_alpha(70),
                );
            }
        }

        if self.want.is_some() {
            painter.text(
                rect.right_bottom() - Vec2::new(12.0, 10.0),
                egui::Align2::RIGHT_BOTTOM,
                "rendering…",
                egui::FontId::proportional(12.0),
                Color32::from_gray(170),
            );
            ui.ctx().request_repaint();
        }
        if let Some(e) = &self.error {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                e,
                egui::FontId::proportional(15.0),
                Color32::from_rgb(240, 120, 110),
            );
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.about;
        egui::Window::new("About openCharts")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::splat(56.0)));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("openCharts").size(20.0).strong());
                        ui.label(format!("Version {}  ·  Charts for Project Flight", env!("CARGO_PKG_VERSION")));
                        ui.label(RichText::new("A Coral Reef Studios product").small().color(ACCENT));
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let busy = matches!(self.upd, UpdState::Checking | UpdState::Downloading(..));
                    if ui.add_enabled(!busy, egui::Button::new("Check for updates")).clicked() {
                        self.upd_manual = true;
                        self.upd = UpdState::Checking;
                        self.upd_rx = Some(update::check(ui.ctx().clone()));
                    }
                    let txt = match &self.upd {
                        UpdState::Checking => "Checking…".to_string(),
                        UpdState::UpToDate => "You have the latest version.".to_string(),
                        UpdState::Available(r) | UpdState::Downloading(r, _) => format!("Version {} is available", r.version),
                        UpdState::Error(_) => "Couldn't reach GitHub.".to_string(),
                        _ => String::new(),
                    };
                    ui.label(RichText::new(txt).weak());
                });
                if ui.link("Releases on GitHub").clicked() {
                    update::open_url(update::RELEASES_URL);
                }
                ui.add_space(8.0);
                ui.label(RichText::new("© 2026 uraXium. All rights reserved.").strong());
                ui.label(
                    "Charts may not be modified or redistributed without prior permission from uraXium.",
                );
                ui.add_space(8.0);
                ui.label(
                    "Unofficial charts for the Roblox game Project Flight. Not affiliated with Project Flight \
                     or Roblox. For simulation only — never use for real-world navigation.",
                );
                ui.add_space(8.0);
                ui.label(RichText::new("Credits").strong());
                ui.label("Some ground layouts: © OpenStreetMap contributors (ODbL 1.0).");
                ui.label("Fonts: Liberation Sans (SIL Open Font License 1.1).");
                ui.label("Built with Rust, egui and resvg.");
            });
        self.about = open;
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        self.poll_update(&ctx);
        if matches!(self.upd, UpdState::Available(_) | UpdState::Downloading(..)) || (self.upd_manual && matches!(self.upd, UpdState::Error(_))) {
            egui::Panel::top("update").frame(egui::Frame::NONE).show(ui, |ui| self.update_banner(ui));
        }

        egui::Panel::left("side")
            .resizable(true)
            .default_size(270.0)
            .show(ui, |ui| self.sidebar(ui));

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("© 2026 uraXium · not for real-world navigation").small().weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&self.status).small());
                    if self.tab != Tab::Info || self.airport.is_none() {
                        ui.label(RichText::new("Drag to pan · scroll to zoom · double-click to fit").small().weak());
                    }
                });
            });
        });

        egui::Panel::top("head").show(ui, |ui| {
            ui.add_space(4.0);
            self.header(ui);
            ui.add_space(4.0);
        });

        egui::CentralPanel::no_frame().show(ui, |ui| {
            let info_airport = if self.tab == Tab::Info {
                self.airport.as_deref().and_then(|i| self.airport_ref(i)).cloned()
            } else {
                None
            };
            match info_airport {
                Some(a) => {
                    let fill = ui.visuals().panel_fill;
                    egui::Frame::NONE.fill(fill).inner_margin(14.0).show(ui, |ui| self.info(ui, &a));
                }
                None => self.viewer(ui),
            }
        });

        self.about_window(&ctx);
    }
}

fn main() -> eframe::Result {
    let bundle = match Bundle::load() {
        Ok(b) => Arc::new(b),
        Err(e) => return show_error(e),
    };
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("openCharts")
            .with_app_id("opencharts")
            .with_icon(Arc::new(egui::IconData { rgba: logo_rgba(256), width: 256, height: 256 }))
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([820.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "openCharts",
        opts,
        Box::new(move |cc| Ok(Box::new(App::new(cc, bundle)))),
    )
}

/// Minimal window shown when the chart bundle can't be opened.
fn show_error(msg: String) -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("openCharts")
            .with_inner_size([520.0, 220.0]),
        ..Default::default()
    };
    eframe::run_ui_native("openCharts", opts, move |ui, _| {
        egui::CentralPanel::default_margins().show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("openCharts can't load its charts").size(17.0).strong());
            ui.add_space(8.0);
            ui.label(&msg);
        });
    })
}

/// Uppercases and drops spaces/punctuation so "bogna1x", "BOGNA 1X" and "bogna-1x" all match.
fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_uppercase()).collect()
}
