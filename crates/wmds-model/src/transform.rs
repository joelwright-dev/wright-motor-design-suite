//! Rigid transforms and port frames.
//!
//! WMDS places components by composing transforms along the mate graph, so this module is the
//! arithmetic underneath every assembly. Lengths are metres. A [`Transform`] is a 3x3 linear
//! part plus a translation; the linear part is normally a rotation, but improper (mirrored)
//! transforms are allowed so that left/right hand variants work.

/// Column-major 3x3 matrix: `cols[i]` is the image of basis vector `i`.
pub type Vec3 = [f64; 3];

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: Vec3, k: f64) -> Vec3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn norm(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}
pub fn normalize(a: Vec3) -> Option<Vec3> {
    let n = norm(a);
    if n < 1e-12 { None } else { Some(scale(a, 1.0 / n)) }
}

/// A rigid (or mirrored) transform: `y = m * x + t`, with `m` stored as three columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub cols: [Vec3; 3],
    pub translation: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Transform::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0, 0.0, 0.0],
    };

    pub fn translation(t: Vec3) -> Transform {
        Transform { translation: t, ..Transform::IDENTITY }
    }

    /// From orthonormal basis columns and an origin.
    pub fn from_basis(x: Vec3, y: Vec3, z: Vec3, origin: Vec3) -> Transform {
        Transform { cols: [x, y, z], translation: origin }
    }

    /// Rotation of `angle` radians about a unit axis (Rodrigues).
    pub fn rotation(axis: Vec3, angle: f64) -> Transform {
        let a = normalize(axis).unwrap_or([0.0, 0.0, 1.0]);
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        let (x, y, z) = (a[0], a[1], a[2]);
        Transform {
            cols: [
                [t * x * x + c, t * x * y + s * z, t * x * z - s * y],
                [t * x * y - s * z, t * y * y + c, t * y * z + s * x],
                [t * x * z + s * y, t * y * z - s * x, t * z * z + c],
            ],
            translation: [0.0; 3],
        }
    }

    /// Reflection through the plane with unit `normal` passing through the origin.
    pub fn mirror(normal: Vec3) -> Transform {
        let n = normalize(normal).unwrap_or([0.0, 1.0, 0.0]);
        let col = |e: Vec3| sub(e, scale(n, 2.0 * dot(e, n)));
        Transform {
            cols: [
                col([1.0, 0.0, 0.0]),
                col([0.0, 1.0, 0.0]),
                col([0.0, 0.0, 1.0]),
            ],
            translation: [0.0; 3],
        }
    }

    /// Apply to a point.
    pub fn point(&self, p: Vec3) -> Vec3 {
        add(
            add(scale(self.cols[0], p[0]), scale(self.cols[1], p[1])),
            add(scale(self.cols[2], p[2]), self.translation),
        )
    }

    /// Apply to a direction (ignores translation).
    pub fn direction(&self, d: Vec3) -> Vec3 {
        add(
            add(scale(self.cols[0], d[0]), scale(self.cols[1], d[1])),
            scale(self.cols[2], d[2]),
        )
    }

    /// `self` then `other`, i.e. the transform equivalent to applying self first.
    pub fn then(&self, other: &Transform) -> Transform {
        Transform {
            cols: [
                other.direction(self.cols[0]),
                other.direction(self.cols[1]),
                other.direction(self.cols[2]),
            ],
            translation: other.point(self.translation),
        }
    }

    pub fn determinant(&self) -> f64 {
        dot(self.cols[0], cross(self.cols[1], self.cols[2]))
    }

    /// True if the transform flips handedness (a mirrored variant).
    pub fn is_mirrored(&self) -> bool {
        self.determinant() < 0.0
    }

    /// Inverse. Exact for orthogonal linear parts (rotations and reflections), which is all
    /// WMDS produces; falls back to a general 3x3 inverse otherwise.
    pub fn inverse(&self) -> Transform {
        let m = self.cols;
        let orthogonal = {
            let g = |i: usize, j: usize| dot(m[i], m[j]);
            (g(0, 0) - 1.0).abs() < 1e-9
                && (g(1, 1) - 1.0).abs() < 1e-9
                && (g(2, 2) - 1.0).abs() < 1e-9
                && g(0, 1).abs() < 1e-9
                && g(0, 2).abs() < 1e-9
                && g(1, 2).abs() < 1e-9
        };
        let inv_cols = if orthogonal {
            // Transpose.
            [
                [m[0][0], m[1][0], m[2][0]],
                [m[0][1], m[1][1], m[2][1]],
                [m[0][2], m[1][2], m[2][2]],
            ]
        } else {
            let det = self.determinant();
            let d = if det.abs() < 1e-15 { 1.0 } else { det };
            let a = cross(m[1], m[2]);
            let b = cross(m[2], m[0]);
            let c = cross(m[0], m[1]);
            [
                [a[0] / d, b[0] / d, c[0] / d],
                [a[1] / d, b[1] / d, c[1] / d],
                [a[2] / d, b[2] / d, c[2] / d],
            ]
        };
        let inv = Transform { cols: inv_cols, translation: [0.0; 3] };
        let t = inv.direction(self.translation);
        Transform { cols: inv_cols, translation: scale(t, -1.0) }
    }
}

/// A port's coordinate frame: origin, mating axis (local z), and a clocking direction (local x).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub origin: Vec3,
    pub axis: Vec3,
    pub clock: Option<Vec3>,
}

