//! WMDS desktop application (Phase 0): load a primitive, edit its parameters, see it in 3D.

mod viewport;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use eframe::egui;
use glam::Vec3;
use wmds_expr::Value;
use wmds_model::{Overrides, ResolvedPort, ResolvedPrimitive};
use wmds_schema::PrimitiveDef;
use wmds_units::Quantity;

use viewport::{Camera, GpuMeshData, ViewportCallback};

const DEPTH_BITS: u8 = 24;

fn main() -> eframe::Result {
    let file = std::env::args().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Wright Motor Design Suite")
            .with_inner_size([1400.0, 900.0]),
        depth_buffer: DEPTH_BITS,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "WMDS",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, file)))),
    )
}

/// Result of a geometry build on the worker thread.
struct BuildResult {
    mesh: Option<Arc<GpuMeshData>>,
    bounds: Option<(Vec3, Vec3)>,
    volume_m3: Option<f64>,
    centroid: Option<[f64; 3]>,
    ports: Vec<ResolvedPort>,
    error: Option<String>,
    elapsed_ms: u128,
    version: u64,
}

/// One editable parameter row.
struct ParamRow {
    name: String,
    unit: String,
    value: f64,
    min: f64,
    max: f64,
    derived: bool,
    doc: String,
}

struct App {
    file: Option<PathBuf>,
    path_text: String,
    def: Option<PrimitiveDef>,
    params: Vec<ParamRow>,
    variants: Vec<(String, Vec<String>, usize)>,
    resolved: Option<ResolvedPrimitive>,
    resolve_error: Option<String>,
    camera: Camera,
    mesh: Option<Arc<GpuMeshData>>,
    ports: Vec<ResolvedPort>,
    volume_m3: Option<f64>,
    centroid: Option<[f64; 3]>,
    build_error: Option<String>,
    build_ms: Option<u128>,
    building: bool,
    dirty: bool,
    build_version: u64,
    rx: Option<Receiver<BuildResult>>,
    show_ports: bool,
    has_renderer: bool,
    framed: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>) -> Self {
        let has_renderer =
            viewport::init(cc, eframe::egui_wgpu::depth_format_from_bits(DEPTH_BITS, 0));
        let mut app = App {
            path_text: file
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            file,
            def: None,
            params: Vec::new(),
            variants: Vec::new(),
            resolved: None,
            resolve_error: None,
            camera: Camera::default(),
            mesh: None,
            ports: Vec::new(),
            volume_m3: None,
            centroid: None,
            build_error: None,
            build_ms: None,
            building: false,
            dirty: false,
            build_version: 0,
            rx: None,
            show_ports: true,
            has_renderer,
            framed: false,
        };
        if app.file.is_some() {
            app.load();
        }
        app
    }

    fn load(&mut self) {
        let Some(path) = self.file.clone() else {
            return;
        };
        self.resolve_error = None;
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                self.resolve_error = Some(format!("cannot read {}: {e}", path.display()));
                return;
            }
        };
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        match wmds_schema::parse_primitive(&name, &src) {
            Ok(def) => {
                self.params = def
                    .params
                    .iter()
                    .map(|p| {
                        let unit = p.unit.clone().unwrap_or_default();
                        let lit = |e: &Option<wmds_expr::Expr>| -> Option<f64> {
                            match e {
                                Some(wmds_expr::Expr::Num(q)) => {
                                    if q.dim.is_dimensionless() {
                                        Some(q.value)
                                    } else {
                                        q.to_unit(&unit).ok()
                                    }
                                }
                                _ => None,
                            }
                        };
                        let value = lit(&p.default).unwrap_or(0.0);
                        let min = lit(&p.min).unwrap_or(if value > 0.0 {
                            value * 0.25
                        } else {
                            value - 1.0
                        });
                        let max = lit(&p.max).unwrap_or(if value > 0.0 {
                            value * 2.0
                        } else {
                            value + 1.0
                        });
                        ParamRow {
                            name: p.name.clone(),
                            unit,
                            value,
                            min,
                            max,
                            derived: p.expr.is_some(),
                            doc: p.doc.clone().unwrap_or_default(),
                        }
                    })
                    .collect();
                self.variants = def
                    .variants
                    .iter()
                    .map(|v| (v.name.clone(), v.options.clone(), 0))
                    .collect();
                self.def = Some(def);
                self.framed = false;
                self.resolve_and_build();
            }
            Err(e) => {
                self.resolve_error = Some(format!("{e:?}"));
                self.def = None;
            }
        }
    }

    fn overrides(&self) -> Overrides {
        let mut o = Overrides::default();
        for p in &self.params {
            if p.derived {
                continue;
            }
            let v = if p.unit.is_empty() {
                Value::Num(Quantity::dimensionless(p.value))
            } else {
                match Quantity::from_unit(p.value, &p.unit) {
                    Ok(q) => Value::Num(q),
                    Err(_) => continue,
                }
            };
            o.params.insert(p.name.clone(), v);
        }
        for (name, options, idx) in &self.variants {
            o.variants.insert(name.clone(), options[*idx].clone());
        }
        o
    }

    fn resolve_and_build(&mut self) {
        let Some(def) = &self.def else { return };
        match wmds_model::resolve(def, &self.overrides()) {
            Ok(r) => {
                // Refresh derived values shown in the panel.
                for row in &mut self.params {
                    if row.derived {
                        if let Some(q) = r.params.get(&row.name).and_then(|v| v.as_quantity()) {
                            row.value = if row.unit.is_empty() {
                                q.value
                            } else {
                                q.to_unit(&row.unit).unwrap_or(q.value)
                            };
                        }
                    }
                }
                self.ports = r.ports.clone();
                self.resolved = Some(r);
                self.resolve_error = None;
                self.dirty = true;
            }
            Err(e) => {
                self.resolve_error = Some(e.to_string());
            }
        }
    }

    fn start_build(&mut self, ctx: &egui::Context) {
        let Some(r) = self.resolved.clone() else {
            return;
        };
        self.build_version += 1;
        let version = self.build_version;
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.building = true;
        self.dirty = false;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let result = build_geometry(&r, version);
            let _ = tx.send(BuildResult {
                elapsed_ms: t0.elapsed().as_millis(),
                ..result
            });
            ctx.request_repaint();
        });
    }

    fn poll_build(&mut self) {
        let Some(rx) = &self.rx else { return };
        if let Ok(res) = rx.try_recv() {
            self.building = false;
            self.rx = None;
            if res.version != self.build_version {
                return;
            }
            self.build_ms = Some(res.elapsed_ms);
            self.build_error = res.error;
            if res.mesh.is_some() {
                self.mesh = res.mesh;
                self.volume_m3 = res.volume_m3;
                self.centroid = res.centroid;
                self.ports = res.ports;
                if let (Some((lo, hi)), false) = (res.bounds, self.framed) {
                    self.camera.frame(lo, hi);
                    self.framed = true;
                }
            }
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Primitive");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.path_text)
                    .desired_width(260.0)
                    .hint_text("path to .prim.kdl"),
            );
            if ui.button("Load").clicked() {
                self.file = Some(PathBuf::from(self.path_text.trim()));
                self.load();
            }
        });
        if let Some(def) = &self.def {
            ui.label(format!("{} v{}", def.id, def.version));
            if !def.description.is_empty() {
                ui.label(egui::RichText::new(&def.description).italics());
            }
            ui.label(format!(
                "{}{}",
                def.category,
                def.sub
                    .as_ref()
                    .map(|s| format!(" / {s}"))
                    .unwrap_or_default()
            ));
            if let Some(m) = &def.material {
                ui.label(format!("material: {m}"));
            }
        }
        if let Some(e) = &self.resolve_error {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 60), e);
        }
        ui.separator();

        let mut changed = false;
        if !self.variants.is_empty() {
            ui.heading("Variants");
            for (name, options, idx) in &mut self.variants {
                egui::ComboBox::from_label(name.as_str())
                    .selected_text(options[*idx].as_str())
                    .show_ui(ui, |ui| {
                        for (i, o) in options.iter().enumerate() {
                            if ui.selectable_value(idx, i, o).changed() {
                                changed = true;
                            }
                        }
                    });
            }
            ui.separator();
        }

        ui.heading("Parameters");
        egui::Grid::new("params")
            .num_columns(3)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for p in &mut self.params {
                    ui.label(&p.name).on_hover_text(&p.doc);
                    if p.derived {
                        ui.label(format!("{:.3} {}", p.value, p.unit));
                        ui.label(egui::RichText::new("derived").weak());
                    } else {
                        let resp = ui.add(
                            egui::Slider::new(&mut p.value, p.min..=p.max)
                                .suffix(format!(" {}", p.unit)),
                        );
                        if resp.changed() {
                            changed = true;
                        }
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        if changed {
            self.resolve_and_build();
        }
        ui.separator();

        ui.heading("Geometry");
        ui.checkbox(&mut self.show_ports, "show ports");
        if self.building {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("building…");
            });
        } else if let Some(ms) = self.build_ms {
            ui.label(format!("built in {ms} ms"));
        }
        if let Some(v) = self.volume_m3 {
            ui.label(format!("volume {:.1} cm³", v * 1e6));
            if let Some(rho) = self
                .def
                .as_ref()
                .and_then(|d| d.material.as_deref())
                .and_then(placeholder_density)
            {
                ui.label(format!(
                    "mass {:.3} kg  (placeholder density {} kg/m³)",
                    v * rho,
                    rho
                ));
            }
        }
        if let Some(c) = self.centroid {
            ui.label(format!(
                "centroid ({:.1}, {:.1}, {:.1}) mm",
                c[0] * 1e3,
                c[1] * 1e3,
                c[2] * 1e3
            ));
        }
        if let Some(e) = &self.build_error {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 60), e);
        }
        if !self.ports.is_empty() {
            ui.separator();
            ui.heading("Ports");
            egui::Grid::new("ports")
                .num_columns(3)
                .spacing([8.0, 2.0])
                .show(ui, |ui| {
                    for p in &self.ports {
                        ui.label(&p.name);
                        ui.label(&p.port_type);
                        ui.label(format!(
                            "({:.0}, {:.0}, {:.0}) mm",
                            p.origin[0].value * 1e3,
                            p.origin[1].value * 1e3,
                            p.origin[2].value * 1e3
                        ));
                        ui.end_row();
                    }
                });
        }
        if !self.has_renderer {
            ui.separator();
            ui.colored_label(
                egui::Color32::YELLOW,
                "no wgpu render state: the viewport is disabled",
            );
        }
        ui.separator();
        ui.label(
            egui::RichText::new("drag: orbit   shift-drag / middle: pan   wheel: zoom   F: frame")
                .weak(),
        );
    }

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        viewport::handle_input(&mut self.camera, ui, &response);
        if ui.input(|i| i.key_pressed(egui::Key::F)) {
            self.framed = false;
            if let Some(m) = &self.mesh {
                let _ = m;
            }
            self.dirty = true;
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(28, 30, 34));
        if self.has_renderer {
            painter.add(eframe::egui_wgpu::Callback::new_paint_callback(
                rect,
                ViewportCallback {
                    mesh: self.mesh.clone(),
                    camera: self.camera,
                    aspect: rect.aspect_ratio(),
                    color: [0.72, 0.76, 0.82, 1.0],
                },
            ));
        }
        // Axis triad and port overlay drawn with the 2D painter on top of the 3D view.
        let origin = Vec3::ZERO;
        let axis_len = self.camera.distance * 0.08;
        for (dir, color, label) in [
            (Vec3::X, egui::Color32::from_rgb(230, 80, 80), "x"),
            (Vec3::Y, egui::Color32::from_rgb(80, 200, 80), "y"),
            (Vec3::Z, egui::Color32::from_rgb(90, 140, 255), "z"),
        ] {
            if let (Some(a), Some(b)) = (
                self.camera.project(rect, origin),
                self.camera.project(rect, origin + dir * axis_len),
            ) {
                painter.line_segment([a, b], egui::Stroke::new(1.5, color));
                painter.text(
                    b,
                    egui::Align2::LEFT_BOTTOM,
                    label,
                    egui::FontId::monospace(11.0),
                    color,
                );
            }
        }
        if self.show_ports {
            let port_len = self.camera.distance * 0.05;
            for p in &self.ports {
                let o = Vec3::new(
                    p.origin[0].value as f32,
                    p.origin[1].value as f32,
                    p.origin[2].value as f32,
                );
                let a = Vec3::new(p.axis[0] as f32, p.axis[1] as f32, p.axis[2] as f32);
                if let (Some(s0), Some(s1)) = (
                    self.camera.project(rect, o),
                    self.camera.project(rect, o + a * port_len),
                ) {
                    let col = egui::Color32::from_rgb(255, 200, 60);
                    painter.circle(s0, 4.0, col, egui::Stroke::new(1.0, egui::Color32::BLACK));
                    painter.line_segment([s0, s1], egui::Stroke::new(2.0, col));
                    painter.text(
                        s0 + egui::vec2(6.0, -6.0),
                        egui::Align2::LEFT_BOTTOM,
                        &p.name,
                        egui::FontId::proportional(12.0),
                        col,
                    );
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_build();
        if self.dirty && !self.building {
            let ctx = ui.ctx().clone();
            self.start_build(&ctx);
        }
        egui::Panel::left("side").resizable(true).show(ui, |ui| {
            ui.set_min_width(360.0);
            egui::ScrollArea::vertical().show(ui, |ui| self.side_panel(ui));
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.viewport(ui));
    }
}

fn placeholder_density(material: &str) -> Option<f64> {
    let m = material.to_ascii_lowercase();
    if m.starts_with("steel") {
        Some(7850.0)
    } else if m.starts_with("alu") {
        Some(2700.0)
    } else if m.starts_with("cfrp") {
        Some(1550.0)
    } else if m.starts_with("gfrp") {
        Some(1900.0)
    } else {
        None
    }
}

#[cfg(feature = "occt")]
fn build_geometry(r: &ResolvedPrimitive, version: u64) -> BuildResult {
    use wmds_geom::GeomKernel;
    let k = wmds_geom_occt::OcctKernel;
    let empty = BuildResult {
        mesh: None,
        bounds: None,
        volume_m3: None,
        centroid: None,
        ports: r.ports.clone(),
        error: None,
        elapsed_ms: 0,
        version,
    };
    let built = match wmds_geom::build_primitive(&k, r) {
        Ok(b) => b,
        Err(e) => {
            return BuildResult {
                error: Some(e.to_string()),
                ..empty
            };
        }
    };
    let Some((_, solid)) = built.best() else {
        return BuildResult {
            error: Some("no geometry level built".into()),
            ..empty
        };
    };
    let mesh = match k.tessellate(solid, 2e-4) {
        Ok(m) => m,
        Err(e) => {
            return BuildResult {
                error: Some(e.to_string()),
                ..empty
            };
        }
    };
    let mp = mesh.mass_props();
    let bounds = mesh.bounds().map(|(lo, hi)| {
        (
            Vec3::new(lo[0] as f32, lo[1] as f32, lo[2] as f32),
            Vec3::new(hi[0] as f32, hi[1] as f32, hi[2] as f32),
        )
    });
    BuildResult {
        mesh: Some(Arc::new(GpuMeshData::from_mesh(&mesh, version))),
        bounds,
        volume_m3: Some(mp.volume),
        centroid: Some(mp.centroid),
        ..empty
    }
}

#[cfg(not(feature = "occt"))]
fn build_geometry(r: &ResolvedPrimitive, version: u64) -> BuildResult {
    BuildResult {
        mesh: None,
        bounds: None,
        volume_m3: None,
        centroid: None,
        ports: r.ports.clone(),
        error: Some("built without the `occt` feature: no geometry kernel".into()),
        elapsed_ms: 0,
        version,
    }
}
