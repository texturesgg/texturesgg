//! Orbit camera that frames the evaluated bounds.
//!
//! Math is in f64; matrices are stored column-major as f32 with WebGPU's
//! [0, 1] depth.

use crate::error::{HsdRenderError, Result};
use crate::geometry::Bounds;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraView {
    Front,
    Back,
    PositiveX,
    NegativeX,
}

impl CameraView {
    pub const ALL: [Self; 4] = [Self::Front, Self::Back, Self::PositiveX, Self::NegativeX];

    /// The view's name in captures and the pixel baseline.
    pub fn id(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Back => "back",
            Self::PositiveX => "positive-x",
            Self::NegativeX => "negative-x",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|view| view.id() == id)
    }

    pub fn orbit(self) -> Orbit {
        let yaw = match self {
            Self::Front => 0.0,
            Self::Back => PI,
            Self::PositiveX => PI / 2.0,
            Self::NegativeX => -PI / 2.0,
        };
        Orbit {
            yaw,
            ..Orbit::default()
        }
    }
}

/// Where the camera looks from: angles around the model, a distance, and a
/// pan of the point it looks at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub yaw: f64,
    pub pitch: f64,
    /// The distance as a multiple of the one that fits the whole model.
    pub zoom: f64,
    /// The point looked at, offset from the model's center in units of its
    /// bounding radius; kept within the bounds.
    pub pan: [f64; 3],
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            zoom: 1.0,
            pan: [0.0; 3],
        }
    }
}

impl Orbit {
    /// The closest zoom: near enough to work on one small part of a model.
    pub const MIN_ZOOM: f64 = 0.02;
    pub const MAX_ZOOM: f64 = 4.0;

    /// Clamp pitch short of the poles, zoom to its range, and the pan to
    /// the model's bounds.
    pub fn clamped(self) -> Result<Self> {
        self.clamped_within(1.0)
    }

    /// [`Self::clamped`] for a camera framing a focus smaller than the
    /// scene: `reach` is how many framed radii the scene extends, so the
    /// camera can still zoom out to, and pan across, all of it.
    pub fn clamped_within(self, reach: f64) -> Result<Self> {
        let reach = if reach.is_finite() {
            reach.max(1.0)
        } else {
            1.0
        };
        if !(self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.zoom.is_finite()
            && self.pan.iter().all(|value| value.is_finite()))
            || self.zoom <= 0.0
        {
            return Err(HsdRenderError::InvalidScene(
                "orbit values are invalid".into(),
            ));
        }
        let length = self
            .pan
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
        let pan = if length > reach {
            self.pan.map(|value| value / length * reach)
        } else {
            self.pan
        };
        Ok(Self {
            yaw: self.yaw,
            pitch: self.pitch.clamp(-PI * 0.495, PI * 0.495),
            zoom: self.zoom.clamp(Self::MIN_ZOOM, Self::MAX_ZOOM * reach),
            pan,
        })
    }

    /// Move the point looked at so the model follows a drag of (`dx`, `dy`)
    /// pixels across a `width`×`height` viewport.
    pub fn panned(self, dx: f64, dy: f64, width: u32, height: u32) -> Self {
        let (radius_per_distance, vertical_half_fov) = fit(width, height);
        // Model radii per pixel at the point looked at.
        let scale = 2.0 * vertical_half_fov.tan() * self.zoom * radius_per_distance.recip()
            / f64::from(height.max(1));
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        // The camera's right and up axes, as `look_at` builds them.
        let right = [cos_yaw, 0.0, -sin_yaw];
        let up = [-sin_pitch * sin_yaw, cos_pitch, -sin_pitch * cos_yaw];
        let pan =
            [0, 1, 2].map(|axis| self.pan[axis] - right[axis] * dx * scale + up[axis] * dy * scale);
        Self { pan, ..self }
    }
}

/// What the camera frames in place of the whole scene: an upright rectangle
/// facing +Z, centered on `center`. A stage's camera range is one; its
/// backdrop makes the whole scene many times larger.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Focus {
    pub center: [f64; 3],
    pub half_width: f64,
    pub half_height: f64,
}

impl Focus {
    /// The radius of the sphere whose fitted distance shows the whole
    /// rectangle from the front in a `width`×`height` viewport.
    pub fn radius(&self, width: u32, height: u32) -> f64 {
        let (radius_per_distance, vertical_half_fov) = fit(width, height);
        let aspect = f64::from(width.max(1)) / f64::from(height.max(1));
        let distance = (self.half_height / vertical_half_fov.tan())
            .max(self.half_width / (vertical_half_fov.tan() * aspect));
        (distance * FIT_MARGIN * radius_per_distance).max(0.01)
    }

    /// How many framed radii the scene's `bounds` extend from the focus, for
    /// [`Orbit::clamped_within`].
    pub fn reach(&self, bounds: &Bounds, width: u32, height: u32) -> f64 {
        let offset = [0, 1, 2]
            .map(|axis| bounds.center[axis] - self.center[axis])
            .iter()
            .map(|offset| offset * offset)
            .sum::<f64>()
            .sqrt();
        (offset + bounds.radius) / self.radius(width, height)
    }
}

