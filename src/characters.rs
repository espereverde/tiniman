use bevy::{prelude::*, transform::TransformSystems};
use bevy_rapier2d::prelude::*;
use bsn_blueprints::{Recipe, ReflectRecipe};

use crate::GameSet;
use crate::animation::{AnimationSet, CurrentClip, FrameAnimation};
use crate::combat::{Hurtbox, Stunned};
use crate::perception::{Noise, NoiseKind};
use crate::weapons::Weapon;

#[derive(Debug, PartialEq, Eq, Hash, Clone, SystemSet)]
pub enum CharacterSet {
    PrepareMovement,
    ApplyMovement,
    UpdateMode,
    SyncSprite,
}

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<MoveStyle>()
            .register_type::<Player>()
            .register_type::<Facing>()
            .register_type::<KinematicBody>()
            .configure_sets(Update, CharacterSet::PrepareMovement.in_set(GameSet::Think))
            .configure_sets(Update, CharacterSet::ApplyMovement.in_set(GameSet::Act))
            .configure_sets(
                PostUpdate,
                (CharacterSet::UpdateMode, CharacterSet::SyncSprite)
                    .chain()
                    .after(PhysicsSet::Writeback)
                    .before(TransformSystems::Propagate),
            )
            .add_systems(PreUpdate, (process_player_input, update_action_elapsed, update_shoot_elapsed))
            .add_systems(
                Update,
                (update_player_air_movement, update_player_ground_movement).in_set(CharacterSet::PrepareMovement),
            )
            .add_systems(Update, (apply_movement, try_to_shoot).chain().in_set(CharacterSet::ApplyMovement))
            .add_systems(PostUpdate, (stop_on_bump, check_ground, landing_noise).chain().in_set(CharacterSet::UpdateMode))
            .add_systems(
                PostUpdate,
                (reset_action_elapsed, sync_action_animation, sync_sprite_face_dir, sync_gun_gesture)
                    .in_set(CharacterSet::SyncSprite),
            );
    }
}

#[derive(Debug, Component)]
pub struct OnGround;

#[derive(Debug, Component)]
pub struct OnAir;

#[derive(Debug, Component)]
struct OnWall;

#[derive(Debug, Component, Reflect, Default)]
#[reflect(Component, Default)]
#[require(InputOrders, CharacterAction, ActionElapsed, ShootGesture, Facing)]
pub struct Player;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Component, Reflect, Default)]
#[reflect(Component, Default)]
pub enum Facing {
    Left,
    #[default]
    Right,
}

impl Facing {
    fn set_dir(&mut self, new_dir: Facing) {
        if *self != new_dir {
            *self = new_dir;
        }
    }

    pub fn sign(&self) -> f32 {
        match self {
            Facing::Left => -1.0,
            Facing::Right => 1.0,
        }
    }

    pub fn towards(dx: f32) -> Option<Facing> {
        if dx > 0.0 {
            Some(Facing::Right)
        } else if dx < 0.0 {
            Some(Facing::Left)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Component, Default)]
pub enum CharacterAction {
    #[default]
    Idle,
    RunStart,
    Running,
    RunEnd,
    Sliding,
    Jumping,
    Falling,
}

impl CharacterAction {
    fn clip_name(&self) -> &'static str {
        match self {
            CharacterAction::Idle => "idle",
            CharacterAction::RunStart => "run_start",
            CharacterAction::Running => "running",
            CharacterAction::RunEnd => "run_end",
            CharacterAction::Sliding => "sliding",
            CharacterAction::Jumping => "jumping",
            CharacterAction::Falling => "falling",
        }
    }
}

#[derive(Debug, Component, Default)]
pub struct ShootGesture {
    active: bool,
}

#[derive(Debug, Component, Default)]
struct ShootElapsed(f32);

#[derive(Debug, Component, Default)]
struct ActionElapsed(f32);

#[derive(Debug, Component, Reflect, Default)]
pub struct InputOrders {
    horiz_dir: f32,
    do_slide: bool,
    do_jump: bool,
    keep_jump: bool,
    do_shoot: bool,
}

