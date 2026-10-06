//! How enemies notice the player.
//!
//! Every enemy starts **unaware**: a stage fits in one screen, so if enemies simply chased the
//! player they would all converge at once. Instead each one has a [`Perception`] (sight cone,
//! hearing, touch) and an [`Awareness`] state machine:
//!
//! ```text
//!             sees player (meter fills)           meter full / touched / shot / alarm
//!  Unaware ───────────────────────────▶ Suspicious ─────────────────────────────▶ Alert
//!     ▲  ◀─────────────────────────────    │  ▲                                     │
//!     │       meter drains to zero         │  └── hears a noise                     │ lost sight
//!     │                                    │                                        │ for `memory`
//!     └─────────────── search time over ── Searching ◀──────────────────────────────┘
//! ```
//!
//! - **Sight**: within `sight_range`, inside a `fov` cone in front of the enemy, and not behind a
//!   wall. Sight fills a suspicion meter, faster when the player is closer; when it's full the
//!   enemy becomes alert. Sleeping enemies (`blind_when_unaware`) don't see at all.
//! - **Touch**: within `feel_range` the player is noticed at once, whatever the facing.
//! - **Hearing**: [`Noise`] messages (shots, hard landings, slides, fallen allies, alarms) make
//!   enemies within `radius * hearing` suspicious and turn them towards the sound.
//! - **Alarm**: an enemy becoming alert alerts the others within its `alarm_radius`.
//! - **Memory**: an alert enemy that loses sight of the player keeps going for the last known
//!   position for `memory` seconds, then searches there for `search_time` seconds.
//!
//! What each enemy *does* in each state is up to its brain (see `enemies.rs`).

use bevy::prelude::*;
use bevy_rapier2d::prelude::*;

use crate::characters::{Facing, Player};
use crate::{DebugView, GameSet};

pub struct PerceptionPlugin;

impl Plugin for PerceptionPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Perception>()
            .register_type::<AwarenessIcon>()
            .add_message::<Noise>()
            .add_message::<EnemyAlerted>()
            .init_resource::<NoiseRings>()
            .add_systems(Update, (look, listen, raise_alarms).chain().in_set(GameSet::Perceive))
            .add_systems(PostUpdate, (update_awareness_icons, collect_noise_rings))
            .add_systems(Last, draw_noise_rings)
            .add_systems(Last, draw_perception.run_if(|debug: Res<DebugView>| debug.0));
    }
}

#[derive(Component, Reflect, Clone, Debug, Default)]
#[reflect(Component, Default)]
#[require(Awareness)]
pub struct Perception {
    /// How far the enemy sees, in pixels.
    pub sight_range: f32,
    /// Width of the vision cone in degrees, centered on the facing direction.
    pub fov: f32,
    /// Eye position relative to the entity, for an enemy facing right.
    pub eye: Vec2,
    /// Multiplier on the radius of the noises it hears.
    pub hearing: f32,
    /// Noticed at once this close, whatever the facing.
    pub feel_range: f32,
    /// Seconds of continuous sight, at the edge of the range, to become alert.
    pub notice_time: f32,
    /// Seconds an alert enemy keeps chasing after losing sight of the player.
    pub memory: f32,
    /// Seconds spent searching the last known position.
    pub search_time: f32,
    /// Allies within this radius become alert when this enemy does.
    pub alarm_radius: f32,
    /// Asleep while unaware: only touch and hearing work.
    pub blind_when_unaware: bool,
}

