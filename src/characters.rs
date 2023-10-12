use bevy::{prelude::*, utils::HashMap};
use bevy_rapier2d::prelude::*;

use crate::asset_manager::PlayerAssets;
use crate::animation::CompAnimation;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, sync_action_animation)
            .add_systems(Update, (
                apply_gravity, 
                player_movement_input,
                move_character
            ).chain())
        ;
    }
    
}

#[derive(Debug, Component)]
pub struct Velocity(Vec2);

#[derive(Debug, Component)]
pub struct Player();

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum CharacterActions{
    Idle,
    Running,
    Pushing
}

#[derive(Debug, Component)]
pub struct CurrentAction(CharacterActions);

#[derive(Debug, Component)]
pub struct ActionAnimations{
    map: HashMap<CharacterActions, CompAnimation>
}

fn sync_action_animation(
    mut commands: Commands,
    anim_query: Query<(Entity, &CurrentAction, &ActionAnimations), Changed<CurrentAction>>
)
{
    for (entity_id, action, animation_map) in anim_query.iter(){
        error!("{:?}", action);
        if let Some(anim) = animation_map.map.get(&action.0){
            commands.entity(entity_id).insert(anim.clone());
        }
    }
}

fn apply_gravity(
    time: Res<Time>,
    mut kinematic: Query<(&mut Velocity, &KinematicCharacterControllerOutput)>
)
{
    for (mut velocity, output) in kinematic.iter_mut(){
        if !output.grounded && velocity.0.y > -100.0 {
            velocity.0.y = -100.0_f32.max(velocity.0.y - 10.0 * time.delta_seconds())
        }
        if output.grounded{
            velocity.0.y = 0.0;
        }
        if output.effective_translation.x.abs() < 0.001{
            velocity.0.x = 0.0;
        }
    }
}

fn move_character(
    time: Res<Time>,
    mut controller_query: Query<(&Velocity, &mut KinematicCharacterController)>
){
    for (velocity, mut controller) in controller_query.iter_mut(){
        controller.translation = Some(velocity.0 * time.delta_seconds())
    }
}

// System that controls the Player character from parses player input.
/// In reads keypresses and modifies the player Entity's Speed component.
pub fn player_movement_input(
    keyboard_input: Res<Input<KeyCode>>,
    time: Res<Time>,
    mut player_query: Query<(Entity, &mut Velocity), With<Player>>
) {
    if let Some((player, mut vel)) = player_query.iter_mut().next() {
        if keyboard_input.pressed(KeyCode::Right) {
            vel.0.x += 40.0 * time.delta_seconds();
            vel.0.x = vel.0.x.min(100.0);
        } else if keyboard_input.pressed(KeyCode::Left) {
            vel.0.x -= 40.0 * time.delta_seconds();
            vel.0.x = vel.0.x.max(-100.0);
        }
    }
}


#[derive(Bundle)]
pub struct CharacterBundle{
    ssheet: SpriteSheetBundle,
    body: RigidBody,
    collision: Collider,
    controller: KinematicCharacterController,
    velocity: Velocity,
    action: CurrentAction,
    animations: ActionAnimations
}

impl CharacterBundle{
    pub fn new(name: &str, assets: &Res<PlayerAssets>) -> Self{
        let atlas_field_name = format!("{name}_texture");
        let atlas_field = assets.field(&atlas_field_name).unwrap();
        let atlas_handle = atlas_field.downcast_ref::<Handle<TextureAtlas>>().unwrap();
        
        let ssheet = SpriteSheetBundle{
            texture_atlas: atlas_handle.clone(),
            ..default()
        };

        let mut animations = ActionAnimations{ map: HashMap::new() };
        animations.map.insert(CharacterActions::Idle, CompAnimation::new_segment(0, 2, 2.0, 0));
        animations.map.insert(CharacterActions::Running, CompAnimation::new_segment(4, 7, 8.0, 0));

        Self { 
            ssheet,
            body: RigidBody::KinematicPositionBased, 
            collision: Collider::cuboid(3.0, 4.0), 
            controller: KinematicCharacterController::default(),
            velocity: Velocity(Vec2::ZERO),
            action: CurrentAction(CharacterActions::Idle),
            animations
            
        }
    }
}