#[derive(Debug, Component, Default, Clone)]
pub struct CharVelocity {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct MoveStyle {
    walk_speed: f32,
    start_delay: f32,
    brake_delay: f32,
    brake_speed: f32,
    slide_speed: f32,
    slide_duration: f32,
    jump_impulse: f32,
    jump_h_speed: f32,
    gravity_acc: f32,
    terminal_vel: f32,
}

/// A box moved by a kinematic character controller that only collides with the level.
/// Characters overlap each other: contact damage is handled by [`crate::combat`].
#[derive(Reflect, Default)]
#[reflect(Recipe, Default)]
pub struct KinematicBody {
    pub half_size: Vec2,
}

impl Recipe for KinematicBody {
    fn bundle(&self) -> impl Bundle + use<> {
        (
            RigidBody::KinematicPositionBased,
            Collider::cuboid(self.half_size.x, self.half_size.y),
            KinematicCharacterController {
                offset: CharacterLength::Absolute(0.2),
                filter_flags: QueryFilterFlags::EXCLUDE_KINEMATIC | QueryFilterFlags::EXCLUDE_SENSORS,
                ..default()
            },
            CharVelocity::default(),
            Hurtbox { half_size: self.half_size },
            OnAir,
        )
    }
}

fn process_player_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut input_query: Query<(&mut InputOrders, Has<Stunned>), With<Player>>,
) {
    let Ok((mut player_input, stunned)) = input_query.single_mut() else { return };
    if stunned {
        *player_input = InputOrders::default();
        return;
    }
    //Calculate desired input direction +1 for right, -1 for left.
    //If both keys are pressed they cancel out.
    player_input.horiz_dir = 0.0;
    if keyboard.pressed(KeyCode::ArrowRight) {
        player_input.horiz_dir += 1.0;
    }
    if keyboard.pressed(KeyCode::ArrowLeft) {
        player_input.horiz_dir -= 1.0;
    }
    //Store jump key press state
    player_input.keep_jump = keyboard.pressed(KeyCode::KeyZ);
    if keyboard.just_pressed(KeyCode::KeyZ) {
        if keyboard.pressed(KeyCode::ArrowDown) {
            player_input.do_slide = true;
            player_input.do_jump = false;
        } else {
            player_input.do_jump = true;
            player_input.do_slide = false;
        }
    } else {
        player_input.do_slide = false;
        player_input.do_jump = false;
    }
    player_input.do_shoot = keyboard.just_pressed(KeyCode::KeyX);
}

fn update_player_air_movement(
    time: Res<Time>,
    mut move_query: Query<
        (&InputOrders, &MoveStyle, &mut CharVelocity, &mut CharacterAction, &mut Facing),
        (With<OnAir>, Without<Stunned>),
    >,
) {
    for (orders, movement, mut velocity, mut c_action, mut face_dir) in move_query.iter_mut() {
        let mut next_action = c_action.clone();

        if velocity.y > 30.0 && !orders.keep_jump {
            velocity.y = 30.0
        } else {
            velocity.y -= movement.gravity_acc * time.delta_secs();
            velocity.y = velocity.y.clamp(-movement.terminal_vel, movement.terminal_vel);
        }

        if velocity.y != 0.0 {
            next_action = CharacterAction::Jumping
        }

        velocity.x = orders.horiz_dir * movement.walk_speed;
        if orders.horiz_dir == 1.0 {
            face_dir.set_dir(Facing::Right);
        } else if orders.horiz_dir == -1.0 {
            face_dir.set_dir(Facing::Left);
        }

        //Change character action if needed
        if *c_action != next_action {
            *c_action = next_action;
        }
    }
}

