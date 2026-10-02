use super::super::scene::HsdTransform;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HsdAnimationChannel {
    RotationX,
    RotationY,
    RotationZ,
    TranslationX,
    TranslationY,
    TranslationZ,
    ScaleX,
    ScaleY,
    ScaleZ,
}

impl HsdAnimationChannel {
    pub fn from_joint_object_type(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::RotationX),
            2 => Some(Self::RotationY),
            3 => Some(Self::RotationZ),
            5 => Some(Self::TranslationX),
            6 => Some(Self::TranslationY),
            7 => Some(Self::TranslationZ),
            8 => Some(Self::ScaleX),
            9 => Some(Self::ScaleY),
            10 => Some(Self::ScaleZ),
            _ => None,
        }
    }

    /// Apply the local-transform portion of `JObjUpdateFunc`.
    pub fn apply_to_transform(self, value: f32, transform: &mut HsdTransform) {
        let value =
            if matches!(self, Self::ScaleX | Self::ScaleY | Self::ScaleZ) && value.abs() < 1.0e-3 {
                1.0e-3
            } else {
                value
            };
        match self {
            Self::RotationX => transform.rotation[0] = value,
            Self::RotationY => transform.rotation[1] = value,
            Self::RotationZ => transform.rotation[2] = value,
            Self::TranslationX => transform.translation[0] = value,
            Self::TranslationY => transform.translation[1] = value,
            Self::TranslationZ => transform.translation[2] = value,
            Self::ScaleX => transform.scale[0] = value,
            Self::ScaleY => transform.scale[1] = value,
            Self::ScaleZ => transform.scale[2] = value,
        }
    }
}

/// A Figa JObj receiver: a local-transform scalar or a `JOBJ_HIDDEN` control.
///
/// `JObjUpdateFunc` thresholds NODE (`11`) and BRANCH (`12`) samples with a
/// strict `> 0.5`: NODE updates only the receiving JObj, while BRANCH updates
/// it and its owned descendants without descending below an INSTANCE JObj.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HsdJointChannel {
    Transform(HsdAnimationChannel),
    Node,
    Branch,
}

impl HsdJointChannel {
    pub fn from_joint_object_type(value: u8) -> Option<Self> {
        match value {
            11 => Some(Self::Node),
            12 => Some(Self::Branch),
            value => HsdAnimationChannel::from_joint_object_type(value).map(Self::Transform),
        }
    }

    /// Visibility controls set `JOBJ_HIDDEN` unless the sample exceeds `0.5`;
    /// a NaN sample therefore hides, matching the source comparison.
    pub fn hides(value: f32) -> bool {
        value.partial_cmp(&0.5) != Some(std::cmp::Ordering::Greater)
    }
}
