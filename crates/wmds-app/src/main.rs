//! WMDS desktop application.
//!
//! Open a primitive to edit its parameters and watch the geometry rebuild, or open a vehicle or
//! assembly to see the whole thing placed by its mate graph.

mod viewport;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use eframe::egui;
use glam::Vec3;
use wmds_expr::Value;
use wmds_geom::GeomKernel;
use wmds_model::{Library, Overrides, PlacedBy, ResolvedAssembly, ResolvedPort, ResolvedPrimitive, resolve};
use wmds_schema::PrimitiveDef;
use wmds_units::Quantity;

use viewport::{Camera, GpuMeshData, ViewportCallback};

const DEPTH_BITS: u8 = 24;

#[cfg(feature = "occt")]
type Kernel = wmds_geom_occt::OcctKernel;
#[cfg(not(feature = "occt"))]
type Kernel = wmds_geom::MeshKernel;

#[cfg(feature = "occt")]
const KERNEL_NAME: &str = "OpenCASCADE";
#[cfg(not(feature = "occt"))]
const KERNEL_NAME: &str = "mesh kernel (preview)";

fn main() -> eframe::Result {
    // Usage: wmds-app [file] [--project DIR] [--screenshot out.png]
    let mut file: Option<PathBuf> = None;
    let mut screenshot: Option<PathBuf> = None;
    let mut project = PathBuf::from(".");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => screenshot = args.next().map(PathBuf::from),
            "--project" => {
                if let Some(p) = args.next() {
                    project = PathBuf::from(p);
                }
            }
            _ => file = Some(PathBuf::from(a)),
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Wright Motor Design Suite")
            .with_inner_size([1500.0, 950.0]),
        depth_buffer: DEPTH_BITS,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "WMDS",
        options,
        Box::new(move |cc| {
            let mut app = App::new(cc, file, project);
            app.screenshot = screenshot;
            Ok(Box::new(app))
        }),
    )
}

/// What is open.
enum Doc {
    None,
    Primitive(Box<PrimitiveDef>),
    Assembly(Box<wmds_schema::AssemblyDef>),
}

/// A row in the parts list.
struct PartRow {
    id: String,
    source: String,
    position: [f64; 3],
    how: String,
    mass: Option<f64>,
    declared: bool,
}

struct BuildResult {
    mesh: Option<Arc<GpuMeshData>>,
    bounds: Option<(Vec3, Vec3)>,
    ports: Vec<(String, [f64; 3], [f64; 3])>,
    parts: Vec<PartRow>,
    total_mass: f64,
    point_mass: f64,
    cg: [f64; 3],
    volume_m3: Option<f64>,
    error: Option<String>,
    elapsed_ms: u128,
    version: u64,
}

