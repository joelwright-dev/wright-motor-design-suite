//! WMDS desktop application.
//!
//! This is where vehicles are designed. Parts come from the library, joints are made by choosing
//! two ports that fit, and the model rebuilds after every change, so the picture on screen is
//! always what the file says.
//!
//! Text files remain the storage format, because they diff, review and version properly. Nobody
//! has to type one.

mod edit;
mod part;
mod viewport;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use eframe::egui;
use glam::Vec3;
use indexmap::IndexMap;
use wmds_expr::Value;
use wmds_geom::GeomKernel;
use wmds_model::{
    Library, Overrides, PlacedBy, ResolvedAssembly, ResolvedPort, ResolvedPrimitive, UnitPorts,
    ports_compatible, resolve,
};
use wmds_schema::{AssemblyDef, PortRef, PrimitiveDef};
use wmds_units::Quantity;

use edit::{CatalogueEntry, EntryKind};
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

const ACCENT: egui::Color32 = egui::Color32::from_rgb(255, 200, 60);
const DANGER: egui::Color32 = egui::Color32::from_rgb(230, 110, 90);
const GOOD: egui::Color32 = egui::Color32::from_rgb(120, 210, 140);

fn main() -> eframe::Result {
    // Usage: wmds-app [file] [--project DIR] [--screenshot out.png]
    let mut file: Option<PathBuf> = None;
    let mut screenshot: Option<PathBuf> = None;
    let mut view: Option<String> = None;
    let mut project = PathBuf::from(".");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => screenshot = args.next().map(PathBuf::from),
            "--view" => view = args.next(),
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
            .with_inner_size([1600.0, 1000.0]),
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
            if let Some(v) = &view {
                app.start_view = Some(v.clone());
            }
            Ok(Box::new(app))
        }),
    )
}

/// What is open.
enum Doc {
    None,
    Primitive(Box<PrimitiveDef>),
    Assembly(Box<AssemblyDef>),
}

/// Which end of a joint is being changed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum JointEnd {
    A,
    B,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Parts,
    Joints,
    Check,
}

/// One editable setting on the selected instance.
struct InstParam {
    name: String,
    unit: String,
    doc: String,
    /// Present when the value is a plain number, which is most of them.
    value: f64,
    min: f64,
    max: f64,
    numeric: bool,
    /// The text form, used for anything that is not a plain number.
    text: String,
    /// What the library says when the vehicle does not override it.
    default_text: String,
    overridden: bool,
    /// A variant, which is a choice from a fixed list rather than a free value.
    options: Vec<String>,
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

/// Something the user asked for, applied after the panel has finished drawing.
enum Action {
    Add(usize),
    Select(String),
    Delete(String),
    MakeRoot(String),
    SetParam(String, String, Option<String>),
    SetVariant(String, String, String),
    PickPort(String, String),
    AddJoint,
    ClearJoint,
    DeleteJoint(String),
    EditJoint(Option<String>),
    RetargetJoint(String, JointEnd, String, String),
    SetPlacement(String, Option<String>),
    SetChassis,
    NewVehicle,
    NewPart,
    Open(String),
    Save,
}

struct BuildResult {
    mesh: Option<Arc<GpuMeshData>>,
    bounds: Option<(Vec3, Vec3)>,
    part_bounds: HashMap<String, (Vec3, Vec3)>,
    ports: Vec<(String, [f64; 3], [f64; 3])>,
    parts: Vec<PartRow>,
    total_mass: f64,
    point_mass: f64,
    cg: [f64; 3],
    volume_m3: Option<f64>,
    error: Option<String>,
    elapsed_ms: u128,
    version: u64,
    /// How many parts came from the mesh cache instead of the geometry kernel.
    reused: usize,
    parts_built: usize,
}

impl BuildResult {
    fn empty(version: u64) -> BuildResult {
        BuildResult {
            mesh: None,
            bounds: None,
            part_bounds: HashMap::new(),
            ports: Vec::new(),
            parts: Vec::new(),
            total_mass: 0.0,
            point_mass: 0.0,
            cg: [0.0; 3],
            volume_m3: None,
            error: None,
            elapsed_ms: 0,
            version,
            reused: 0,
            parts_built: 0,
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

/// The chassis controls, held separately so a half-made change does not disturb the model.
#[derive(Default, Clone, PartialEq)]
struct ChassisUi {
    system: String,
    configuration: String,
    width: String,
    rail_section: String,
    lengths: IndexMap<String, f64>,
}

struct App {
    project: PathBuf,
    file: Option<PathBuf>,
    path_text: String,
    doc: Doc,
    lib: Option<Arc<Library>>,
    lib_note: String,
    params: Vec<ParamRow>,
    variants: Vec<(String, Vec<String>, usize)>,
    load_error: Option<String>,

    // editing
    tab: Tab,
    part_tab: PartTab,
    /// Text being typed into a field, held until the field is finished with.
    scratch: HashMap<String, String>,
    part_note: String,
    new_part_id: String,
    catalogue: Arc<Vec<CatalogueEntry>>,
    search: String,
    selected: Option<String>,
    inst_params: Vec<InstParam>,
    unsaved: bool,
    status: String,
    chassis_ui: ChassisUi,

    // joint making
    mateable: Arc<Vec<UnitPorts>>,
    used_ports: HashSet<(String, String)>,
    joint_a: Option<(String, String)>,
    joint_b: Option<(String, String)>,
    joint_candidates: Vec<(String, String, f64)>,
    joint_note: String,
    /// The joint whose ends are open for changing, if any.
    editing_joint: Option<String>,

    camera: Camera,
    mesh: Option<Arc<GpuMeshData>>,
    ports: Vec<(String, [f64; 3], [f64; 3])>,
    parts: Vec<PartRow>,
    part_bounds: HashMap<String, (Vec3, Vec3)>,
    total_mass: f64,
    point_mass: f64,
    cg: [f64; 3],
    volume_m3: Option<f64>,
    build_error: Option<String>,
    warnings: Vec<String>,
    build_ms: Option<u128>,
    building: bool,
    dirty: bool,
    build_version: u64,
    rx: Option<Receiver<BuildResult>>,

    /// Tessellated parts, kept between builds so an edit to one part does not rebuild the rest.
    cache: Arc<std::sync::Mutex<wmds_geom::MeshCache>>,
    cache_note: String,

    show_ports: bool,
    show_cg: bool,
    has_renderer: bool,
    framed: bool,
    screenshot: Option<PathBuf>,
    /// A named view asked for on the command line, applied once the model has been framed.
    start_view: Option<String>,
    frames_since_build: u32,
    screenshot_requested: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>, project: PathBuf) -> Self {
        let has_renderer =
            viewport::init(cc, eframe::egui_wgpu::depth_format_from_bits(DEPTH_BITS, 0));
        let mut app = App {
            project,
            path_text: file
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            file,
            doc: Doc::None,
            lib: None,
            lib_note: String::new(),
            params: Vec::new(),
            variants: Vec::new(),
            load_error: None,
            tab: Tab::Parts,
            part_tab: PartTab::Shape,
            scratch: HashMap::new(),
            part_note: String::new(),
            new_part_id: String::new(),
            catalogue: Arc::new(Vec::new()),
            search: String::new(),
            selected: None,
            inst_params: Vec::new(),
            unsaved: false,
            status: String::new(),
            chassis_ui: ChassisUi::default(),
            mateable: Arc::new(Vec::new()),
            used_ports: HashSet::new(),
            joint_a: None,
            joint_b: None,
            joint_candidates: Vec::new(),
            joint_note: String::new(),
            editing_joint: None,
            camera: Camera::default(),
            mesh: None,
            ports: Vec::new(),
            parts: Vec::new(),
            part_bounds: HashMap::new(),
            total_mass: 0.0,
            point_mass: 0.0,
            cg: [0.0; 3],
            volume_m3: None,
            build_error: None,
            warnings: Vec::new(),
            build_ms: None,
            building: false,
            dirty: false,
            build_version: 0,
            rx: None,
            cache: Arc::new(std::sync::Mutex::new(wmds_geom::MeshCache::new())),
            cache_note: String::new(),
            show_ports: true,
            show_cg: true,
            has_renderer,
            framed: false,
            screenshot: None,
            start_view: None,
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
                    self.lib_note
                        .push_str(&format!("  ({} file(s) failed)", l.failures.len()));
                }
                self.catalogue = Arc::new(edit::catalogue(&l));
                self.lib = Some(Arc::new(l));
            }
            Err(e) => {
                self.lib_note = format!("no library: {e}");
                self.lib = None;
            }
        }
    }