impl Frame {
    /// Build the transform that maps frame-local coordinates into the parent's coordinates.
    /// Local z is the mating axis; local x is the clocking direction projected perpendicular to
    /// z, or an arbitrary perpendicular when no clocking is declared.
    pub fn to_transform(&self) -> Transform {
        let z = normalize(self.axis).unwrap_or([0.0, 0.0, 1.0]);
        let x = self
            .clock
            .and_then(|c| normalize(sub(c, scale(z, dot(c, z)))))
            .unwrap_or_else(|| {
                let helper = if z[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
                normalize(cross(helper, z)).unwrap_or([1.0, 0.0, 0.0])
            });
        let y = cross(z, x);
        Transform::from_basis(x, y, z, self.origin)
    }
}

/// How two mated ports are oriented relative to each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MateAxis {
    /// The ports face each other: their mating axes are antiparallel. Bolted faces, flanges.
    #[default]
    Opposed,
    /// The ports share a direction: their mating axes are parallel. Coaxial pins and shafts.
    Aligned,
}

impl MateAxis {
    /// Transform applied in port-A's frame before port B's frame is matched to it.
    pub fn convention(&self) -> Transform {
        match self {
            // Rotate 180 degrees about x: z -> -z, y -> -y.
            MateAxis::Opposed => Transform {
                cols: [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]],
                translation: [0.0; 3],
            },
            MateAxis::Aligned => Transform::IDENTITY,
        }
    }
}

/// Given port A already placed in the world and port B's frame in its own instance's
/// coordinates, return the world transform that instance B must have for the ports to mate.
///
/// * `a_world` - port A's frame expressed in world coordinates
/// * `b_local` - port B's frame expressed in instance B's local coordinates
/// * `axis` - the mating convention
/// * `offset` - optional extra transform expressed in port A's frame, so a positive z offset is
///   a shim that pushes the two parts apart along the mating axis
pub fn solve_mate(a_world: &Transform, b_local: &Transform, axis: MateAxis, offset: Option<Transform>) -> Transform {
    let base = match offset {
        Some(o) => o.then(a_world),
        None => *a_world,
    };
    let target = axis.convention().then(&base);
    b_local.inverse().then(&target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }
    fn close3(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|i| close(a[i], b[i]))
    }

    #[test]
    fn compose_and_invert() {
        let r = Transform::rotation([0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2);
        assert!(close3(r.direction([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]));
        let t = Transform::translation([1.0, 2.0, 3.0]);
        let rt = r.then(&t);
        assert!(close3(rt.point([1.0, 0.0, 0.0]), [1.0, 3.0, 3.0]));
        let back = rt.then(&rt.inverse());
        assert!(close3(back.translation, [0.0; 3]));
        assert!(close3(back.cols[0], [1.0, 0.0, 0.0]) && close3(back.cols[1], [0.0, 1.0, 0.0]));
    }

    #[test]
    fn mirror_flips_handedness() {
        let m = Transform::mirror([0.0, 1.0, 0.0]);
        assert!(m.is_mirrored());
        assert!(close3(m.point([1.0, 2.0, 3.0]), [1.0, -2.0, 3.0]));
        assert!(close(m.then(&m).determinant(), 1.0));
    }

    #[test]
    fn frame_axis_becomes_local_z() {
        let f = Frame { origin: [1.0, 0.0, 0.0], axis: [1.0, 0.0, 0.0], clock: None };
        let t = f.to_transform();
        assert!(close3(t.direction([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
        assert!(close3(t.point([0.0, 0.0, 0.0]), [1.0, 0.0, 0.0]));
        assert!(close(t.determinant(), 1.0), "frame must be right-handed");
    }

    #[test]
    fn opposed_mate_brings_ports_together_facing() {
        // Port A at the origin of the world facing +z.
        let a_world = Frame { origin: [0.0, 0.0, 0.0], axis: [0.0, 0.0, 1.0], clock: None }.to_transform();
        // Port B sits 2 m along its own instance's x axis, facing +x.
        let b_local = Frame { origin: [2.0, 0.0, 0.0], axis: [1.0, 0.0, 0.0], clock: None }.to_transform();
        let placement = solve_mate(&a_world, &b_local, MateAxis::Opposed, None);
        // After placement, port B in world coordinates must sit at A's origin, facing -z.
        let b_world = b_local.then(&placement);
        assert!(close3(b_world.translation, [0.0; 3]), "{:?}", b_world.translation);
        assert!(close3(b_world.direction([0.0, 0.0, 1.0]), [0.0, 0.0, -1.0]));
    }

    #[test]
    fn aligned_mate_keeps_axes_parallel() {
        let a_world = Frame { origin: [1.0, 0.0, 0.0], axis: [1.0, 0.0, 0.0], clock: None }.to_transform();
        let b_local = Frame { origin: [0.0, 0.0, 0.0], axis: [0.0, 0.0, 1.0], clock: None }.to_transform();
        let placement = solve_mate(&a_world, &b_local, MateAxis::Aligned, None);
        let b_world = b_local.then(&placement);
        assert!(close3(b_world.translation, [1.0, 0.0, 0.0]));
        assert!(close3(b_world.direction([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
    }

    #[test]
    fn mate_offset_shifts_along_the_axis() {
        let a_world = Frame { origin: [0.0; 3], axis: [0.0, 0.0, 1.0], clock: None }.to_transform();
        let b_local = Transform::IDENTITY;
        let offset = Transform::translation([0.0, 0.0, 0.005]);
        let placement = solve_mate(&a_world, &b_local, MateAxis::Opposed, Some(offset));
        let b_world = b_local.then(&placement);
        assert!(close3(b_world.translation, [0.0, 0.0, 0.005]), "{:?}", b_world.translation);
    }
}
