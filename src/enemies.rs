//! Enemy brains: what each kind of enemy does in each [`AwarenessState`].
//!
//! | Brain    | Unaware                        | Suspicious        | Alert                         | Searching                  |
//! |----------|--------------------------------|-------------------|-------------------------------|----------------------------|
//! | Walker   | patrols / guards, looks around | stops, faces noise| runs at the player, shouts    | walks to last seen, looks  |
//! | Sentry   | hides under its helmet         | peeks             | opens, fires, closes, repeats | peeks                      |
//! | Flyer    | sleeps on its perch            | wakes up, hovers  | flies at the player           | flies to last seen         |

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_rapier2d::prelude::*;

use crate::GameSet;
use crate::animation::CurrentClip;
use crate::characters::{CharVelocity, Facing, OnGround};
use crate::combat::{Health, Hurtbox, Shielded};
use crate::perception::{Awareness, AwarenessState, Noise, NoiseKind};
use crate::weapons::Weapon;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Enemy>()
            .register_type::<Walker>()
            .register_type::<Sentry>()
            .register_type::<Flyer>()
            .add_systems(Update, (walker_brain, sentry_brain, flyer_brain).in_set(GameSet::Think))
            .add_systems(PostUpdate, remember_home.after(TransformSystems::Propagate))
            .add_observer(enemy_down_noise);
    }
}

#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component, Default)]
#[require(BrainTimer)]
pub struct Enemy {
    /// Radius in which other enemies hear this one go down.
    pub death_noise: f32,
}

/// Where the enemy was placed; it returns there when it calms down.
#[derive(Component)]
pub struct Home(pub Vec2);

/// General purpose timer for brains (looking around, firing cycles...).
#[derive(Component, Default)]
pub struct BrainTimer(f32);

fn remember_home(mut commands: Commands, enemies: Query<(Entity, &GlobalTransform), (With<Enemy>, Without<Home>)>) {
    for (entity, transform) in &enemies {
        commands.entity(entity).insert(Home(transform.translation().truncate()));
    }
}

fn enemy_down_noise(
    remove: On<Remove, Enemy>,
    enemies: Query<(&Enemy, &Health, &GlobalTransform)>,
    mut noises: MessageWriter<Noise>,
) {
    if let Ok((enemy, health, transform)) = enemies.get(remove.entity)
        && health.hp <= 0
    {
        noises.write(Noise {
            position: transform.translation().truncate(),
            radius: enemy.death_noise,
            kind: NoiseKind::AllyDown,
            source: remove.entity,
        });
    }
}

const TERMINAL_VELOCITY: f32 = 210.0;

fn fall(velocity: &mut CharVelocity, on_ground: bool, gravity: f32, dt: f32) {
    velocity.y = if on_ground { -1.0 } else { (velocity.y - gravity * dt).max(-TERMINAL_VELOCITY) };
}

fn face(facing: &mut Facing, dx: f32) {
    if let Some(towards) = Facing::towards(dx) {
        if *facing != towards {
            *facing = towards;
        }
    }
}

/// Patrols around its home (or stands guard if `patrol_speed` is 0) and chases the player.
#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component, Default)]
pub struct Walker {
    pub patrol_speed: f32,
    pub chase_speed: f32,
    /// How far from home it patrols.
    pub patrol_range: f32,
    pub gravity: f32,
    /// Seconds between turning around while standing.
    pub look_period: f32,
}