    fn load(&mut self) {
        let Some(path) = self.file.clone() else {
            return;
        };
        self.load_error = None;
        self.params.clear();
        self.variants.clear();
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
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
                    self.variants = def
                        .variants
                        .iter()
                        .map(|v| (v.name.clone(), v.options.clone(), 0))
                        .collect();
                    self.doc = Doc::Primitive(Box::new(def));
                }
                Err(e) => {
                    self.load_error = Some(miette_string(e));
                    self.doc = Doc::None;
                    return;
                }
            }
        } else {
            match wmds_schema::parse_assembly(&name, &src) {
                Ok(def) => {
                    self.sync_chassis_ui(&def);
                    self.doc = Doc::Assembly(Box::new(def));
                }
                Err(e) => {
                    self.load_error = Some(miette_string(e));
                    self.doc = Doc::None;
                    return;
                }
            }
        }
        self.selected = None;
        self.inst_params.clear();
        self.scratch.clear();
        self.part_note.clear();
        self.clear_joint();
        self.unsaved = false;
        self.status = format!("opened {}", path.display());
        self.framed = false;
        self.dirty = true;
    }

    fn sync_chassis_ui(&mut self, def: &AssemblyDef) {
        self.chassis_ui = match &def.chassis {
            Some(c) => ChassisUi {
                system: c.system.clone(),
                configuration: c.configuration.clone(),
                width: c.width.clone(),
                rail_section: c.rail_section.clone(),
                lengths: c
                    .section_lengths
                    .iter()
                    .map(|(k, e)| (k.clone(), length_mm(e).unwrap_or(1000.0)))
                    .collect(),
            },
            None => ChassisUi::default(),
        };
    }

    fn assembly(&self) -> Option<&AssemblyDef> {
        match &self.doc {
            Doc::Assembly(d) => Some(d),
            _ => None,
        }
    }

    fn assembly_mut(&mut self) -> Option<&mut AssemblyDef> {
        match &mut self.doc {
            Doc::Assembly(d) => Some(d),
            _ => None,
        }
    }

    fn clear_joint(&mut self) {
        self.joint_a = None;
        self.joint_b = None;
        self.joint_candidates.clear();
        self.joint_note.clear();
    }

    /// The ports that could legally join the one already chosen, nearest first.
    ///
    /// The ordering matters more than it sounds: a chassis offers dozens of identical stations,
    /// and the one you want is nearly always the closest one that fits.
    fn recompute_candidates(&mut self) {
        self.joint_candidates.clear();
        self.joint_note.clear();
        let (Some(lib), Some((au, ap))) = (self.lib.clone(), self.joint_a.clone()) else {
            return;
        };
        let Some(a) = find_port(&self.mateable, &au, &ap) else {
            return;
        };
        let mut rejected = 0usize;
        for u in self.mateable.iter() {
            if u.unit == au {
                continue;
            }
            for p in &u.ports {
                if self.used_ports.contains(&(u.unit.clone(), p.name.clone())) {
                    continue;
                }
                let ok = ports_compatible(
                    &lib,
                    &a.port_type,
                    &a.params,
                    &format!("{au}.{ap}"),
                    &p.port_type,
                    &p.params,
                    &format!("{}.{}", u.unit, p.name),
                )
                .is_ok();
                if !ok {
                    rejected += 1;
                    continue;
                }
                let d = dist(a.world, p.world);
                self.joint_candidates.push((u.unit.clone(), p.name.clone(), d));
            }
        }
        self.joint_candidates
            .sort_by(|x, y| x.2.partial_cmp(&y.2).unwrap_or(std::cmp::Ordering::Equal));
        self.joint_note = if self.joint_candidates.is_empty() {
            format!(
                "nothing in this vehicle can bolt to a {} port. {rejected} other ports were checked.",
                a.port_type
            )
        } else {
            format!(
                "{} of {} free ports can take a {}",
                self.joint_candidates.len(),
                self.joint_candidates.len() + rejected,
                a.port_type
            )
        };
    }


    /// Which ports could take the place of one end of an existing joint.
    ///
    /// Moving a part to a different mounting point is the most basic design act there is, and
    /// doing it by hand means editing a station number in a file and hoping. This asks the mate
    /// checker the same question the joint picker asks, holding the other end fixed.
    fn retarget_options(&self, mate_id: &str, end: JointEnd) -> Vec<(String, String, f64)> {
        let (Some(lib), Some(def)) = (self.lib.clone(), self.assembly()) else {
            return Vec::new();
        };
        let Some(m) = def.mates.iter().find(|m| m.id == mate_id) else {
            return Vec::new();
        };
        let (moving, fixed) = match end {
            JointEnd::A => (&m.a, &m.b),
            JointEnd::B => (&m.b, &m.a),
        };
        let Some(anchor) = find_port(&self.mateable, &fixed.instance, &fixed.port) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for u in self.mateable.iter() {
            if u.unit == fixed.instance {
                continue;
            }
            for p in &u.ports {
                let key = (u.unit.clone(), p.name.clone());
                let is_current = key.0 == moving.instance && key.1 == moving.port;
                if !is_current && self.used_ports.contains(&key) {
                    continue;
                }
                if ports_compatible(
                    &lib,
                    &anchor.port_type,
                    &anchor.params,
                    "a",
                    &p.port_type,
                    &p.params,
                    "b",
                )
                .is_ok()
                {
                    out.push((u.unit.clone(), p.name.clone(), dist(anchor.world, p.world)));
                }
            }
        }
        out.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    /// Rebuild the editable settings for whichever instance is selected.
    fn sync_inst_params(&mut self) {
        self.inst_params.clear();
        let (Some(lib), Some(sel)) = (self.lib.clone(), self.selected.clone()) else {
            return;
        };
        let Some(def) = self.assembly() else { return };
        // Cloned so the rest of this can write back to `self` without the definition still
        // being borrowed out of it.
        let Some(inst) = def.instances.iter().find(|i| i.id == sel).cloned() else {
            return;
        };
        let (params, variants): (Vec<_>, Vec<_>) = match &inst.source {
            wmds_schema::InstanceSource::Primitive(pid) => match lib.primitive(pid) {
                Some(p) => (
                    p.params.clone(),
                    p.variants
                        .iter()
                        .map(|v| (v.name.clone(), v.options.clone()))
                        .collect(),
                ),
                None => return,
            },
            wmds_schema::InstanceSource::Assembly(aid) => match lib.assembly(aid) {
                // A sub-assembly's parameters are its dials; it has no separate variants.
                Some(a) => (a.params.clone(), Vec::new()),
                None => return,
            },
        };

        for p in &params {
            if p.expr.is_some() {
                continue; // derived from the others; not something to set
            }
            let unit = p.unit.clone().unwrap_or_default();
            let default_text = p
                .default
                .as_ref()
                .map(wmds_schema::expr_text)
                .unwrap_or_default();
            let over = inst.params.get(&p.name);
            let text = over.map(wmds_schema::expr_text).unwrap_or_default();
            let value = over
                .and_then(|e| number_in(e, &unit))
                .or_else(|| p.default.as_ref().and_then(|e| number_in(e, &unit)))
                .unwrap_or(0.0);
            let dmin = p.min.as_ref().and_then(|e| number_in(e, &unit));
            let dmax = p.max.as_ref().and_then(|e| number_in(e, &unit));
            let numeric = over.map(|e| number_in(e, &unit).is_some()).unwrap_or(true)
                && (p.default.is_none() || p.default.as_ref().and_then(|e| number_in(e, &unit)).is_some());
            self.inst_params.push(InstParam {
                name: p.name.clone(),
                unit,
                doc: p.doc.clone().unwrap_or_default(),
                value,
                min: dmin.unwrap_or(if value > 0.0 { value * 0.2 } else { value - 100.0 }),
                max: dmax.unwrap_or(if value > 0.0 { value * 3.0 } else { value + 100.0 }),
                numeric,
                text,
                default_text,
                overridden: over.is_some(),
                options: Vec::new(),
            });
        }
        for (name, options) in variants {
            let chosen = inst
                .variants
                .get(&name)
                .map(wmds_schema::expr_text)
                .map(|s| s.trim_matches('"').to_string())
                .unwrap_or_else(|| options.first().cloned().unwrap_or_default());
            self.inst_params.push(InstParam {
                name,
                unit: String::new(),
                doc: "which hand or version of the part".into(),
                value: 0.0,
                min: 0.0,
                max: 0.0,
                numeric: false,
                text: chosen.clone(),
                default_text: options.first().cloned().unwrap_or_default(),
                overridden: true,
                options,
            });
        }
    }

    fn apply(&mut self, a: Action) {
        match a {
            Action::Add(idx) => {
                let entry = self.catalogue.get(idx).cloned();
                let (Some(entry), Some(def)) = (entry, self.assembly_mut()) else {
                    return;
                };
                let id = edit::add_instance(def, &entry);
                self.status = format!("added {id}");
                self.selected = Some(id);
                self.unsaved = true;
                self.dirty = true;
                self.sync_inst_params();
            }
            Action::Select(id) => {
                self.selected = Some(id);
                self.sync_inst_params();
            }
            Action::Delete(id) => {
                if let Some(def) = self.assembly_mut() {
                    let n = edit::remove_instance(def, &id);
                    self.status = if n > 0 {
                        format!("deleted {id} and {n} joint(s) that used it")
                    } else {
                        format!("deleted {id}")
                    };
                }
                if self.selected.as_deref() == Some(id.as_str()) {
                    self.selected = None;
                    self.inst_params.clear();
                }
                self.clear_joint();
                self.unsaved = true;
                self.dirty = true;
            }
            Action::MakeRoot(id) => {
                if let Some(def) = self.assembly_mut() {
                    def.root = Some(id.clone());
                }
                self.status = format!("{id} is now the fixed part everything else is placed from");
                self.unsaved = true;
                self.dirty = true;
            }
            Action::SetParam(inst, name, value) => {
                if let Some(def) = self.assembly_mut() {
                    edit::set_param(def, &inst, &name, value);
                }
                self.unsaved = true;
                self.dirty = true;
                self.sync_inst_params();
            }
            Action::SetVariant(inst, name, option) => {
                if let Some(def) = self.assembly_mut() {
                    edit::set_variant(def, &inst, &name, &option);
                }
                self.unsaved = true;
                self.dirty = true;
                self.sync_inst_params();
            }
            Action::PickPort(unit, port) => {
                if self.joint_a.is_none() {
                    self.joint_a = Some((unit, port));
                    self.joint_b = None;
                    self.recompute_candidates();
                } else if self.joint_a.as_ref() == Some(&(unit.clone(), port.clone())) {
                    self.clear_joint();
                } else if self
                    .joint_candidates
                    .iter()
                    .any(|(u, p, _)| u == &unit && p == &port)
                {
                    self.joint_b = Some((unit, port));
                } else {
                    // Not compatible with the current first pick, so treat it as a new start.
                    self.joint_a = Some((unit, port));
                    self.joint_b = None;
                    self.recompute_candidates();
                }
                self.tab = Tab::Joints;
            }
            Action::ClearJoint => self.clear_joint(),
            Action::EditJoint(id) => self.editing_joint = id,
            Action::RetargetJoint(id, end, unit, port) => {
                if let Some(def) = self.assembly_mut()
                    && let Some(m) = def.mates.iter_mut().find(|m| m.id == id)
                {
                    let r = PortRef { instance: unit.clone(), port: port.clone() };
                    match end {
                        JointEnd::A => m.a = r,
                        JointEnd::B => m.b = r,
                    }
                }
                self.status = format!("joint {id} now goes to {unit}.{port}");
                self.unsaved = true;
                self.dirty = true;
            }
            Action::SetPlacement(id, at) => {
                if let Some(def) = self.assembly_mut()
                    && let Some(i) = def.instances.iter_mut().find(|i| i.id == id)
                {
                    i.placement = at.as_ref().map(|text| wmds_schema::Placement {
                        at: part::text_expr(text),
                        rotate: i.placement.as_ref().and_then(|p| p.rotate.clone()),
                        mirror: i.placement.as_ref().and_then(|p| p.mirror.clone()),
                        justification: i
                            .placement
                            .as_ref()
                            .map(|p| p.justification.clone())
                            .unwrap_or_else(|| "placed by hand in the editor".into()),
                    });
                }
                self.status = match at {
                    Some(_) => format!("{id} is now placed explicitly"),
                    None => format!("{id} is back to being placed by its joints"),
                };
                self.unsaved = true;
                self.dirty = true;
            }
            Action::AddJoint => {
                let (Some(a), Some(b)) = (self.joint_a.clone(), self.joint_b.clone()) else {
                    return;
                };
                let bolt = self
                    .lib
                    .clone()
                    .and_then(|_| find_port(&self.mateable, &a.0, &a.1))
                    .and_then(|p| {
                        p.params
                            .get("bolt")
                            .map(|v| value_text(v))
                            .or_else(|| p.params.get("size").map(|v| value_text(v)))
                    });
                if let Some(def) = self.assembly_mut() {
                    let id = edit::add_mate(
                        def,
                        PortRef { instance: a.0.clone(), port: a.1.clone() },
                        PortRef { instance: b.0.clone(), port: b.1.clone() },
                        Some(edit::default_fastener(bolt.as_deref())),
                    );
                    self.status = format!("joined {}.{} to {}.{} as {id}", a.0, a.1, b.0, b.1);
                }
                self.clear_joint();
                self.unsaved = true;
                self.dirty = true;
            }
            Action::DeleteJoint(id) => {
                if let Some(def) = self.assembly_mut() {
                    edit::remove_mate(def, &id);
                }
                self.status = format!("removed joint {id}");
                self.unsaved = true;
                self.dirty = true;
            }
            Action::SetChassis => {
                let ui = self.chassis_ui.clone();
                if let Some(def) = self.assembly_mut() {
                    edit::set_chassis(
                        def,
                        &ui.system,
                        &ui.configuration,
                        &ui.width,
                        &ui.rail_section,
                        &ui.lengths,
                    );
                }
                self.status = format!("chassis set to {} {}", ui.configuration, ui.width);
                self.clear_joint();
                self.unsaved = true;
                self.dirty = true;
                self.framed = false;
            }
            Action::NewVehicle => {
                let def = edit::new_vehicle("new/vehicle");
                self.sync_chassis_ui(&def);
                self.doc = Doc::Assembly(Box::new(def));
                self.file = None;
                self.path_text = "vehicles/new-vehicle.veh.kdl".into();
                self.selected = None;
                self.inst_params.clear();
                self.clear_joint();
                self.load_error = None;
                self.status = "new vehicle: add a chassis first, then parts".into();
                self.unsaved = true;
                self.framed = false;
                self.dirty = true;
            }
            Action::NewPart => self.new_part(),
            Action::Open(p) => {
                self.file = Some(PathBuf::from(p.trim()));
                self.load();
            }
            Action::Save => match self.doc {
                Doc::Primitive(_) => self.save_part(),
                _ => self.save(),
            },
        }
    }

    fn save(&mut self) {
        let path = PathBuf::from(self.path_text.trim());
        if path.as_os_str().is_empty() {
            self.status = "give the file a name first".into();
            return;
        }
        let Some(def) = self.assembly() else {
            self.status = "only vehicles and assemblies can be saved from here".into();
            return;
        };
        let text = wmds_schema::write_assembly(def);
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            self.status = format!("cannot make {}: {e}", parent.display());
            return;
        }
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.file = Some(path.clone());
                self.unsaved = false;
                self.status = format!("saved {}", path.display());
            }
            Err(e) => self.status = format!("cannot write {}: {e}", path.display()),
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
                        if row.derived
                            && let Some(q) = r.params.get(&row.name).and_then(|v| v.as_quantity())
                        {
                            row.value = if row.unit.is_empty() {
                                q.value
                            } else {
                                q.to_unit(&row.unit).unwrap_or(q.value)
                            };
                        }
                    }
                    let d = r
                        .material
                        .as_deref()
                        .and_then(|m| self.lib.as_ref().and_then(|l| l.density(m)));
                    Some(Job::Primitive(Box::new(r), d))
                }
                Err(e) => {
                    self.load_error = Some(e.to_string());
                    None
                }
            },
            Doc::Assembly(def) => match &self.lib {
                Some(lib) => {
                    let lib = lib.clone();
                    let mut extra = Vec::new();
                    let mut chassis_error = None;
                    if let Some(req) = &def.chassis {
                        match wmds_model::generate_chassis(&lib, req) {
                            Ok(g) => extra.push((req.id.clone(), g.assembly)),
                            Err(e) => chassis_error = Some(format!("chassis: {e}")),
                        }
                    }
                    if let Some(e) = chassis_error {
                        self.load_error = Some(e);
                        self.building = false;
                        self.rx = None;
                        return;
                    }
                    match wmds_model::resolve_assembly(&lib, def, &Overrides::default(), extra) {
                        Ok(a) => {
                            self.load_error = if a.errors.is_empty() {
                                None
                            } else {
                                Some(a.errors.join("\n"))
                            };
                            self.warnings = a.warnings.clone();
                            self.mateable = Arc::new(a.mateable.clone());
                            self.used_ports = def
                                .mates
                                .iter()
                                .flat_map(|m| {
                                    [
                                        (m.a.instance.clone(), m.a.port.clone()),
                                        (m.b.instance.clone(), m.b.port.clone()),
                                    ]
                                })
                                .collect();
                            let densities = lib
                                .materials
                                .iter()
                                .map(|(k, v)| (k.clone(), v.density_si()))
                                .collect();
                            Some(Job::Assembly(Box::new(a), densities))
                        }
                        Err(e) => {
                            self.load_error = Some(e.to_string());
                            None
                        }
                    }
                }
                None => {
                    self.load_error = Some(
                        "a vehicle needs a library; set --project to the repository root".into(),
                    );
                    None
                }
            },
        };
        // Keep the joint picker honest after the model changed under it.
        if self.joint_a.is_some() {
            self.recompute_candidates();
        }
        let Some(job) = job else {
            self.building = false;
            self.rx = None;
            return;
        };
        let cache = self.cache.clone();
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let result = run_job(job, version, &cache);
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
            self.cache_note = if res.parts_built > 0 {
                format!("{} of {} parts reused", res.reused, res.parts_built)
            } else {
                String::new()
            };
            if res.mesh.is_some() {
                self.mesh = res.mesh;
                self.ports = res.ports;
                self.parts = res.parts;
                self.part_bounds = res.part_bounds;
                self.total_mass = res.total_mass;
                self.point_mass = res.point_mass;
                self.cg = res.cg;
                self.volume_m3 = res.volume_m3;
                if let (Some((lo, hi)), false) = (res.bounds, self.framed) {
                    self.camera.frame(lo, hi);
                    if let Some(v) = self.start_view.take()
                        && !self.camera.set_view(&v)
                    {
                        eprintln!("unknown view `{v}`; try one of {:?}", Camera::VIEWS);
                    }
                    self.framed = true;
                }
            }
        }
    }


    // ----------------------------------------------------------------- the part editor

    /// Edit the open primitive: its dimensions, its shape and its mounting points.
    ///
    /// This is the half of the brief that had nothing behind it. A part used to mean a file
    /// someone wrote by hand; here it is a list of dimensions, a list of shapes and a list of
    /// ports, and the geometry rebuilds as each one changes.
    fn part_panel(&mut self, ui: &mut egui::Ui) {
        let mut acts: Vec<PartAction> = Vec::new();
        let Doc::Primitive(def) = &self.doc else {
            return;
        };
        let def = def.clone();

        ui.horizontal(|ui| {
            for (t, name) in [
                (PartTab::Shape, "Shape"),
                (PartTab::Ports, "Mounting"),
                (PartTab::About, "About"),
            ] {
                if ui.selectable_label(self.part_tab == t, name).clicked() {
                    self.part_tab = t;
                }
            }
        });
        ui.separator();

        match self.part_tab {
            PartTab::Shape => {
                self.part_dimensions(ui, &def, &mut acts);
                ui.separator();
                self.part_geometry(ui, &def, &mut acts);
                ui.separator();
                self.part_variants(ui, &def, &mut acts);
            }
            PartTab::Ports => self.part_ports(ui, &def, &mut acts),
            PartTab::About => self.part_about(ui, &def, &mut acts),
        }

        for a in acts {
            self.apply_part(a);
        }
    }

    fn part_dimensions(&mut self, ui: &mut egui::Ui, def: &PrimitiveDef, acts: &mut Vec<PartAction>) {
        ui.heading("Dimensions");
        ui.label(
            egui::RichText::new(
                "Every shape below is written in terms of these, so changing one here changes \
                 the part everywhere it is used.",
            )
            .weak()
            .small(),
        );
        let scratch = &mut self.scratch;
        for (i, p) in def.params.iter().enumerate() {
            let derived = p.expr.is_some();
            ui.horizontal(|ui| {
                if ui
                    .small_button("x")
                    .on_hover_text("remove this dimension")
                    .clicked()
                {
                    acts.push(PartAction::RemoveParam(p.name.clone()));
                }
                if let Some(v) = edit_text(
                    ui,
                    scratch,
                    &format!("pname:{i}"),
                    &p.name,
                    110.0,
                    "name",
                ) {
                    acts.push(PartAction::RenameParam(p.name.clone(), v));
                }
                if derived {
                    if let Some(v) = edit_text(
                        ui,
                        scratch,
                        &format!("pexpr:{i}"),
                        &p.expr.as_ref().map(wmds_schema::expr_text).unwrap_or_default(),
                        170.0,
                        "computed from the others",
                    ) {
                        acts.push(PartAction::SetParamField(p.name.clone(), "expr".into(), v));
                    }
                    ui.label(egui::RichText::new("computed").weak().small());
                } else {
                    if let Some(v) = edit_text(
                        ui,
                        scratch,
                        &format!("pdef:{i}"),
                        &p.default.as_ref().map(wmds_schema::expr_text).unwrap_or_default(),
                        110.0,
                        "default",
                    ) {
                        acts.push(PartAction::SetParamField(p.name.clone(), "default".into(), v));
                    }
                    if let Some(v) = edit_text(
                        ui,
                        scratch,
                        &format!("pmin:{i}"),
                        &p.min.as_ref().map(wmds_schema::expr_text).unwrap_or_default(),
                        70.0,
                        "min",
                    ) {
                        acts.push(PartAction::SetParamField(p.name.clone(), "min".into(), v));
                    }
                    if let Some(v) = edit_text(
                        ui,
                        scratch,
                        &format!("pmax:{i}"),
                        &p.max.as_ref().map(wmds_schema::expr_text).unwrap_or_default(),
                        70.0,
                        "max",
                    ) {
                        acts.push(PartAction::SetParamField(p.name.clone(), "max".into(), v));
                    }
                }
            });
            if let Some(d) = &p.doc {
                ui.label(egui::RichText::new(format!("      {d}")).weak().small());
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Add a dimension").clicked() {
                acts.push(PartAction::AddParam);
            }
            if !self.part_note.is_empty() {
                ui.colored_label(DANGER, egui::RichText::new(&self.part_note).small());
            }
        });
    }

    fn part_geometry(&mut self, ui: &mut egui::Ui, def: &PrimitiveDef, acts: &mut Vec<PartAction>) {
        ui.heading("Shape");
        for (li, lvl) in def.geometry.iter().enumerate() {
            if def.geometry.len() > 1 {
                ui.label(egui::RichText::new(&lvl.level).strong().small());
            }
            let count = lvl.features.len();
            for (fi, f) in lvl.features.iter().enumerate() {
                let kind = part::feature_kind(&f.op);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&f.op).strong());
                        if let Some(k) = kind {
                            ui.label(egui::RichText::new(k.doc).weak().small());
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("x").on_hover_text("remove").clicked() {
                                acts.push(PartAction::RemoveFeature(li, fi));
                            }
                            if fi + 1 < count && ui.small_button("v").on_hover_text("later").clicked() {
                                acts.push(PartAction::MoveFeature(li, fi, 1));
                            }
                            if fi > 0 && ui.small_button("^").on_hover_text("earlier").clicked() {
                                acts.push(PartAction::MoveFeature(li, fi, -1));
                            }
                        });
                    });
                    if kind.map(|k| !k.args.is_empty()).unwrap_or(true) {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("name").small());
                            if let Some(v) = edit_text(
                                ui,
                                &mut self.scratch,
                                &format!("fname:{li}:{fi}"),
                                f.name.as_deref().unwrap_or(""),
                                120.0,
                                "so a cut can name it",
                            ) {
                                acts.push(PartAction::SetFeatureName(li, fi, v));
                            }
                        });
                    }
                    // A boolean names bodies built before it, so offer those by name rather
                    // than making someone remember them.
                    let earlier = part::bodies_before(def, li, fi);
                    for (key, hint, _) in kind.map(|k| k.args).unwrap_or(&[]) {
                        let current = f
                            .args
                            .get(*key)
                            .map(wmds_schema::expr_text)
                            .unwrap_or_default();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(*key).small().monospace());
                            if kind.map(|k| k.boolean).unwrap_or(false)
                                && matches!(*key, "a" | "b" | "of")
                            {
                                let mut chosen = current.clone();
                                egui::ComboBox::from_id_salt(format!("fb:{li}:{fi}:{key}"))
                                    .width(150.0)
                                    .selected_text(if chosen.is_empty() {
                                        "choose a shape".to_string()
                                    } else {
                                        chosen.clone()
                                    })
                                    .show_ui(ui, |ui| {
                                        for b in &earlier {
                                            ui.selectable_value(&mut chosen, b.clone(), b);
                                        }
                                    });
                                if chosen != current {
                                    acts.push(PartAction::SetFeatureArg(
                                        li,
                                        fi,
                                        (*key).to_string(),
                                        chosen,
                                    ));
                                }
                                if earlier.is_empty() {
                                    ui.colored_label(
                                        DANGER,
                                        egui::RichText::new("nothing is built before this")
                                            .small(),
                                    );
                                }
                            } else if let Some(v) = edit_text(
                                ui,
                                &mut self.scratch,
                                &format!("farg:{li}:{fi}:{key}"),
                                &current,
                                200.0,
                                hint,
                            ) {
                                acts.push(PartAction::SetFeatureArg(li, fi, (*key).to_string(), v));
                            }
                            ui.label(egui::RichText::new(*hint).weak().small());
                        });
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label("add");
                for k in part::FEATURES {
                    if ui.small_button(k.op).on_hover_text(k.doc).clicked() {
                        acts.push(PartAction::AddFeature(li, k.op.to_string()));
                    }
                }
            });
        }
    }

    fn part_variants(&mut self, ui: &mut egui::Ui, def: &PrimitiveDef, acts: &mut Vec<PartAction>) {
        egui::CollapsingHeader::new("Handed versions")
            .default_open(!def.variants.is_empty())
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "A handed part is authored once. The mirrored option reflects both the \
                         shape and the mounting points, so one file serves both sides.",
                    )
                    .weak()
                    .small(),
                );
                for (i, v) in def.variants.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("x").clicked() {
                            acts.push(PartAction::RemoveVariant(v.name.clone()));
                        }
                        ui.label(egui::RichText::new(&v.name).monospace());
                        ui.label(
                            egui::RichText::new(v.options.join(" / ")).small(),
                        );
                        if let Some(m) = &v.mirror_when {
                            ui.label(
                                egui::RichText::new(format!("mirrors on {m} about {}", v.mirror_plane))
                                    .weak()
                                    .small(),
                            );
                        }
                        let _ = i;
                    });
                }
                if ui.button("Add a handed version").clicked() {
                    acts.push(PartAction::AddVariant);
                }
            });
    }

    fn part_ports(&mut self, ui: &mut egui::Ui, def: &PrimitiveDef, acts: &mut Vec<PartAction>) {
        ui.heading("Mounting points");
        ui.label(
            egui::RichText::new(
                "A port is where this part bolts to something else. Its type decides what it \
                 will fit, and the fields below come from that type, so the checker and the \
                 editor can never disagree about what a joint needs.",
            )
            .weak()
            .small(),
        );
        let lib = self.lib.clone();
        let types: Vec<String> = lib
            .as_ref()
            .map(|l| l.port_types.keys().cloned().collect())
            .unwrap_or_default();

        for (i, p) in def.ports.iter().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button("x").on_hover_text("remove").clicked() {
                        acts.push(PartAction::RemovePort(p.name.clone()));
                    }
                    if let Some(v) = edit_text(
                        ui,
                        &mut self.scratch,
                        &format!("portname:{i}"),
                        &p.name,
                        130.0,
                        "name",
                    ) {
                        acts.push(PartAction::SetPortField(p.name.clone(), "name".into(), v));
                    }
                    let mut chosen = p.port_type.clone();
                    egui::ComboBox::from_id_salt(format!("porttype:{i}"))
                        .width(190.0)
                        .selected_text(chosen.clone())
                        .show_ui(ui, |ui| {
                            for t in &types {
                                let doc = lib
                                    .as_ref()
                                    .and_then(|l| l.port_types.get(t))
                                    .map(|d| d.doc.clone())
                                    .unwrap_or_default();
                                ui.selectable_value(&mut chosen, t.clone(), t)
                                    .on_hover_text(doc);
                            }
                        });
                    if chosen != p.port_type {
                        acts.push(PartAction::SetPortType(p.name.clone(), chosen));
                    }
                });
                if let Some(d) = lib.as_ref().and_then(|l| l.port_types.get(&p.port_type)) {
                    ui.label(egui::RichText::new(&d.doc).weak().small());
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("at").small().monospace());
                    if let Some(v) = edit_text(
                        ui,
                        &mut self.scratch,
                        &format!("portat:{i}"),
                        &wmds_schema::expr_text(&p.at),
                        190.0,
                        "(x, y, z)",
                    ) {
                        acts.push(PartAction::SetPortField(p.name.clone(), "at".into(), v));
                    }
                    ui.label(egui::RichText::new("axis").small().monospace());
                    if let Some(v) = edit_text(
                        ui,
                        &mut self.scratch,
                        &format!("portax:{i}"),
                        &part::axis_text(&p.axis),
                        70.0,
                        "z",
                    ) {
                        acts.push(PartAction::SetPortField(p.name.clone(), "axis".into(), v));
                    }
                    ui.label(egui::RichText::new("clock").small().monospace());
                    let clock = p.clock.as_ref().map(part::axis_text).unwrap_or_default();
                    if let Some(v) = edit_text(
                        ui,
                        &mut self.scratch,
                        &format!("portck:{i}"),
                        &clock,
                        70.0,
                        "x",
                    ) {
                        acts.push(PartAction::SetPortField(p.name.clone(), "clock".into(), v));
                    }
                });
                if p.clock.is_none() {
                    ui.colored_label(
                        ACCENT,
                        egui::RichText::new(
                            "no clock: a bolted joint using this port can end up rotated any \
                             way round the axis",
                        )
                        .small(),
                    );
                }
                if let Some(l) = &lib {
                    for slot in part::port_type_params(l, &p.port_type) {
                        let current = p
                            .params
                            .get(&slot.name)
                            .map(wmds_schema::expr_text)
                            .unwrap_or_default();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&slot.name).small().monospace());
                            if let Some(v) = edit_text(
                                ui,
                                &mut self.scratch,
                                &format!("portp:{i}:{}", slot.name),
                                &current,
                                150.0,
                                &slot.start,
                            ) {
                                acts.push(PartAction::SetPortParam(
                                    p.name.clone(),
                                    slot.name.clone(),
                                    v,
                                ));
                            }
                            let label = if slot.optional {
                                format!("{} (optional)", slot.kind)
                            } else {
                                slot.kind.clone()
                            };
                            ui.label(egui::RichText::new(label).weak().small());
                            if current.is_empty() && !slot.optional {
                                ui.colored_label(
                                    DANGER,
                                    egui::RichText::new("required").small(),
                                );
                            }
                        });
                    }
                }
            });
        }

        ui.horizontal(|ui| {
            ui.label("add a mounting point");
            let mut chosen = String::new();
            egui::ComboBox::from_id_salt("addport")
                .width(200.0)
                .selected_text("choose a type")
                .show_ui(ui, |ui| {
                    for t in &types {
                        let doc = lib
                            .as_ref()
                            .and_then(|l| l.port_types.get(t))
                            .map(|d| d.doc.clone())
                            .unwrap_or_default();
                        if ui.selectable_label(false, t).on_hover_text(doc).clicked() {
                            chosen = t.clone();
                        }
                    }
                });
            if !chosen.is_empty() {
                acts.push(PartAction::AddPort(chosen));
            }
        });
    }

    fn part_about(&mut self, ui: &mut egui::Ui, def: &PrimitiveDef, acts: &mut Vec<PartAction>) {
        let lib = self.lib.clone();
        egui::Grid::new("about")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("id");
                if let Some(v) = edit_text(ui, &mut self.scratch, "pid", &def.id, 260.0, "category/family/name") {
                    acts.push(PartAction::SetAbout("id".into(), v));
                }
                ui.end_row();
                ui.label("description");
                if let Some(v) = edit_text(
                    ui,
                    &mut self.scratch,
                    "pdesc",
                    &def.description,
                    260.0,
                    "what it is",
                ) {
                    acts.push(PartAction::SetAbout("description".into(), v));
                }
                ui.end_row();
                ui.label("category");
                if let Some(v) = edit_text(ui, &mut self.scratch, "pcat", &def.category, 160.0, "suspension") {
                    acts.push(PartAction::SetAbout("category".into(), v));
                }
                ui.end_row();
                ui.label("material");
                let mut chosen = def.material.clone().unwrap_or_default();
                let materials: Vec<String> = lib
                    .as_ref()
                    .map(|l| l.materials.keys().cloned().collect())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("pmat")
                    .width(240.0)
                    .selected_text(chosen.clone())
                    .show_ui(ui, |ui| {
                        for m in &materials {
                            let d = lib
                                .as_ref()
                                .and_then(|l| l.materials.get(m))
                                .map(|x| x.description.clone())
                                .unwrap_or_default();
                            ui.selectable_value(&mut chosen, m.clone(), m).on_hover_text(d);
                        }
                    });
                if Some(&chosen) != def.material.as_ref() {
                    acts.push(PartAction::SetAbout("material".into(), chosen));
                }
                ui.end_row();
            });

        ui.separator();
        ui.heading("Mass");
        let declared = matches!(def.massprops, wmds_schema::MassPropsDef::Declared { .. });
        ui.horizontal(|ui| {
            if ui
                .selectable_label(!declared, "from the shape")
                .on_hover_text("volume times the material density")
                .clicked()
                && declared
            {
                acts.push(PartAction::SetMassComputed);
            }
            if ui
                .selectable_label(declared, "declared")
                .on_hover_text("for a bought-in part drawn as an envelope")
                .clicked()
                && !declared
            {
                acts.push(PartAction::SetMassDeclared);
            }
        });
        if let wmds_schema::MassPropsDef::Declared { mass, cg, .. } = &def.massprops {
            ui.horizontal(|ui| {
                ui.label("mass");
                if let Some(v) = edit_text(
                    ui,
                    &mut self.scratch,
                    "pmass",
                    &wmds_schema::expr_text(mass),
                    100.0,
                    "3.4 kg",
                ) {
                    acts.push(PartAction::SetAbout("mass".into(), v));
                }
                ui.label("centre");
                let c = cg.as_ref().map(wmds_schema::expr_text).unwrap_or_default();
                if let Some(v) = edit_text(ui, &mut self.scratch, "pcg", &c, 180.0, "(0 mm, 0 mm, 0 mm)") {
                    acts.push(PartAction::SetAbout("cg".into(), v));
                }
            });
        }

        ui.separator();
        ui.heading("Compliance tags");
        ui.label(
            egui::RichText::new(
                "What this part claims to be. The rules read these rather than part names, so a \
                 bought-in part satisfies a rule as long as it says what it is.",
            )
            .weak()
            .small(),
        );
        if let Some(v) = edit_text(
            ui,
            &mut self.scratch,
            "ptags",
            &def.compliance_tags.join(" "),
            340.0,
            "braking braking.disc",
        ) {
            acts.push(PartAction::SetAbout("tags".into(), v));
        }
    }

    /// Apply one edit to the open part, then rebuild.
    fn apply_part(&mut self, a: PartAction) {
        let lib = self.lib.clone();
        let Doc::Primitive(def) = &mut self.doc else {
            return;
        };
        self.part_note.clear();
        match a {
            PartAction::AddParam => {
                let n = part::add_param(def);
                self.status = format!("added dimension {n}");
            }
            PartAction::RemoveParam(name) => match part::remove_param(def, &name) {
                Ok(()) => self.status = format!("removed dimension {name}"),
                Err(uses) => {
                    // Refusing is the useful behaviour: deleting a dimension the shape depends
                    // on produces a part that fails to resolve, and the error appears nowhere
                    // near the cause.
                    self.part_note = format!(
                        "{name} is still used by {}. Change those first.",
                        uses.join(", ")
                    );
                }
            },
            PartAction::RenameParam(from, to) => {
                part::rename_param(def, &from, &to);
                self.status = format!("renamed {from} to {to} everywhere it was used");
            }
            PartAction::SetParamField(name, field, value) => {
                if let Some(p) = def.params.iter_mut().find(|p| p.name == name) {
                    let e = if value.trim().is_empty() {
                        None
                    } else {
                        Some(part::text_expr(&value))
                    };
                    match field.as_str() {
                        "default" => p.default = e,
                        "min" => p.min = e,
                        "max" => p.max = e,
                        "expr" => p.expr = e,
                        _ => {}
                    }
                }
            }
            PartAction::AddFeature(level, op) => {
                part::add_feature(def, level, &op);
                self.status = format!("added a {op}");
            }
            PartAction::RemoveFeature(level, index) => part::remove_feature(def, level, index),
            PartAction::MoveFeature(level, index, d) => part::move_feature(def, level, index, d),
            PartAction::SetFeatureName(level, index, name) => {
                part::set_feature_name(def, level, index, &name)
            }
            PartAction::SetFeatureArg(level, index, key, value) => {
                part::set_feature_arg(def, level, index, &key, &value)
            }
            PartAction::AddVariant => {
                part::add_variant(def);
                self.status = "added a handed version; the mirrored option flips shape and ports".into();
            }
            PartAction::RemoveVariant(name) => part::remove_variant(def, &name),
            PartAction::AddPort(t) => {
                if let Some(l) = &lib {
                    let n = part::add_port(def, l, &t);
                    self.status = format!("added mounting point {n}");
                }
            }
            PartAction::RemovePort(name) => part::remove_port(def, &name),
            PartAction::SetPortType(name, t) => {
                if let Some(l) = &lib {
                    part::set_port_type(def, l, &name, &t);
                }
            }
            PartAction::SetPortField(name, field, value) => {
                part::set_port_field(def, &name, &field, &value)
            }
            PartAction::SetPortParam(name, key, value) => {
                part::set_port_param(def, &name, &key, &value)
            }
            PartAction::SetMassComputed => def.massprops = wmds_schema::MassPropsDef::Computed,
            PartAction::SetMassDeclared => {
                def.massprops = wmds_schema::MassPropsDef::Declared {
                    mass: part::text_expr("1 kg"),
                    cg: Some(part::text_expr("(0 mm, 0 mm, 0 mm)")),
                    inertia: None,
                }
            }
            PartAction::SetAbout(field, value) => match field.as_str() {
                "id" => def.id = value,
                "description" => def.description = value,
                "category" => def.category = value,
                "material" => {
                    def.material = if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    }
                }
                "tags" => {
                    def.compliance_tags =
                        value.split_whitespace().map(|s| s.to_string()).collect()
                }
                "mass" => {
                    if let wmds_schema::MassPropsDef::Declared { mass, .. } = &mut def.massprops {
                        *mass = part::text_expr(&value);
                    }
                }
                "cg" => {
                    if let wmds_schema::MassPropsDef::Declared { cg, .. } = &mut def.massprops {
                        *cg = Some(part::text_expr(&value));
                    }
                }
                _ => {}
            },
        }
        // The preview sliders are derived from the definition, so they have to follow it.
        let refreshed = match &self.doc {
            Doc::Primitive(d) => Some((param_rows(d), d.variants.clone())),
            _ => None,
        };
        if let Some((rows, variants)) = refreshed {
            let keep: HashMap<String, f64> =
                self.params.iter().map(|p| (p.name.clone(), p.value)).collect();
            self.params = rows;
            // Keep whatever the person had set on the preview sliders for dimensions that still
            // exist, so editing the shape does not silently reset the view.
            for p in &mut self.params {
                if !p.derived
                    && let Some(v) = keep.get(&p.name)
                {
                    p.value = (*v).clamp(p.min.min(p.max), p.max.max(p.min));
                }
            }
            self.variants = variants
                .iter()
                .map(|v| (v.name.clone(), v.options.clone(), 0))
                .collect();
        }
        self.unsaved = true;
        self.dirty = true;
    }

    /// Write the open part to its file.
    fn save_part(&mut self) {
        let Doc::Primitive(def) = &self.doc else {
            return;
        };
        let path = PathBuf::from(self.path_text.trim());
        if path.as_os_str().is_empty() {
            self.status = "give the part a file name first".into();
            return;
        }
        // Refuse to write something that cannot be read back, rather than corrupting the file.
        let text = wmds_schema::write_primitive(def);
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "part.prim.kdl".into());
        if let Err(e) = wmds_schema::parse_primitive(&name, &text) {
            self.status = format!("not saved: the part is not valid yet. {}", miette_string(e));
            return;
        }
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            self.status = format!("cannot make {}: {e}", parent.display());
            return;
        }
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.file = Some(path.clone());
                self.unsaved = false;
                self.status = format!("saved {}", path.display());
                // A new or changed part has to reach the catalogue, or it cannot be added to a
                // vehicle until the application is restarted.
                self.load_library();
            }
            Err(e) => self.status = format!("cannot write {}: {e}", path.display()),
        }
    }

    /// Start a new part from scratch.
    fn new_part(&mut self) {
        let id = if self.new_part_id.trim().is_empty() {
            "misc/new-part".to_string()
        } else {
            self.new_part_id.trim().to_string()
        };
        let def = part::new_primitive(&id);
        self.params = param_rows(&def);
        self.variants = Vec::new();
        self.path_text = format!("library/{}.prim.kdl", id);
        self.doc = Doc::Primitive(Box::new(def));
        self.file = None;
        self.scratch.clear();
        self.part_tab = PartTab::Shape;
        self.load_error = None;
        self.status = format!("new part {id}: set its dimensions, shape and mounting points");
        self.unsaved = true;
        self.framed = false;
        self.dirty = true;
    }

    // ----------------------------------------------------------------- the toolbar

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let mut actions: Vec<Action> = Vec::new();
        ui.horizontal(|ui| {
            if ui.button("New vehicle").clicked() {
                actions.push(Action::NewVehicle);
            }
            if ui.button("New part").clicked() {
                actions.push(Action::NewPart);
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.path_text)
                    .desired_width(340.0)
                    .hint_text("vehicles/my-car/my-car.veh.kdl"),
            );
            if ui.button("Open").clicked() {
                let p = self.path_text.clone();
                actions.push(Action::Open(p));
            }
            let save = ui.add_enabled(
                !matches!(self.doc, Doc::None),
                egui::Button::new(if self.unsaved { "Save *" } else { "Save" }),
            );
            if save.clicked() {
                actions.push(Action::Save);
            }
            if self.unsaved {
                ui.colored_label(ACCENT, "unsaved changes");
            }
            ui.separator();
            ui.label("view");
            for v in Camera::VIEWS {
                if ui.small_button(v).clicked() {
                    self.camera.set_view(v);
                }
            }
            ui.separator();
            if self.building {
                ui.spinner();
                ui.label("rebuilding");
            } else if let Some(ms) = self.build_ms {
                let note = if self.cache_note.is_empty() {
                    format!("{ms} ms · {KERNEL_NAME}")
                } else {
                    format!("{ms} ms · {KERNEL_NAME} · {}", self.cache_note)
                };
                ui.label(egui::RichText::new(note).weak().monospace());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !self.status.is_empty() {
                    ui.label(egui::RichText::new(&self.status).weak());
                }
            });
        });
        for a in actions {
            self.apply(a);
        }
    }

    // ----------------------------------------------------------------- the side panel

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        let mut actions: Vec<Action> = Vec::new();

        // Header: what is open.
        match &self.doc {
            Doc::None => {
                ui.heading("Nothing open");
                ui.label("Start a new vehicle, or open one from the box above.");
            }
            Doc::Primitive(d) => {
                ui.heading(&d.id);
                if !d.description.is_empty() {
                    ui.label(egui::RichText::new(&d.description).italics());
                }
                ui.label("A single part. Vehicles are where parts get put together.");
            }
            Doc::Assembly(d) => {
                ui.heading(&d.id);
                if !d.description.is_empty() {
                    ui.label(egui::RichText::new(&d.description).italics());
                }
                ui.label(
                    egui::RichText::new(format!(
                        "{} part(s), {} joint(s)",
                        d.instances.len(),
                        d.mates.len()
                    ))
                    .weak(),
                );
            }
        }
        ui.label(egui::RichText::new(&self.lib_note).weak());
        if let Some(e) = &self.load_error {
            ui.colored_label(DANGER, shorten(e, 400));
        }
        ui.separator();

        if matches!(self.doc, Doc::Primitive(_)) {
            self.part_panel(ui);
            ui.separator();
            self.primitive_preview(ui);
            return;
        }
        if matches!(self.doc, Doc::None) {
            return;
        }

        ui.horizontal(|ui| {
            for (t, name) in [
                (Tab::Parts, "Parts"),
                (Tab::Joints, "Joints"),
                (Tab::Check, "Check"),
            ] {
                if ui.selectable_label(self.tab == t, name).clicked() {
                    self.tab = t;
                }
            }
        });
        ui.separator();

        match self.tab {
            Tab::Parts => {
                self.chassis_section(ui, &mut actions);
                ui.separator();
                self.catalogue_section(ui, &mut actions);
                ui.separator();
                self.parts_section(ui, &mut actions);
                ui.separator();
                self.selected_section(ui, &mut actions);
            }
            Tab::Joints => self.joints_tab(ui, &mut actions),
            Tab::Check => self.check_tab(ui),
        }

        for a in actions {
            self.apply(a);
        }
    }

    /// The preview sliders: try the part at other sizes without changing its defaults.
    fn primitive_preview(&mut self, ui: &mut egui::Ui) {
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
                            if ui
                                .add(
                                    egui::Slider::new(&mut p.value, p.min..=p.max)
                                        .suffix(format!(" {}", p.unit)),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
        }
        if changed {
            self.dirty = true;
        }
    }

    fn chassis_section(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(lib) = self.lib.clone() else { return };
        egui::CollapsingHeader::new("Chassis")
            .default_open(self.chassis_ui.system.is_empty())
            .show(ui, |ui| {
                if lib.chassis.is_empty() {
                    ui.label("no chassis systems in the library");
                    return;
                }
                let before = self.chassis_ui.clone();
                let systems: Vec<String> = lib.chassis.keys().cloned().collect();
                if self.chassis_ui.system.is_empty() {
                    self.chassis_ui.system = systems[0].clone();
                }
                combo(ui, "system", &mut self.chassis_ui.system, &systems);
                let Some(cdef) = lib.chassis.get(&self.chassis_ui.system) else {
                    return;
                };
                let configs: Vec<String> = cdef.configurations.keys().cloned().collect();
                if !configs.contains(&self.chassis_ui.configuration) {
                    self.chassis_ui.configuration = configs.first().cloned().unwrap_or_default();
                }
                combo(ui, "length", &mut self.chassis_ui.configuration, &configs);

                let widths: Vec<String> = cdef.width_configs.keys().cloned().collect();
                if !widths.contains(&self.chassis_ui.width) {
                    self.chassis_ui.width = widths.first().cloned().unwrap_or_default();
                }
                combo(ui, "width", &mut self.chassis_ui.width, &widths);

                let rails: Vec<String> = cdef.rail_sections.keys().cloned().collect();
                if !rails.contains(&self.chassis_ui.rail_section) {
                    self.chassis_ui.rail_section = rails.first().cloned().unwrap_or_default();
                }
                combo(ui, "rail", &mut self.chassis_ui.rail_section, &rails);

                // The chosen length is a list of sections; each one gets its own dimension.
                let kinds: Vec<String> = cdef
                    .configurations
                    .get(&self.chassis_ui.configuration)
                    .cloned()
                    .unwrap_or_default();
                self.chassis_ui.lengths.retain(|k, _| kinds.contains(k));
                for kind in &kinds {
                    let sk = cdef.section_kinds.get(kind);
                    let (lo, hi) = sk
                        .map(|s| {
                            (
                                s.length_min.to_unit("mm").unwrap_or(500.0),
                                s.length_max.to_unit("mm").unwrap_or(3000.0),
                            )
                        })
                        .unwrap_or((500.0, 3000.0));
                    let entry = self
                        .chassis_ui
                        .lengths
                        .entry(kind.clone())
                        .or_insert(((lo + hi) * 0.5).round());
                    *entry = entry.clamp(lo, hi);
                    ui.add(
                        egui::Slider::new(entry, lo..=hi)
                            .text(format!("{kind} length"))
                            .suffix(" mm")
                            .step_by(50.0),
                    );
                }
                if self.chassis_ui != before || ui.button("Apply chassis").clicked() {
                    actions.push(Action::SetChassis);
                }
                ui.label(
                    egui::RichText::new(
                        "Changing the chassis moves the mounting grid, so joints made to old \
                         station numbers may land elsewhere. The Check tab lists any that broke.",
                    )
                    .weak()
                    .small(),
                );
            });
    }

    fn catalogue_section(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let catalogue = self.catalogue.clone();
        egui::CollapsingHeader::new("Add a part")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("find");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .desired_width(200.0)
                            .hint_text("battery, wishbone, upright"),
                    );
                    if ui.small_button("clear").clicked() {
                        self.search.clear();
                    }
                });
                let needle = self.search.to_lowercase();
                let matches: Vec<usize> = catalogue
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| {
                        needle.is_empty()
                            || e.id.to_lowercase().contains(&needle)
                            || e.description.to_lowercase().contains(&needle)
                            || e.category.to_lowercase().contains(&needle)
                    })
                    .map(|(i, _)| i)
                    .collect();
                if matches.is_empty() {
                    ui.label(egui::RichText::new("nothing in the library matches").weak());
                    return;
                }
                egui::ScrollArea::vertical()
                    .max_height(230.0)
                    .id_salt("catalogue")
                    .show(ui, |ui| {
                        let mut last_category = String::new();
                        for i in matches {
                            let e = &catalogue[i];
                            if e.category != last_category {
                                last_category = e.category.clone();
                                ui.label(egui::RichText::new(&e.category).strong().small());
                            }
                            ui.horizontal(|ui| {
                                if ui.small_button("add").clicked() {
                                    actions.push(Action::Add(i));
                                }
                                let name = e.id.rsplit('/').next().unwrap_or(&e.id);
                                let label = if e.kind == EntryKind::Assembly {
                                    egui::RichText::new(format!("{name}  (sub-assembly)"))
                                } else {
                                    egui::RichText::new(name)
                                };
                                ui.label(label).on_hover_text(format!(
                                    "{}\n{}",
                                    e.id,
                                    if e.description.is_empty() {
                                        "no description"
                                    } else {
                                        &e.description
                                    }
                                ));
                            });
                        }
                    });
            });
    }

    fn parts_section(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let rows: Vec<(String, String, bool)> = match self.assembly() {
            Some(d) => d
                .instances
                .iter()
                .map(|i| {
                    (
                        i.id.clone(),
                        match &i.source {
                            wmds_schema::InstanceSource::Primitive(p) => p.clone(),
                            wmds_schema::InstanceSource::Assembly(a) => a.clone(),
                        },
                        d.root.as_deref() == Some(i.id.as_str()),
                    )
                })
                .collect(),
            None => Vec::new(),
        };
        // The chassis is generated rather than listed, but it is a part of the vehicle and
        // leaving it out of the list makes the list a lie.
        let has_chassis = self
            .assembly()
            .map(|d| d.chassis.is_some())
            .unwrap_or(false);
        ui.heading(format!("In this vehicle ({})", rows.len() + has_chassis as usize));
        if has_chassis {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("chassis").monospace());
                ui.label(egui::RichText::new("generated").weak().small());
            });
        }
        let placed: HashMap<&str, &PartRow> = self.parts.iter().map(|p| (p.id.as_str(), p)).collect();
        for (id, source, is_root) in &rows {
            ui.horizontal(|ui| {
                let selected = self.selected.as_deref() == Some(id.as_str());
                let unplaced = placed
                    .get(id.as_str())
                    .map(|p| p.how == "NOT PLACED")
                    .unwrap_or(false);
                let mut text = egui::RichText::new(id).monospace();
                if unplaced {
                    text = text.color(DANGER);
                }
                if ui
                    .selectable_label(selected, text)
                    .on_hover_text(format!(
                        "{source}\n{}",
                        placed.get(id.as_str()).map(|p| p.how.as_str()).unwrap_or("")
                    ))
                    .clicked()
                {
                    actions.push(Action::Select(id.clone()));
                }
                if *is_root {
                    ui.label(egui::RichText::new("root").small().color(GOOD));
                }
                if unplaced {
                    ui.label(egui::RichText::new("not joined to anything").small().color(DANGER));
                }
            });
        }
        if rows.is_empty() && !has_chassis {
            ui.label(egui::RichText::new("empty. add a chassis, then parts.").weak());
        }
    }

    fn selected_section(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(sel) = self.selected.clone() else {
            ui.label(egui::RichText::new("select a part to change its dimensions").weak());
            return;
        };
        ui.heading(&sel);
        ui.horizontal(|ui| {
            if ui.button("Delete").clicked() {
                actions.push(Action::Delete(sel.clone()));
            }
            if ui
                .button("Make root")
                .on_hover_text("the one part that stays put; everything else is placed from it")
                .clicked()
            {
                actions.push(Action::MakeRoot(sel.clone()));
            }
            if ui.button("Join this…").clicked() {
                self.tab = Tab::Joints;
                self.clear_joint();
            }
        });
        // Explicit placement, for the cases a joint cannot express: a part that hangs off
        // nothing yet, or one deliberately positioned while its mounting is being worked out.
        let placed_by_hand = self
            .assembly()
            .and_then(|d| d.instances.iter().find(|i| i.id == sel))
            .and_then(|i| i.placement.as_ref())
            .map(|p| wmds_schema::expr_text(&p.at));
        ui.horizontal(|ui| {
            ui.label("position");
            match &placed_by_hand {
                Some(at) => {
                    if let Some(v) = edit_text(
                        ui,
                        &mut self.scratch,
                        &format!("place:{sel}"),
                        at,
                        180.0,
                        "(0 mm, 0 mm, 0 mm)",
                    ) {
                        actions.push(Action::SetPlacement(sel.clone(), Some(v)));
                    }
                    if ui
                        .small_button("use joints")
                        .on_hover_text("go back to being positioned by what it bolts to")
                        .clicked()
                    {
                        actions.push(Action::SetPlacement(sel.clone(), None));
                    }
                }
                None => {
                    ui.label(egui::RichText::new("from its joints").weak());
                    if ui
                        .small_button("place by hand")
                        .on_hover_text(
                            "positions it at fixed coordinates instead; a joint is better                              wherever one exists, because it follows the chassis",
                        )
                        .clicked()
                    {
                        actions.push(Action::SetPlacement(
                            sel.clone(),
                            Some("(0 mm, 0 mm, 0 mm)".into()),
                        ));
                    }
                }
            }
        });

        if self.inst_params.is_empty() {
            ui.label(egui::RichText::new("this part has nothing to adjust").weak());
            return;
        }
        for p in &mut self.inst_params {
            if !p.options.is_empty() {
                let before = p.text.clone();
                egui::ComboBox::from_id_salt(format!("variant_{}", p.name))
                    .selected_text(p.text.clone())
                    .show_ui(ui, |ui| {
                        for o in &p.options {
                            ui.selectable_value(&mut p.text, o.clone(), o);
                        }
                    });
                ui.label(egui::RichText::new(&p.name).small().weak());
                if p.text != before {
                    actions.push(Action::SetVariant(sel.clone(), p.name.clone(), p.text.clone()));
                }
                continue;
            }
            ui.horizontal(|ui| {
                if p.numeric {
                    let r = ui.add(
                        egui::Slider::new(&mut p.value, p.min..=p.max)
                            .text(&p.name)
                            .suffix(format!(" {}", p.unit)),
                    );
                    if r.drag_stopped() || r.lost_focus() || (r.changed() && !r.dragged()) {
                        let text = if p.unit.is_empty() {
                            format!("{}", round3(p.value))
                        } else {
                            format!("{} {}", round3(p.value), p.unit)
                        };
                        actions.push(Action::SetParam(
                            sel.clone(),
                            p.name.clone(),
                            Some(text),
                        ));
                    }
                    r.on_hover_text(&p.doc);
                } else {
                    ui.label(&p.name).on_hover_text(&p.doc);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut p.text)
                            .desired_width(140.0)
                            .hint_text(p.default_text.clone()),
                    );
                    if r.lost_focus() {
                        let v = if p.text.trim().is_empty() {
                            None
                        } else {
                            Some(p.text.clone())
                        };
                        actions.push(Action::SetParam(sel.clone(), p.name.clone(), v));
                    }
                }
                if p.overridden {
                    if ui
                        .small_button("reset")
                        .on_hover_text(format!("back to the library value: {}", p.default_text))
                        .clicked()
                    {
                        actions.push(Action::SetParam(sel.clone(), p.name.clone(), None));
                    }
                } else {
                    ui.label(egui::RichText::new("library").weak().small());
                }
            });
        }
    }

    // ----------------------------------------------------------------- joints

    fn joints_tab(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.heading("Make a joint");
        ui.label(
            egui::RichText::new(
                "Click a yellow port in the 3D view, or pick one below. Only ports that \
                 actually fit are offered second.",
            )
            .weak()
            .small(),
        );

        let mateable = self.mateable.clone();

        // First port.
        ui.horizontal(|ui| {
            ui.label("from");
            let text = match &self.joint_a {
                Some((u, p)) => format!("{u}.{p}"),
                None => "choose a port".into(),
            };
            egui::ComboBox::from_id_salt("joint_a")
                .width(280.0)
                .selected_text(text)
                .show_ui(ui, |ui| {
                    for u in mateable.iter() {
                        for p in &u.ports {
                            if self.used_ports.contains(&(u.unit.clone(), p.name.clone())) {
                                continue;
                            }
                            let label = format!("{}.{}", u.unit, p.name);
                            if ui.selectable_label(false, &label).clicked() {
                                actions.push(Action::PickPort(u.unit.clone(), p.name.clone()));
                            }
                        }
                    }
                });
            if self.joint_a.is_some() && ui.small_button("clear").clicked() {
                actions.push(Action::ClearJoint);
            }
        });

        if let Some((au, ap)) = self.joint_a.clone() {
            if let Some(a) = find_port(&mateable, &au, &ap) {
                ui.label(
                    egui::RichText::new(format!("a {} port on {au}", a.port_type))
                        .weak()
                        .small(),
                );
            }
            ui.horizontal(|ui| {
                ui.label("to");
                let text = match &self.joint_b {
                    Some((u, p)) => format!("{u}.{p}"),
                    None => "choose what it bolts to".into(),
                };
                egui::ComboBox::from_id_salt("joint_b")
                    .width(280.0)
                    .selected_text(text)
                    .show_ui(ui, |ui| {
                        for (u, p, d) in &self.joint_candidates {
                            let label = format!("{u}.{p}   {:.0} mm away", d * 1e3);
                            if ui.selectable_label(false, label).clicked() {
                                actions.push(Action::PickPort(u.clone(), p.clone()));
                            }
                        }
                    });
            });
            let col = if self.joint_candidates.is_empty() { DANGER } else { GOOD };
            ui.colored_label(col, egui::RichText::new(&self.joint_note).small());
        }

        let ready = self.joint_a.is_some() && self.joint_b.is_some();
        if ui
            .add_enabled(ready, egui::Button::new("Bolt them together"))
            .clicked()
        {
            actions.push(Action::AddJoint);
        }

        ui.separator();
        let mates: Vec<(String, String, String, String, String, bool)> = match self.assembly() {
            Some(d) => d
                .mates
                .iter()
                .map(|m| {
                    let broken = self
                        .mateable
                        .iter()
                        .find(|u| u.unit == m.a.instance)
                        .map(|u| !u.ports.iter().any(|p| p.name == m.a.port))
                        .unwrap_or(true)
                        || self
                            .mateable
                            .iter()
                            .find(|u| u.unit == m.b.instance)
                            .map(|u| !u.ports.iter().any(|p| p.name == m.b.port))
                            .unwrap_or(true);
                    (
                        m.id.clone(),
                        m.a.instance.clone(),
                        m.a.port.clone(),
                        m.b.instance.clone(),
                        m.b.port.clone(),
                        broken,
                    )
                })
                .collect(),
            None => Vec::new(),
        };
        ui.heading(format!("Joints ({})", mates.len()));
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .id_salt("mates")
            .show(ui, |ui| {
                for (id, ai, ap, bi, bp, broken) in &mates {
                    let open = self.editing_joint.as_deref() == Some(id.as_str());
                    ui.horizontal(|ui| {
                        if ui.small_button("x").on_hover_text("remove this joint").clicked() {
                            actions.push(Action::DeleteJoint(id.clone()));
                        }
                        if ui
                            .small_button(if open { "done" } else { "move" })
                            .on_hover_text("change what this joint connects to")
                            .clicked()
                        {
                            actions.push(Action::EditJoint(if open {
                                None
                            } else {
                                Some(id.clone())
                            }));
                        }
                        let t = format!("{ai}.{ap}  to  {bi}.{bp}");
                        let rt = if *broken {
                            egui::RichText::new(t).monospace().small().color(DANGER)
                        } else {
                            egui::RichText::new(t).monospace().small()
                        };
                        ui.label(rt).on_hover_text(if *broken {
                            "one of these ports no longer exists"
                        } else {
                            id.as_str()
                        });
                    });
                    if open {
                        // Each end can be moved to any port the other end will still accept.
                        for (end, unit, port) in
                            [(JointEnd::A, ai, ap), (JointEnd::B, bi, bp)]
                        {
                            let options = self.retarget_options(id, end);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(match end {
                                        JointEnd::A => "  from",
                                        JointEnd::B => "  to  ",
                                    })
                                    .small(),
                                );
                                egui::ComboBox::from_id_salt(format!("rt:{id}:{end:?}"))
                                    .width(260.0)
                                    .selected_text(format!("{unit}.{port}"))
                                    .show_ui(ui, |ui| {
                                        for (u, p, d) in &options {
                                            let label =
                                                format!("{u}.{p}   {:.0} mm away", d * 1e3);
                                            if ui.selectable_label(false, label).clicked() {
                                                actions.push(Action::RetargetJoint(
                                                    id.clone(),
                                                    end,
                                                    u.clone(),
                                                    p.clone(),
                                                ));
                                            }
                                        }
                                    });
                                ui.label(
                                    egui::RichText::new(format!("{} fit", options.len()))
                                        .weak()
                                        .small(),
                                );
                            });
                        }
                    }
                }
                if mates.is_empty() {
                    ui.label(egui::RichText::new("nothing is bolted together yet").weak());
                }
            });
    }

    fn check_tab(&mut self, ui: &mut egui::Ui) {
        if self.total_mass > 0.0 || self.point_mass > 0.0 {
            ui.heading("Mass");
            ui.label(format!("{:.1} kg modelled", self.total_mass));
            if self.point_mass > 0.0 {
                ui.label(format!("{:.1} kg declared point masses", self.point_mass));
                ui.label(format!("{:.1} kg total", self.total_mass + self.point_mass));
            }
            ui.label(format!(
                "centre of gravity ({:.0}, {:.0}, {:.0}) mm",
                self.cg[0] * 1e3,
                self.cg[1] * 1e3,
                self.cg[2] * 1e3
            ));
            ui.separator();
        }

        let unplaced: Vec<&PartRow> = self.parts.iter().filter(|p| p.how == "NOT PLACED").collect();
        ui.heading("Problems");
        if let Some(e) = &self.build_error {
            for line in e.lines() {
                ui.colored_label(DANGER, egui::RichText::new(line).small());
            }
        }
        if !unplaced.is_empty() {
            ui.colored_label(
                DANGER,
                format!(
                    "{} part(s) are not joined to anything, so they sit at the origin",
                    unplaced.len()
                ),
            );
            for p in &unplaced {
                ui.label(egui::RichText::new(format!("  {}", p.id)).monospace().small());
            }
        }
        if self.build_error.is_none() && unplaced.is_empty() {
            ui.colored_label(GOOD, "everything resolves and every part is placed");
        }

        if !self.warnings.is_empty() {
            ui.separator();
            ui.heading(format!("Warnings ({})", self.warnings.len()));
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .id_salt("warnings")
                .show(ui, |ui| {
                    for w in &self.warnings {
                        ui.label(egui::RichText::new(w).small().weak());
                    }
                });
        }

        ui.separator();
        ui.heading("Parts placed");
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .id_salt("parts_detail")
            .show(ui, |ui| {
                egui::Grid::new("parts")
                    .num_columns(3)
                    .spacing([8.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for p in &self.parts {
                            ui.label(egui::RichText::new(&p.id).monospace().small())
                                .on_hover_text(format!("{}\n{}", p.source, p.how));
                            ui.label(
                                egui::RichText::new(format!(
                                    "{:.0}, {:.0}, {:.0}",
                                    p.position[0] * 1e3,
                                    p.position[1] * 1e3,
                                    p.position[2] * 1e3
                                ))
                                .small(),
                            );
                            match p.mass {
                                Some(m) => {
                                    let t = egui::RichText::new(format!("{m:.1} kg")).small();
                                    ui.label(if p.declared { t.strong() } else { t });
                                }
                                None => {
                                    ui.label("-");
                                }
                            }
                            ui.end_row();
                        }
                    });
            });

        ui.separator();
        ui.checkbox(&mut self.show_ports, "show ports");
        ui.checkbox(&mut self.show_cg, "show centre of gravity");
        if !self.has_renderer {
            ui.colored_label(
                egui::Color32::YELLOW,
                "no wgpu render state: the viewport is disabled",
            );
        }
    }

    // ----------------------------------------------------------------- the 3D view

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let mut actions: Vec<Action> = Vec::new();
        let size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        viewport::handle_input(&mut self.camera, ui, &response);
        if ui.input(|i| i.key_pressed(egui::Key::F)) {
            self.framed = false;
            self.dirty = true;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.clear_joint();
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

        // Axes.
        let axis_len = self.camera.distance * 0.08;
        for (dir, color, label) in [
            (Vec3::X, egui::Color32::from_rgb(230, 80, 80), "x"),
            (Vec3::Y, egui::Color32::from_rgb(80, 200, 80), "y"),
            (Vec3::Z, egui::Color32::from_rgb(90, 140, 255), "z"),
        ] {
            if let (Some(a), Some(b)) = (
                self.camera.project(rect, Vec3::ZERO),
                self.camera.project(rect, dir * axis_len),
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

        // The selected part, boxed so it can be picked out of a full vehicle.
        if let Some(sel) = &self.selected
            && let Some((lo, hi)) = self.part_bounds.get(sel).copied()
        {
            draw_box(&painter, &self.camera, rect, lo, hi, ACCENT);
            if let Some(p) = self.camera.project(rect, (lo + hi) * 0.5) {
                painter.text(
                    p + egui::vec2(0.0, -12.0),
                    egui::Align2::CENTER_BOTTOM,
                    sel,
                    egui::FontId::proportional(13.0),
                    ACCENT,
                );
            }
        }

        // Ports, and picking them.
        let mut hits: Vec<(String, String, egui::Pos2)> = Vec::new();
        if self.show_ports {
            let candidates: HashSet<(&str, &str)> = self
                .joint_candidates
                .iter()
                .map(|(u, p, _)| (u.as_str(), p.as_str()))
                .collect();
            let picking = self.tab == Tab::Joints;
            let label_them = self.mateable.iter().map(|u| u.ports.len()).sum::<usize>() <= 24;
            for u in self.mateable.iter() {
                let unit_selected = self.selected.as_deref() == Some(u.unit.as_str());
                for p in &u.ports {
                    let key = (u.unit.clone(), p.name.clone());
                    let used = self.used_ports.contains(&key);
                    let is_a = self.joint_a.as_ref() == Some(&key);
                    let is_b = self.joint_b.as_ref() == Some(&key);
                    let is_candidate = candidates.contains(&(u.unit.as_str(), p.name.as_str()));

                    // While a joint is being made, show what matters: the pick and its options.
                    // Otherwise show what is already bolted, plus the selected part's own ports.
                    let show = if picking && self.joint_a.is_some() {
                        is_a || is_b || is_candidate
                    } else if picking {
                        !used || unit_selected
                    } else {
                        used || unit_selected
                    };
                    if !show {
                        continue;
                    }
                    let world = Vec3::new(p.world[0] as f32, p.world[1] as f32, p.world[2] as f32);
                    let Some(s) = self.camera.project(rect, world) else {
                        continue;
                    };
                    let (col, r) = if is_a {
                        (egui::Color32::from_rgb(120, 220, 255), 6.0)
                    } else if is_b {
                        (GOOD, 6.0)
                    } else if is_candidate {
                        (ACCENT, 4.5)
                    } else if used {
                        (egui::Color32::from_rgb(150, 150, 160), 2.5)
                    } else {
                        (egui::Color32::from_rgb(200, 170, 90), 3.0)
                    };
                    painter.circle(s, r, col, egui::Stroke::new(1.0, egui::Color32::BLACK));
                    if is_a || is_b || (label_them && !used) {
                        painter.text(
                            s + egui::vec2(7.0, -7.0),
                            egui::Align2::LEFT_BOTTOM,
                            format!("{}.{}", u.unit, p.name),
                            egui::FontId::proportional(11.0),
                            col,
                        );
                    }
                    if picking && !used {
                        hits.push((u.unit.clone(), p.name.clone(), s));
                    }
                }
            }
        }

        // A click near a drawn port picks it. Dragging still orbits, so this never fights the
        // camera.
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let mut best: Option<(f32, &(String, String, egui::Pos2))> = None;
            for h in &hits {
                let d = h.2.distance(pos);
                if d < 14.0 && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                    best = Some((d, h));
                }
            }
            if let Some((_, h)) = best {
                actions.push(Action::PickPort(h.0.clone(), h.1.clone()));
            }
        }

        if self.show_cg && self.total_mass > 0.0 {
            let cg = Vec3::new(self.cg[0] as f32, self.cg[1] as f32, self.cg[2] as f32);
            if let Some(p) = self.camera.project(rect, cg) {
                let col = egui::Color32::from_rgb(255, 120, 200);
                painter.circle_filled(p, 5.0, col);
                painter.circle_stroke(p, 9.0, egui::Stroke::new(1.5, col));
                painter.text(
                    p + egui::vec2(12.0, -4.0),
                    egui::Align2::LEFT_CENTER,
                    "cg",
                    egui::FontId::proportional(12.0),
                    col,
                );
            }
        }

        painter.text(
            rect.left_bottom() + egui::vec2(10.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            "drag orbit   shift-drag pan   wheel zoom   F frame   click a port to join   Esc cancel",
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(120),
        );

        for a in actions {
            self.apply(a);
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else {
            return;
        };
        let settled =
            self.mesh.is_some() || self.build_error.is_some() || self.load_error.is_some();
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
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(2.0);
            self.toolbar(ui);
            ui.add_space(2.0);
        });
        egui::Panel::left("side").resizable(true).show(ui, |ui| {
            ui.set_min_width(400.0);
            egui::ScrollArea::vertical()
                .id_salt("side")
                .show(ui, |ui| self.side_panel(ui));
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.viewport(ui));
    }
}