impl Perception {
    pub fn eye_position(&self, position: Vec2, facing: Facing) -> Vec2 {
        position + Vec2::new(self.eye.x * facing.sign(), self.eye.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AwarenessState {
    #[default]
    Unaware,
    Suspicious,
    Alert,
    Searching,
}

#[derive(Component, Debug, Clone, Default)]
pub struct Awareness {
    pub state: AwarenessState,
    /// 0..1, how close the enemy is to becoming alert.
    pub suspicion: f32,
    /// Where the enemy thinks the player is (or the noise came from).
    pub last_known: Vec2,
    /// Whether the player is in sight this frame.
    pub sees_player: bool,
    /// Seconds since the player was last seen.
    pub since_seen: f32,
    /// Seconds in the current state.
    pub in_state: f32,
    /// State when alarms were last checked, to detect transitions from any source.
    previous: AwarenessState,
}

impl Awareness {
    pub fn is_unaware(&self) -> bool {
        self.state == AwarenessState::Unaware
    }

    /// Becomes alert, looking for the player at `position`.
    pub fn alert_at(&mut self, position: Vec2) {
        self.last_known = position;
        self.suspicion = 1.0;
        self.since_seen = 0.0;
        self.set_state(AwarenessState::Alert);
    }

    fn set_state(&mut self, state: AwarenessState) {
        if self.state != state {
            self.state = state;
            self.in_state = 0.0;
        }
    }

    /// Became alert since alarms were last checked.
    fn just_alerted(&self) -> bool {
        self.state == AwarenessState::Alert && self.previous != AwarenessState::Alert
    }
}

/// Who made a noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseKind {
    /// Shots, landings, slides: draws attention to that spot.
    Player,
    /// An ally went down there.
    AllyDown,
    /// An alert ally shouting where the player is: alerts the listener.
    Alarm,
}

/// A sound heard by enemies within `radius * hearing` of `position`.
#[derive(Message, Debug, Clone)]
pub struct Noise {
    pub position: Vec2,
    pub radius: f32,
    pub kind: NoiseKind,
    pub source: Entity,
}

/// An enemy became alert (by sight, noise, alarm or damage).
#[derive(Message, Debug, Clone)]
pub struct EnemyAlerted(pub Entity);

/// The "?" / "!" icon over an enemy's head. Must be a child of an entity with [`Awareness`].
#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component, Default)]
pub struct AwarenessIcon;

/// Rate at which suspicion drains when the player is out of sight.
const SUSPICION_DECAY: f32 = 0.35;
/// Suspicion set by hearing a noise: halfway to alert, so a glimpse finishes the job.
const NOISE_SUSPICION: f32 = 0.5;

fn look(
    time: Res<Time>,
    rapier: ReadRapierContext,
    player: Query<&GlobalTransform, With<Player>>,
    mut enemies: Query<(&Perception, &mut Awareness, &mut Facing, &GlobalTransform)>,
) -> Result {
    let rapier = rapier.single()?;
    let dt = time.delta_secs();
    let player_position = player.single().ok().map(|transform| transform.translation().truncate());

    for (perception, mut awareness, mut facing, transform) in &mut enemies {
        awareness.in_state += dt;
        let eye = perception.eye_position(transform.translation().truncate(), *facing);

        let (sees, touches, distance) = match player_position {
            Some(target) => {
                let to_player = target - eye;
                let distance = to_player.length();
                let in_line_of_sight = || {
                    rapier
                        .cast_ray(eye, to_player.normalize_or_zero(), distance, true, QueryFilter::only_fixed())
                        .is_none()
                };
                let touches = distance <= perception.feel_range && in_line_of_sight();
                let blind = perception.blind_when_unaware && awareness.is_unaware();
                let in_cone = {
                    let forward = Vec2::new(facing.sign(), 0.0);
                    forward.angle_to(to_player).abs().to_degrees() <= perception.fov / 2.0
                };
                let sees = touches || (!blind && distance <= perception.sight_range && in_cone && in_line_of_sight());
                (sees, touches, distance)
            }
            None => (false, false, f32::INFINITY),
        };
        awareness.sees_player = sees;

        if sees {
            let target = player_position.unwrap_or_default();
            awareness.last_known = target;
            awareness.since_seen = 0.0;
            if touches || awareness.state == AwarenessState::Searching {
                // Already on edge, or right next to them: no doubt about it.
                awareness.alert_at(target);
            } else if awareness.state != AwarenessState::Alert {
                // Closer is faster: up to 3x at point-blank range.
                let closeness = 1.0 - (distance / perception.sight_range).clamp(0.0, 1.0);
                awareness.suspicion += dt / perception.notice_time.max(0.01) * (1.0 + 2.0 * closeness);
                if awareness.suspicion >= 1.0 {
                    awareness.alert_at(target);
                } else {
                    awareness.set_state(AwarenessState::Suspicious);
                }
            }
            if let Some(towards) = Facing::towards(target.x - eye.x)
                && awareness.state == AwarenessState::Alert
            {
                *facing = towards;
            }
            continue;
        }

        awareness.since_seen += dt;
        match awareness.state {
            AwarenessState::Unaware => {}
            AwarenessState::Suspicious => {
                awareness.suspicion -= SUSPICION_DECAY * dt;
                if awareness.suspicion <= 0.0 {
                    awareness.suspicion = 0.0;
                    awareness.set_state(AwarenessState::Unaware);
                }
            }
            AwarenessState::Alert => {
                if awareness.since_seen > perception.memory {
                    awareness.set_state(AwarenessState::Searching);
                }
            }
            AwarenessState::Searching => {
                if awareness.in_state > perception.search_time {
                    awareness.suspicion = 0.0;
                    awareness.set_state(AwarenessState::Unaware);
                }
            }
        }
    }
    Ok(())
}

fn listen(
    mut noises: MessageReader<Noise>,
    mut enemies: Query<(Entity, &Perception, &mut Awareness, &mut Facing, &GlobalTransform)>,
) {
    for noise in noises.read() {
        for (entity, perception, mut awareness, mut facing, transform) in &mut enemies {
            if entity == noise.source {
                continue;
            }
            let position = transform.translation().truncate();
            if position.distance(noise.position) > noise.radius * perception.hearing {
                continue;
            }
            match (noise.kind, awareness.state) {
                (NoiseKind::Alarm, AwarenessState::Alert) => {}
                (NoiseKind::Alarm, _) => awareness.alert_at(noise.position),
                // Already chasing: a noise is just a hint where the player went.
                (_, AwarenessState::Alert) => {
                    if !awareness.sees_player {
                        awareness.last_known = noise.position;
                    }
                }
                (_, AwarenessState::Searching) => {
                    awareness.last_known = noise.position;
                    awareness.in_state = 0.0;
                }
                (_, AwarenessState::Unaware | AwarenessState::Suspicious) => {
                    awareness.last_known = noise.position;
                    awareness.suspicion = awareness.suspicion.max(NOISE_SUSPICION);
                    awareness.set_state(AwarenessState::Suspicious);
                    if let Some(towards) = Facing::towards(noise.position.x - position.x) {
                        *facing = towards;
                    }
                }
            }
        }
    }
}

/// Enemies that just became alert (by sight, noise, alarm or damage) shout: allies around them
/// become alert too, and shout in turn.
fn raise_alarms(
    mut enemies: Query<(Entity, &Perception, &mut Awareness)>,
    mut noises: MessageWriter<Noise>,
    mut alerted: MessageWriter<EnemyAlerted>,
) {
    for (entity, perception, mut awareness) in &mut enemies {
        if awareness.just_alerted() {
            alerted.write(EnemyAlerted(entity));
        }
        if awareness.just_alerted() && perception.alarm_radius > 0.0 {
            noises.write(Noise {
                position: awareness.last_known,
                radius: perception.alarm_radius,
                kind: NoiseKind::Alarm,
                source: entity,
            });
        }
        awareness.previous = awareness.state;
    }
}

/// Frame of the "?" and "!" icons in `enemies.png`.
const QUESTION_ICON: usize = 12;
const EXCLAIM_ICON: usize = 13;
/// The "!" shows this long after becoming alert, like in Metal Gear.
const EXCLAIM_TIME: f32 = 0.8;

fn update_awareness_icons(
    mut icons: Query<(&ChildOf, &mut Visibility, &mut Sprite), With<AwarenessIcon>>,
    enemies: Query<&Awareness>,
) {
    for (child_of, mut visibility, mut sprite) in &mut icons {
        let Ok(awareness) = enemies.get(child_of.parent()) else { continue };
        let icon = match awareness.state {
            AwarenessState::Unaware => None,
            AwarenessState::Suspicious | AwarenessState::Searching => Some(QUESTION_ICON),
            AwarenessState::Alert if awareness.in_state < EXCLAIM_TIME => Some(EXCLAIM_ICON),
            AwarenessState::Alert => None,
        };
        visibility.set_if_neq(if icon.is_some() { Visibility::Inherited } else { Visibility::Hidden });
        if let (Some(icon), Some(atlas)) = (icon, sprite.texture_atlas.as_mut())
            && atlas.index != icon
        {
            atlas.index = icon;
        }
    }
}

/// Rings showing how far the player's noises carry (at `hearing: 1.0`), so players learn what
/// enemies hear without reading about it.
#[derive(Resource, Default)]
struct NoiseRings(Vec<(Vec2, f32, f32)>);

const NOISE_RING_TIME: f32 = 0.35;

fn collect_noise_rings(mut noises: MessageReader<Noise>, mut rings: ResMut<NoiseRings>) {
    for noise in noises.read() {
        if noise.kind == NoiseKind::Player {
            rings.0.push((noise.position, noise.radius, 0.0));
        }
    }
}

fn draw_noise_rings(time: Res<Time>, mut rings: ResMut<NoiseRings>, mut gizmos: Gizmos) {
    for (position, radius, age) in &mut rings.0 {
        *age += time.delta_secs();
        let t = (*age / NOISE_RING_TIME).min(1.0);
        let grow = 1.0 - (1.0 - t) * (1.0 - t);
        gizmos.circle_2d(
            Isometry2d::from_translation(*position),
            *radius * grow,
            Color::srgba(1.0, 1.0, 1.0, 0.7 * (1.0 - t)),
        );
    }
    rings.0.retain(|(_, _, age)| *age < NOISE_RING_TIME);
}

/// Debug view (F1): vision cones, touch and hearing ranges, suspicion meters and last known
/// positions, colored by state.
fn draw_perception(mut gizmos: Gizmos, enemies: Query<(&Perception, &Awareness, &Facing, &GlobalTransform)>) {
    for (perception, awareness, facing, transform) in &enemies {
        let position = transform.translation().truncate();
        let eye = perception.eye_position(position, *facing);
        let color = match awareness.state {
            AwarenessState::Unaware => Color::srgba(0.6, 0.6, 0.6, 0.6),
            AwarenessState::Suspicious => Color::srgb(1.0, 0.85, 0.1),
            AwarenessState::Alert => Color::srgb(1.0, 0.15, 0.2),
            AwarenessState::Searching => Color::srgb(0.3, 0.6, 1.0),
        };

        if !(perception.blind_when_unaware && awareness.is_unaware()) {
            let half = (perception.fov / 2.0).min(180.0).to_radians();
            let forward = if facing.sign() > 0.0 { 0.0 } else { std::f32::consts::PI };
            let steps = 16;
            let arc = (0..=steps).map(|i| {
                let angle = forward - half + 2.0 * half * i as f32 / steps as f32;
                eye + Vec2::from_angle(angle) * perception.sight_range
            });
            if perception.fov >= 360.0 {
                gizmos.linestrip_2d(arc, color);
            } else {
                gizmos.linestrip_2d(std::iter::once(eye).chain(arc).chain(std::iter::once(eye)), color);
            }
        }
        gizmos.circle_2d(Isometry2d::from_translation(position), perception.feel_range, color.with_alpha(0.3));

        if awareness.suspicion > 0.0 && awareness.state != AwarenessState::Alert {
            let start = position + Vec2::new(-6.0, 9.0);
            gizmos.line_2d(start, start + Vec2::X * 12.0 * awareness.suspicion.min(1.0), color);
        }
        if awareness.state != AwarenessState::Unaware && !awareness.sees_player {
            let p = awareness.last_known;
            gizmos.line_2d(p + Vec2::new(-2.0, -2.0), p + Vec2::new(2.0, 2.0), color);
            gizmos.line_2d(p + Vec2::new(-2.0, 2.0), p + Vec2::new(2.0, -2.0), color);
        }
    }
}