#[allow(clippy::type_complexity)]
fn walker_brain(
    time: Res<Time>,
    rapier: ReadRapierContext,
    mut walkers: Query<(
        &Walker,
        &Awareness,
        &Home,
        &Hurtbox,
        &GlobalTransform,
        &mut CharVelocity,
        &mut Facing,
        &mut CurrentClip,
        &mut BrainTimer,
        Has<OnGround>,
        Option<&KinematicCharacterControllerOutput>,
    )>,
) -> Result {
    let rapier = rapier.single()?;
    let dt = time.delta_secs();
    for (walker, awareness, home, hurtbox, transform, mut velocity, mut facing, mut clip, mut timer, on_ground, output) in
        &mut walkers
    {
        let position = transform.translation().truncate();
        fall(&mut velocity, on_ground, walker.gravity, dt);

        // Can't go further that way: a wall last frame, or no floor ahead.
        let blocked = |dir: f32| {
            output.is_some_and(|output| {
                output.desired_translation.x.signum() == dir
                    && (output.desired_translation.x - output.effective_translation.x).abs() > 0.05
            })
        };
        let ledge = |dir: f32| {
            let foot = position + Vec2::new(dir * (hurtbox.half_size.x + 1.0), -hurtbox.half_size.y + 1.0);
            on_ground && rapier.cast_ray(foot, Vec2::NEG_Y, 4.0, true, QueryFilter::only_fixed()).is_none()
        };
        let mut look_around = |facing: &mut Facing, period: f32| {
            timer.0 += dt;
            if timer.0 > period {
                timer.0 = 0.0;
                *facing = if *facing == Facing::Left { Facing::Right } else { Facing::Left };
            }
        };

        let (speed, animation) = match awareness.state {
            AwarenessState::Unaware => {
                let from_home = position.x - home.0.x;
                if walker.patrol_speed > 0.0 {
                    let dir = facing.sign();
                    if blocked(dir) || ledge(dir) || from_home * dir > walker.patrol_range {
                        *facing = if dir > 0.0 { Facing::Left } else { Facing::Right };
                    }
                    (walker.patrol_speed * facing.sign(), "walk")
                } else if from_home.abs() > 2.0 && !ledge(-from_home.signum()) {
                    // Back to its post.
                    face(&mut facing, -from_home);
                    (14.0 * facing.sign(), "walk")
                } else {
                    look_around(&mut facing, walker.look_period);
                    (0.0, "idle")
                }
            }
            // Stops to stare where the noise came from (perception already turned it).
            AwarenessState::Suspicious => (0.0, "idle"),
            AwarenessState::Alert => {
                let dx = awareness.last_known.x - position.x;
                face(&mut facing, dx);
                let dir = facing.sign();
                if dx.abs() < 2.0 || ledge(dir) || blocked(dir) {
                    (0.0, "alert")
                } else {
                    (walker.chase_speed * dir, "alert")
                }
            }
            AwarenessState::Searching => {
                let dx = awareness.last_known.x - position.x;
                let dir = dx.signum();
                if dx.abs() > 3.0 && !ledge(dir) && !blocked(dir) {
                    face(&mut facing, dx);
                    (walker.patrol_speed.max(14.0) * dir, "walk")
                } else {
                    look_around(&mut facing, 0.8);
                    (0.0, "idle")
                }
            }
        };
        velocity.x = speed;
        clip.set_if_neq(CurrentClip::new(animation));
    }
    Ok(())
}

/// Hides under its helmet until alerted; then opens, fires a spread and closes, in a loop.
#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component, Default)]
pub struct Sentry {
    /// Seconds open (vulnerable) per cycle.
    pub open_time: f32,
    /// Seconds closed between openings.
    pub closed_time: f32,
    /// Bullets per volley.
    pub burst: u32,
    /// Degrees between bullets of a volley.
    pub spread: f32,
}

#[derive(Component, Default)]
struct SentryCycle {
    open: bool,
    fired: bool,
}

