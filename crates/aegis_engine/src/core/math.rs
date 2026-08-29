//! Module mathématique natif Rust (remplace la crate `glam`).
//! Fournit les structures vectorielles et matricielles alignées `#[repr(C)]` pour Vulkan 1.4.

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

// ============================================================================
// Vec2
// ============================================================================
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub const ONE: Self = Self { x: 1.0, y: 1.0 };
    pub const X: Self = Self { x: 1.0, y: 0.0 };
    pub const Y: Self = Self { x: 0.0, y: 1.0 };
    pub const NEG_X: Self = Self { x: -1.0, y: 0.0 };
    pub const NEG_Y: Self = Self { x: 0.0, y: -1.0 };

    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn splat(v: f32) -> Self {
        Self { x: v, y: v }
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len > 0.0 { self / len } else { Self::ZERO }
    }

    #[inline]
    pub fn normalize_or_zero(self) -> Self {
        self.normalize()
    }

    #[inline]
    pub fn distance(self, rhs: Self) -> f32 {
        (self - rhs).length()
    }

    #[inline]
    pub fn distance_squared(self, rhs: Self) -> f32 {
        (self - rhs).length_squared()
    }

    #[inline]
    pub fn lerp(self, rhs: Self, s: f32) -> Self {
        self + (rhs - self) * s
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }

    #[inline]
    pub const fn from_array(a: [f32; 2]) -> Self {
        Self { x: a[0], y: a[1] }
    }
}

impl From<[f32; 2]> for Vec2 {
    #[inline]
    fn from(a: [f32; 2]) -> Self {
        Self::from_array(a)
    }
}

impl Add for Vec2 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

impl AddAssign for Vec2 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Vec2 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self { x: self.x - rhs.x, y: self.y - rhs.y }
    }
}

impl SubAssign for Vec2 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self { x: self.x * rhs, y: self.y * rhs }
    }
}

impl MulAssign<f32> for Vec2 {
    #[inline]
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}

impl Mul<Vec2> for f32 {
    type Output = Vec2;
    #[inline]
    fn mul(self, rhs: Vec2) -> Vec2 {
        rhs * self
    }
}

impl Div<f32> for Vec2 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: f32) -> Self {
        Self { x: self.x / rhs, y: self.y / rhs }
    }
}

impl Neg for Vec2 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self { x: -self.x, y: -self.y }
    }
}

// ============================================================================
// Vec3
// ============================================================================
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };
    pub const ONE: Self = Self { x: 1.0, y: 1.0, z: 1.0 };
    pub const X: Self = Self { x: 1.0, y: 0.0, z: 0.0 };
    pub const Y: Self = Self { x: 0.0, y: 1.0, z: 0.0 };
    pub const Z: Self = Self { x: 0.0, y: 0.0, z: 1.0 };
    pub const NEG_X: Self = Self { x: -1.0, y: 0.0, z: 0.0 };
    pub const NEG_Y: Self = Self { x: 0.0, y: -1.0, z: 0.0 };
    pub const NEG_Z: Self = Self { x: 0.0, y: 0.0, z: -1.0 };

    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub const fn splat(v: f32) -> Self {
        Self { x: v, y: v, z: v }
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    #[inline]
    pub fn cross(self, rhs: Self) -> Self {
        Self {
            x: self.y * rhs.z - self.z * rhs.y,
            y: self.z * rhs.x - self.x * rhs.z,
            z: self.x * rhs.y - self.y * rhs.x,
        }
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len > 0.0 { self / len } else { Self::ZERO }
    }

    #[inline]
    pub fn normalize_or_zero(self) -> Self {
        self.normalize()
    }

    #[inline]
    pub fn distance(self, rhs: Self) -> f32 {
        (self - rhs).length()
    }

    #[inline]
    pub fn distance_squared(self, rhs: Self) -> f32 {
        (self - rhs).length_squared()
    }

    #[inline]
    pub fn min(self, rhs: Self) -> Self {
        Self {
            x: self.x.min(rhs.x),
            y: self.y.min(rhs.y),
            z: self.z.min(rhs.z),
        }
    }

    #[inline]
    pub fn max(self, rhs: Self) -> Self {
        Self {
            x: self.x.max(rhs.x),
            y: self.y.max(rhs.y),
            z: self.z.max(rhs.z),
        }
    }

    #[inline]
    pub fn max_element(self) -> f32 {
        self.x.max(self.y).max(self.z)
    }

    #[inline]
    pub fn lerp(self, rhs: Self, s: f32) -> Self {
        self + (rhs - self) * s
    }

    #[inline]
    pub const fn extend(self, w: f32) -> Vec4 {
        Vec4 { x: self.x, y: self.y, z: self.z, w }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    #[inline]
    pub const fn from_array(a: [f32; 3]) -> Self {
        Self { x: a[0], y: a[1], z: a[2] }
    }
}

impl From<[f32; 3]> for Vec3 {
    #[inline]
    fn from(a: [f32; 3]) -> Self {
        Self::from_array(a)
    }
}

impl Add for Vec3 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self { x: self.x + rhs.x, y: self.y + rhs.y, z: self.z + rhs.z }
    }
}

