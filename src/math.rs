use crate::prelude::*;
use avian3d::prelude::*;

pub(crate) const METERS_PER_INCH: f32 = 0.0254;
pub(crate) fn rigid_body_compound_collider(
    rigid_body_transform: &Transform,
    collider_entities: impl IntoIterator<Item = Entity>,
    q_collider: &Query<(&Position, &Rotation, &Collider, Option<&CollisionLayers>)>,
    filter: &SpatialQueryFilter,
) -> Option<Collider> {
    let mut colliders = Vec::new();
    for entity in collider_entities.into_iter() {
        let (position, rotation, collider, layers) = q_collider.get(entity).ok()?;
        let layers = layers.copied().unwrap_or_default();
        if filter.test(entity, layers) {
            // In the rigid body's own frame: the compound is cast under the body's rotation.
            let to_body = rigid_body_transform.rotation.inverse();
            let relative_translation = to_body * (position.0 - rigid_body_transform.translation);
            let relative_rotation = to_body * rotation.0;
            if let Some(compound) = collider.shape_scaled().as_compound() {
                // Need to unpack compound shapes because we are later returning a big compound collider for the whole rigid body
                // and parry crashes on nested compound shapes
                for (isometry, shape) in compound.shapes() {
                    let translation = isometry.translation;
                    let rotation = isometry.rotation;
                    colliders.push((
                        relative_translation + relative_rotation * translation,
                        relative_rotation * rotation,
                        shape.clone().into(),
                    ));
                }
            } else {
                colliders.push((relative_translation, relative_rotation, collider.clone()));
            }
        }
    }

    (!colliders.is_empty()).then(|| Collider::compound(colliders))
}
