//! M2 (MD20) model types and the build-12340 (version 264) reader in [`wrath`], with its external
//! `.skin` and `.anim` files. Particle emitters are read by benilla-formats from the raw bytes.

mod error;
mod model;
mod skin;
mod track;
pub mod wrath;

pub use error::Error;
pub use model::{
    M2ArrayString, M2Attachment, M2BlendMode, M2Bone, M2BoneFlags, M2Camera, M2EventMarker,
    M2Format, M2Header, M2Material, M2Model, M2PlayableAnim, M2RawData, M2RenderFlags, M2Texture,
    M2TextureTransform, M2TextureType, M2Vertex, C2, C3,
};
pub use skin::{SkinBatch, SkinSection};
pub use track::{
    CubicValue, M2QuatTrack, M2ScalarSplineTrack, M2ScalarTrack, M2SplineKey, M2Track,
    M2Vec3SplineTrack, M2Vec3Track,
};

#[cfg(test)]
mod wrath_tests;
