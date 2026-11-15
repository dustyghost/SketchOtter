//! Project camera bookmarks, independent of sculpt geometry and history.
use serde::{Deserialize, Serialize};

pub const MAX_CAMERAS: usize = 128;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedCamera {
    pub name: String,
    pub view: View,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub zoom: f32,
    /// Screen offset divided by the viewport's shorter dimension. Orbit's
    /// model-space pivot remains the shared origin, as in ordinary navigation.
    pub pan: [f32; 2],
    pub perspective: bool,
    pub field_of_view: f32,
    pub perspective_radius: f32,
}

impl View {
    /// Ignore conversion round-off, while still detecting deliberate navigation.
    pub(crate) fn matches(&self, other: &Self) -> bool {
        fn close(a: f32, b: f32, floor: f32) -> bool {
            (a - b).abs() <= 8. * f32::EPSILON * a.abs().max(b.abs()).max(floor)
        }
        fn angle(a: f32, b: f32) -> bool {
            let delta = (a - b).rem_euclid(std::f32::consts::TAU);
            delta.min(std::f32::consts::TAU - delta) <= 1e-5
        }
        self.perspective == other.perspective
            && angle(self.yaw, other.yaw)
            && angle(self.pitch, other.pitch)
            && angle(self.roll, other.roll)
            && close(self.zoom, other.zoom, 1e-6)
            && self
                .pan
                .iter()
                .zip(other.pan)
                .all(|(&a, b)| close(a, b, 1.))
            && close(self.field_of_view, other.field_of_view, 1.)
            && close(self.perspective_radius, other.perspective_radius, 1.)
    }
    pub fn validate(&self) -> Result<(), String> {
        if ![self.yaw, self.pitch, self.roll, self.pan[0], self.pan[1]]
            .iter()
            .all(|v| v.is_finite())
            || !(1e-6..=8.).contains(&self.zoom)
            || !(20. ..=75.).contains(&self.field_of_view)
            || !self.perspective_radius.is_finite()
            || self.perspective_radius < 1.
            || !(self.perspective_radius / (0.7 * (self.field_of_view.to_radians() * 0.5).tan()))
                .is_finite()
        {
            return Err("Invalid saved camera view".into());
        }
        Ok(())
    }
}

pub fn validate(cameras: &[SavedCamera]) -> Result<(), String> {
    if cameras.len() > MAX_CAMERAS {
        return Err(format!(
            "A project can hold up to {MAX_CAMERAS} saved cameras"
        ));
    }
    for (index, camera) in cameras.iter().enumerate() {
        validate_name(&camera.name, &cameras[..index], None)?;
        camera.view.validate()?;
    }
    Ok(())
}

pub(crate) fn validate_name(
    name: &str,
    cameras: &[SavedCamera],
    except: Option<usize>,
) -> Result<(), String> {
    if name.trim().is_empty() || name.chars().count() > 128 || name.chars().any(char::is_control) {
        return Err("Use a camera name of 1–128 characters without line breaks".into());
    }
    if cameras
        .iter()
        .enumerate()
        .any(|(i, camera)| Some(i) != except && camera.name == name)
    {
        return Err("A camera with that name already exists; choose another name".into());
    }
    Ok(())
}
