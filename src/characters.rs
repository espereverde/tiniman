use bevy::{prelude::*, utils::HashMap};
use bevy_rapier2d::prelude::*;

use crate::asset_manager::PlayerAssets;
use crate::animation::CompAnimation;

const GRAVITY: f32 = 700.0;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_type::<Movement>()
            .add_systems(Update, sync_action_animation)
            .add_systems(Update, (
                apply_gravity, 
                player_movement_input,
                move_character
            ).chain())
        ;
    }
    
}

#[derive(Debug, Component, Reflect)]
pub struct Movement{
    vel: Vec2,
    max_vel: Vec2,
    forward_acc: f32, 
    glide_acc: f32, 
    brake_acc: f32
}

const PLAYER_MOV: Movement = Movement{
    vel: Vec2::ZERO,
    max_vel: Vec2 { x: 80., y: 120. },
    forward_acc: 140.0,
    glide_acc: 140.0,
    brake_acc: 300.0
};

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
    mut kinematic: Query<(&mut Movement, &KinematicCharacterControllerOutput)>
)
{
    for (mut movement, output) in kinematic.iter_mut(){
        if !output.grounded {
            movement.vel.y = (movement.vel.y - GRAVITY * time.delta_seconds()).clamp(-movement.max_vel.y, movement.max_vel.y);
        }
        if output.grounded{
            movement.vel.y = -0.1;
        }
        if output.effective_translation.x.abs() < 0.001{
            movement.vel.x = 0.0;
        }
    }
}

fn move_character(
    time: Res<Time>,
    mut controller_query: Query<(&Movement, &mut KinematicCharacterController)>
){
    for (velocity, mut controller) in controller_query.iter_mut(){
        controller.translation = Some(velocity.vel * time.delta_seconds())
    }
}

// System that controls the Player character from parses player input.
/// In reads keypresses and modifies the player Entity's Speed component.
pub fn player_movement_input(
    keyboard_input: Res<Input<KeyCode>>,
    time: Res<Time>,
    mut player_query: Query<(&mut CurrentAction, &mut Movement, &mut TextureAtlasSprite), With<Player>>
) {
    if let Some((mut c_action, mut movement, mut sprite)) = player_query.iter_mut().next() {
        
        let mut next_action: CharacterActions; 
        //Set very small velocities to 0 to avoid drift
        if movement.vel.x.abs() < 1.0 { 
            movement.vel.x = 0.0;
            next_action = CharacterActions::Idle;
        }else {
            next_action = CharacterActions::Running;
        }
        
        //Calculate desired input direction +1 for right, -1 for left. 
        //If both keys are pressed they cancel out.
        let mut input_dir: f32= 0.0;
        if keyboard_input.pressed(KeyCode::Right) { input_dir += 1.0; }
        if keyboard_input.pressed(KeyCode::Left) { input_dir -= 1.0; }
        // Get the current velocity direction. If character is not moving we take the input direction.
        let vel_dir = 
            if movement.vel.x == 0.0 { input_dir }
            else { movement.vel.x.signum()};

        //If no input direction the character is gliding and we slowly reduce its velocity
        if input_dir == 0.0 {
            movement.vel.x -= movement.glide_acc * time.delta_seconds() * vel_dir;
        }
        //If both directions coincide we use the forward acceleration
        else if input_dir == vel_dir {
            movement.vel.x += movement.forward_acc * time.delta_seconds() * input_dir;
            next_action = CharacterActions::Running;
        //If directions are different we use the brake acceleration
        } else{
            movement.vel.x += movement.brake_acc * time.delta_seconds() * input_dir;
            next_action = CharacterActions::Running;
        }
        //Clamp velocity
        movement.vel.x = movement.vel.x.clamp(-movement.max_vel.x, movement.max_vel.x);
        //Change character action if needed
        if c_action.0 != next_action { c_action.0 = next_action; }

        if input_dir == 1.0 {
            sprite.flip_x = false;
        }else if input_dir == -1.0{
            sprite.flip_x = true;
        }
    }
}


#[derive(Bundle)]
pub struct CharacterBundle{
    ssheet: SpriteSheetBundle,
    body: RigidBody,
    collision: Collider,
    controller: KinematicCharacterController,
    movement: Movement,
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
            movement: PLAYER_MOV,
            action: CurrentAction(CharacterActions::Idle),
            animations
        }
    }
}