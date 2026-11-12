//! Module for the types that represent input events for Avian Pickup.

use bevy_platform::collections::HashSet;

use crate::{
    interaction::{HoldError, ShadowParams},
    prelude::*,
    verb::{SetVerb, Verb},
};

pub(super) mod prelude {
    pub use super::{AvianPickupAction, AvianPickupInput, HoldProp, ReleaseProp};
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<AvianPickupInput>()
        .add_systems(PostUpdate, set_verbs_according_to_input);
}

/// Message for picking up and throwing objects.
/// Send this to tell Avian Pickup to do its thing.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq)]
#[cfg_attr(
    feature = "serialize",
    derive(serde::Serialize, serde::Deserialize),
    reflect(Serialize, Deserialize)
)]
pub struct AvianPickupInput {
    /// The entity of the [`AvianPickupActor`] that the event is related to.
    pub actor: Entity,
    /// The kind of input that the event represents.
    pub action: AvianPickupAction,
}

/// The kind of input that the [`AvianPickupInput`] represents.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
#[cfg_attr(
    feature = "serialize",
    derive(serde::Serialize, serde::Deserialize),
    reflect(Serialize, Deserialize)
)]
pub enum AvianPickupAction {
    /// The left mouse button was just pressed this update.
    Throw,
    /// The right mouse button was just pressed this update.
    Drop,
    /// The right mouse button was pressed.
    Pull,
}

impl AvianPickupAction {
    pub(crate) fn iter() -> impl Iterator<Item = Self> {
        [Self::Throw, Self::Drop, Self::Pull].iter().copied()
    }
}

/// An [`EntityCommand`] on an [`AvianPickupActor`]: hold this prop now, whatever the actor aims
/// at and however far the prop is, past the pull and its cooldowns. For a game that has
/// already chosen what is taken (a focus, a touch, an NPC's plan) and only wants it held.
///
/// The prop must be a dynamic rigid body in the actor's environment that nobody holds, and
/// the actor must hold nothing; otherwise nothing happens and the log says why.
///
/// ```
/// # use avian_pickup::prelude::*;
/// # use bevy::prelude::*;
/// fn take(mut commands: Commands, hand: Single<Entity, With<AvianPickupActor>>, prop: Entity) {
///     commands.entity(*hand).queue(HoldProp(prop));
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoldProp(pub Entity);

impl EntityCommand for HoldProp {
    type Out = ();

    fn apply(self, entity_world: EntityWorldMut) {
        let actor = entity_world.id();
        entity_world
            .into_world_mut()
            .run_system_cached_with(hold_prop, (actor, self.0))
            .unwrap();
    }
}

fn hold_prop(
    In((actor, prop)): In<(Entity, Entity)>,
    mut commands: Commands,
    mut q_actor: Query<(&AvianPickupActorState, &mut Cooldown), With<AvianPickupActor>>,
    q_prop: Query<(&RigidBody, Has<HeldProp>)>,
    environments: PhysicsEnvironmentLookup,
) {
    let Ok((state, mut cooldown)) = q_actor.get_mut(actor) else {
        error!("`HoldProp` was queued on {actor}, which is no `AvianPickupActor`. Ignoring.");
        return;
    };
    if state.is_holding() {
        warn!("`HoldProp`: {actor} already holds something. Ignoring.");
        return;
    }
    match q_prop.get(prop) {
        Ok((body, false)) if body.is_dynamic() => {}
        Ok((_, true)) => {
            warn!("`HoldProp`: {prop} is already held. Ignoring.");
            return;
        }
        _ => {
            error!("`HoldProp`: {prop} is not a dynamic rigid body. Ignoring.");
            return;
        }
    }
    let environment = environments.environment_of(actor);
    if environment.is_none() || environment != environments.environment_of(prop) {
        error!("`HoldProp`: {actor} and {prop} are not in one `PhysicsEnvironment`. Ignoring.");
        return;
    }
    cooldown.hold();
    commands.entity(actor).queue(SetVerb::new(Verb::Hold(prop)));
}

