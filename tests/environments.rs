//! Pickup in avian's environments: an actor finds, holds and throws only what is in its own
//! [`PhysicsEnvironment`], in that environment's frame, wherever the environment stands in
//! the world.

use std::f32::consts::FRAC_PI_2;

use avian_pickup::prelude::*;
use avian3d::prelude::*;
use bevy::{prelude::*, time::TimeUpdateStrategy};
use core::time::Duration;

#[derive(Resource, Default)]
struct Dropped(Vec<PropDropped>);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        PhysicsPlugins::default(),
        AvianPickupPlugin::default(),
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )))
    .init_resource::<Dropped>()
    .add_systems(
        Update,
        |mut read: MessageReader<PropDropped>, mut dropped: ResMut<Dropped>| {
            dropped.0.extend(read.read().copied());
        },
    );
    app.finish();
    app
}

/// An environment with a floor, standing at `at` in the world.
fn environment(app: &mut App, at: Transform) -> Entity {
    let env = app.world_mut().spawn((PhysicsEnvironment, at)).id();
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(20.0, 1.0, 20.0),
        Transform::from_xyz(0.0, -0.5, 0.0),
        ChildOf(env),
    ));
    env
}

/// Away from the origin and turned a quarter: its frame's -Z is the world's -X.
fn far_turned() -> Transform {
    Transform::from_xyz(100.0, 3.0, -40.0).with_rotation(Quat::from_rotation_y(FRAC_PI_2))
}

fn actor(app: &mut App, env: Entity, at: Vec3) -> Entity {
    app.world_mut()
        .spawn((
            AvianPickupActor::default(),
            Transform::from_translation(at),
            ChildOf(env),
        ))
        .id()
}

fn crate_at(app: &mut App, env: Entity, at: Vec3) -> Entity {
    app.world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(0.3, 0.3, 0.3),
            Transform::from_translation(at),
            ChildOf(env),
        ))
        .id()
}

fn step(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

/// `frames` frames with `action` held.
fn act(app: &mut App, actor: Entity, action: AvianPickupAction, frames: usize) {
    for _ in 0..frames {
        app.world_mut()
            .write_message(AvianPickupInput { actor, action });
        app.update();
    }
}

fn state(app: &App, actor: Entity) -> AvianPickupActorState {
    *app.world().get::<AvianPickupActorState>(actor).unwrap()
}

fn position(app: &App, entity: Entity) -> Vec3 {
    app.world().get::<Position>(entity).unwrap().0
}

/// The prop ahead of the actor in a far, turned environment is picked up, held ahead of the
/// actor in that environment's frame, and thrown along the actor's forward in that frame.
#[test]
fn holds_and_throws_in_the_environments_frame() {
    let mut app = app();
    let env = environment(&mut app, far_turned());
    let hand = actor(&mut app, env, Vec3::new(0.0, 1.0, 0.0));
    let prop = crate_at(&mut app, env, Vec3::new(0.0, 1.0, -1.5));
    step(&mut app, 5);

    act(&mut app, hand, AvianPickupAction::Pull, 10);
    assert_eq!(state(&app, hand), AvianPickupActorState::Holding(prop));
    step(&mut app, 90);
    // 0.6 m (the preferred distance) past the crate's own 0.15 m, straight ahead.
    let at = position(&app, prop);
    assert!(
        at.distance(Vec3::new(0.0, 1.0, -0.75)) < 0.05,
        "held ahead of the actor in its environment's frame, at {at}"
    );

    act(&mut app, hand, AvianPickupAction::Throw, 1);
    step(&mut app, 2);
    assert_eq!(state(&app, hand), AvianPickupActorState::Idle);
    let velocity = app.world().get::<LinearVelocity>(prop).unwrap().0;
    assert!(
        velocity.z < -4.0 && velocity.x.abs() < 0.5,
        "thrown along the actor's forward in the frame, at {velocity} m/s"
    );
}

/// A prop of another environment standing where the actor's world-space aim leads is never
/// found: the actor's queries are its own environment's.
#[test]
fn another_environments_prop_is_not_found() {
    let mut app = app();
    let here = environment(&mut app, Transform::IDENTITY);
    let there = environment(&mut app, Transform::IDENTITY);
    let hand = actor(&mut app, here, Vec3::new(0.0, 1.0, 0.0));
    let theirs = crate_at(&mut app, there, Vec3::new(0.0, 1.0, -1.5));
    step(&mut app, 5);

    act(&mut app, hand, AvianPickupAction::Pull, 30);
    assert_eq!(state(&app, hand), AvianPickupActorState::Idle);
    let drift = app.world().get::<LinearVelocity>(theirs).unwrap().0.xz();
    assert!(drift.length() < 0.05, "not pulled either, at {drift} m/s");
}

/// A held prop stays on the actor's side of a wall the actor walks up to.
#[test]
fn a_held_prop_is_blocked_by_a_wall() {
    let mut app = app();
    let env = environment(&mut app, far_turned());
    let hand = actor(&mut app, env, Vec3::new(0.0, 1.0, 0.0));
    let prop = crate_at(&mut app, env, Vec3::new(0.0, 1.0, -1.0));
    // Its near face at z = -1.45.
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(6.0, 6.0, 0.1),
        Transform::from_xyz(0.0, 1.0, -1.5),
        ChildOf(env),
    ));
    step(&mut app, 5);
    act(&mut app, hand, AvianPickupAction::Pull, 10);
    assert_eq!(state(&app, hand), AvianPickupActorState::Holding(prop));

    // The actor 0.45 m from the wall: the prop's preferred place is behind it.
    app.world_mut()
        .get_mut::<Transform>(hand)
        .unwrap()
        .translation = Vec3::new(0.0, 1.0, -1.0);
    step(&mut app, 90);
    assert_eq!(state(&app, hand), AvianPickupActorState::Holding(prop));
    let at = position(&app, prop);
    assert!(
        at.z > -1.45 + 0.15 - 0.03 && at.z < -1.0,
        "between the actor and the wall, at {at}"
    );
}

