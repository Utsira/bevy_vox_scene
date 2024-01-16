use bevy::utils::HashMap;
use block_mesh::{MergeVoxel, Voxel as BlockyVoxel};
use dot_vox::Model;
use ndshape::RuntimeShape;
use ndshape::Shape;

// trait implementation rules requires the use of a newtype to allow meshing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Voxel {
    pub index: u8,
    pub is_translucent: bool,
}

pub const EMPTY_VOXEL: Voxel = Voxel {
    index: 255,
    is_translucent: false,
};

impl BlockyVoxel for Voxel {
    fn get_visibility(&self) -> block_mesh::VoxelVisibility {
        match (self.index, self.is_translucent) {
            (255, _) => block_mesh::VoxelVisibility::Empty,
            (_, true) => block_mesh::VoxelVisibility::Translucent,
            (_, false) => block_mesh::VoxelVisibility::Opaque,
        }
    }
}

impl MergeVoxel for Voxel {
    type MergeValue = Voxel;

    fn merge_value(&self) -> Self::MergeValue {
        *self
    }
}

pub struct VoxelData {
    pub shape: RuntimeShape<u32, 3>,
    pub voxels: Vec<Voxel>,
}

pub(crate) fn load_from_model(
    model: &Model,
    translucent_voxels: &HashMap<u8, f32>,
) -> (VoxelData, Vec<f32>) {
    let shape = RuntimeShape::<u32, 3>::new([model.size.x + 2, model.size.z + 2, model.size.y + 2]);
    let mut voxels = vec![EMPTY_VOXEL; shape.size() as usize];
    let mut refraction_indices: Vec<f32> = Vec::new();

    model.voxels.iter().for_each(|voxel| {
        let index = shape.linearize([
            model.size.x - voxel.x as u32,
            voxel.z as u32 + 1,
            voxel.y as u32 + 1,
        ]) as usize;
        let ior = translucent_voxels.get(&voxel.i);
        let is_translucent = ior.is_some();
        if let Some(ior) = ior {
            refraction_indices.push(*ior);
        }
        voxels[index] = Voxel {
            index: voxel.i,
            is_translucent,
        };
    });

    (VoxelData { shape, voxels }, refraction_indices)
}