// System that controls the Player character from parses player input.
/// In reads keypresses and modifies the player Entity's Speed component.
fn update_player_ground_movement(
    mut player_query: Query<
        (
            Entity,
            &InputOrders,
            &mut CharacterAction,
            &ActionElapsed,
            &MoveStyle,
            &mut CharVelocity,
            &mut Facing,
            &GlobalTransform,
        ),
        (With<Player>, With<OnGround>, Without<Stunned>),
    >,
    mut noises: MessageWriter<Noise>,
) {
    let Some((player, orders, mut c_action, a_elapsed, movement, mut velocity, mut face_dir, transform)) =
        player_query.iter_mut().next()
    else {
        return;
    };

    let mut next_action = c_action.clone();

    let vel_dir = if velocity.x == 0.0 { face_dir.sign() } else { velocity.x.signum() };

    if orders.do_jump {
        *c_action = CharacterAction::Jumping;
        velocity.x = movement.jump_h_speed * orders.horiz_dir;
        velocity.y = movement.jump_impulse;
        if orders.horiz_dir == 1.0 {
            face_dir.set_dir(Facing::Right);
        } else if orders.horiz_dir == -1.0 {
            face_dir.set_dir(Facing::Left);
        }
        return;
    }

    velocity.y = -1.0;
    match *c_action {
        CharacterAction::Jumping => {
            if orders.horiz_dir == 1.0 {
                face_dir.set_dir(Facing::Right);
                velocity.x = movement.walk_speed * orders.horiz_dir;
                next_action = CharacterAction::Running;
            } else if orders.horiz_dir == -1.0 {
                face_dir.set_dir(Facing::Left);
                velocity.x = movement.walk_speed * orders.horiz_dir;
                next_action = CharacterAction::Running;
            } else {
                velocity.x = 0.0;
                next_action = CharacterAction::Idle;
            }
        }
        CharacterAction::Idle => {
            if orders.do_slide {
                velocity.x = movement.slide_speed * vel_dir;
                next_action = CharacterAction::Sliding;
            } else if orders.horiz_dir == 1.0 {
                face_dir.set_dir(Facing::Right);
                velocity.x = 60.0 * orders.horiz_dir;
                next_action = CharacterAction::RunStart;
            } else if orders.horiz_dir == -1.0 {
                face_dir.set_dir(Facing::Left);
                velocity.x = 60.0 * orders.horiz_dir;
                next_action = CharacterAction::RunStart;
            }
        }
        CharacterAction::RunStart => {
            if orders.do_slide {
                velocity.x = movement.slide_speed * vel_dir;
                next_action = CharacterAction::Sliding;
            } else if orders.horiz_dir == 0.0 {
                velocity.x = 0.0;
                next_action = CharacterAction::Idle;
            } else if a_elapsed.0 <= movement.start_delay {
                velocity.x = 0.0;
            } else {
                velocity.x = movement.walk_speed * orders.horiz_dir;
                if orders.horiz_dir == 1.0 {
                    face_dir.set_dir(Facing::Right);
                } else {
                    face_dir.set_dir(Facing::Left);
                }
                next_action = CharacterAction::Running
            }
        }
        CharacterAction::Running => {
            if orders.do_slide {
                velocity.x = movement.slide_speed * vel_dir;
                next_action = CharacterAction::Sliding;
            } else if orders.horiz_dir == vel_dir {
                velocity.x = movement.walk_speed * orders.horiz_dir;
            } else {
                velocity.x = 0.0;
                next_action = CharacterAction::RunEnd;
            }
        }
        CharacterAction::RunEnd => {
            if orders.do_slide {
                velocity.x = movement.slide_speed * vel_dir;
                next_action = CharacterAction::Sliding;
            } else if orders.horiz_dir == 0.0 {
                velocity.x = 0.0;
                if a_elapsed.0 > movement.brake_delay {
                    next_action = CharacterAction::Idle;
                }
            } else if orders.horiz_dir == vel_dir {
                velocity.x = 60.0 * orders.horiz_dir;
                next_action = CharacterAction::RunStart;
            } else if a_elapsed.0 <= movement.brake_delay {
                velocity.x = 0.0;
            } else {
                velocity.x = movement.walk_speed * orders.horiz_dir;
                if orders.horiz_dir == 1.0 {
                    face_dir.set_dir(Facing::Right);
                } else {
                    face_dir.set_dir(Facing::Left);
                }
                next_action = CharacterAction::Running;
            }
        }
        CharacterAction::Sliding => {
            if a_elapsed.0 > movement.slide_duration {
                if orders.horiz_dir == 1.0 {
                    face_dir.set_dir(Facing::Right);
                    velocity.x = movement.walk_speed * orders.horiz_dir;
                    next_action = CharacterAction::Running;
                } else if orders.horiz_dir == -1.0 {
                    face_dir.set_dir(Facing::Left);
                    velocity.x = movement.walk_speed * orders.horiz_dir;
                    next_action = CharacterAction::Running;
                } else {
                    velocity.x = 0.0;
                    next_action = CharacterAction::Idle;
                }
            }
        }
        _ => {}
    }

    // Sliding scrapes the floor: a small noise.
    if next_action == CharacterAction::Sliding && *c_action != CharacterAction::Sliding {
        noises.write(Noise {
            position: transform.translation().truncate(),
            radius: 20.0,
            kind: NoiseKind::Player,
            source: player,
        });
    }

    //Change character action if needed
    if *c_action != next_action {
        *c_action = next_action;
    }
}

fn try_to_shoot(
    mut commands: Commands,
    mut query: Query<(Entity, &InputOrders, &GlobalTransform, &Facing, &mut Weapon)>,
    mut noises: MessageWriter<Noise>,
) {
    for (e_id, orders, transform, facing, mut weapon) in query.iter_mut() {
        if orders.do_shoot {
            let origin = transform.translation();
            let direction = Vec2::new(facing.sign(), 0.0);
            if weapon.try_shoot(&mut commands, e_id, origin, direction) {
                commands.entity(e_id).insert((ShootGesture { active: true }, ShootElapsed(0.0)));
                noises.write(Noise {
                    position: origin.truncate(),
                    radius: weapon.noise,
                    kind: NoiseKind::Player,
                    source: e_id,
                });
            }
        }
    }
}