/// A held prop that leaves the actor's environment is let go (a forced drop).
#[test]
fn a_prop_that_leaves_the_environment_is_dropped() {
    let mut app = app();
    let here = environment(&mut app, far_turned());
    let there = environment(&mut app, Transform::IDENTITY);
    let hand = actor(&mut app, here, Vec3::new(0.0, 1.0, 0.0));
    let prop = crate_at(&mut app, here, Vec3::new(0.0, 1.0, -1.0));
    step(&mut app, 5);
    act(&mut app, hand, AvianPickupAction::Pull, 10);
    assert_eq!(state(&app, hand), AvianPickupActorState::Holding(prop));

    app.world_mut().trigger(TransferToEnvironment {
        entity: prop,
        world: there,
    });
    step(&mut app, 5);
    assert_eq!(state(&app, hand), AvianPickupActorState::Idle);
    let dropped = &app.world().resource::<Dropped>().0;
    assert_eq!(
        dropped.as_slice(),
        [PropDropped {
            prop,
            actor: hand,
            forced: true
        }]
    );
    assert!(app.world().get::<HeldProp>(prop).is_none());
}

/// `HoldProp` holds the named prop at once, wherever the actor aims; a `HoldPose` on the prop
/// is where it is held in the actor's frame; `ReleaseProp` lets go at the prop's own velocity.
#[test]
fn a_named_prop_is_held_at_its_pose_and_released() {
    let mut app = app();
    let env = environment(&mut app, far_turned());
    let hand = actor(&mut app, env, Vec3::new(0.0, 1.0, 0.0));
    // Behind the actor, out of any cone.
    let prop = crate_at(&mut app, env, Vec3::new(0.3, 0.6, 1.0));
    let pose = Transform::from_xyz(0.1, -0.2, -0.4).with_rotation(Quat::from_rotation_z(1.0));
    app.world_mut().entity_mut(prop).insert(HoldPose(pose));
    step(&mut app, 5);

    app.world_mut()
        .commands()
        .entity(hand)
        .queue(HoldProp(prop));
    step(&mut app, 90);
    assert_eq!(state(&app, hand), AvianPickupActorState::Holding(prop));
    let at = position(&app, prop);
    assert!(
        at.distance(Vec3::new(0.1, 0.8, -0.4)) < 0.03,
        "at its pose in the actor's frame, at {at}"
    );
    let turned = app.world().get::<Rotation>(prop).unwrap().0;
    assert!(turned.angle_between(pose.rotation) < 0.05);

    // The hand moves at 1.2 m/s along x; let go mid-motion: the prop leaves with that.
    for _ in 0..30 {
        app.world_mut()
            .get_mut::<Transform>(hand)
            .unwrap()
            .translation
            .x += 0.02;
        app.update();
    }
    app.world_mut().commands().entity(hand).queue(ReleaseProp);
    step(&mut app, 2);
    assert_eq!(state(&app, hand), AvianPickupActorState::Idle);
    let velocity = app.world().get::<LinearVelocity>(prop).unwrap().0;
    assert!(
        (velocity.x - 1.2).abs() < 0.15,
        "released at the hand's velocity, {velocity} m/s"
    );
    assert_eq!(
        app.world().resource::<Dropped>().0.as_slice(),
        [PropDropped {
            prop,
            actor: hand,
            forced: false
        }]
    );
}