/// The model radius per unit of fitted camera distance for a viewport, and
/// the vertical half field of view.
fn fit(width: u32, height: u32) -> (f64, f64) {
    let aspect = f64::from(width.max(1)) / f64::from(height.max(1));
    let vertical_half_fov = FOV_DEGREES * PI / 360.0;
    let horizontal_half_fov = (vertical_half_fov.tan() * aspect).atan();
    (
        vertical_half_fov.min(horizontal_half_fov).sin() / FIT_MARGIN,
        vertical_half_fov,
    )
}

const FOV_DEGREES: f64 = 45.0;
const FIT_MARGIN: f64 = 1.08;

#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub view: [f32; 16],
    pub view_projection: [f32; 16],
    pub position: [f64; 3],
    pub fov_degrees: f64,
    pub aspect: f64,
    pub near: f64,
    pub far: f64,
    pub distance: f64,
}

impl Camera {
    /// Frame `focus`, or the whole of `bounds` without one. The clip planes
    /// take in all of `bounds` either way.
    pub fn frame(
        bounds: &Bounds,
        focus: Option<&Focus>,
        width: u32,
        height: u32,
        orbit: Orbit,
    ) -> Result<Self> {
        let fov_degrees = FOV_DEGREES;
        let aspect = f64::from(width) / f64::from(height);
        let (center, radius) = focus.map_or((bounds.center, bounds.radius), |focus| {
            (focus.center, focus.radius(width, height))
        });
        let fitted_distance = radius / fit(width, height).0;
        let distance = fitted_distance * orbit.zoom;
        let target = [0, 1, 2].map(|axis| center[axis] + orbit.pan[axis] * radius);
        let direction = [
            orbit.yaw.sin() * orbit.pitch.cos(),
            orbit.pitch.sin(),
            orbit.yaw.cos() * orbit.pitch.cos(),
        ];
        let position = [0, 1, 2].map(|axis| target[axis] + direction[axis] * distance);
        let (nearest, farthest) = if focus.is_some() {
            let to_center = [0, 1, 2]
                .map(|axis| position[axis] - bounds.center[axis])
                .iter()
                .map(|offset| offset * offset)
                .sum::<f64>()
                .sqrt();
            (to_center - bounds.radius, to_center + bounds.radius)
        } else {
            // The model lies within two radii of a panned target.
            (
                distance - bounds.radius * 2.0,
                distance + bounds.radius * 2.0,
            )
        };
        let near = nearest.max(distance / 1000.0).max(0.01);
        let far = farthest.max(100.0);
        let view = look_at(position, target, [0.0, 1.0, 0.0])?;
        let projection = perspective(fov_degrees * PI / 180.0, aspect, near, far);
        Ok(Self {
            view,
            view_projection: multiply(&projection, &view),
            position,
            fov_degrees,
            aspect,
            near,
            far,
            distance,
        })
    }
}

fn look_at(eye: [f64; 3], target: [f64; 3], up: [f64; 3]) -> Result<[f32; 16]> {
    let z = normalize([eye[0] - target[0], eye[1] - target[1], eye[2] - target[2]])?;
    let x = normalize(cross(up, z))?;
    let y = cross(z, x);
    Ok([
        x[0],
        y[0],
        z[0],
        0.0,
        x[1],
        y[1],
        z[1],
        0.0,
        x[2],
        y[2],
        z[2],
        0.0,
        -dot(x, eye),
        -dot(y, eye),
        -dot(z, eye),
        1.0,
    ]
    .map(|value| value as f32))
}

fn perspective(fov_radians: f64, aspect: f64, near: f64, far: f64) -> [f32; 16] {
    let f = 1.0 / (fov_radians / 2.0).tan();
    let range = 1.0 / (near - far);
    [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        far * range,
        -1.0,
        0.0,
        0.0,
        far * near * range,
        0.0,
    ]
    .map(|value| value as f32)
}

/// Column-major product; like the site, accumulates the f32 inputs in f64.
fn multiply(left: &[f32; 16], right: &[f32; 16]) -> [f32; 16] {
    let mut output = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            let mut value = 0.0_f64;
            for inner in 0..4 {
                value += f64::from(left[inner * 4 + row]) * f64::from(right[column * 4 + inner]);
            }
            output[column * 4 + row] = value as f32;
        }
    }
    output
}

