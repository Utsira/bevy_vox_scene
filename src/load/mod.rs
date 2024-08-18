mod components;
mod parse_model;
mod parse_scene;

use anyhow::anyhow;
use bevy::{
    asset::{io::Reader, AssetLoader, AsyncReadExt, Handle, LoadContext},
    color::LinearRgba,
    log::info,
    pbr::StandardMaterial,
    scene::Scene,
    utils::HashSet,
};
use components::LayerInfo;
pub use components::{VoxelLayer, VoxelModelInstance};
use parse_scene::{find_model_names, parse_scene_graph};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    model::{MaterialProperty, VoxelModel, VoxelPalette},
    VoxelContext, VoxelData, VoxelQueryable,
};

/// An asset loader capable of loading models in `.vox` files as [`bevy::scene::Scene`]s.
///
/// It converts Magica Voxel's left-handed Z-up space to bevy's right-handed Y-up space.
/// The meshes generated by this asset loader only use standard [`bevy::render::mesh::Mesh`] attributes for easier compatibility with shaders.
/// You can load multiple models from the same `.vox` file by appending `#{name}` to the asset loading path, where `{name}` corresponds to the object's name in the Magical Voxel world editor.
/// You can load unnamed models by appending `#model{no}` to the asset loading path, where `{no}` corresponds to the model index in the file. Note that this index is subject to change if you delete models in the Magica Voxel file.
pub(super) struct VoxSceneLoader {
    pub(super) global_settings: Option<VoxLoaderSettings>,
}

/// Settings for the VoxSceneLoader.
#[derive(Serialize, Deserialize, Clone)]
pub struct VoxLoaderSettings {
    /// The length of each side of a single voxel. Defaults to 1.0.
    pub voxel_size: f32,
    /// Whether the outer-most faces of the model should be meshed. Defaults to true. Set this to false if the outer faces of a
    /// model will never be visible, for instance if the model id part of a 3D tileset.
    pub mesh_outer_faces: bool,
    /// Multiplier for emissive strength. Defaults to 2.0.
    pub emission_strength: f32,
    /// Defaults to `true` to more accurately reflect the colours in Magica Voxel.
    pub uses_srgb: bool,
    /// Magica Voxel doesn't let you adjust the roughness for the default "diffuse" block type, so it can be adjusted with this setting. Defaults to 0.8.
    pub diffuse_roughness: f32,
}

impl Default for VoxLoaderSettings {
    fn default() -> Self {
        Self {
            voxel_size: 1.0,
            mesh_outer_faces: true,
            emission_strength: 10.0,
            uses_srgb: true,
            diffuse_roughness: 0.8,
        }
    }
}

#[derive(Error, Debug)]
pub enum VoxLoaderError {
    #[error(transparent)]
    InvalidAsset(#[from] anyhow::Error),
}

impl AssetLoader for VoxSceneLoader {
    type Asset = Scene;
    type Settings = VoxLoaderSettings;
    type Error = VoxLoaderError;

    async fn load<'a>(
        &'a self,
        reader: &'a mut Reader<'_>,
        _settings: &'a VoxLoaderSettings,
        _load_context: &'a mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| VoxLoaderError::InvalidAsset(anyhow!(e)))?;
        self.process_vox_file(&bytes, _load_context, _settings)
    }

    fn extensions(&self) -> &[&str] {
        &["vox"]
    }
}

impl VoxSceneLoader {
    fn process_vox_file<'a>(
        &self,
        bytes: &'a [u8],
        mut load_context: &'a mut LoadContext,
        settings: &'a VoxLoaderSettings,
    ) -> Result<Scene, VoxLoaderError> {
        let file = match dot_vox::load_bytes(bytes) {
            Ok(data) => data,
            Err(error) => return Err(VoxLoaderError::InvalidAsset(anyhow!(error))),
        };
        info!("Loading {}", load_context.asset_path());
        let settings = self.global_settings.clone().unwrap_or(settings.clone());

        // Palette
        let palette = VoxelPalette::from_data(
            &file,
            settings.diffuse_roughness,
            settings.emission_strength,
        );
        let translucent_material = palette.create_material_in_load_context(load_context);
        let opaque_material = load_context.labeled_asset_scope("material".to_string(), |_| {
            let mut opaque_material = translucent_material.clone();
            opaque_material.specular_transmission_texture = None;
            opaque_material.specular_transmission = 0.0;
            opaque_material
        });
        if palette.emission == MaterialProperty::VariesPerElement {
            load_context.labeled_asset_scope("material-no-emission".to_string(), |_| {
                let mut non_emissive = translucent_material.clone();
                non_emissive.emissive_texture = None;
                non_emissive.emissive = LinearRgba::BLACK;
                non_emissive
            });
        }
        let indices_of_refraction = palette.indices_of_refraction.clone();

        // Scene graph
        let layers: Vec<LayerInfo> = file
            .layers
            .iter()
            .map(|layer| LayerInfo {
                name: layer.name(),
                is_hidden: layer.hidden(),
            })
            .collect();

        let model_count = file.models.len();
        let mut subassets: HashSet<String> = HashSet::new();
        let mut model_names: Vec<Option<String>> = vec![None; model_count];
        find_model_names(&mut model_names, &file.scenes, &file.scenes[0], None);
        let scene = parse_scene_graph(
            &mut load_context,
            &file.scenes,
            &file.scenes[0],
            None,
            &mut model_names,
            &mut subassets,
            &layers,
            settings.voxel_size,
        );

        // Models

        model_names
            .iter()
            .zip(file.models)
            .enumerate()
            .for_each(|(index, (maybe_name, model))| {
                let name = maybe_name.clone().unwrap_or(format!("model-{}", index));
                let data =
                    VoxelData::from_model(&model, settings.mesh_outer_faces, settings.voxel_size);
                let (visible_voxels, ior) = data.visible_voxels(&indices_of_refraction);
                let mesh = load_context.labeled_asset_scope(format!("{}@mesh", name), |_| {
                    crate::model::mesh::mesh_model(&visible_voxels, &data)
                });

                let material: Handle<StandardMaterial> = if let Some(ior) = ior {
                    load_context.labeled_asset_scope(format!("{}@material", name), |_| {
                        let mut material = translucent_material.clone();
                        material.ior = ior;
                        material.thickness = data.size().min_element() as f32;
                        material
                    })
                } else {
                    load_context.labeled_asset_scope(format!("{}@material", name), |_| {
                        let mut opaque_material = translucent_material.clone();
                        opaque_material.specular_transmission_texture = None;
                        opaque_material.specular_transmission = 0.0;
                        opaque_material
                    })
                };
                load_context.labeled_asset_scope(format!("{}@model", name), |_| VoxelModel {
                    name,
                    data,
                    mesh,
                    material,
                    has_translucency: ior.is_some(),
                });
            });

        let transmissive_material = load_context
            .add_labeled_asset("material-transmissive".to_string(), translucent_material);
        load_context.add_labeled_asset(
            "voxel-context".to_string(),
            VoxelContext {
                palette,
                opaque_material,
                transmissive_material,
            },
        );
        Ok(scene)
    }
}
