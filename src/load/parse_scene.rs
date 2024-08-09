use bevy::{
    asset::LoadContext,
    core::Name,
    log::warn,
    math::{Mat3, Mat4, Quat, Vec3},
    pbr::PbrBundle,
    prelude::{
        default, BuildWorldChildren, EntityWorldMut, SpatialBundle, Transform, Visibility, World,
        WorldChildBuilder,
    },
    scene::Scene,
    utils::HashSet,
};
use dot_vox::{Frame, SceneNode};

use crate::{VoxelLayer, VoxelModelInstance};

use super::components::LayerInfo;

pub(super) fn find_model_names(
    name_for_model: &mut Vec<Option<String>>,
    graph: &Vec<SceneNode>,
    scene_node: &SceneNode,
    parent_name: Option<&String>,
) {
    match scene_node {
        SceneNode::Transform {
            attributes,
            frames: _,
            child,
            layer_id: _,
        } => {
            let (accumulated, node_name) =
                get_accumulated_and_node_name(parent_name, attributes.get("_name"));
            match &graph[*child as usize] {
                SceneNode::Group {
                    attributes: _,
                    children,
                } => {
                    for grandchild in children {
                        find_model_names(
                            name_for_model,
                            graph,
                            &graph[*grandchild as usize],
                            accumulated.as_ref(),
                        );
                    }
                }
                SceneNode::Shape {
                    attributes: _,
                    models,
                } => {
                    let model_id = models[0].model_id as usize;
                    match (&name_for_model[model_id], node_name) {
                        (None, Some(name)) | (Some(_), Some(name)) => {
                            name_for_model[model_id] = Some(name.to_string())
                        }
                        (None, None) | (Some(_), None) => (),
                    };
                }
                _ => {}
            }
        }
        _ => {}
    }
}

