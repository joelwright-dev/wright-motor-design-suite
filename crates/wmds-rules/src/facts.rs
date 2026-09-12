//! The facts a rule can ask about, and the name resolution that exposes them.
//!
//! Rules read the design through this layer rather than through the raw model, so that
//! primitives can be reorganised without rewriting every rule. Adding a fact here is what makes
//! a new kind of rule possible.

use indexmap::IndexMap;
use wmds_expr::{Env, Value};
use wmds_model::ResolvedAssembly;
use wmds_units::{Dim, Quantity};

/// Everything a rule pack may ask about a vehicle.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub id: String,
    /// ADR vehicle category: MA, MB, MC, NA and so on.
    pub category: String,
    pub markets: Vec<String>,

    /// Mass of the parts that have geometry or a declared mass, in kg.
    pub modelled_mass: f64,
    /// Declared point masses by load state, in kg.
    pub point_mass: IndexMap<String, f64>,
    /// Centre of gravity of everything counted, in metres.
    pub cg: [f64; 3],
    /// Bounding box of the built geometry, in metres.
    pub bounds: Option<([f64; 3], [f64; 3])>,

    pub chassis: Option<ChassisFacts>,
    pub parts: Vec<PartFacts>,
    pub mates: Vec<MateFacts>,
    /// Results of simulations that have been run, keyed by simulation name.
    pub simulations: IndexMap<String, IndexMap<String, Quantity>>,
    /// Declarations a person has attached, keyed by rule id.
    pub declarations: IndexMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct ChassisFacts {
    pub system: String,
    pub configuration: String,
    pub width_config: String,
    pub rail_section: String,
    /// Metres.
    pub length: f64,
    pub width: f64,
    pub grid_pitch: f64,
    pub mass: f64,
    pub section_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PartFacts {
    pub id: String,
    pub source: String,
    pub category: String,
    pub material: String,
    /// kg, zero when unknown.
    pub mass: f64,
    /// Metres, in vehicle coordinates.
    pub position: [f64; 3],
    pub tags: Vec<String>,
    pub placed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MateFacts {
    pub id: String,
    pub dof: String,
    pub stage: String,
    pub fastener_kind: String,
    pub fastener_size: String,
    pub fastener_grade: String,
    pub quantity: u32,
    pub has_torque: bool,
    pub compatible: bool,
}

impl Facts {
    /// Mass in a given load state. `kerb` is the modelled parts plus kerb point masses; `laden`
    /// adds the laden ones on top.
    pub fn mass(&self, state: &str) -> f64 {
        let base = self.modelled_mass + self.point_mass.get("kerb").copied().unwrap_or(0.0);
        match state {
            "kerb" => base,
            "laden" | "gvm" => base + self.point_mass.get("laden").copied().unwrap_or(0.0),
            other => base + self.point_mass.get(other).copied().unwrap_or(0.0),
        }
    }

    /// Build the parts of the facts that come from a resolved assembly. Mass figures come from
    /// the caller, because computing them needs a geometry kernel.
    pub fn from_assembly(asm: &ResolvedAssembly, def: &wmds_schema::AssemblyDef) -> Facts {
        let mut f = Facts {
            id: asm.id.clone(),
            ..Default::default()
        };
        if let Some(v) = &def.vehicle {
            f.category = v.category.clone();
            f.markets = v.markets.clone();
        }
        for p in &asm.point_masses {
            *f.point_mass.entry(p.state.clone()).or_insert(0.0) += p.mass.value;
        }
        for i in &asm.instances {
            f.parts.push(PartFacts {
                id: i.id.clone(),
                source: i.source_id.clone(),
                category: i.source_id.split('/').next().unwrap_or("").to_string(),
                material: i.primitive.material.clone().unwrap_or_default(),
                mass: 0.0,
                position: i.placement.translation,
                tags: Vec::new(),
                placed: i.placed_by != wmds_model::PlacedBy::Unreached,
            });
        }
        for m in &asm.mates {
            let fs = m.fasteners.as_ref();
            f.mates.push(MateFacts {
                id: m.id.clone(),
                dof: m.dof.name().to_string(),
                stage: m.stage.name().to_string(),
                fastener_kind: fs.map(|x| x.kind.clone()).unwrap_or_default(),
                fastener_size: fs.map(|x| x.size.clone()).unwrap_or_default(),
                fastener_grade: fs.map(|x| x.grade.clone()).unwrap_or_default(),
                quantity: fs.map(|x| x.quantity).unwrap_or(0),
                has_torque: fs.map(|x| x.torque.is_some()).unwrap_or(false),
                compatible: m.compatible.is_ok(),
            });
        }
        f
    }
}

fn len(v: f64) -> Value {
    Value::Num(Quantity::new(v, Dim::LENGTH))
}

fn mass(v: f64) -> Value {
    Value::Num(Quantity::new(v, Dim::MASS))
}

fn vec3(v: [f64; 3]) -> Value {
    Value::Tuple(vec![len(v[0]), len(v[1]), len(v[2])])
}

fn strings(v: &[String]) -> Value {
    Value::List(v.iter().map(|s| Value::Str(s.clone())).collect())
}

impl PartFacts {
    fn to_record(&self) -> Value {
        Value::record(vec![
            ("id".into(), Value::Str(self.id.clone())),
            ("source".into(), Value::Str(self.source.clone())),
            ("category".into(), Value::Str(self.category.clone())),
            ("material".into(), Value::Str(self.material.clone())),
            ("mass".into(), mass(self.mass)),
            ("position".into(), vec3(self.position)),
            ("x".into(), len(self.position[0])),
            ("y".into(), len(self.position[1])),
            ("z".into(), len(self.position[2])),
            ("tags".into(), strings(&self.tags)),
            ("placed".into(), Value::Bool(self.placed)),
        ])
    }
}

impl MateFacts {
    fn to_record(&self) -> Value {
        Value::record(vec![
            ("id".into(), Value::Str(self.id.clone())),
            ("dof".into(), Value::Str(self.dof.clone())),
            ("stage".into(), Value::Str(self.stage.clone())),
            ("fastener".into(), Value::Str(self.fastener_kind.clone())),
            ("size".into(), Value::Str(self.fastener_size.clone())),
            ("grade".into(), Value::Str(self.fastener_grade.clone())),
            ("quantity".into(), Value::num(self.quantity as f64)),
            ("has_torque".into(), Value::Bool(self.has_torque)),
            ("compatible".into(), Value::Bool(self.compatible)),
        ])
    }
}

/// Name resolution for rule expressions.
///
/// Paths available:
/// * `vehicle.category`, `vehicle.markets`
/// * `vehicle.mass.kerb`, `.laden`, `.gvm`, `vehicle.mass.modelled`
/// * `vehicle.cg` and `vehicle.cg.x|y|z`
/// * `vehicle.length`, `.width`, `.height` from the bounding box
/// * `chassis.mass`, `.length`, `.width`, `.grid_pitch`, `.system`, `.configuration`,
///   `.width_config`, `.rail_section`, `.sections`
/// * `parts` and `mates` as lists of records, for `count(filter(parts, p -> ...))`
/// * `result.<name>` for the simulation named by the rule being evaluated
pub struct FactEnv<'a> {
    pub facts: &'a Facts,
    /// Results of the simulation this rule asked for, exposed as `result`.
    pub result: Option<&'a IndexMap<String, Quantity>>,
}

impl Env for FactEnv<'_> {
    fn lookup(&self, path: &[String]) -> Option<Value> {
        let f = self.facts;
        let root = path[0].as_str();
        let rest = &path[1..];
        let value = match root {
            "vehicle" => {
                let Some(field) = rest.first().map(|s| s.as_str()) else {
                    return None;
                };
                match field {
                    "category" => Value::Str(f.category.clone()),
                    "markets" => strings(&f.markets),
                    "mass" => {
                        let state = rest.get(1).map(|s| s.as_str()).unwrap_or("kerb");
                        return Some(match state {
                            "modelled" => mass(f.modelled_mass),
                            s => mass(f.mass(s)),
                        });
                    }
                    "cg" => {
                        let v = vec3(f.cg);
                        return wmds_expr::walk(&v, &rest[1..]);
                    }
                    "length" => len(f.bounds.map(|(lo, hi)| hi[0] - lo[0]).unwrap_or(0.0)),
                    "width" => len(f.bounds.map(|(lo, hi)| hi[1] - lo[1]).unwrap_or(0.0)),
                    "height" => len(f.bounds.map(|(lo, hi)| hi[2] - lo[2]).unwrap_or(0.0)),
                    "part_count" => Value::num(f.parts.len() as f64),
                    _ => return None,
                }
            }
            "chassis" => {
                let c = f.chassis.as_ref()?;
                let Some(field) = rest.first().map(|s| s.as_str()) else {
                    return None;
                };
                match field {
                    "system" => Value::Str(c.system.clone()),
                    "configuration" => Value::Str(c.configuration.clone()),
                    "width_config" => Value::Str(c.width_config.clone()),
                    "rail_section" => Value::Str(c.rail_section.clone()),
                    "length" => len(c.length),
                    "width" => len(c.width),
                    "grid_pitch" => len(c.grid_pitch),
                    "mass" => mass(c.mass),
                    "sections" => Value::num(c.section_count as f64),
                    _ => return None,
                }
            }
            "parts" => Value::List(f.parts.iter().map(|p| p.to_record()).collect()),
            "mates" => Value::List(f.mates.iter().map(|m| m.to_record()).collect()),
            "result" => {
                let r = self.result?;
                let field = rest.first()?;
                return r.get(field.as_str()).map(|q| Value::Num(*q));
            }
            _ => return None,
        };
        if rest.len() > 1 && matches!(root, "vehicle" | "chassis") {
            return wmds_expr::walk(&value, &rest[1..]);
        }
        if root == "parts" || root == "mates" {
            return wmds_expr::walk(&value, rest);
        }
        Some(value)
    }
}