impl BuildResult {
    fn empty(version: u64) -> BuildResult {
        BuildResult {
            mesh: None,
            bounds: None,
            ports: Vec::new(),
            parts: Vec::new(),
            total_mass: 0.0,
            point_mass: 0.0,
            cg: [0.0; 3],
            volume_m3: None,
            error: None,
            elapsed_ms: 0,
            version,
        }
    }
}

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
    project: PathBuf,
    file: Option<PathBuf>,
    path_text: String,
    doc: Doc,
    lib: Option<Library>,
    lib_note: String,
    params: Vec<ParamRow>,
    variants: Vec<(String, Vec<String>, usize)>,
    load_error: Option<String>,

    camera: Camera,
    mesh: Option<Arc<GpuMeshData>>,
    ports: Vec<(String, [f64; 3], [f64; 3])>,
    parts: Vec<PartRow>,
    total_mass: f64,
    point_mass: f64,
    cg: [f64; 3],
    volume_m3: Option<f64>,
    build_error: Option<String>,
    build_ms: Option<u128>,
    building: bool,
    dirty: bool,
    build_version: u64,
    rx: Option<Receiver<BuildResult>>,

    show_ports: bool,
    show_cg: bool,
    has_renderer: bool,
    framed: bool,
    screenshot: Option<PathBuf>,
    frames_since_build: u32,
    screenshot_requested: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>, project: PathBuf) -> Self {
        let has_renderer = viewport::init(cc, eframe::egui_wgpu::depth_format_from_bits(DEPTH_BITS, 0));
        let mut app = App {
            project,
            path_text: file.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            file,
            doc: Doc::None,
            lib: None,
            lib_note: String::new(),
            params: Vec::new(),
            variants: Vec::new(),
            load_error: None,
            camera: Camera::default(),
            mesh: None,
            ports: Vec::new(),
            parts: Vec::new(),
            total_mass: 0.0,
            point_mass: 0.0,
            cg: [0.0; 3],
            volume_m3: None,
            build_error: None,
            build_ms: None,
            building: false,
            dirty: false,
            build_version: 0,
            rx: None,
            show_ports: true,
            show_cg: true,
            has_renderer,
            framed: false,
            screenshot: None,
            frames_since_build: 0,
            screenshot_requested: false,
        };
        app.load_library();
        if app.file.is_some() {
            app.load();
        }
        app
    }

    fn load_library(&mut self) {
        match Library::load(&self.project) {
            Ok(l) => {
                self.lib_note = format!("library: {}", l.summary());
                if !l.failures.is_empty() {
                    self.lib_note.push_str(&format!("  ({} file(s) failed)", l.failures.len()));
                }
                self.lib = Some(l);
            }
            Err(e) => {
                self.lib_note = format!("no library: {e}");
                self.lib = None;
            }
        }
    }

    fn load(&mut self) {
        let Some(path) = self.file.clone() else { return };
        self.load_error = None;
        self.params.clear();
        self.variants.clear();
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                self.load_error = Some(format!("cannot read {}: {e}", path.display()));
                self.doc = Doc::None;
                return;
            }
        };
        if name.ends_with(".prim.kdl") {
            match wmds_schema::parse_primitive(&name, &src) {
                Ok(def) => {
                    self.params = param_rows(&def);
                    self.variants = def.variants.iter().map(|v| (v.name.clone(), v.options.clone(), 0)).collect();
                    self.doc = Doc::Primitive(Box::new(def));
                }
                Err(e) => {
                    self.load_error = Some(format!("{:?}", miette_string(e)));
                    self.doc = Doc::None;
                    return;
                }
            }
        } else {
            match wmds_schema::parse_assembly(&name, &src) {
                Ok(def) => self.doc = Doc::Assembly(Box::new(def)),
                Err(e) => {
                    self.load_error = Some(format!("{:?}", miette_string(e)));
                    self.doc = Doc::None;
                    return;
                }
            }
        }
        self.framed = false;
        self.dirty = true;
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

    fn start_build(&mut self, ctx: &egui::Context) {
        self.build_version += 1;
        let version = self.build_version;
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.building = true;
        self.dirty = false;
        let ctx = ctx.clone();

        // Everything the worker needs, resolved on this thread so the worker owns plain data.
        let job: Option<Job> = match &self.doc {
            Doc::None => None,
            Doc::Primitive(def) => match resolve(def, &self.overrides()) {
                Ok(r) => {
                    // Show derived values straight away.
                    for row in &mut self.params {
                        if row.derived {
                            if let Some(q) = r.params.get(&row.name).and_then(|v| v.as_quantity()) {
                                row.value = if row.unit.is_empty() { q.value } else { q.to_unit(&row.unit).unwrap_or(q.value) };
                            }
                        }
                    }
                    Some(Job::Primitive(Box::new(r)))
                }
                Err(e) => {
                    self.load_error = Some(e.to_string());
                    None
                }
            },
            Doc::Assembly(def) => match (&self.lib, def) {
                (Some(lib), def) => {
                    let mut extra = Vec::new();
                    if let Some(req) = &def.chassis {
                        match wmds_model::generate_chassis(lib, req) {
                            Ok(g) => extra.push((req.id.clone(), g.assembly)),
                            Err(e) => {
                                self.load_error = Some(format!("chassis: {e}"));
                                return;
                            }
                        }
                    }
                    match wmds_model::resolve_assembly(lib, def, &Overrides::default(), extra) {
                        Ok(a) => {
                            self.load_error = if a.errors.is_empty() { None } else { Some(a.errors.join("\n")) };
                            Some(Job::Assembly(Box::new(a)))
                        }
                        Err(e) => {
                            self.load_error = Some(e.to_string());
                            None
                        }
                    }
                }
                _ => {
                    self.load_error = Some("a vehicle needs a library; set --project to the repository root".into());
                    None
                }
            },
        };
        let Some(job) = job else {
            self.building = false;
            self.rx = None;
            return;
        };
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let result = run_job(job, version);
            let _ = tx.send(BuildResult { elapsed_ms: t0.elapsed().as_millis(), ..result });
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
                self.ports = res.ports;
                self.parts = res.parts;
                self.total_mass = res.total_mass;
                self.point_mass = res.point_mass;
                self.cg = res.cg;
                self.volume_m3 = res.volume_m3;
                if let (Some((lo, hi)), false) = (res.bounds, self.framed) {
                    self.camera.frame(lo, hi);
                    self.framed = true;
                }
            }
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Open");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.path_text).desired_width(250.0).hint_text("a .prim.kdl or .veh.kdl file"));
            if ui.button("Load").clicked() {
                self.file = Some(PathBuf::from(self.path_text.trim()));
                self.load();
            }
        });
        ui.label(egui::RichText::new(&self.lib_note).weak());

        match &self.doc {
            Doc::None => {}
            Doc::Primitive(d) => {
                ui.label(format!("{} v{}", d.id, d.version));
                if !d.description.is_empty() {
                    ui.label(egui::RichText::new(&d.description).italics());
                }
                if let Some(m) = &d.material {
                    ui.label(format!("material: {m}"));
                }
            }
            Doc::Assembly(d) => {
                ui.label(format!("{} v{}", d.id, d.version));
                if !d.description.is_empty() {
                    ui.label(egui::RichText::new(&d.description).italics());
                }
                if let Some(v) = &d.vehicle {
                    ui.label(format!("category {}", v.category));
                }
                if let Some(c) = &d.chassis {
                    ui.label(format!("chassis {} {} {}", c.system, c.configuration, c.width));
                }
            }
        }
        if let Some(e) = &self.load_error {
            ui.colored_label(egui::Color32::from_rgb(230, 110, 90), e);
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
        if !self.params.is_empty() {
            ui.heading("Parameters");
            egui::Grid::new("params").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
                for p in &mut self.params {
                    ui.label(&p.name).on_hover_text(&p.doc);
                    if p.derived {
                        ui.label(format!("{:.3} {}", p.value, p.unit));
                        ui.label(egui::RichText::new("derived").weak());
                    } else {
                        if ui.add(egui::Slider::new(&mut p.value, p.min..=p.max).suffix(format!(" {}", p.unit))).changed() {
                            changed = true;
                        }
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
            ui.separator();
        }
        if changed {
            self.dirty = true;
        }

        ui.heading("View");
        ui.checkbox(&mut self.show_ports, "ports");
        ui.checkbox(&mut self.show_cg, "centre of gravity");
        if self.building {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("building…");
            });
        } else if let Some(ms) = self.build_ms {
            ui.label(format!("built in {ms} ms with {KERNEL_NAME}"));
        }
        if let Some(e) = &self.build_error {
            ui.colored_label(egui::Color32::from_rgb(230, 110, 90), e);
        }

        if self.total_mass > 0.0 {
            ui.separator();
            ui.heading("Mass");
            ui.label(format!("{:.1} kg modelled", self.total_mass));
            if self.point_mass > 0.0 {
                ui.label(format!("{:.1} kg declared point masses", self.point_mass));
                ui.label(format!("{:.1} kg total", self.total_mass + self.point_mass));
            }
            ui.label(format!("cg ({:.0}, {:.0}, {:.0}) mm", self.cg[0] * 1e3, self.cg[1] * 1e3, self.cg[2] * 1e3));
            if let Some(v) = self.volume_m3 {
                ui.label(format!("volume {:.1} cm3", v * 1e6));
            }
            ui.label(egui::RichText::new("densities are placeholders").weak());
        }

        if !self.parts.is_empty() {
            ui.separator();
            ui.heading(format!("Parts ({})", self.parts.len()));
            egui::Grid::new("parts").num_columns(3).spacing([8.0, 2.0]).striped(true).show(ui, |ui| {
                for p in &self.parts {
                    ui.label(&p.id).on_hover_text(format!("{}\n{}", p.source, p.how));
                    ui.label(format!("{:.0}, {:.0}, {:.0}", p.position[0] * 1e3, p.position[1] * 1e3, p.position[2] * 1e3));
                    match p.mass {
                        Some(m) => {
                            let t = format!("{m:.1} kg");
                            ui.label(if p.declared { egui::RichText::new(t).strong() } else { egui::RichText::new(t) });
                        }
                        None => {
                            ui.label("-");
                        }
                    }
                    ui.end_row();
                }
            });
        }

        if !self.ports.is_empty() && self.parts.is_empty() {
            ui.separator();
            ui.heading("Ports");
            for (name, o, _) in &self.ports {
                ui.label(format!("{name}  ({:.0}, {:.0}, {:.0}) mm", o[0] * 1e3, o[1] * 1e3, o[2] * 1e3));
            }
        }

        if !self.has_renderer {
            ui.separator();
            ui.colored_label(egui::Color32::YELLOW, "no wgpu render state: the viewport is disabled");
        }
        ui.separator();
        ui.label(egui::RichText::new("drag orbit   shift-drag or middle pan   wheel zoom   F frame").weak());
    }

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        viewport::handle_input(&mut self.camera, ui, &response);
        if ui.input(|i| i.key_pressed(egui::Key::F)) {
            self.framed = false;
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
        let axis_len = self.camera.distance * 0.08;
        for (dir, color, label) in [
            (Vec3::X, egui::Color32::from_rgb(230, 80, 80), "x"),
            (Vec3::Y, egui::Color32::from_rgb(80, 200, 80), "y"),
            (Vec3::Z, egui::Color32::from_rgb(90, 140, 255), "z"),
        ] {
            if let (Some(a), Some(b)) = (self.camera.project(rect, Vec3::ZERO), self.camera.project(rect, dir * axis_len)) {
                painter.line_segment([a, b], egui::Stroke::new(1.5, color));
                painter.text(b, egui::Align2::LEFT_BOTTOM, label, egui::FontId::monospace(11.0), color);
            }
        }
        if self.show_ports {
            let port_len = self.camera.distance * 0.03;
            let col = egui::Color32::from_rgb(255, 200, 60);
            // Above a few dozen ports the labels are unreadable, so draw markers only.
            let label_them = self.ports.len() <= 24;
            for (name, o, a) in &self.ports {
                let origin = Vec3::new(o[0] as f32, o[1] as f32, o[2] as f32);
                let axis = Vec3::new(a[0] as f32, a[1] as f32, a[2] as f32);
                if let (Some(s0), Some(s1)) = (self.camera.project(rect, origin), self.camera.project(rect, origin + axis * port_len)) {
                    painter.circle(s0, 3.0, col, egui::Stroke::new(1.0, egui::Color32::BLACK));
                    painter.line_segment([s0, s1], egui::Stroke::new(1.5, col));
                    if label_them {
                        painter.text(s0 + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, name, egui::FontId::proportional(12.0), col);
                    }
                }
            }
        }
        if self.show_cg && self.total_mass > 0.0 {
            let cg = Vec3::new(self.cg[0] as f32, self.cg[1] as f32, self.cg[2] as f32);
            if let Some(p) = self.camera.project(rect, cg) {
                let col = egui::Color32::from_rgb(255, 120, 200);
                painter.circle_filled(p, 5.0, col);
                painter.circle_stroke(p, 9.0, egui::Stroke::new(1.5, col));
                painter.text(p + egui::vec2(12.0, -4.0), egui::Align2::LEFT_CENTER, "cg", egui::FontId::proportional(12.0), col);
            }
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        let settled = self.mesh.is_some() || self.build_error.is_some() || self.load_error.is_some();
        if settled && !self.building {
            self.frames_since_build += 1;
        }
        ctx.request_repaint();
        if self.frames_since_build >= 5 && !self.screenshot_requested {
            self.screenshot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let [w, h] = image.size;
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            match image::save_buffer(&path, &rgba, w as u32, h as u32, image::ColorType::Rgba8) {
                Ok(()) => eprintln!("wrote screenshot to {}", path.display()),
                Err(e) => eprintln!("screenshot failed: {e}"),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
        self.handle_screenshot(ui.ctx());
        egui::Panel::left("side").resizable(true).show(ui, |ui| {
            ui.set_min_width(380.0);
            egui::ScrollArea::vertical().show(ui, |ui| self.side_panel(ui));
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.viewport(ui));
    }
}

// --------------------------------------------------------------------------- the build job

enum Job {
    Primitive(Box<ResolvedPrimitive>),
    Assembly(Box<ResolvedAssembly>),
}

fn run_job(job: Job, version: u64) -> BuildResult {
    let k = Kernel::default();
    match job {
        Job::Primitive(r) => {
            let built = match wmds_geom::build_primitive(&k, &r) {
                Ok(b) => b,
                Err(e) => return BuildResult { error: Some(e.to_string()), ..BuildResult::empty(version) },
            };
            let Some((_, solid)) = built.best() else {
                return BuildResult { error: Some("no geometry level built".into()), ..BuildResult::empty(version) };
            };
            let mesh = match k.tessellate(solid, 2e-4) {
                Ok(m) => m,
                Err(e) => return BuildResult { error: Some(e.to_string()), ..BuildResult::empty(version) },
            };
            let mp = mesh.mass_props();
            let density = r.material.as_deref().and_then(wmds_geom::placeholder_density);
            BuildResult {
                mesh: Some(Arc::new(GpuMeshData::from_mesh(&mesh, version))),
                bounds: bounds_of(&mesh),
                ports: r.ports.iter().map(port_tuple).collect(),
                total_mass: density.map(|d| mp.volume * d).unwrap_or(0.0),
                cg: mp.centroid,
                volume_m3: Some(mp.volume),
                ..BuildResult::empty(version)
            }
        }
        Job::Assembly(asm) => {
            let built = wmds_geom::build_assembly(&k, &asm);
            let mesh = wmds_geom::assembly_mesh(&k, &built, 2e-4);
            let masses = wmds_geom::assembly_masses(&k, &built);
            let (total, cg, _unknown) = wmds_geom::roll_up(&masses);
            let point_mass: f64 = asm.point_masses.iter().map(|p| p.mass.value).sum();

            let mut parts = Vec::new();
            for i in &asm.instances {
                let m = masses.iter().find(|m| m.id == i.id);
                parts.push(PartRow {
                    id: i.id.clone(),
                    source: i.source_id.clone(),
                    position: i.placement.translation,
                    how: match &i.placed_by {
                        PlacedBy::Root => "root".into(),
                        PlacedBy::Mate(m) => format!("placed by mate {m}"),
                        PlacedBy::Free(w) => format!("placed explicitly: {w}"),
                        PlacedBy::Unreached => "NOT PLACED".into(),
                    },
                    mass: m.and_then(|m| m.mass),
                    declared: m.map(|m| m.from_declaration).unwrap_or(false),
                });
            }

            // Show only the ports a mate actually uses; a chassis has dozens of free stations
            // and drawing all of them buries the ones that matter.
            let mut ports = Vec::new();
            for mate in asm.mates.iter().filter(|m| m.compatible.is_ok()) {
                for (inst, port) in [(&mate.a, &mate.a_port), (&mate.b, &mate.b_port)] {
                    if let Some(i) = asm.instances.iter().find(|i| &i.id == inst || i.id.ends_with(&format!(".{inst}"))) {
                        if let Some(f) = i.port_world(port) {
                            ports.push((format!("{inst}.{port}"), f.translation, f.direction([0.0, 0.0, 1.0])));
                        }
                    }
                }
            }

            let errors = if asm.errors.is_empty() { None } else { Some(asm.errors.join("\n")) };
            BuildResult {
                mesh: Some(Arc::new(GpuMeshData::from_mesh(&mesh, version))),
                bounds: bounds_of(&mesh),
                ports,
                parts,
                total_mass: total,
                point_mass,
                cg,
                volume_m3: None,
                error: errors,
                ..BuildResult::empty(version)
            }
        }
    }
}

fn bounds_of(mesh: &wmds_geom::Mesh) -> Option<(Vec3, Vec3)> {
    mesh.bounds().map(|(lo, hi)| {
        (
            Vec3::new(lo[0] as f32, lo[1] as f32, lo[2] as f32),
            Vec3::new(hi[0] as f32, hi[1] as f32, hi[2] as f32),
        )
    })
}

fn port_tuple(p: &ResolvedPort) -> (String, [f64; 3], [f64; 3]) {
    (
        p.name.clone(),
        [p.origin[0].value, p.origin[1].value, p.origin[2].value],
        p.axis,
    )
}

fn param_rows(def: &PrimitiveDef) -> Vec<ParamRow> {
    def.params
        .iter()
        .map(|p| {
            let unit = p.unit.clone().unwrap_or_default();
            let lit = |e: &Option<wmds_expr::Expr>| -> Option<f64> {
                let mut e = e.as_ref()?;
                if let wmds_expr::Expr::TextOr(_, inner) = e {
                    e = inner;
                }
                match e {
                    wmds_expr::Expr::Num(q) => {
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
            let min = lit(&p.min).unwrap_or(if value > 0.0 { value * 0.25 } else { value - 1.0 });
            let max = lit(&p.max).unwrap_or(if value > 0.0 { value * 2.0 } else { value + 1.0 });
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
        .collect()
}

fn miette_string(e: wmds_schema::SchemaErrors) -> String {
    format!("{:?}", miette::Report::new(e))
}
