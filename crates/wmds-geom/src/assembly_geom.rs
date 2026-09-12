//! Building the geometry of a whole assembly.
//!
//! Each instance's geometry is built in its own coordinates and then moved into place by the
//! transform the placement solver worked out, so geometry and position come from the same
//! source of truth.

use wmds_model::{PlacedInstance, ResolvedAssembly};

use crate::{GeomKernel, MassProps, Mesh, Result, Vec3, build_primitive};

/// One instance's geometry, positioned in assembly coordinates.
pub struct BuiltPart<S> {
    pub id: String,
    pub source_id: String,
    pub material: Option<String>,
    pub solid: S,
    /// Which geometry level was used.
    pub level: String,
    /// Mass and centre of gravity declared by the primitive, already placed into assembly
    /// coordinates. Envelope models declare their mass because the volume of the box they are
    /// drawn as says nothing about what they weigh.
    pub declared: Option<(f64, Vec3)>,
}

pub struct BuiltAssembly<S> {
    pub parts: Vec<BuiltPart<S>>,
    /// Instances whose geometry could not be built, with the reason.
    pub failures: Vec<(String, String)>,
}

impl<S> BuiltAssembly<S> {
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

/// Build and place every instance in a resolved assembly.
pub fn build_assembly<K: GeomKernel>(k: &K, asm: &ResolvedAssembly) -> BuiltAssembly<K::Solid> {
    let mut parts = Vec::new();
    let mut failures = Vec::new();
    for inst in &asm.instances {
        match build_one(k, inst) {
            Ok(p) => parts.push(p),
            Err(e) => failures.push((inst.id.clone(), e.to_string())),
        }
    }
    BuiltAssembly { parts, failures }
}

fn build_one<K: GeomKernel>(k: &K, inst: &PlacedInstance) -> Result<BuiltPart<K::Solid>> {
    let built = build_primitive(k, &inst.primitive)?;
    let (level, solid) = built
        .best()
        .map(|(l, s)| (l.to_string(), s.clone()))
        .ok_or_else(|| {
            crate::GeomError::Feature(inst.id.clone(), "no geometry level built".into())
        })?;
    let placed = k.placed(&solid, &inst.placement)?;
    let declared = match &inst.primitive.massprops {
        wmds_model::ResolvedMassProps::Declared { mass, cg, .. } => {
            let local = cg
                .map(|c| [c[0].value, c[1].value, c[2].value])
                .unwrap_or([0.0; 3]);
            Some((mass.value, inst.placement.point(local)))
        }
        wmds_model::ResolvedMassProps::Computed => None,
    };
    Ok(BuiltPart {
        id: inst.id.clone(),
        source_id: inst.source_id.clone(),
        material: inst.primitive.material.clone(),
        solid: placed,
        level,
        declared,
    })
}

/// Tessellate every part and merge into one mesh, for display.
pub fn assembly_mesh<K: GeomKernel>(
    k: &K,
    built: &BuiltAssembly<K::Solid>,
    tolerance: f64,
) -> Mesh {
    let mut out = Mesh::default();
    for p in &built.parts {
        if let Ok(m) = k.tessellate(&p.solid, tolerance) {
            out.extend(&m);
        }
    }
    out
}

/// Mass properties per part, with density looked up from the material name.
pub struct PartMass {
    pub id: String,
    pub material: Option<String>,
    pub density: Option<f64>,
    pub volume: f64,
    pub mass: Option<f64>,
    pub centroid: Vec3,
    /// True when the mass came from the primitive rather than from geometry and a density.
    pub from_declaration: bool,
}

/// Mass of every part.
///
/// A primitive that declares its mass is believed; anything else is its geometry volume times a
/// placeholder density. Declared mass wins because an envelope model is drawn as a box for
/// packaging, and the box's volume is not the part.
pub fn assembly_masses<K: GeomKernel>(k: &K, built: &BuiltAssembly<K::Solid>) -> Vec<PartMass> {
    let mut out = Vec::new();
    for p in &built.parts {
        let mp = k.mass_props(&p.solid).ok();
        let volume = mp.as_ref().map(|m| m.volume).unwrap_or(0.0);
        let density = p.material.as_deref().and_then(placeholder_density);
        let (mass, centroid, declared) = match p.declared {
            Some((m, cg)) => (Some(m), cg, true),
            None => (
                density.map(|d| volume * d),
                mp.as_ref().map(|m| m.centroid).unwrap_or([0.0; 3]),
                false,
            ),
        };
        out.push(PartMass {
            id: p.id.clone(),
            material: p.material.clone(),
            density,
            volume,
            mass,
            centroid,
            from_declaration: declared,
        });
    }
    out
}

/// Total mass and centre of gravity of the parts that have a density.
pub fn roll_up(masses: &[PartMass]) -> (f64, Vec3, usize) {
    let mut total = 0.0;
    let mut moment = [0.0; 3];
    let mut unknown = 0;
    for m in masses {
        match m.mass {
            Some(mass) => {
                total += mass;
                for i in 0..3 {
                    moment[i] += mass * m.centroid[i];
                }
            }
            None => unknown += 1,
        }
    }
    let cg = if total > 0.0 {
        [moment[0] / total, moment[1] / total, moment[2] / total]
    } else {
        [0.0; 3]
    };
    (total, cg, unknown)
}

/// Placeholder densities in kg/m^3, keyed by the material id prefix.
///
/// These stand in until the material database lands. Every number they produce is labelled as a
/// placeholder wherever it is shown, because a mass that looks authoritative and is not is worse
/// than no mass at all.
pub fn placeholder_density(material: &str) -> Option<f64> {
    let m = material.to_ascii_lowercase();
    let table: &[(&str, f64)] = &[
        ("steel", 7850.0),
        ("stainless", 7900.0),
        ("alu", 2700.0),
        ("mag", 1800.0),
        ("titanium", 4500.0),
        ("cfrp", 1550.0),
        ("gfrp", 1900.0),
        ("polymer", 1200.0),
        ("plastic", 1200.0),
        ("rubber", 1100.0),
        ("elastomer", 1100.0),
        ("glass", 2500.0),
        ("foam", 100.0),
        ("wood", 700.0),
    ];
    table
        .iter()
        .find(|(k, _)| m.starts_with(k))
        .map(|(_, d)| *d)
}

/// Mass properties of a merged mesh, for a quick whole-assembly figure.
pub fn mesh_mass_props(mesh: &Mesh) -> MassProps {
    mesh.mass_props()
}
