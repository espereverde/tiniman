//! Room Rush: every LDtk level is a room. Reach the exit as fast as you can.
//!
//! - The exit is **locked while any enemy is alert**: kill them (or lose them) to open it, so being
//!   seen costs time.
//! - Clearing a room awards up to three badges: **Fast** (under the room's `Par_time`), **Ghost**
//!   (no enemy became alert) and **Clean** (no damage taken).
//! - `R` restarts the room at any time; after a clear, `Z`/`X` skip to the next room.
//! - `1`..`9` jump to a room (for testing rooms while editing them in LDtk).

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_ecs_ldtk::prelude::*;

use crate::GameStates;
use crate::characters::Player;
use crate::combat::{Defeated, Health, Hurtbox, Team};
use crate::enemies::Enemy;
use crate::perception::{Awareness, AwarenessState, EnemyAlerted};
use crate::weapons::Bullet;

pub struct RoomPlugin;

impl Plugin for RoomPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Exit>()
            .init_resource::<RoomRun>()
            .init_resource::<BestBadges>()
            .add_systems(
                Update,
                (start_room, lower_exits, count_alerts, lock_exits, reach_exit, player_defeated, tick_room, next_or_restart)
                    .chain()
                    .after(crate::GameSet::Resolve)
                    .run_if(in_state(GameStates::Running)),
            );
    }
}

/// Run condition: the room is being played (not cleared, failed or loading).
pub fn room_playing(run: Res<RoomRun>) -> bool {
    run.phase == RoomPhase::Playing
}

