use bevy::prelude::*;
use bevy_vox_scene::VoxScenePlugin;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, VoxScenePlugin::default()))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(30.0, 30.0, 60.0).looking_at(Vec3::ZERO, Vec3::Y),
        EnvironmentMapLight {
            diffuse_map: assets.load("pisa_diffuse.ktx2"),
            specular_map: assets.load("pisa_specular.ktx2"),
            intensity: 500.0,
            ..default()
        },
    ));

    commands.spawn(
        // Load a single model using the name assigned to it in MagicaVoxel
        // If a model is nested in a named group, than the group will form part of the path
        // Path components are separated with a slash
        SceneRoot(assets.load("study.vox#workstation/desk")),
    );
}
