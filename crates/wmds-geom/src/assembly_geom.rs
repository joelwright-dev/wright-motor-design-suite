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
    /// Where the density came from, for reports that have to be honest about their inputs.
    pub density_source: DensitySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensitySource {
    /// The mass was declared, so no density was needed.
    NotNeeded,
    /// From the material database.
    Database,
    /// Guessed from the material name because the database does not define it.
    Placeholder,
    /// Neither, so this part has no mass.
    None,
}

impl DensitySource {
    pub fn label(&self) -> &'static str {
        match self {
            DensitySource::NotNeeded => "declared",
            DensitySource::Database => "material database",
            DensitySource::Placeholder => "PLACEHOLDER density",
            DensitySource::None => "no density",
        }
    }
}

/// Mass of every part.
///
/// A primitive that declares its mass is believed; anything else is its geometry volume times a
/// placeholder density. Declared mass wins because an envelope model is drawn as a box for
/// packaging, and the box's volume is not the part.
pub fn assembly_masses<K: GeomKernel>(
    k: &K,
    built: &BuiltAssembly<K::Solid>,
    density_of: &dyn Fn(&str) -> Option<f64>,
) -> Vec<PartMass> {
    let mut out = Vec::new();
    for p in &built.parts {
        let mp = k.mass_props(&p.solid).ok();
        let volume = mp.as_ref().map(|m| m.volume).unwrap_or(0.0);
        let material = p.material.as_deref().unwrap_or("");
        // The database first. A guess from the name is a fallback, and it is labelled as one.
        let (density, source) = match density_of(material) {
            Some(d) => (Some(d), DensitySource::Database),
            None => match placeholder_density(material) {
                Some(d) => (Some(d), DensitySource::Placeholder),
                None => (None, DensitySource::None),
            },
        };
        let (mass, centroid, declared, source) = match p.declared {
            Some((m, cg)) => (Some(m), cg, true, DensitySource::NotNeeded),
            None => (
                density.map(|d| volume * d),
                mp.as_ref().map(|m| m.centroid).unwrap_or([0.0; 3]),
                false,
                source,
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
            density_source: source,
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

/// Last-resort densities in kg/m^3, guessed from the material id prefix.
///
/// Used only when the material database does not define the material. Every number they produce
/// is labelled as a placeholder wherever it is shown, because a mass that looks authoritative
/// and is not is worse than no mass at all.
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

// --------------------------------------------------------------------------- the mesh cache

/// One primitive tessellated in its own coordinates, with the mass properties of that mesh.
#[derive(Debug, Clone)]
pub struct CachedPart {
    pub mesh: Mesh,
    pub props: MassProps,
}

/// Tessellated primitives, keyed by everything the primitive resolved to.
///
/// This exists because of the editor. Building a vehicle means changing one thing at a time, and
/// re-tessellating the other forty-eight parts through a solid modelling kernel after every
/// nudge of a slider makes the application unusable. Placement is a rigid transform of the mesh,
/// which costs nothing, so only a part whose own resolved definition changed is rebuilt.
///
/// The key is the debug form of the resolved primitive. That is deliberately total: it covers
/// every parameter, variant, material and geometry feature, so two parts share an entry only if
/// they are genuinely the same part.
#[derive(Default)]
pub struct MeshCache {
    entries: std::collections::HashMap<String, std::sync::Arc<CachedPart>>,
    pub hits: usize,
    pub misses: usize,
}

impl MeshCache {
    pub fn new() -> MeshCache {
        MeshCache::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Forget everything. Only needed when the geometry kernel itself changes.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.hits = 0;
        self.misses = 0;
    }

    pub fn get_or_build<K: GeomKernel>(
        &mut self,
        k: &K,
        prim: &wmds_model::ResolvedPrimitive,
        tolerance: f64,
    ) -> Result<std::sync::Arc<CachedPart>> {
        let key = format!("{prim:?}");
        if let Some(hit) = self.entries.get(&key) {
            self.hits += 1;
            return Ok(hit.clone());
        }
        self.misses += 1;
        let built = build_primitive(k, prim)?;
        let (_, solid) = built.best().ok_or_else(|| {
            crate::GeomError::Feature(prim.id.clone(), "no geometry level built".into())
        })?;
        let mesh = k.tessellate(solid, tolerance)?;
        let props = mesh.mass_props();
        let entry = std::sync::Arc::new(CachedPart { mesh, props });
        self.entries.insert(key, entry.clone());
        Ok(entry)
    }
}

/// A whole assembly, tessellated and placed.
#[derive(Default)]
pub struct MeshedAssembly {
    /// Every part merged into one mesh, for display.
    pub mesh: Mesh,
    /// Bounds per instance id, and per sub-assembly unit, in assembly coordinates.
    pub bounds: std::collections::HashMap<String, (Vec3, Vec3)>,
    pub masses: Vec<PartMass>,
    pub failures: Vec<(String, String)>,
}

/// Build every instance through the cache and place it.
///
/// Equivalent to `build_assembly` followed by `assembly_mesh` and `assembly_masses`, but the
/// geometry kernel only sees parts it has not already been asked about. Mass properties come
/// from the tessellated mesh rather than from the solid, which at this tolerance agrees to far
/// better than the accuracy of the densities involved.
pub fn build_assembly_cached<K: GeomKernel>(
    k: &K,
    asm: &ResolvedAssembly,
    cache: &mut MeshCache,
    tolerance: f64,
    density_of: &dyn Fn(&str) -> Option<f64>,
) -> MeshedAssembly {
    let mut out = MeshedAssembly::default();
    for inst in &asm.instances {
        let part = match cache.get_or_build(k, &inst.primitive, tolerance) {
            Ok(p) => p,
            Err(e) => {
                out.failures.push((inst.id.clone(), e.to_string()));
                continue;
            }
        };
        let placed = transformed(&part.mesh, &inst.placement);
        if let Some((lo, hi)) = placed.bounds() {
            // A sub-assembly's parts carry a dotted id. Record the unit as well, because that
            // is the name the parts list shows and the name a person selects.
            let unit = inst.id.split('.').next().unwrap_or(&inst.id).to_string();
            out.bounds
                .entry(unit)
                .and_modify(|b: &mut (Vec3, Vec3)| {
                    for i in 0..3 {
                        b.0[i] = b.0[i].min(lo[i]);
                        b.1[i] = b.1[i].max(hi[i]);
                    }
                })
                .or_insert((lo, hi));
            out.bounds.insert(inst.id.clone(), (lo, hi));
        }
        out.mesh.extend(&placed);

        // Mass. Declared wins; see `assembly_masses` for why.
        let material = inst.primitive.material.as_deref().unwrap_or("");
        let (density, source) = match density_of(material) {
            Some(d) => (Some(d), DensitySource::Database),
            None => match placeholder_density(material) {
                Some(d) => (Some(d), DensitySource::Placeholder),
                None => (None, DensitySource::None),
            },
        };
        let declared = match &inst.primitive.massprops {
            wmds_model::ResolvedMassProps::Declared { mass, cg, .. } => {
                let local = cg
                    .map(|c| [c[0].value, c[1].value, c[2].value])
                    .unwrap_or([0.0; 3]);
                Some((mass.value, inst.placement.point(local)))
            }
            wmds_model::ResolvedMassProps::Computed => None,
        };
        // Volume is unchanged by a rigid move, and a mirrored part has the same volume as the
        // one it mirrors, so the sign is dropped rather than propagated.
        let volume = part.props.volume.abs();
        let (mass, centroid, from_declaration, source) = match declared {
            Some((m, cg)) => (Some(m), cg, true, DensitySource::NotNeeded),
            None => (
                density.map(|d| volume * d),
                inst.placement.point(part.props.centroid),
                false,
                source,
            ),
        };
        out.masses.push(PartMass {
            id: inst.id.clone(),
            material: inst.primitive.material.clone(),
            density,
            volume,
            mass,
            centroid,
            from_declaration,
            density_source: source,
        });
    }
    out
}

/// Move a mesh into assembly coordinates.
fn transformed(mesh: &Mesh, t: &wmds_model::Transform) -> Mesh {
    let flip = t.determinant() < 0.0;
    let positions: Vec<Vec3> = mesh.positions.iter().map(|p| t.point(*p)).collect();
    let normals: Vec<Vec3> = mesh
        .normals
        .iter()
        .map(|n| {
            let d = t.direction(*n);
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            let d = if len > 0.0 {
                [d[0] / len, d[1] / len, d[2] / len]
            } else {
                d
            };
            // A mirror turns the surface inside out, so the outward normal turns with it.
            if flip { [-d[0], -d[1], -d[2]] } else { d }
        })
        .collect();
    let triangles = if flip {
        // Winding has to reverse as well, or every mirrored part renders inside out.
        mesh.triangles.iter().map(|x| [x[0], x[2], x[1]]).collect()
    } else {
        mesh.triangles.clone()
    };
    Mesh {
        positions,
        normals,
        triangles,
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use crate::MeshKernel;

    fn prim(span_mm: f64) -> wmds_model::ResolvedPrimitive {
        let src = format!(
            r#"primitive "test/bar" version="0.1.0" {{
                description "a bar"
                category "test"
                params {{
                    span unit="mm" default={span_mm}
                }}
                material "steel/e355-tube"
                geometry level="manufacture" {{
                    box "b" size="(span, 40 mm, 40 mm)" at="(0 mm, 0 mm, 0 mm)"
                }}
                massprops computed=#true
                ports {{
                }}
                manufacturing {{
                    method "machined" scale="1..*" {{
                        export "step"
                    }}
                }}
            }}"#
        );
        let def = wmds_schema::parse_primitive("t.prim.kdl", &src).expect("parses");
        wmds_model::resolve(&def, &wmds_model::Overrides::default()).expect("resolves")
    }

    #[test]
    fn the_same_part_is_only_built_once() {
        let k = MeshKernel::default();
        let mut cache = MeshCache::new();
        let p = prim(400.0);
        let a = cache.get_or_build(&k, &p, 2e-4).unwrap();
        let b = cache.get_or_build(&k, &p, 2e-4).unwrap();
        assert_eq!(cache.misses, 1, "the second ask must not reach the kernel");
        assert_eq!(cache.hits, 1);
        assert!(std::sync::Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn changing_a_parameter_builds_a_new_part() {
        // The cache must not be so eager that an edit appears to do nothing.
        let k = MeshKernel::default();
        let mut cache = MeshCache::new();
        cache.get_or_build(&k, &prim(400.0), 2e-4).unwrap();
        cache.get_or_build(&k, &prim(420.0), 2e-4).unwrap();
        assert_eq!(cache.misses, 2);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn a_mirrored_part_keeps_its_volume() {
        let k = MeshKernel::default();
        let mut cache = MeshCache::new();
        let part = cache.get_or_build(&k, &prim(400.0), 2e-4).unwrap();
        let before = part.props.volume;
        let m = wmds_model::Transform::mirror([0.0, 1.0, 0.0]);
        let moved = transformed(&part.mesh, &m);
        let after = moved.mass_props().volume;
        assert!(
            (after - before).abs() < 1e-9,
            "mirroring changed the volume from {before} to {after}; the winding did not flip"
        );
    }

    #[test]
    fn moving_a_part_moves_its_centre_and_not_its_size() {
        let k = MeshKernel::default();
        let mut cache = MeshCache::new();
        let part = cache.get_or_build(&k, &prim(400.0), 2e-4).unwrap();
        let t = wmds_model::Transform::translation([1.5, 0.0, 0.25]);
        let moved = transformed(&part.mesh, &t);
        let mp = moved.mass_props();
        assert!((mp.volume - part.props.volume).abs() < 1e-12);
        assert!((mp.centroid[0] - (part.props.centroid[0] + 1.5)).abs() < 1e-9);
        assert!((mp.centroid[2] - (part.props.centroid[2] + 0.25)).abs() < 1e-9);
    }
}
