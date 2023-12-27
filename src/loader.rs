use anyhow::{anyhow, Context};
use bevy::{
    asset::{io::Reader, AssetLoader, AsyncReadExt, LoadContext},
    render::mesh::Mesh,
    utils::BoxedFuture,
};
use block_mesh::QuadCoordinateConfig;
use thiserror::Error;

/// An asset loader capable of loading models in `.vox` files as usable [`bevy::render::mesh::Mesh`]es.
///
/// The meshes generated by this asset loader only use standard [`bevy::render::mesh::Mesh`] attributes for easier compatibility with shaders.
/// You can load multiple models from the same `.vox` file by appending `#model{no}` to the asset loading path, where `{no}` corresponds to the model index in the file.
pub struct VoxLoader {
    /// Whether to flip the UVs vertically when meshing the models.
    /// You may want to change this to false if you aren't using Vulkan as a graphical backend for bevy , else this should default to true.
    pub(crate) config: QuadCoordinateConfig,
    pub(crate) v_flip_face: bool,
}

#[derive(Error, Debug)]
pub enum VoxLoaderError {
    #[error(transparent)]
    InvalidAsset(#[from] anyhow::Error),
}

impl AssetLoader for VoxLoader {
    type Asset = Mesh;
    type Settings = ();
    type Error = VoxLoaderError;

    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        _settings: &'a Self::Settings,
        load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<Self::Asset, VoxLoaderError>> {
        Box::pin(async move {
            let mut bytes = Vec::new();
            reader
                .read_to_end(&mut bytes)
                .await
                .map_err(|e| VoxLoaderError::InvalidAsset(anyhow!(e)))?;
            Ok(self.process_vox_file(&bytes, load_context)?)
        })
    }

    fn extensions(&self) -> &[&str] {
        &["vox"]
    }
}

impl VoxLoader {
    fn process_vox_file<'a>(
        &self,
        bytes: &'a [u8],
        load_context: &'a mut LoadContext,
    ) -> Result<Mesh, VoxLoaderError> {
        let file = match dot_vox::load_bytes(bytes) {
            Ok(data) => data,
            Err(error) => return Err(VoxLoaderError::InvalidAsset(anyhow!(error))),
        };

        let palette: Vec<[f32; 4]> = file
            .palette
            .iter()
            .map(|color| {
                let rgba: [u8; 4] = color.into();
                rgba.map(|byte| byte as f32 / u8::MAX as f32)
            })
            .collect();

        let mut default_mesh: Option<Mesh> = None;
        for (index, model) in file.models.iter().enumerate() {
            let (shape, buffer) = crate::voxel::load_from_model(model);
            let mesh =
                crate::mesh::mesh_model(shape, &buffer, &palette, &self.config, self.v_flip_face);

            match index {
                0 => {
                    default_mesh = Some(mesh);
                }
                _ => {
                    load_context.add_labeled_asset(format!("model{}", index), mesh);
                }
            }
        }

        Ok(default_mesh.context("No models found in vox file")?)
    }
}
