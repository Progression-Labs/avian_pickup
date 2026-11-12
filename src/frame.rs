//! Poses in a [`PhysicsEnvironment`]'s frame. Every [`Position`], [`Rotation`] and velocity is
//! relative to the environment its body is in, and so is every spatial query: an actor's
//! [`GlobalTransform`] is brought into its environment's frame before it meets any of them.

use bevy_ecs::system::SystemParam;

use crate::prelude::*;

/// Resolves an actor's [`PhysicsEnvironment`] and its pose there.
#[derive(SystemParam)]
pub(crate) struct ActorFrames<'w, 's> {
    environments: PhysicsEnvironmentLookup<'w, 's>,
    transforms: Query<'w, 's, &'static GlobalTransform>,
}

impl ActorFrames<'_, '_> {
    /// The environment `actor` is in and `actor`'s pose in that environment's frame.
    /// `None` (said once) for an actor in no environment: it picks up nothing.
    pub(crate) fn get(&self, actor: Entity) -> Option<(Entity, Transform)> {
        let Some(environment) = self.environments.environment_of(actor) else {
            bevy_log::error_once!(
                "`AvianPickupActor` {actor} is in no `PhysicsEnvironment`: spawn it under one; it picks up nothing."
            );
            return None;
        };
        let (Ok(frame), Ok(pose)) = (self.transforms.get(environment), self.transforms.get(actor))
        else {
            error!("Actor entity was deleted or in an invalid state. Ignoring.");
            return None;
        };
        let local = frame.affine().inverse() * pose.affine();
        Some((environment, Transform::from_matrix(local.into())))
    }

    /// The environment `entity` is in.
    pub(crate) fn environment_of(&self, entity: Entity) -> Option<Entity> {
        self.environments.environment_of(entity)
    }
}