pub fn apply_movement(time: Res<Time>, mut controller_query: Query<(&CharVelocity, &mut KinematicCharacterController)>) {
    for (velocity, mut controller) in controller_query.iter_mut() {
        controller.translation = Some(Vec2::new(velocity.x, velocity.y) * time.delta_secs());
    }
}

fn stop_on_bump(mut move_query: Query<(&mut CharVelocity, &KinematicCharacterControllerOutput)>) {
    for (mut velocity, kin_output) in move_query.iter_mut() {
        //Only check collisions while going up. There was a glitch if we use .abs() like we did in the x axis.
        //The character sometimes got stuck in a cliff upper corner
        if (kin_output.desired_translation.y - kin_output.effective_translation.y) > 0.1 && velocity.y > 0.0 {
            velocity.y = -4.0;
        }
    }
}

/// Fall speed of the last frame in the air, to tell hard landings from small hops.
#[derive(Component, Default)]
struct FallSpeed(f32);

fn check_ground(
    mut command: Commands,
    mut ground_query: Query<(
        Entity,
        &mut CharVelocity,
        &KinematicCharacterControllerOutput,
        Has<OnGround>,
        Has<OnAir>,
    )>,
) {
    for (char_id, mut velocity, output, on_ground, on_air) in ground_query.iter_mut() {
        if on_ground && (!output.grounded || velocity.y > 0.0) {
            command.entity(char_id).insert(OnAir).remove::<OnGround>();
        }
        if on_air && output.grounded && velocity.y < 0.0 {
            command.entity(char_id).insert((OnGround, FallSpeed(-velocity.y))).remove::<OnAir>();
            velocity.y = -1.0;
        }
    }
}

/// Landing from a big fall makes noise; the player can drop from a ledge right behind an enemy
/// and give themselves away. A jump on flat ground lands at about the jump impulse, under this.
const HARD_LANDING_SPEED: f32 = 170.0;

/// Landing from a big fall makes noise; the player can drop from a ledge right behind an enemy
/// and give themselves away.
fn landing_noise(landed: Query<(Entity, &GlobalTransform, &FallSpeed), (With<Player>, Added<FallSpeed>)>, mut noises: MessageWriter<Noise>) {
    for (player, transform, fall) in &landed {
        if fall.0 > HARD_LANDING_SPEED {
            noises.write(Noise {
                position: transform.translation().truncate(),
                radius: 32.0,
                kind: NoiseKind::Player,
                source: player,
            });
        }
    }
}

fn update_action_elapsed(time: Res<Time>, mut el_query: Query<&mut ActionElapsed>) {
    for mut elapsed in el_query.iter_mut() {
        elapsed.0 += time.delta_secs();
    }
}

fn update_shoot_elapsed(time: Res<Time>, mut el_query: Query<(&mut ShootGesture, &mut ShootElapsed)>) {
    for (mut gesture, mut elapsed) in el_query.iter_mut() {
        if gesture.active {
            elapsed.0 += time.delta_secs();
            if elapsed.0 > 0.25 {
                elapsed.0 = 0.0;
                gesture.active = false;
            }
        }
    }
}

fn reset_action_elapsed(mut query: Query<&mut ActionElapsed, Changed<CharacterAction>>) {
    for mut elapsed in &mut query {
        elapsed.0 = 0.0;
    }
}

fn sync_action_animation(
    mut anim_query: Query<
        (&CharacterAction, &mut CurrentClip, &ShootGesture),
        (With<AnimationSet>, Changed<CharacterAction>),
    >,
) {
    for (action, mut clip, gesture) in anim_query.iter_mut() {
        clip.set_if_neq(CurrentClip {
            name: action.clip_name(),
            alternate: gesture.active.then_some("gun"),
        });
    }
}

fn sync_gun_gesture(mut anim_query: Query<(&mut FrameAnimation, &ShootGesture), Changed<ShootGesture>>) {
    for (mut animation, gesture) in anim_query.iter_mut() {
        if gesture.active {
            animation.set_alternate("gun");
        } else {
            animation.default_alter();
        }
    }
}

pub fn sync_sprite_face_dir(mut query: Query<(&Facing, &mut Sprite), Or<(Changed<Facing>, Added<Sprite>)>>) {
    for (face_dir, mut sprite) in query.iter_mut() {
        sprite.flip_x = *face_dir == Facing::Left;
    }
}

