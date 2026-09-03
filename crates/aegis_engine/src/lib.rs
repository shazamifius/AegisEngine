//! aegis_engine — moteur 3D Vulkan ecrit a la main.
//!
//! ⚠ **Il n'y a AUCUN support VR aujourd'hui**, malgre ce que le nom du projet a longtemps
//! annonce. Ce qui existait — `vr/openxr_context.rs`, des parametres d'ecart inter-oculaire et de
//! fovea fixe — n'etait appele par rien et dort desormais sous `_openxr_context.rs`. Le dire
//! importe : la machine de reference du projet est un Meta Quest 2, et croire qu'une base VR
//! existe deja fausserait toute estimation.

pub mod chrono_gpu;
pub mod core;
pub mod geometry;
pub mod image;
pub mod mesure;
pub mod render;
pub mod scene;
pub mod shaders;
pub mod ui;

pub use core::math;
pub use core::bytes;
#[cfg(feature = "fenetre")]
pub use core::engine::Engine;
pub use core::gpu_context::GpuContext;
