#![cfg_attr(debug_assertions, allow(dead_code))]

use bevy::prelude::*;
use bevy::window::{Monitor, PrimaryMonitor, PrimaryWindow, WindowResolution};
use bevy_rapier2d::prelude::*;

mod animation;
mod blueprints;
mod characters;
mod combat;
mod enemies;
mod hud;
mod level;
mod perception;
mod room;
mod weapons;

use animation::AnimationPlugin;
use blueprints::BlueprintsPlugin;
use characters::CharacterPlugin;
use combat::CombatPlugin;
use enemies::EnemyPlugin;
use hud::HudPlugin;
use level::LevelPlugin;
use perception::PerceptionPlugin;
use room::RoomPlugin;
use weapons::WeaponsPlugin;

const BACKGROUNDCOLOR: [u8; 3] = [0, 0, 0];

/// A stage is a single NES-sized screen. The window shows it at the largest integer scale that
/// fits the monitor (see `fit_window_to_monitor`), starting at this one.
pub const STAGE_SIZE: Vec2 = Vec2::new(256.0, 240.0);
const WINDOW_SCALE: u32 = 2;

#[derive(Clone, Copy, Eq, PartialEq, Debug, Hash, Default, States)]
pub enum GameStates {
    #[default]
    AssetLoading,
    Running,
}

/// Order of the gameplay systems inside `Update`.
#[derive(Debug, PartialEq, Eq, Hash, Clone, SystemSet)]
pub enum GameSet {
    /// Enemies look and listen.
    Perceive,
    /// Player input and enemy brains decide velocities.
    Think,
    /// Velocities are handed to the physics, weapons fire, bullets fly.
    Act,
    /// Damage, deaths and stage status.
    Resolve,
}

/// Debug view toggled with F1: vision cones, hearing ranges and physics colliders.
#[derive(Resource, Default)]
pub struct DebugView(pub bool);

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb_u8(BACKGROUNDCOLOR[0], BACKGROUNDCOLOR[1], BACKGROUNDCOLOR[2])))
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest()) // prevents blurry sprites
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Tinyman".into(),
                        resolution: WindowResolution::new(
                            STAGE_SIZE.x as u32 * WINDOW_SCALE,
                            STAGE_SIZE.y as u32 * WINDOW_SCALE,
                        ),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .init_state::<GameStates>()
        .init_resource::<DebugView>()
        .configure_sets(
            Update,
            (GameSet::Perceive, GameSet::Think, GameSet::Act, GameSet::Resolve)
                .chain()
                .run_if(in_state(GameStates::Running).and_then(room::room_playing)),
        )
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(6.0))
        .add_plugins(RapierDebugRenderPlugin { enabled: false, ..default() })
        .add_plugins((
            BlueprintsPlugin,
            AnimationPlugin,
            CharacterPlugin,
            WeaponsPlugin,
            CombatPlugin,
            PerceptionPlugin,
            EnemyPlugin,
            LevelPlugin,
            RoomPlugin,
            HudPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (toggle_debug_view, fit_window_to_monitor))
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Picks the largest integer scale at which the whole stage fits on the primary monitor.
fn fit_window_to_monitor(
    monitor: Query<&Monitor, With<PrimaryMonitor>>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Ok(monitor) = monitor.single() else { return };
    *done = true;
    let logical_size =
        Vec2::new(monitor.physical_width as f32, monitor.physical_height as f32) / monitor.scale_factor as f32;
    // Leave room for the taskbar and the title bar.
    let available = logical_size * Vec2::new(0.9, 0.85);
    let scale = (available / STAGE_SIZE).min_element().floor().max(1.0);
    window.resolution.set(STAGE_SIZE.x * scale, STAGE_SIZE.y * scale);
    window.position.center(MonitorSelection::Primary);
}

fn toggle_debug_view(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut debug: ResMut<DebugView>,
    mut rapier_debug: ResMut<DebugRenderContext>,
) {
    if keyboard.just_pressed(KeyCode::F1) {
        debug.0 = !debug.0;
        rapier_debug.enabled = debug.0;
    }
}