fn normalize(vector: [f64; 3]) -> Result<[f64; 3]> {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if !length.is_finite() || length == 0.0 {
        return Err(HsdRenderError::InvalidDrawWork(
            "camera vector cannot be normalized".into(),
        ));
    }
    Ok(vector.map(|component| component / length))
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_bounds() -> Bounds {
        Bounds {
            min: [-1.0; 3],
            max: [1.0; 3],
            center: [0.0; 3],
            radius: 3.0_f64.sqrt(),
        }
    }

    #[test]
    fn front_view_looks_down_negative_z() {
        let camera =
            Camera::frame(&unit_bounds(), None, 642, 528, CameraView::Front.orbit()).unwrap();
        assert!(camera.position[0].abs() < 1e-12 && camera.position[2] > 0.0);
        // The model center projects to the middle of the viewport.
        let m = camera.view_projection;
        let clip_w = m[15];
        assert!((m[12] / clip_w).abs() < 1e-6 && (m[13] / clip_w).abs() < 1e-6);
        let depth = m[14] / clip_w;
        assert!((0.0..1.0).contains(&depth));
    }

    /// Battlefield's camera range inside a backdrop 40 times its size.
    fn stage() -> (Bounds, Focus) {
        (
            Bounds {
                min: [-8000.0; 3],
                max: [8000.0; 3],
                center: [0.0, 500.0, -2000.0],
                radius: 8000.0 * 3.0_f64.sqrt(),
            },
            Focus {
                center: [0.0, 55.5, 0.0],
                half_width: 200.0,
                half_height: 114.5,
            },
        )
    }

    #[test]
    fn a_focus_fills_the_viewport_whatever_its_shape() {
        let (bounds, focus) = stage();
        // Wide, square, and tall viewports each fit the limiting side.
        for (width, height) in [(1600, 900), (192, 192), (400, 900)] {
            let camera = Camera::frame(
                &bounds,
                Some(&focus),
                width,
                height,
                CameraView::Front.orbit(),
            )
            .unwrap();
            let corner = project(
                &camera,
                [
                    focus.center[0] + focus.half_width,
                    focus.center[1] + focus.half_height,
                    0.0,
                ],
            );
            let fill = corner[0].max(corner[1]);
            assert!(corner[0] <= 1.0 && corner[1] <= 1.0, "{corner:?}");
            assert!((fill - 1.0 / FIT_MARGIN).abs() < 1e-3, "{corner:?}");
            // The clip planes still hold the whole backdrop.
            assert!(camera.far > bounds.radius);
        }
    }

    #[test]
    fn a_focused_orbit_reaches_the_whole_scene() {
        let (bounds, focus) = stage();
        let reach = focus.reach(&bounds, 1600, 900);
        assert!(reach > 40.0, "{reach}");
        let far_out = Orbit {
            zoom: 1000.0,
            pan: [0.0, 1000.0, 0.0],
            ..Orbit::default()
        };
        let plain = far_out.clamped().unwrap();
        assert_eq!((plain.zoom, plain.pan[1]), (Orbit::MAX_ZOOM, 1.0));
        let focused = far_out.clamped_within(reach).unwrap();
        assert_eq!(focused.zoom, Orbit::MAX_ZOOM * reach);
        assert!((focused.pan[1] - reach).abs() < 1e-9);
        // A reach below one never tightens the plain limits.
        assert_eq!(far_out.clamped_within(0.1).unwrap(), plain);
    }

    /// Where `point` lands in normalized device coordinates.
    fn project(camera: &Camera, point: [f64; 3]) -> [f64; 2] {
        let m = camera.view_projection.map(f64::from);
        let clip = [0, 1, 3].map(|row| {
            m[row] * point[0] + m[4 + row] * point[1] + m[8 + row] * point[2] + m[12 + row]
        });
        [clip[0] / clip[2], clip[1] / clip[2]]
    }

    #[test]
    fn panning_follows_the_drag_at_any_angle() {
        let (width, height) = (642, 528);
        for orbit in [
            CameraView::Front.orbit(),
            Orbit {
                yaw: 0.7,
                pitch: -0.4,
                zoom: 0.3,
                ..Orbit::default()
            },
        ] {
            let before = Camera::frame(&unit_bounds(), None, width, height, orbit).unwrap();
            let panned = orbit.panned(40.0, -25.0, width, height).clamped().unwrap();
            let after = Camera::frame(&unit_bounds(), None, width, height, panned).unwrap();
            // The center moves with the pointer: 40 px right, 25 px up.
            let (a, b) = (project(&before, [0.0; 3]), project(&after, [0.0; 3]));
            let moved = [
                (b[0] - a[0]) * f64::from(width) / 2.0,
                (b[1] - a[1]) * f64::from(height) / 2.0,
            ];
            assert!((moved[0] - 40.0).abs() < 0.5, "{moved:?}");
            assert!((moved[1] - 25.0).abs() < 0.5, "{moved:?}");
        }
    }

    #[test]
    fn pan_stays_within_the_bounds_and_zoom_within_range() {
        let orbit = Orbit {
            zoom: 0.001,
            pan: [3.0, 4.0, 0.0],
            ..Orbit::default()
        }
        .clamped()
        .unwrap();
        assert_eq!(orbit.zoom, Orbit::MIN_ZOOM);
        assert!((orbit.pan[0] - 0.6).abs() < 1e-12 && (orbit.pan[1] - 0.8).abs() < 1e-12);
    }
}