pub(super) fn parse_scene_graph(
    context: &mut LoadContext,
    graph: &Vec<SceneNode>,
    scene_node: &SceneNode,
    parent_name: Option<&String>,
    model_names: &mut Vec<Option<String>>,
    subassets: &mut HashSet<String>,
    layers: &Vec<LayerInfo>,
    scene_scale: f32,
) -> Scene {
    let mut world = World::default();
    match scene_node {
        SceneNode::Transform {
            attributes,
            frames: _, // nb for the root node we ignore the transform
            child,
            layer_id,
        } => {
            let (accumulated, node_name) =
                get_accumulated_and_node_name(parent_name, attributes.get("_name"));
            let mut node = world.spawn_empty();
            load_xform_child(
                context,
                graph,
                &graph[*child as usize],
                &mut node,
                accumulated.as_ref(),
                model_names,
                subassets,
                layers,
                scene_scale,
            );

            let maybe_layer = layers.get(*layer_id as usize);
            if let Some(layer) = maybe_layer {
                node.insert(VoxelLayer {
                    id: *layer_id,
                    name: layer.name.clone(),
                });
            }
            let node_is_hidden = parse_bool(attributes.get("_hidden").cloned());
            let layer_is_hidden = maybe_layer.map_or(false, |v| v.is_hidden);
            let visibility = if node_is_hidden || layer_is_hidden {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            node.insert(visibility);
            if let Some(node_name) = node_name.clone() {
                node.insert(Name::new(node_name.clone()));
            }
        }
        _ => {}
    }
    Scene::new(world)
}

fn load_xform_node(
    context: &mut LoadContext,
    builder: &mut WorldChildBuilder,
    graph: &Vec<SceneNode>,
    scene_node: &SceneNode,
    parent_name: Option<&String>,
    model_names: &mut Vec<Option<String>>,
    subassets: &mut HashSet<String>,
    layers: &Vec<LayerInfo>,
    scene_scale: f32,
) {
    match scene_node {
        SceneNode::Transform {
            attributes,
            frames,
            child,
            layer_id,
        } => {
            let (accumulated, node_name) =
                get_accumulated_and_node_name(parent_name, attributes.get("_name"));
            let mut node = builder.spawn_empty();
            load_xform_child(
                context,
                graph,
                &graph[*child as usize],
                &mut node,
                accumulated.as_ref(),
                model_names,
                subassets,
                layers,
                scene_scale,
            );
            node.insert(Transform::from_matrix(transform_from_frame(
                &frames[0],
                scene_scale,
            )));

            let maybe_layer = layers.get(*layer_id as usize);
            if let Some(layer) = maybe_layer {
                node.insert(VoxelLayer {
                    id: *layer_id,
                    name: layer.name.clone(),
                });
            }
            let node_is_hidden = parse_bool(attributes.get("_hidden").cloned());
            let layer_is_hidden = maybe_layer.map_or(false, |v| v.is_hidden);
            let visibility = if node_is_hidden || layer_is_hidden {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            node.insert(visibility);
            if let Some(node_name) = node_name.clone() {
                node.insert(Name::new(node_name.clone()));
                // create sub-asset
                if subassets.insert(node_name.clone()) {
                    context.labeled_asset_scope(node_name, |context| {
                        parse_scene_graph(
                            context,
                            graph,
                            scene_node,
                            parent_name,
                            model_names,
                            subassets,
                            layers,
                            scene_scale,
                        )
                    });
                }
            }
        }
        SceneNode::Group { .. } | SceneNode::Shape { .. } => {
            warn!("Found Group or Shape Node without a parent Transform");
            let mut node = builder.spawn_empty();
            load_xform_child(
                context,
                graph,
                scene_node,
                &mut node,
                parent_name,
                model_names,
                subassets,
                layers,
                scene_scale,
            );
        }
    }
}

fn load_xform_child(
    context: &mut LoadContext,
    graph: &Vec<SceneNode>,
    scene_node: &SceneNode,
    node: &mut EntityWorldMut,
    parent_name: Option<&String>,
    model_names: &mut Vec<Option<String>>,
    subassets: &mut HashSet<String>,
    layers: &Vec<LayerInfo>,
    scene_scale: f32,
) {
    match scene_node {
        SceneNode::Transform { .. } => {
            warn!("Found nested Transform nodes");
            node.insert(SpatialBundle::default());
            node.with_children(|builder| {
                load_xform_node(
                    context,
                    builder,
                    graph,
                    scene_node,
                    parent_name,
                    model_names,
                    subassets,
                    layers,
                    scene_scale,
                );
            });
        }
        SceneNode::Group {
            attributes: _,
            children,
        } => {
            node.insert(SpatialBundle::default());
            node.with_children(|builder| {
                for child in children {
                    load_xform_node(
                        context,
                        builder,
                        graph,
                        &graph[*child as usize],
                        parent_name,
                        model_names,
                        subassets,
                        layers,
                        scene_scale,
                    );
                }
            });
        }
        SceneNode::Shape {
            attributes: _,
            models,
        } => {
            let model_id = models[0].model_id as usize;
            let model_name = model_names[model_id]
                .clone()
                .unwrap_or(format!("model-{}", model_id));
            node.insert((
                PbrBundle {
                    mesh: context.get_label_handle(format!("{}@mesh", model_name)),
                    material: context.get_label_handle(format!("{}@material", model_name)),
                    ..default()
                },
                VoxelModelInstance(context.get_label_handle(format!("{}@model", model_name))),
            ));
        }
    }
}

fn get_accumulated_and_node_name(
    parent_name: Option<&String>,
    node_name: Option<&String>,
) -> (Option<String>, Option<String>) {
    match (parent_name, node_name) {
        (None, None) => (None, None),
        (None, Some(node_name)) => (Some(node_name.to_string()), Some(node_name.to_string())),
        (Some(parent_name), None) => (Some(parent_name.to_string()), None), // allow group name to pass down through unnamed child
        (Some(parent_name), Some(node_name)) => {
            let accumulated = format!("{}/{}", parent_name, node_name);
            (Some(accumulated.clone()), Some(accumulated))
        }
    }
}

fn parse_bool(value: Option<String>) -> bool {
    match value.as_deref() {
        Some("1") => true,
        Some("0") => false,
        Some(_) => {
            warn!("Invalid boolean string");
            false
        }
        None => false,
    }
}

fn transform_from_frame(frame: &Frame, scene_scale: f32) -> Mat4 {
    let Some(position) = frame.position() else {
        return Mat4::IDENTITY;
    };
    let position =
        Vec3::new(-position.x as f32, position.z as f32, position.y as f32) * scene_scale;
    let translation = Mat4::from_translation(position);
    let rotation = if let Some(orientation) = frame.orientation() {
        let (rotation, scale) = &orientation.to_quat_scale();
        let scale: Vec3 = (*scale).into();
        let quat = Quat::from_array(*rotation);
        let (axis, angle) = quat.to_axis_angle();
        let mat3 = Mat3::from_axis_angle(Vec3::new(-axis.x, axis.z, axis.y), angle)
            * Mat3::from_diagonal(scale);
        Mat4::from_mat3(mat3)
    } else {
        Mat4::IDENTITY
    };
    translation * rotation
}
