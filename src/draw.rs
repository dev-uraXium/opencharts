// openCharts — Copyright (C) 2026 uraXium
// SPDX-License-Identifier: AGPL-3.0-only

//! Drawing on charts: pen, highlighter and eraser.
//!
//! Marks are stored per chart in chart coordinates (0..1 across the chart), so
//! they stay in place when you pan or zoom. They're saved to a small JSON file
//! in the user's app-data folder and never touch the chart bundle itself.

use std::collections::BTreeMap;
use std::path::PathBuf;

use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Pen,
    Highlighter,
    Eraser,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Mark {
    /// Points in chart coordinates (0..1 of the chart's width/height).
    pub pts: Vec<[f32; 2]>,
    pub rgba: [u8; 4],
    /// Line width as a fraction of the chart's width.
    pub w: f32,
}

pub const COLORS: [(&str, [u8; 3]); 6] = [
    ("Red", [0xd6, 0x2a, 0x2a]),
    ("Blue", [0x1f, 0x5f, 0xd6]),
    ("Green", [0x1d, 0x9a, 0x4c]),
    ("Magenta", [0xb8, 0x2c, 0xb8]),
    ("Orange", [0xf0, 0x8a, 0x10]),
    ("Black", [0x17, 0x17, 0x17]),
];
pub const SIZES: [(&str, f32); 3] = [("S", 0.0012), ("M", 0.0024), ("L", 0.0048)];
const HISTORY: usize = 200;

pub struct Draw {
    pub on: bool,
    pub tool: Tool,
    pub color: usize,
    pub size: usize,
    notes: BTreeMap<String, Vec<Mark>>,
    undo: Vec<(String, Vec<Mark>)>,
    redo: Vec<(String, Vec<Mark>)>,
    cur: Option<Mark>,
    erasing: bool,
}

fn notes_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }?;
    Some(base.join("openCharts").join("drawings.json"))
}