#[allow(clippy::type_complexity)]
fn sentry_brain(
    mut commands: Commands,
    time: Res<Time>,
    mut sentries: Query<(
        Entity,
        &Sentry,
        &Awareness,
        &GlobalTransform,
        &mut CharVelocity,
        &mut Facing,
        &mut CurrentClip,
        &mut BrainTimer,
        &mut Weapon,
        Option<&mut SentryCycle>,
        Has<OnGround>,
        Has<Shielded>,
    )>,
) {
    let dt = time.delta_secs();
    for (entity, sentry, awareness, transform, mut velocity, mut facing, mut clip, mut timer, mut weapon, cycle, on_ground, shielded) in
        &mut sentries
    {
        let Some(mut cycle) = cycle else {
            commands.entity(entity).insert(SentryCycle::default());
            continue;
        };
        let position = transform.translation();
        velocity.x = 0.0;
        fall(&mut velocity, on_ground, 450.0, dt);

        let animation = match awareness.state {
            AwarenessState::Unaware => {
                *cycle = SentryCycle::default();
                timer.0 = 0.0;
                "idle"
            }
            AwarenessState::Suspicious | AwarenessState::Searching => {
                *cycle = SentryCycle::default();
                timer.0 = 0.0;
                face(&mut facing, awareness.last_known.x - position.x);
                "peek"
            }
            AwarenessState::Alert => {
                timer.0 += dt;
                face(&mut facing, awareness.last_known.x - position.x);
                if cycle.open {
                    if !cycle.fired && timer.0 > 0.25 {
                        cycle.fired = true;
                        let aim = (awareness.last_known - position.truncate()).normalize_or(Vec2::X * facing.sign());
                        let burst = sentry.burst.max(1);
                        for i in 0..burst {
                            let offset = (i as f32 - (burst - 1) as f32 / 2.0) * sentry.spread.to_radians();
                            let direction = Vec2::from_angle(offset).rotate(aim);
                            weapon.try_shoot(&mut commands, entity, position, direction);
                        }
                    }
                    if timer.0 > sentry.open_time {
                        *cycle = SentryCycle::default();
                        timer.0 = 0.0;
                    }
                } else if timer.0 > sentry.closed_time {
                    cycle.open = true;
                    timer.0 = 0.0;
                }
                match (cycle.open, cycle.fired && timer.0 < 0.5) {
                    (true, true) => "shoot",
                    (true, false) => "open",
                    (false, _) => "idle",
                }
            }
        };
        clip.set_if_neq(CurrentClip::new(animation));

        let should_shield = !matches!(animation, "open" | "shoot");
        if should_shield && !shielded {
            commands.entity(entity).insert(Shielded);
        } else if !should_shield && shielded {
            commands.entity(entity).remove::<Shielded>();
        }
    }
}

/// Sleeps on its perch; flies straight at the player once awake.
#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component, Default)]
pub struct Flyer {
    pub speed: f32,
    /// Speed when flying back to its perch.
    pub return_speed: f32,
}

#[allow(clippy::type_complexity)]
fn flyer_brain(
    time: Res<Time>,
    mut flyers: Query<(
        &Flyer,
        &Awareness,
        &Home,
        &GlobalTransform,
        &mut CharVelocity,
        &mut Facing,
        &mut CurrentClip,
        &mut BrainTimer,
    )>,
) {
    for (flyer, awareness, home, transform, mut velocity, mut facing, mut clip, mut timer) in &mut flyers {
        timer.0 += time.delta_secs();
        let position = transform.translation().truncate();
        let hover = Vec2::new(0.0, (timer.0 * 6.0).sin() * 8.0);

        let (new_velocity, animation) = match awareness.state {
            AwarenessState::Unaware => {
                let to_home = home.0 - position;
                if to_home.length() < 1.5 {
                    (Vec2::ZERO, "idle")
                } else {
                    (to_home.normalize() * flyer.return_speed, "fly")
                }
            }
            AwarenessState::Suspicious => (hover, "fly"),
            AwarenessState::Alert => ((awareness.last_known - position).normalize_or_zero() * flyer.speed, "alert"),
            AwarenessState::Searching => {
                let to_target = awareness.last_known - position;
                if to_target.length() > 3.0 {
                    (to_target.normalize() * flyer.speed * 0.6, "fly")
                } else {
                    (hover, "fly")
                }
            }
        };
        velocity.x = new_velocity.x;
        velocity.y = new_velocity.y;
        if new_velocity.x.abs() > 1.0 {
            face(&mut facing, new_velocity.x);
        }
        clip.set_if_neq(CurrentClip::new(animation));
    }
}
