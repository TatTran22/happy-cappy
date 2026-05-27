pub mod catalog;
pub mod manifest;
pub mod resolver;
pub mod runtime;

pub use catalog::{
    BundledPet, CatalogEntry, CatalogLoadError, CatalogSource, PetCatalog,
};
pub use manifest::{Animation, FrameGeometry, ManifestError, PetManifest};
pub use resolver::{lookup_with_fallback, resolve_animation_chain};
pub use runtime::{
    BehaviorIntent, BehaviorMode, Direction, Personality, PetRuntime, PetState, PetTick,
};