impl Draw {
    pub fn load() -> Self {
        let notes = notes_path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            on: false,
            tool: Tool::Pen,
            color: 0,
            size: 1,
            notes,
            undo: Vec::new(),
            redo: Vec::new(),
            cur: None,
            erasing: false,
        }
    }

    fn save(&self) {
        let Some(p) = notes_path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec(&self.notes) {
            let tmp = p.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &p);
            }
        }
    }

    /// Records the chart's current marks so the change can be undone, then applies `f`.
    fn edit(&mut self, id: &str, f: impl FnOnce(&mut Vec<Mark>)) {
        let v = self.notes.entry(id.to_string()).or_default();
        self.undo.push((id.to_string(), v.clone()));
        if self.undo.len() > HISTORY {
            self.undo.remove(0);
        }
        self.redo.retain(|(c, _)| c != id);
        f(v);
        if v.is_empty() {
            self.notes.remove(id);
        }
        self.save();
    }

    pub fn has_marks(&self, id: &str) -> bool {
        self.notes.get(id).is_some_and(|v| !v.is_empty())
    }
    pub fn can_undo(&self, id: &str) -> bool {
        self.undo.iter().any(|(c, _)| c == id)
    }
    pub fn can_redo(&self, id: &str) -> bool {
        self.redo.iter().any(|(c, _)| c == id)
    }

    pub fn undo(&mut self, id: &str) {
        if let Some(i) = self.undo.iter().rposition(|(c, _)| c == id) {
            let (_, prev) = self.undo.remove(i);
            let now = self.notes.remove(id).unwrap_or_default();
            self.redo.push((id.to_string(), now));
            if !prev.is_empty() {
                self.notes.insert(id.to_string(), prev);
            }
            self.save();
        }
    }

    pub fn redo(&mut self, id: &str) {
        if let Some(i) = self.redo.iter().rposition(|(c, _)| c == id) {
            let (_, next) = self.redo.remove(i);
            let now = self.notes.remove(id).unwrap_or_default();
            self.undo.push((id.to_string(), now));
            if !next.is_empty() {
                self.notes.insert(id.to_string(), next);
            }
            self.save();
        }
    }

    pub fn clear(&mut self, id: &str) {
        if self.has_marks(id) {
            self.edit(id, |v| v.clear());
        }
    }

    fn new_mark(&self, p: [f32; 2]) -> Mark {
        let [r, g, b] = COLORS[self.color].1;
        let base = SIZES[self.size].1;
        match self.tool {
            Tool::Highlighter => Mark { pts: vec![p], rgba: [r, g, b, 90], w: base * 4.0 },
            _ => Mark { pts: vec![p], rgba: [r, g, b, 255], w: base },
        }
    }

    /// Handles pointer input over the chart while draw mode is on.
    pub fn input(&mut self, id: &str, resp: &egui::Response, img: Rect) {
        if !self.on || img.width() < 1.0 {
            self.cur = None;
            return;
        }
        let to_chart = |p: Pos2| [(p.x - img.min.x) / img.width(), (p.y - img.min.y) / img.height()];
        let primary = egui::PointerButton::Primary;
        let pos = resp.interact_pointer_pos();

        if self.tool == Tool::Eraser {
            if !resp.dragged_by(primary) {
                self.erasing = false;
            }
            if let (true, Some(p)) = (resp.dragged_by(primary) || resp.clicked(), pos) {
                let r = 10.0;
                let hit = |m: &Mark| {
                    let pts: Vec<Pos2> = m.pts.iter().map(|q| to_screen(*q, img)).collect();
                    let half = m.w * img.width() / 2.0;
                    if pts.len() == 1 {
                        return pts[0].distance(p) <= r + half;
                    }
                    pts.windows(2).any(|s| seg_dist(p, s[0], s[1]) <= r + half)
                };
                let any = self.notes.get(id).is_some_and(|v| v.iter().any(&hit));
                if any {
                    if self.erasing {
                        // same eraser stroke: change without a new undo step
                        if let Some(v) = self.notes.get_mut(id) {
                            v.retain(|m| !hit(m));
                            if v.is_empty() {
                                self.notes.remove(id);
                            }
                        }
                        self.save();
                    } else {
                        self.erasing = resp.dragged_by(primary);
                        self.edit(id, |v| v.retain(|m| !hit(m)));
                    }
                }
            }
            return;
        }

        if resp.drag_started_by(primary) {
            let start = resp.ctx.input(|i| i.pointer.press_origin()).or(pos);
            if let Some(o) = start {
                let mut m = self.new_mark(to_chart(o));
                if let Some(p) = pos {
                    m.pts.push(to_chart(p));
                }
                self.cur = Some(m);
            }
        } else if resp.dragged_by(primary) {
            if let (Some(m), Some(p)) = (self.cur.as_mut(), pos) {
                let last = to_screen(*m.pts.last().unwrap(), img);
                if last.distance(p) >= 1.5 {
                    m.pts.push(to_chart(p));
                }
            }
        }
        if resp.drag_stopped_by(primary) {
            if let Some(m) = self.cur.take() {
                self.edit(id, |v| v.push(m));
            }
        }
        if resp.clicked_by(primary) {
            if let Some(p) = pos {
                let m = self.new_mark(to_chart(p));
                self.edit(id, |v| v.push(m));
            }
        }
    }

    pub fn paint(&self, id: &str, painter: &egui::Painter, img: Rect) {
        let marks = self.notes.get(id).into_iter().flatten().chain(self.cur.iter());
        for m in marks {
            let [r, g, b, a] = m.rgba;
            let color = Color32::from_rgba_unmultiplied(r, g, b, a);
            let w = (m.w * img.width()).max(1.0);
            let pts: Vec<Pos2> = m.pts.iter().map(|q| to_screen(*q, img)).collect();
            if pts.len() == 1 {
                painter.circle_filled(pts[0], w / 2.0, color);
            } else {
                painter.add(Shape::line(pts, Stroke::new(w, color)));
            }
        }
    }

    /// Small coloured dot showing where the pen/eraser is, while hovering in draw mode.
    pub fn cursor(&self, ui: &egui::Ui, resp: &egui::Response, img: Rect, painter: &egui::Painter) {
        if !self.on {
            return;
        }
        let Some(p) = resp.hover_pos() else { return };
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        if self.tool == Tool::Eraser {
            painter.circle_stroke(p, 10.0, Stroke::new(1.5, Color32::from_gray(90)));
        } else {
            let [r, g, b] = COLORS[self.color].1;
            let mut w = SIZES[self.size].1 * img.width();
            if self.tool == Tool::Highlighter {
                w *= 4.0;
            }
            painter.circle_stroke(p, (w / 2.0).max(2.0), Stroke::new(1.0, Color32::from_rgb(r, g, b)));
        }
    }
}

fn to_screen(q: [f32; 2], img: Rect) -> Pos2 {
    Pos2::new(img.min.x + q[0] * img.width(), img.min.y + q[1] * img.height())
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 <= f32::EPSILON {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}
