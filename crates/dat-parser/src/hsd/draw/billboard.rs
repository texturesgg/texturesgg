//! Billboards: joints the game turns to face the camera as it draws them.
//!
//! `HSD_JObjMakePositionMtx` (sysdolphin `displayfunc.c`) builds a
//! billboarded joint's view-space matrix from its unbillboarded one, keeping
//! position and each axis's scale and replacing the rotation; only that
//! joint's own display objects use it, and its children keep the joint's
//! unbillboarded matrix. Here the result goes back to world space through the
//! inverse view, since evaluated draw work is in world space.

use crate::descriptor::jobj::flags::{
    BILLBOARD, BILLBOARD_FIELD, HBILLBOARD, PBILLBOARD, RBILLBOARD, VBILLBOARD,
};
use crate::math::Mat4;

type Vec3 = [f32; 3];

/// `world` billboarded for a joint with `flags`, under the camera's `view`
/// and its inverse; `rotate_z` is the joint's own Z rotation, which a
/// rotating billboard keeps. `world` unchanged for a joint with no billboard.
pub(super) fn billboarded(
    world: Mat4,
    flags: u32,
    rotate_z: f32,
    view: &Mat4,
    view_inverse: &Mat4,
) -> Mat4 {
    let kind = flags & BILLBOARD_FIELD;
    if kind == 0 {
        return world;
    }
    let perspective = flags & PBILLBOARD != 0;
    let modelview = view.mul(&world);
    let facing = match kind {
        BILLBOARD => billboard(&modelview, perspective),
        VBILLBOARD => vertical(&modelview, perspective),
        HBILLBOARD => horizontal(&modelview, perspective),
        RBILLBOARD => rotating(&modelview, rotate_z),
        // `HSD_JObjMakePositionMtx` panics on any other value of the field.
        _ => return world,
    };
    view_inverse.mul(&facing)
}

fn column(matrix: &Mat4, index: usize) -> Vec3 {
    [matrix.0[index][0], matrix.0[index][1], matrix.0[index][2]]
}

fn length(vector: Vec3) -> f32 {
    vector.iter().map(|value| value * value).sum::<f32>().sqrt()
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn scaled(vector: Vec3, scale: f32) -> Vec3 {
    vector.map(|value| value * scale)
}

/// A matrix with these axis columns at `position`.
fn from_columns(x: Vec3, y: Vec3, z: Vec3, position: Vec3) -> Mat4 {
    Mat4([
        [x[0], x[1], x[2], 0.0],
        [y[0], y[1], y[2], 0.0],
        [z[0], z[1], z[2], 0.0],
        [position[0], position[1], position[2], 1.0],
    ])
}

/// `mkBillBoardMtx`: the joint faces the camera, its Y kept upright on
/// screen (or, as a perspective billboard, facing the eye).
fn billboard(source: &Mat4, perspective: bool) -> Mat4 {
    let sx = length(column(source, 0));
    let mut sz = length(column(source, 2));
    let mut ay = column(source, 1);
    let sy = length(ay);
    let position = column(source, 3);
    let (ax, az) = if perspective {
        let ax = cross(position, ay);
        ay = cross(ax, position);
        sz /= -length(position);
        (ax, position)
    } else {
        let z = [0.0, 0.0, 1.0];
        let ax = cross(ay, z);
        ay = cross(z, ax);
        (ax, z)
    };
    from_columns(
        scaled(ax, sx / length(ax)),
        scaled(ay, sy / length(ay)),
        scaled(az, sz),
        position,
    )
}

/// `mkVBillBoardMtx`: the joint turns about its own Y to face the camera.
fn vertical(source: &Mat4, perspective: bool) -> Mat4 {
    let position = column(source, 3);
    let ay = column(source, 1);
    let sx = length(column(source, 0));
    let sz = length(column(source, 2));
    let ax = if perspective {
        cross(position, ay)
    } else {
        cross(ay, [0.0, 0.0, 1.0])
    };
    let az = cross(ax, ay);
    from_columns(
        scaled(ax, sx / length(ax)),
        ay,
        scaled(az, sz / length(az)),
        position,
    )
}

/// `mkHBillBoardMtx`: the joint turns about its own X to face the camera.
fn horizontal(source: &Mat4, perspective: bool) -> Mat4 {
    let position = column(source, 3);
    let ax = column(source, 0);
    let sy = length(column(source, 1));
    let sz = length(column(source, 2));
    let az = if perspective {
        let horizontal = (position[0] * position[0] + position[2] * position[2]).sqrt();
        let up = [
            -position[1] / horizontal * position[0],
            horizontal,
            -position[1] / horizontal * position[2],
        ];
        cross(ax, up)
    } else {
        cross(ax, [0.0, 1.0, 0.0])
    };
    let ay = cross(az, ax);
    from_columns(
        ax,
        scaled(ay, sy / length(ay)),
        scaled(az, sz / length(az)),
        position,
    )
}

/// `mkRBillBoardMtx`: the joint faces the camera square on, keeping only
/// its own Z rotation.
fn rotating(source: &Mat4, rotate_z: f32) -> Mat4 {
    let (sin, cos) = rotate_z.sin_cos();
    let sx = length(column(source, 0));
    let sy = length(column(source, 1));
    let sz = length(column(source, 2));
    from_columns(
        [cos * sx, sin * sx, 0.0],
        [-sin * sy, cos * sy, 0.0],
        [0.0, 0.0, sz],
        column(source, 3),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: Mat4, expected: Mat4) {
        for (a, e) in actual.0.iter().flatten().zip(expected.0.iter().flatten()) {
            assert!((a - e).abs() < 1e-4, "{actual:?} != {expected:?}");
        }
    }

    /// A billboard turned away from the camera comes back square on to it,
    /// keeping where it is and how big it is, whatever the camera's angle.
    #[test]
    fn a_billboard_faces_the_camera_and_keeps_its_place_and_size() {
        // The joint is turned a quarter turn about Y, scaled 2, at (1, 2, 3).
        let world = Mat4::from_srt(
            [2.0; 3],
            [0.0, std::f32::consts::FRAC_PI_2, 0.0],
            [1.0, 2.0, 3.0],
        );
        // The camera looks along world X: its view turns world -X into -Z.
        let view = Mat4::from_srt(
            [1.0; 3],
            [0.0, std::f32::consts::FRAC_PI_2, 0.0],
            [0.0, 0.0, -10.0],
        );
        let view_inverse = crate::hsd::draw::psmtx_inverse_affine(view).unwrap();
        let facing = billboarded(world, BILLBOARD, 0.0, &view, &view_inverse);
        let in_view = view.mul(&facing);
        // In view space it is square on: X right, Y up, Z toward the eye,
        // each at the joint's scale.
        assert_close(
            Mat4([
                [2.0, 0.0, 0.0, 0.0],
                [0.0, 2.0, 0.0, 0.0],
                [0.0, 0.0, 2.0, 0.0],
                view.mul(&world).0[3],
            ]),
            in_view,
        );
        // A joint without a billboard is left as it is.
        assert_close(billboarded(world, 0, 0.0, &view, &view_inverse), world);
    }
}