/// An [`EntityCommand`] on an [`AvianPickupActor`]: let go of what it holds now, past the
/// drop's cooldown. The prop keeps the velocity it has (clamped as any drop's), so a prop
/// following a moving hand leaves with the hand's velocity. Sends [`PropDropped`] on the
/// next physics step. Nothing happens for an actor that holds nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseProp;

impl EntityCommand for ReleaseProp {
    type Out = ();

    fn apply(self, entity_world: EntityWorldMut) {
        let actor = entity_world.id();
        entity_world
            .into_world_mut()
            .run_system_cached_with(release_prop, actor)
            .unwrap();
    }
}

fn release_prop(
    In(actor): In<Entity>,
    mut commands: Commands,
    q_actor: Query<&crate::verb::Holding, With<AvianPickupActor>>,
) {
    if let Ok(holding) = q_actor.get(actor) {
        commands.entity(actor).queue(SetVerb::new(Verb::Drop {
            prop: holding.0,
            forced: false,
        }));
    }
}

fn set_verbs_according_to_input(
    mut r_input: MessageReader<AvianPickupInput>,
    mut commands: Commands,
    q_actor: Query<
        (
            Entity,
            Option<&AvianPickupActorState>,
            Option<&Cooldown>,
            Has<GlobalTransform>,
            Has<ShadowParams>,
            Has<HoldError>,
        ),
        With<AvianPickupActor>,
    >,
) {
    let mut unhandled_actors: HashSet<_> = q_actor.iter().map(|(entity, ..)| entity).collect();
    'outer: for &event in r_input.read() {
        let action = event.action;
        let actor = event.actor;
        unhandled_actors.remove(&actor);
        let Ok((_entity, state, cooldown, has_global_transform, has_shadow, has_error)) =
            q_actor.get(actor)
        else {
            error!(
                "`AvianPickupEvent` was triggered on an entity without `AvianPickupActor`. Ignoring."
            );
            continue;
        };

        // Doing these checks now so that we can report issues early.
        let checks = [
            (has_global_transform, "GlobalTransform"),
            (has_shadow, "ShadowParams"),
            (has_error, "HoldError"),
        ];
        for (has_component, component_name) in checks.iter() {
            if !has_component {
                error!(
                    "`AvianPickupEvent` was triggered on an entity without `{component_name}`. Ignoring."
                );
                continue 'outer;
            }
        }

        let Some(&state) = state else {
            error!(
                "`AvianPickupEvent` was triggered on an entity without `AvianPickupActorState`. Ignoring."
            );
            continue;
        };

        let Some(cooldown) = cooldown else {
            error!("`AvianPickupEvent` was triggered on an entity without `Cooldown`. Ignoring.");
            continue;
        };

        let verb = match action {
            AvianPickupAction::Throw
                if cooldown.is_finished(AvianPickupAction::Throw)
                    && matches!(state, AvianPickupActorState::Holding(..)) =>
            {
                let AvianPickupActorState::Holding(prop) = state else {
                    unreachable!()
                };
                Some(Verb::Throw(prop))
            }
            AvianPickupAction::Drop
                if matches!(state, AvianPickupActorState::Holding(..))
                    && cooldown.is_finished(AvianPickupAction::Drop) =>
            {
                let AvianPickupActorState::Holding(prop) = state else {
                    unreachable!()
                };
                Some(Verb::Drop {
                    prop,
                    forced: false,
                })
            }
            AvianPickupAction::Pull
                if matches!(
                    state,
                    AvianPickupActorState::Idle | AvianPickupActorState::Pulling(..)
                ) && cooldown.is_finished(AvianPickupAction::Pull) =>
            {
                Some(Verb::Pull)
            }
            _ => None,
        };
        commands.entity(actor).queue(SetVerb::new(verb));
    }
    for &actor in unhandled_actors.iter() {
        commands.entity(actor).queue(SetVerb::new(None));
    }
}
