//! Small renderer-neutral matrix type used by source and HSD semantic layers.
//!
//! Column-major storage: `m[col][row]`, matching the renderer and shader convention.

/// 4x4 matrix in column-major order.
#[derive(Debug, Clone, Copy)]
pub struct Mat4(pub [[f32; 4]; 4]);

impl Mat4 {
    pub fn zero() -> Self {
        Self([[0.0; 4]; 4])
    }

    pub fn identity() -> Self {
        Self([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Build an affine matrix from a 3x4 row-major matrix.
    pub fn from_row_major_3x4(rows: [[f32; 4]; 3]) -> Self {
        Self([
            [rows[0][0], rows[1][0], rows[2][0], 0.0],
            [rows[0][1], rows[1][1], rows[2][1], 0.0],
            [rows[0][2], rows[1][2], rows[2][2], 0.0],
            [rows[0][3], rows[1][3], rows[2][3], 1.0],
        ])
    }

    /// Build a local transform from Euler XYZ rotation (radians), scale, and translation.
    /// Order: T * Rz * Ry * Rx * S (HSD convention)
    pub fn from_srt(scale: [f32; 3], rotation: [f32; 3], translation: [f32; 3]) -> Self {
        let [rx, ry, rz] = rotation;
        let (sx, sy, sz) = (scale[0], scale[1], scale[2]);
        let (cx, sx_r) = (rx.cos(), rx.sin());
        let (cy, sy_r) = (ry.cos(), ry.sin());
        let (cz, sz_r) = (rz.cos(), rz.sin());

        // Combined rotation Rz * Ry * Rx, then scale each column, then set translation
        Self([
            [cy * cz * sx, cy * sz_r * sx, -sy_r * sx, 0.0],
            [
                (sx_r * sy_r * cz - cx * sz_r) * sy,
                (sx_r * sy_r * sz_r + cx * cz) * sy,
                sx_r * cy * sy,
                0.0,
            ],
            [
                (cx * sy_r * cz + sx_r * sz_r) * sz,
                (cx * sy_r * sz_r - sx_r * cz) * sz,
                cx * cy * sz,
                0.0,
            ],
            [translation[0], translation[1], translation[2], 1.0],
        ])
    }

    /// Multiply two matrices: self * rhs
    pub fn mul(&self, rhs: &Mat4) -> Mat4 {
        let mut out = [[0.0f32; 4]; 4];
        for (c, column) in out.iter_mut().enumerate() {
            for (r, value) in column.iter_mut().enumerate() {
                *value = self.0[0][r] * rhs.0[c][0]
                    + self.0[1][r] * rhs.0[c][1]
                    + self.0[2][r] * rhs.0[c][2]
                    + self.0[3][r] * rhs.0[c][3];
            }
        }
        Mat4(out)
    }

    pub fn add_scaled(&mut self, rhs: &Mat4, scale: f32) {
        for c in 0..4 {
            for r in 0..4 {
                self.0[c][r] += rhs.0[c][r] * scale;
            }
        }
    }

    /// Transform a position (w=1): applies rotation, scale, and translation.
    pub fn transform_point(&self, p: [f32; 3]) -> [f32; 3] {
        [
            self.0[0][0] * p[0] + self.0[1][0] * p[1] + self.0[2][0] * p[2] + self.0[3][0],
            self.0[0][1] * p[0] + self.0[1][1] * p[1] + self.0[2][1] * p[2] + self.0[3][1],
            self.0[0][2] * p[0] + self.0[1][2] * p[1] + self.0[2][2] * p[2] + self.0[3][2],
        ]
    }
}
