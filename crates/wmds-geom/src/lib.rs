//! Geometry abstraction.
//!
//! Everything above this crate talks to geometry through [`GeomKernel`]. Kernel adapters
//! (OpenCASCADE first, others later) implement the trait in their own crates. This crate also
//! owns the kernel-independent parts: the triangle [`Mesh`] type, [`MassProps`] computed from a
//! closed mesh, and the interpreter that turns a resolved primitive's feature list into solids.
//!
//! Units: metres everywhere in this crate. Adapters convert to whatever their kernel prefers.

pub mod assembly_geom;
pub mod features;
pub mod mesh;
pub mod mesh_kernel;

pub use assembly_geom::{
    BuiltAssembly, BuiltPart, DensitySource, PartMass, assembly_masses, assembly_mesh,
    build_assembly, placeholder_density, roll_up,
};
pub use features::{BuiltGeometry, build_level, build_primitive};
pub use mesh::{MassProps, Mesh};
pub use mesh_kernel::MeshKernel;

use std::path::Path;

use thiserror::Error;
use wmds_model::Transform;

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

    /// Rectangular hollow section, hollow along the local x axis, centred at `centre`.
    /// `size` is (length, width, height); `wall` is the wall thickness on all four faces.
    /// This is the chassis rail shape, so both kernels are expected to make it cheaply.
    fn box_tube(&self, size: Vec3, wall: f64, centre: Vec3) -> Result<Self::Solid> {
        if wall <= 0.0 || wall * 2.0 >= size[1].min(size[2]) {
            return Err(GeomError::Kernel(format!(
                "wall {wall} is not valid for a {} x {} section",
                size[1], size[2]
            )));
        }
        let outer = self.make_box(size, centre)?;
        // Run the bore past both ends so the subtraction leaves no sliver.
        let inner = self.make_box(
            [size[0] + 1e-3, size[1] - 2.0 * wall, size[2] - 2.0 * wall],
            centre,
        )?;
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
    /// Rotate about an axis through the origin.
    fn rotated(&self, s: &Self::Solid, axis: Vec3, angle: f64) -> Result<Self::Solid>;

    /// Apply a general placement. The default decomposes it into a mirror (when the transform
    /// flips handedness), a rotation and a translation, which is all any kernel needs to expose.
    fn placed(&self, s: &Self::Solid, t: &Transform) -> Result<Self::Solid> {
        let mut out = s.clone();
        let mut linear = *t;
        linear.translation = [0.0; 3];
        if linear.is_mirrored() {
            out = self.mirrored(&out, [0.0; 3], [1.0, 0.0, 0.0])?;
            // Undo the mirror from the linear part so what remains is a pure rotation.
            linear = Transform::mirror([1.0, 0.0, 0.0]).then(&linear);
        }
        if let Some((axis, angle)) = axis_angle(&linear) {
            out = self.rotated(&out, axis, angle)?;
        }
        if t.translation != [0.0; 3] {
            out = self.translated(&out, t.translation)?;
        }
        Ok(out)
    }

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

/// Extract an axis and angle from a rotation. Returns `None` for the identity.
pub fn axis_angle(t: &Transform) -> Option<(Vec3, f64)> {
    let m = t.cols;
    // cols[i][j] is row j of column i, so the matrix element (row r, col c) is cols[c][r].
    let trace = m[0][0] + m[1][1] + m[2][2];
    let cos = ((trace - 1.0) / 2.0).clamp(-1.0, 1.0);
    let angle = cos.acos();
    if angle.abs() < 1e-9 {
        return None;
    }
    if (angle - std::f64::consts::PI).abs() < 1e-6 {
        // Near 180 degrees the skew part vanishes; take the axis from the largest diagonal.
        let d = [m[0][0], m[1][1], m[2][2]];
        let i = (0..3).max_by(|a, b| d[*a].total_cmp(&d[*b])).unwrap();
        let mut axis = [0.0; 3];
        axis[i] = ((d[i] + 1.0) / 2.0).max(0.0).sqrt();
        let other = |j: usize| (m[i][j] + m[j][i]) / 2.0;
        for j in 0..3 {
            if j != i && axis[i] > 1e-12 {
                axis[j] = other(j) / axis[i];
            }
        }
        return normalize(axis).ok().map(|a| (a, angle));
    }
    let s = 2.0 * angle.sin();
    let axis = [
        (m[1][2] - m[2][1]) / s,
        (m[2][0] - m[0][2]) / s,
        (m[0][1] - m[1][0]) / s,
    ];
    normalize(axis).ok().map(|a| (a, angle))
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