// --------------------------------------------------------------------------- small helpers

/// An edit to the open part.
enum PartAction {
    AddParam,
    RemoveParam(String),
    RenameParam(String, String),
    SetParamField(String, String, String),
    AddFeature(usize, String),
    RemoveFeature(usize, usize),
    MoveFeature(usize, usize, isize),
    SetFeatureName(usize, usize, String),
    SetFeatureArg(usize, usize, String, String),
    AddVariant,
    RemoveVariant(String),
    AddPort(String),
    RemovePort(String),
    SetPortType(String, String),
    SetPortField(String, String, String),
    SetPortParam(String, String, String),
    SetAbout(String, String),
    SetMassComputed,
    SetMassDeclared,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PartTab {
    Shape,
    Ports,
    About,
}

/// A text field whose value is committed when it loses focus or Enter is pressed.
///
/// Editing a live model character by character would reparse and rebuild on every keystroke, and
/// half-typed text is rarely valid. The scratch map holds what is being typed until it is
/// finished with, then hands it over once.
fn edit_text(
    ui: &mut egui::Ui,
    scratch: &mut HashMap<String, String>,
    key: &str,
    current: &str,
    width: f32,
    hint: &str,
) -> Option<String> {
    let buf = scratch
        .entry(key.to_string())
        .or_insert_with(|| current.to_string());
    let r = ui.add(
        egui::TextEdit::singleline(buf)
            .desired_width(width)
            .hint_text(hint),
    );
    let done = r.lost_focus() || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
    if done {
        let value = buf.clone();
        scratch.remove(key);
        if value != current {
            return Some(value);
        }
    } else if !r.has_focus() && buf.as_str() != current {
        // The model changed underneath, so show the model rather than a stale buffer.
        scratch.remove(key);
    }
    None
}


fn combo(ui: &mut egui::Ui, label: &str, current: &mut String, options: &[String]) {
    egui::ComboBox::from_label(label)
        .selected_text(current.clone())
        .show_ui(ui, |ui| {
            for o in options {
                ui.selectable_value(current, o.clone(), o);
            }
        });
}

fn find_port<'a>(
    mateable: &'a [UnitPorts],
    unit: &str,
    port: &str,
) -> Option<&'a wmds_model::PortSlot> {
    mateable
        .iter()
        .find(|u| u.unit == unit)?
        .ports
        .iter()
        .find(|p| p.name == port)
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

