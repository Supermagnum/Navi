//! Minimal 3-vector / 3x3 rotation maths (no external linear-algebra crate).

pub type Vec3 = [f64; 3];
/// Row-major 3x3 matrix.
pub type Mat3 = [[f64; 3]; 3];

pub const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

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
    if n.is_finite() && n > 1e-9 {
        Some([a[0] / n, a[1] / n, a[2] / n])
    } else {
        None
    }
}

pub fn mat_vec(m: &Mat3, v: Vec3) -> Vec3 {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}

pub fn mat_mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

pub fn transpose(m: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = m[j][i];
        }
    }
    out
}

/// Rotation about the X axis by `rad` (right-hand rule).
pub fn rot_x(rad: f64) -> Mat3 {
    let (s, c) = rad.sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}

/// Rotation about the Y axis by `rad` (right-hand rule).
pub fn rot_y(rad: f64) -> Mat3 {
    let (s, c) = rad.sin_cos();
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

/// Smallest rotation taking unit vector `a` onto unit vector `b` (Rodrigues).
pub fn rotation_between(a: Vec3, b: Vec3) -> Mat3 {
    let c = dot(a, b);
    if c > 1.0 - 1e-12 {
        return IDENTITY;
    }
    if c < -1.0 + 1e-12 {
        // 180 degrees about any axis perpendicular to `a`.
        let helper = if a[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let n = normalize(cross(a, helper)).unwrap_or([0.0, 0.0, 1.0]);
        let mut m = IDENTITY;
        for (i, row) in m.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = 2.0 * n[i] * n[j] - if i == j { 1.0 } else { 0.0 };
            }
        }
        return m;
    }
    let v = cross(a, b);
    let k = 1.0 / (1.0 + c);
    let vx = [[0.0, -v[2], v[1]], [v[2], 0.0, -v[0]], [-v[1], v[0], 0.0]];
    let vx2 = mat_mul(&vx, &vx);
    let mut m = IDENTITY;
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] += vx[i][j] + vx2[i][j] * k;
        }
    }
    m
}

/// Angle in degrees between two unit vectors.
pub fn angle_deg(a: Vec3, b: Vec3) -> f64 {
    dot(a, b).clamp(-1.0, 1.0).acos().to_degrees()
}

pub fn is_finite_mat(m: &Mat3) -> bool {
    m.iter().flatten().all(|x| x.is_finite())
}