impl AddAssign for Vec3 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Vec3 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self { x: self.x - rhs.x, y: self.y - rhs.y, z: self.z - rhs.z }
    }
}

impl SubAssign for Vec3 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self { x: self.x * rhs, y: self.y * rhs, z: self.z * rhs }
    }
}

impl Mul<Vec3> for f32 {
    type Output = Vec3;
    #[inline]
    fn mul(self, rhs: Vec3) -> Vec3 {
        rhs * self
    }
}

impl MulAssign<f32> for Vec3 {
    #[inline]
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}

impl Mul<Vec3> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self { x: self.x * rhs.x, y: self.y * rhs.y, z: self.z * rhs.z }
    }
}

impl Div<f32> for Vec3 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: f32) -> Self {
        Self { x: self.x / rhs, y: self.y / rhs, z: self.z / rhs }
    }
}

impl DivAssign<f32> for Vec3 {
    #[inline]
    fn div_assign(&mut self, rhs: f32) {
        *self = *self / rhs;
    }
}

impl Neg for Vec3 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self { x: -self.x, y: -self.y, z: -self.z }
    }
}

// ============================================================================
// Vec4
// ============================================================================
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0, w: 0.0 };
    pub const ONE: Self = Self { x: 1.0, y: 1.0, z: 1.0, w: 1.0 };

    #[inline]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    #[inline]
    pub const fn splat(v: f32) -> Self {
        Self { x: v, y: v, z: v, w: v }
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z + self.w * rhs.w
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len > 0.0 { self / len } else { Self::ZERO }
    }

    #[inline]
    pub const fn truncate(self) -> Vec3 {
        Vec3 { x: self.x, y: self.y, z: self.z }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }

    #[inline]
    pub const fn from_array(a: [f32; 4]) -> Self {
        Self { x: a[0], y: a[1], z: a[2], w: a[3] }
    }
}

impl From<[f32; 4]> for Vec4 {
    #[inline]
    fn from(a: [f32; 4]) -> Self {
        Self::from_array(a)
    }
}

impl Add for Vec4 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self { x: self.x + rhs.x, y: self.y + rhs.y, z: self.z + rhs.z, w: self.w + rhs.w }
    }
}

impl Sub for Vec4 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self { x: self.x - rhs.x, y: self.y - rhs.y, z: self.z - rhs.z, w: self.w - rhs.w }
    }
}

impl Mul<f32> for Vec4 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self { x: self.x * rhs, y: self.y * rhs, z: self.z * rhs, w: self.w * rhs }
    }
}

impl Mul<Vec4> for f32 {
    type Output = Vec4;
    #[inline]
    fn mul(self, rhs: Vec4) -> Vec4 {
        rhs * self
    }
}

impl Div<f32> for Vec4 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: f32) -> Self {
        Self { x: self.x / rhs, y: self.y / rhs, z: self.z / rhs, w: self.w / rhs }
    }
}

impl Neg for Vec4 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self { x: -self.x, y: -self.y, z: -self.z, w: -self.w }
    }
}