/// A wall between the actor and a prop's `HoldPose` holds the prop on the actor's side.
#[test]
fn a_hold_pose_behind_a_wall_is_held_short() {
    let mut app = app();
    let env = environment(&mut app, far_turned());
    let hand = actor(&mut app, env, Vec3::new(0.0, 1.0, 0.0));
    let prop = crate_at(&mut app, env, Vec3::new(0.0, 1.0, -0.4));
    app.world_mut()
        .entity_mut(prop)
        .insert(HoldPose(Transform::from_xyz(0.0, 0.0, -1.0)));
    // Its near face at z = -0.65.
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(6.0, 6.0, 0.1),
        Transform::from_xyz(0.0, 1.0, -0.7),
        ChildOf(env),
    ));
    step(&mut app, 5);
    app.world_mut()
        .commands()
        .entity(hand)
        .queue(HoldProp(prop));
    step(&mut app, 120);
    assert_eq!(state(&app, hand), AvianPickupActorState::Holding(prop));
    let at = position(&app, prop);
    assert!(
        at.z > -0.65 + 0.15 - 0.03 && at.z < -0.3,
        "short of the wall, at {at}"
    );
}

/// A prop let go of weighs what it weighed before it was taken, not the pickup's 1 kg.
#[test]
fn a_released_prop_has_its_own_mass_again() {
    let mut app = app();
    let env = environment(&mut app, far_turned());
    let hand = actor(&mut app, env, Vec3::new(0.0, 1.0, 0.0));
    let prop = crate_at(&mut app, env, Vec3::new(0.0, 1.0, -1.0));
    app.world_mut()
        .entity_mut(prop)
        .insert(ColliderDensity(400.0));
    step(&mut app, 5);
    let mass = |app: &App| app.world().get::<ComputedMass>(prop).unwrap().value();
    let own = mass(&app);
    assert!(
        (own - 0.027 * 400.0).abs() < 0.01,
        "a 0.3 m cube at 400 kg/m3, {own} kg"
    );

    app.world_mut()
        .commands()
        .entity(hand)
        .queue(HoldProp(prop));
    step(&mut app, 10);
    assert!((mass(&app) - 1.0).abs() < 1e-3, "held at the pickup's mass");
    app.world_mut().commands().entity(hand).queue(ReleaseProp);
    step(&mut app, 10);
    assert!(
        (mass(&app) - own).abs() < 0.01,
        "{} kg after, {own} kg before",
        mass(&app)
    );
}

/// What the actor's obstacle filter leaves out stays left out while it holds: a body the
/// actor stands inside (its own) does not stop a prop held at a pose beyond it.
#[test]
fn the_obstacle_filters_exclusions_hold_while_holding() {
    let mut app = app();
    let env = environment(&mut app, far_turned());
    let hand = actor(&mut app, env, Vec3::new(0.0, 1.0, 0.0));
    // The actor's own body, between its origin and where the prop is held.
    let body = app
        .world_mut()
        .spawn((
            RigidBody::Kinematic,
            Collider::cylinder(0.32, 1.8),
            CollisionLayers::new(LayerMask::DEFAULT, LayerMask::NONE),
            Transform::from_xyz(0.0, 0.9, -0.2),
            ChildOf(env),
        ))
        .id();
    app.world_mut()
        .get_mut::<AvianPickupActor>(hand)
        .unwrap()
        .obstacle_filter = SpatialQueryFilter::from_excluded_entities([body]);
    let prop = crate_at(&mut app, env, Vec3::new(0.0, 1.0, -1.2));
    app.world_mut().entity_mut(prop).insert((
        CollisionLayers::new(LayerMask(2), LayerMask::ALL),
        HoldPose(Transform::from_xyz(0.0, 0.0, -0.4)),
    ));
    step(&mut app, 5);
    app.world_mut()
        .commands()
        .entity(hand)
        .queue(HoldProp(prop));
    step(&mut app, 90);
    let at = position(&app, prop);
    assert!(
        at.distance(Vec3::new(0.0, 1.0, -0.4)) < 0.03,
        "at its pose past the body, at {at}"
    );
}