/// The numeric value of an expression in a given unit, when it has one.
fn number_in(e: &wmds_expr::Expr, unit: &str) -> Option<f64> {
    let mut e = e;
    while let wmds_expr::Expr::TextOr(_, inner) = e {
        e = inner;
    }
    match e {
        wmds_expr::Expr::Num(q) => {
            if unit.is_empty() || q.dim.is_dimensionless() {
                Some(q.value)
            } else {
                q.to_unit(unit).ok()
            }
        }
        _ => None,
    }
}

fn length_mm(e: &wmds_expr::Expr) -> Option<f64> {
    number_in(e, "mm")
}

fn value_text(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Num(q) => format!("{}", q.value),
        other => format!("{other:?}"),
    }
}

/// Draw a wireframe box, used to show which part is selected.
fn draw_box(
    painter: &egui::Painter,
    camera: &Camera,
    rect: egui::Rect,
    lo: Vec3,
    hi: Vec3,
    color: egui::Color32,
) {
    let c = [
        Vec3::new(lo.x, lo.y, lo.z),
        Vec3::new(hi.x, lo.y, lo.z),
        Vec3::new(hi.x, hi.y, lo.z),
        Vec3::new(lo.x, hi.y, lo.z),
        Vec3::new(lo.x, lo.y, hi.z),
        Vec3::new(hi.x, lo.y, hi.z),
        Vec3::new(hi.x, hi.y, hi.z),
        Vec3::new(lo.x, hi.y, hi.z),
    ];
    const EDGES: [(usize, usize); 12] = [
        (0, 1), (1, 2), (2, 3), (3, 0),
        (4, 5), (5, 6), (6, 7), (7, 4),
        (0, 4), (1, 5), (2, 6), (3, 7),
    ];
    let stroke = egui::Stroke::new(1.2, color);
    for (a, b) in EDGES {
        if let (Some(pa), Some(pb)) = (camera.project(rect, c[a]), camera.project(rect, c[b])) {
            painter.line_segment([pa, pb], stroke);
        }
    }
}