#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component, Default)]
pub struct Exit {
    pub half_size: Vec2,
    #[reflect(ignore)]
    pub locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RoomPhase {
    /// Waiting for the room (and the player) to spawn.
    Starting,
    Playing,
    Cleared { since: f32, result: RoomResult },
    Failed { since: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoomResult {
    pub time: f32,
    pub badges: Badges,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Badges {
    pub fast: bool,
    pub ghost: bool,
    pub clean: bool,
}

impl Badges {
    fn union(self, other: Badges) -> Badges {
        Badges { fast: self.fast || other.fast, ghost: self.ghost || other.ghost, clean: self.clean || other.clean }
    }
}

/// The current attempt at the current room.
#[derive(Resource, Debug)]
pub struct RoomRun {
    pub room: usize,
    pub room_count: usize,
    pub name: String,
    pub par: f32,
    pub time: f32,
    pub alerts: u32,
    pub phase: RoomPhase,
}

impl Default for RoomRun {
    fn default() -> Self {
        Self {
            room: 0,
            room_count: 1,
            name: String::new(),
            par: 0.0,
            time: 0.0,
            alerts: 0,
            phase: RoomPhase::Starting,
        }
    }
}

/// Every badge ever earned in each room (this session).
#[derive(Resource, Default)]
pub struct BestBadges(pub HashMap<String, Badges>);

/// Par time of rooms without a `Par_time` field.
const DEFAULT_PAR: f32 = 10.0;
/// Clear screen shown at least this long, and at most `AUTO_ADVANCE`, before the next room.
const MIN_RESULTS_TIME: f32 = 0.6;
const AUTO_ADVANCE: f32 = 4.0;
const FAILED_RESTART: f32 = 1.2;

/// The player spawning means the room (re)started.
fn start_room(
    new_player: Query<(), Added<Player>>,
    mut run: ResMut<RoomRun>,
    projects: Query<&LdtkProjectHandle>,
    project_assets: Res<Assets<LdtkProject>>,
) {
    if new_player.is_empty() {
        return;
    }
    let project = projects.single().ok().and_then(|handle| project_assets.get(handle));
    if let Some(project) = project {
        run.room_count = project.iter_raw_levels().count().max(1);
        if let Some(level) = project.iter_raw_levels().nth(run.room) {
            run.name = level.identifier.clone();
            run.par = level.get_float_field("Par_time").copied().unwrap_or(DEFAULT_PAR);
        }
    }
    run.time = 0.0;
    run.alerts = 0;
    run.phase = RoomPhase::Playing;
}

/// Draws doors behind the characters.
fn lower_exits(mut exits: Query<&mut Transform, Added<Exit>>) {
    for mut transform in &mut exits {
        transform.translation.z -= 0.5;
    }
}

fn count_alerts(mut alerted: MessageReader<EnemyAlerted>, mut run: ResMut<RoomRun>) {
    let count = alerted.read().count() as u32;
    if count > 0 && run.phase == RoomPhase::Playing {
        run.alerts += count;
    }
}

fn lock_exits(enemies: Query<&Awareness, With<Enemy>>, mut exits: Query<(&mut Exit, &mut Sprite)>) {
    let locked = enemies.iter().any(|awareness| awareness.state == AwarenessState::Alert);
    for (mut exit, mut sprite) in &mut exits {
        if exit.locked != locked {
            exit.locked = locked;
        }
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            let frame = if locked { 0 } else { 1 };
            if atlas.index != frame {
                atlas.index = frame;
            }
        }
    }
}

fn reach_exit(
    mut run: ResMut<RoomRun>,
    mut best: ResMut<BestBadges>,
    player: Query<(&GlobalTransform, &Hurtbox, &Health), With<Player>>,
    exits: Query<(&GlobalTransform, &Exit)>,
) {
    if run.phase != RoomPhase::Playing {
        return;
    }
    let Ok((player_transform, player_box, health)) = player.single() else { return };
    let player_center = player_transform.translation().truncate();
    let reached = exits.iter().any(|(transform, exit)| {
        !exit.locked
            && Hurtbox { half_size: exit.half_size }.overlaps_box(
                transform.translation().truncate(),
                player_box,
                player_center,
            )
    });
    if !reached {
        return;
    }
    let badges = Badges { fast: run.time <= run.par, ghost: run.alerts == 0, clean: health.hp >= health.max };
    let entry = best.0.entry(run.name.clone()).or_default();
    *entry = entry.union(badges);
    run.phase = RoomPhase::Cleared { since: 0.0, result: RoomResult { time: run.time, badges } };
}

fn player_defeated(mut defeated: MessageReader<Defeated>, mut run: ResMut<RoomRun>) {
    if defeated.read().any(|defeat| defeat.team == Team::Player) && run.phase == RoomPhase::Playing {
        run.phase = RoomPhase::Failed { since: 0.0 };
    }
}

fn tick_room(time: Res<Time>, mut run: ResMut<RoomRun>) {
    let dt = time.delta_secs();
    match &mut run.phase {
        RoomPhase::Playing => run.time += dt,
        RoomPhase::Cleared { since, .. } | RoomPhase::Failed { since } => *since += dt,
        RoomPhase::Starting => {}
    }
}

fn next_or_restart(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut run: ResMut<RoomRun>,
    mut selection: ResMut<LevelSelection>,
    worlds: Query<Entity, With<LdtkProjectHandle>>,
    bullets: Query<Entity, With<Bullet>>,
) {
    const ROOM_KEYS: [KeyCode; 9] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    let jump = ROOM_KEYS.iter().position(|key| keyboard.just_pressed(*key)).filter(|room| *room < run.room_count);
    let restart = match run.phase {
        RoomPhase::Starting => return,
        _ if jump.is_some() => {
            run.room = jump.unwrap_or_default();
            *selection = LevelSelection::index(run.room);
            false
        }
        RoomPhase::Cleared { since, .. } => {
            let skip = keyboard.any_just_pressed([KeyCode::KeyZ, KeyCode::KeyX]) && since > MIN_RESULTS_TIME;
            if keyboard.just_pressed(KeyCode::KeyR) {
                true
            } else if skip || since > AUTO_ADVANCE {
                run.room = (run.room + 1) % run.room_count;
                *selection = LevelSelection::index(run.room);
                false
            } else {
                return;
            }
        }
        RoomPhase::Failed { since } if since > FAILED_RESTART || keyboard.just_pressed(KeyCode::KeyR) => true,
        RoomPhase::Failed { .. } => return,
        RoomPhase::Playing if keyboard.just_pressed(KeyCode::KeyR) => true,
        RoomPhase::Playing => return,
    };
    if restart {
        for world in &worlds {
            commands.entity(world).insert(Respawn);
        }
    }
    for bullet in &bullets {
        commands.entity(bullet).despawn();
    }
    run.phase = RoomPhase::Starting;
}
