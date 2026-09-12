//! Geometry abstraction.
//!
//! Everything above this crate talks to geometry through [`GeomKernel`]. Kernel adapters
//! (OpenCASCADE first, others later) implement the trait in their own crates. This crate also
//! owns the kernel-independent parts: the triangle [`Mesh`] type, [`MassProps`] computed from a
//! closed mesh, and the interpreter that turns a resolved primitive's feature list into solids.
//!
//! Units: metres everywhere in this crate. Adapters convert to whatever their kernel prefers.

pub mod features;
pub mod mesh;

pub use features::{BuiltGeometry, build_level, build_primitive};
pub use mesh::{MassProps, Mesh};

use std::path::Path;

use thiserror::Error;

pub type Vec3 = [f64; 3];

#[derive(Error, Debug, Clone, PartialEq)]
pub enum GeomError {
    #[error("kernel error: {0}")]
    Kernel(String),
    #[error("operation not supported by this kernel: {0}")]
    Unsupported(String),
    #[error("feature `{0}`: {1}")]
    Feature(String, String),
    #[error("io error: {0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, GeomError>;

/// A B-rep or mesh kernel. All lengths in metres, all angles in radians.
pub trait GeomKernel {
    type Solid: Clone;

    /// Axis-aligned box of the given size centred at `centre`.
    fn make_box(&self, size: Vec3, centre: Vec3) -> Result<Self::Solid>;
    /// Cylinder of radius `r` whose axis runs from `a` to `b`.
    fn cylinder_between(&self, a: Vec3, b: Vec3, r: f64) -> Result<Self::Solid>;
    /// Cylinder centred at `centre`, axis direction `axis`, height `h`.
    fn cylinder(&self, centre: Vec3, axis: Vec3, r: f64, h: f64) -> Result<Self::Solid> {
        let n = normalize(axis)?;
        let half = [n[0] * h / 2.0, n[1] * h / 2.0, n[2] * h / 2.0];
        self.cylinder_between(sub(centre, half), add(centre, half), r)
    }
    /// Hollow tube of outside diameter `od` and wall `wall` from `a` to `b`.
    fn tube_between(&self, a: Vec3, b: Vec3, od: f64, wall: f64) -> Result<Self::Solid> {
        if wall <= 0.0 || wall * 2.0 >= od {
            return Err(GeomError::Kernel(format!(
                "tube wall {wall} is not valid for od {od}"
            )));
        }
        let outer = self.cylinder_between(a, b, od / 2.0)?;
        // Extend the inner cylinder slightly past the ends so the subtraction is clean.
        let n = normalize(sub(b, a))?;
        let eps = 1e-6;
        let a2 = sub(a, scale(n, eps));
        let b2 = add(b, scale(n, eps));
        let inner = self.cylinder_between(a2, b2, od / 2.0 - wall)?;
        self.subtract(&outer, &inner)
    }

    fn union(&self, a: &Self::Solid, b: &Self::Solid) -> Result<Self::Solid>;
    fn subtract(&self, a: &Self::Solid, b: &Self::Solid) -> Result<Self::Solid>;
    fn intersect(&self, a: &Self::Solid, b: &Self::Solid) -> Result<Self::Solid>;

    /// Convex hull. Kernels may return `Unsupported`.
    fn hull(&self, _s: &Self::Solid) -> Result<Self::Solid> {
        Err(GeomError::Unsupported("hull".into()))
    }

    fn translated(&self, s: &Self::Solid, offset: Vec3) -> Result<Self::Solid>;
    /// Mirror about the plane through `origin` with normal `normal`.
    fn mirrored(&self, s: &Self::Solid, origin: Vec3, normal: Vec3) -> Result<Self::Solid>;

    /// Triangulate with a chordal tolerance in metres.
    fn tessellate(&self, s: &Self::Solid, tolerance: f64) -> Result<Mesh>;

    fn write_step(&self, s: &Self::Solid, path: &Path) -> Result<()>;
    fn read_step(&self, path: &Path) -> Result<Self::Solid>;
    fn write_stl(&self, s: &Self::Solid, path: &Path) -> Result<()> {
        let m = self.tessellate(s, 1e-4)?;
        m.write_stl_binary(path)
            .map_err(|e| GeomError::Io(e.to_string()))
    }

    /// Mass properties for unit density, from the tessellation unless the kernel can do better.
    fn mass_props(&self, s: &Self::Solid) -> Result<MassProps> {
        let m = self.tessellate(s, 2e-4)?;
        Ok(m.mass_props())
    }
}

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: Vec3, k: f64) -> Vec3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn length(a: Vec3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
pub fn normalize(a: Vec3) -> Result<Vec3> {
    let l = length(a);
    if l == 0.0 {
        return Err(GeomError::Kernel("zero-length direction".into()));
    }
    Ok(scale(a, 1.0 / l))
}