// ============================================================================
// Quat (Quaternion)
// ============================================================================
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Self = Self { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    #[inline]
    pub const fn from_xyzw(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        let half = angle * 0.5;
        let s = half.sin();
        let norm_axis = axis.normalize();
        Self {
            x: norm_axis.x * s,
            y: norm_axis.y * s,
            z: norm_axis.z * s,
            w: half.cos(),
        }
    }

    pub fn from_rotation_y(angle: f32) -> Self {
        Self::from_axis_angle(Vec3::Y, angle)
    }

    pub fn from_rotation_x(angle: f32) -> Self {
        Self::from_axis_angle(Vec3::X, angle)
    }

    pub fn from_rotation_z(angle: f32) -> Self {
        Self::from_axis_angle(Vec3::Z, angle)
    }
}

// ============================================================================
// Mat3 (Matrice 3x3 Column-Major)
// ============================================================================
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3 {
    pub cols: [Vec3; 3],
}

impl Default for Mat3 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat3 {
    pub const IDENTITY: Self = Self {
        cols: [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ],
    };

    pub const ZERO: Self = Self {
        cols: [Vec3::ZERO; 3],
    };

    #[inline]
    pub fn from_cols(c0: Vec3, c1: Vec3, c2: Vec3) -> Self {
        Self { cols: [c0, c1, c2] }
    }
}