// --------------------------------------------------------------------------- the build job

enum Job {
    Primitive(Box<ResolvedPrimitive>, Option<f64>),
    /// The assembly, plus the densities its materials resolve to.
    Assembly(
        Box<ResolvedAssembly>,
        std::collections::HashMap<String, f64>,
    ),
}

fn run_job(job: Job, version: u64, cache: &std::sync::Mutex<wmds_geom::MeshCache>) -> BuildResult {
    let k = Kernel::default();
    match job {
        Job::Primitive(r, density_from_library) => {
            let built = match wmds_geom::build_primitive(&k, &r) {
                Ok(b) => b,
                Err(e) => {
                    return BuildResult {
                        error: Some(e.to_string()),
                        ..BuildResult::empty(version)
                    };
                }
            };
            let Some((_, solid)) = built.best() else {
                return BuildResult {
                    error: Some("no geometry level built".into()),
                    ..BuildResult::empty(version)
                };
            };
            let mesh = match k.tessellate(solid, 2e-4) {
                Ok(m) => m,
                Err(e) => {
                    return BuildResult {
                        error: Some(e.to_string()),
                        ..BuildResult::empty(version)
                    };
                }
            };
            let mp = mesh.mass_props();
            let density = density_from_library
                .or_else(|| r.material.as_deref().and_then(wmds_geom::placeholder_density));
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
        Job::Assembly(asm, densities) => {
            let density_of = |m: &str| densities.get(m).copied();
            let mut guard = cache.lock().expect("the mesh cache is not poisoned");
            let hits_before = guard.hits;
            let built = wmds_geom::build_assembly_cached(&k, &asm, &mut guard, 2e-4, &density_of);
            let reused = guard.hits - hits_before;
            drop(guard);

            let mesh = built.mesh;
            let part_bounds: HashMap<String, (Vec3, Vec3)> = built
                .bounds
                .iter()
                .map(|(k, (lo, hi))| {
                    (
                        k.clone(),
                        (
                            Vec3::new(lo[0] as f32, lo[1] as f32, lo[2] as f32),
                            Vec3::new(hi[0] as f32, hi[1] as f32, hi[2] as f32),
                        ),
                    )
                })
                .collect();
            let masses = built.masses;
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
                        PlacedBy::Mate(m) => format!("placed by joint {m}"),
                        PlacedBy::Free(w) => format!("placed explicitly: {w}"),
                        PlacedBy::Unreached => "NOT PLACED".into(),
                    },
                    mass: m.and_then(|m| m.mass),
                    declared: m.map(|m| m.from_declaration).unwrap_or(false),
                });
            }

            let errors = if asm.errors.is_empty() {
                None
            } else {
                Some(asm.errors.join("\n"))
            };
            BuildResult {
                mesh: Some(Arc::new(GpuMeshData::from_mesh(&mesh, version))),
                bounds: bounds_of(&mesh),
                part_bounds,
                ports: Vec::new(),
                parts,
                total_mass: total,
                point_mass,
                cg,
                volume_m3: None,
                error: errors,
                reused,
                parts_built: asm.instances.len(),
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
                number_in(e.as_ref()?, &unit)
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
        .collect()
}

fn miette_string(e: wmds_schema::SchemaErrors) -> String {
    format!("{:?}", miette::Report::new(e))
}