// ============================================================================
// Mat4 (Matrice 4x4 Column-Major pour Vulkan)
// ============================================================================
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    pub cols: [Vec4; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        cols: [
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ],
    };

    pub const ZERO: Self = Self {
        cols: [Vec4::ZERO; 4],
    };

    #[inline]
    pub fn from_cols(c0: Vec4, c1: Vec4, c2: Vec4, c3: Vec4) -> Self {
        Self { cols: [c0, c1, c2, c3] }
    }

    #[inline]
    pub fn from_translation(translation: Vec3) -> Self {
        Self {
            cols: [
                Vec4::new(1.0, 0.0, 0.0, 0.0),
                Vec4::new(0.0, 1.0, 0.0, 0.0),
                Vec4::new(0.0, 0.0, 1.0, 0.0),
                Vec4::new(translation.x, translation.y, translation.z, 1.0),
            ],
        }
    }

    #[inline]
    pub fn from_scale(scale: Vec3) -> Self {
        Self {
            cols: [
                Vec4::new(scale.x, 0.0, 0.0, 0.0),
                Vec4::new(0.0, scale.y, 0.0, 0.0),
                Vec4::new(0.0, 0.0, scale.z, 0.0),
                Vec4::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    #[inline]
    pub fn from_rotation_x(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            cols: [
                Vec4::new(1.0, 0.0, 0.0, 0.0),
                Vec4::new(0.0, c, s, 0.0),
                Vec4::new(0.0, -s, c, 0.0),
                Vec4::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    #[inline]
    pub fn from_rotation_y(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            cols: [
                Vec4::new(c, 0.0, -s, 0.0),
                Vec4::new(0.0, 1.0, 0.0, 0.0),
                Vec4::new(s, 0.0, c, 0.0),
                Vec4::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    #[inline]
    pub fn from_rotation_z(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            cols: [
                Vec4::new(c, s, 0.0, 0.0),
                Vec4::new(-s, c, 0.0, 0.0),
                Vec4::new(0.0, 0.0, 1.0, 0.0),
                Vec4::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    pub fn from_quat(q: Quat) -> Self {
        let x2 = q.x + q.x; let y2 = q.y + q.y; let z2 = q.z + q.z;
        let xx = q.x * x2;  let xy = q.x * y2;  let xz = q.x * z2;
        let yy = q.y * y2;  let yz = q.y * z2;  let zz = q.z * z2;
        let wx = q.w * x2;  let wy = q.w * y2;  let wz = q.w * z2;

        Self {
            cols: [
                Vec4::new(1.0 - (yy + zz), xy + wz, xz - wy, 0.0),
                Vec4::new(xy - wz, 1.0 - (xx + zz), yz + wx, 0.0),
                Vec4::new(xz + wy, yz - wx, 1.0 - (xx + yy), 0.0),
                Vec4::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    pub fn from_scale_rotation_translation(scale: Vec3, rotation: Quat, translation: Vec3) -> Self {
        let rot = Self::from_quat(rotation);
        Self {
            cols: [
                rot.cols[0] * scale.x,
                rot.cols[1] * scale.y,
                rot.cols[2] * scale.z,
                Vec4::new(translation.x, translation.y, translation.z, 1.0),
            ],
        }
    }

    /// Matrice de perspective adaptée à la convention de profondeur Vulkan `[0.0, 1.0]`.
    pub fn perspective_rh(fov_y_radians: f32, aspect_ratio: f32, z_near: f32, z_far: f32) -> Self {
        let inv_tan = 1.0 / (fov_y_radians * 0.5).tan();
        let m00 = inv_tan / aspect_ratio;
        let m11 = inv_tan;
        let m22 = z_far / (z_near - z_far);
        let m32 = (z_far * z_near) / (z_near - z_far);

        Self {
            cols: [
                Vec4::new(m00, 0.0, 0.0, 0.0),
                Vec4::new(0.0, m11, 0.0, 0.0),
                Vec4::new(0.0, 0.0, m22, -1.0),
                Vec4::new(0.0, 0.0, m32, 0.0),
            ],
        }
    }

    /// Projection **orthographique** droitière, profondeur Vulkan dans `[0, 1]`.
    ///
    /// C'est la projection d'une lumière directionnelle : le soleil est si loin que ses rayons
    /// arrivent parallèles, donc sans perspective. Une projection perspective donnerait des ombres
    /// qui s'écartent en s'éloignant — un défaut classique et immédiatement visible.
    ///
    /// La même convention de profondeur que [`Mat4::perspective_rh`] : un point à `z = -near`
    /// atterrit en 0, un point à `z = -far` en 1. Les mélanger produirait des ombres décalées sans
    /// qu'aucune ligne ne paraisse fausse.
    pub fn orthographic_rh(
        gauche: f32,
        droite: f32,
        bas: f32,
        haut: f32,
        proche: f32,
        loin: f32,
    ) -> Self {
        let m00 = 2.0 / (droite - gauche);
        let m11 = 2.0 / (haut - bas);
        let m22 = 1.0 / (proche - loin);
        let m30 = -(droite + gauche) / (droite - gauche);
        let m31 = -(haut + bas) / (haut - bas);
        let m32 = proche / (proche - loin);

        Self {
            cols: [
                Vec4::new(m00, 0.0, 0.0, 0.0),
                Vec4::new(0.0, m11, 0.0, 0.0),
                Vec4::new(0.0, 0.0, m22, 0.0),
                Vec4::new(m30, m31, m32, 1.0),
            ],
        }
    }

    /// Matrice de vue `look_at_rh` (Droitier).
    pub fn look_at_rh(eye: Vec3, center: Vec3, up: Vec3) -> Self {
        let f = (center - eye).normalize();
        let s = f.cross(up).normalize();
        let u = s.cross(f);

        Self {
            cols: [
                Vec4::new(s.x, u.x, -f.x, 0.0),
                Vec4::new(s.y, u.y, -f.y, 0.0),
                Vec4::new(s.z, u.z, -f.z, 0.0),
                Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
            ],
        }
    }

    pub fn transpose(&self) -> Self {
        Self {
            cols: [
                Vec4::new(self.cols[0].x, self.cols[1].x, self.cols[2].x, self.cols[3].x),
                Vec4::new(self.cols[0].y, self.cols[1].y, self.cols[2].y, self.cols[3].y),
                Vec4::new(self.cols[0].z, self.cols[1].z, self.cols[2].z, self.cols[3].z),
                Vec4::new(self.cols[0].w, self.cols[1].w, self.cols[2].w, self.cols[3].w),
            ],
        }
    }

    pub fn inverse(&self) -> Self {
        let m = self.to_cols_array_2d();
        let s0 = m[0][0] * m[1][1] - m[1][0] * m[0][1];
        let s1 = m[0][0] * m[1][2] - m[1][0] * m[0][2];
        let s2 = m[0][0] * m[1][3] - m[1][0] * m[0][3];
        let s3 = m[0][1] * m[1][2] - m[1][1] * m[0][2];
        let s4 = m[0][1] * m[1][3] - m[1][1] * m[0][3];
        let s5 = m[0][2] * m[1][3] - m[1][2] * m[0][3];

        let c5 = m[2][2] * m[3][3] - m[3][2] * m[2][3];
        let c4 = m[2][1] * m[3][3] - m[3][1] * m[2][3];
        let c3 = m[2][1] * m[3][2] - m[3][1] * m[2][2];
        let c2 = m[2][0] * m[3][3] - m[3][0] * m[2][3];
        let c1 = m[2][0] * m[3][2] - m[3][0] * m[2][2];
        let c0 = m[2][0] * m[3][1] - m[3][0] * m[2][1];

        let det = s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0;
        if det.abs() < 1e-8 {
            return Self::IDENTITY;
        }

        let inv_det = 1.0 / det;
        let mut inv = [[0.0; 4]; 4];

        inv[0][0] = ( m[1][1] * c5 - m[1][2] * c4 + m[1][3] * c3) * inv_det;
        inv[0][1] = (-m[0][1] * c5 + m[0][2] * c4 - m[0][3] * c3) * inv_det;
        inv[0][2] = ( m[3][1] * s5 - m[3][2] * s4 + m[3][3] * s3) * inv_det;
        inv[0][3] = (-m[2][1] * s5 + m[2][2] * s4 - m[2][3] * s3) * inv_det;

        inv[1][0] = (-m[1][0] * c5 + m[1][2] * c2 - m[1][3] * c1) * inv_det;
        inv[1][1] = ( m[0][0] * c5 - m[0][2] * c2 + m[0][3] * c1) * inv_det;
        inv[1][2] = (-m[3][0] * s5 + m[3][2] * s2 - m[3][3] * s1) * inv_det;
        inv[1][3] = ( m[2][0] * s5 - m[2][2] * s2 + m[2][3] * s1) * inv_det;

        inv[2][0] = ( m[1][0] * c4 - m[1][1] * c2 + m[1][3] * c0) * inv_det;
        inv[2][1] = (-m[0][0] * c4 + m[0][1] * c2 - m[0][3] * c0) * inv_det;
        inv[2][2] = ( m[3][0] * s4 - m[3][1] * s2 + m[3][3] * s0) * inv_det;
        inv[2][3] = (-m[2][0] * s4 + m[2][1] * s2 - m[2][3] * s0) * inv_det;

        inv[3][0] = (-m[1][0] * c3 + m[1][1] * c1 - m[1][2] * c0) * inv_det;
        inv[3][1] = ( m[0][0] * c3 - m[0][1] * c1 + m[0][2] * c0) * inv_det;
        inv[3][2] = (-m[3][0] * s3 + m[3][1] * s1 - m[3][2] * s0) * inv_det;
        inv[3][3] = ( m[2][0] * s3 - m[2][1] * s1 + m[2][2] * s0) * inv_det;

        Self {
            cols: [
                Vec4::from_array(inv[0]),
                Vec4::from_array(inv[1]),
                Vec4::from_array(inv[2]),
                Vec4::from_array(inv[3]),
            ],
        }
    }

    #[inline]
    pub fn transform_point3(&self, point: Vec3) -> Vec3 {
        (*self * point.extend(1.0)).truncate()
    }

    #[inline]
    pub fn is_nan(&self) -> bool {
        self.cols.iter().any(|c| c.x.is_nan() || c.y.is_nan() || c.z.is_nan() || c.w.is_nan())
    }

    #[inline]
    pub fn to_cols_array_2d(&self) -> [[f32; 4]; 4] {
        [
            self.cols[0].to_array(),
            self.cols[1].to_array(),
            self.cols[2].to_array(),
            self.cols[3].to_array(),
        ]
    }
}

impl Mul<Mat4> for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let mut cols = [Vec4::ZERO; 4];
        for i in 0..4 {
            let col = rhs.cols[i];
            cols[i] = self.cols[0] * col.x
                    + self.cols[1] * col.y
                    + self.cols[2] * col.z
                    + self.cols[3] * col.w;
        }
        Self { cols }
    }
}

impl Mul<Vec4> for Mat4 {
    type Output = Vec4;
    fn mul(self, rhs: Vec4) -> Vec4 {
        self.cols[0] * rhs.x + self.cols[1] * rhs.y + self.cols[2] * rhs.z + self.cols[3] * rhs.w
    }
}
