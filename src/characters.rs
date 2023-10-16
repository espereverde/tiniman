use bevy::{prelude::*, utils::HashMap, transform::TransformSystem};
use bevy_rapier2d::prelude::*;

use crate::asset_manager::PlayerAssets;
use crate::animation::CompAnimation;

const GRAVITY: f32 = 700.0;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_type::<Movement>()
            .add_systems(PreUpdate, process_player_input)
            .add_systems(Update, (
                update_air_movement,
                update_ground_movement,
                apply_movement
            ).chain())
            .add_systems(Update, sync_action_animation)
            .add_systems(PostUpdate, (stop_on_bump, check_ground, debug_collisions).chain()
                .after(PhysicsSet::Writeback)
                .before(TransformSystem::TransformPropagate)
            )
        ;
    }
}

#[derive(Debug, Component)]
struct OnGround;

#[derive(Debug, Component)]
struct OnAir;

#[derive(Debug, Component)]
pub struct Player();

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum CharacterActions{
    Idle,
    Running,
    Jumping,
    Falling
}

#[derive(Debug, Component)]
pub struct CurrentAction(CharacterActions);

#[derive(Debug, Component)]
pub struct ActionAnimations{
    map: HashMap<CharacterActions, CompAnimation>
}

#[derive(Debug, Component, Reflect, Default)]
struct InputOrders{
    horiz_dir: f32,
    do_jump: bool,
    keep_jump: bool
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
    max_vel: Vec2 { x: 80., y: 220. },
    forward_acc: 140.0,
    glide_acc: 140.0,
    brake_acc: 300.0
};

fn process_player_input(
    keyboard: Res<Input<KeyCode>>,
    mut input_query: Query<&mut InputOrders, With<Player>>
){
    if let Ok(mut player_input) = input_query.get_single_mut(){
        //Calculate desired input direction +1 for right, -1 for left. 
        //If both keys are pressed they cancel out.
        player_input.horiz_dir = 0.0;
        if keyboard.pressed(KeyCode::Right) { player_input.horiz_dir += 1.0; }
        if keyboard.pressed(KeyCode::Left) { player_input.horiz_dir -= 1.0; }
        //Store jump key press state
        player_input.do_jump = keyboard.just_pressed(KeyCode::Z);
        player_input.keep_jump = keyboard.pressed(KeyCode::Z);
    }
}

fn update_air_movement(
    time: Res<Time>,
    mut move_query: Query<(&InputOrders, &mut Movement, &mut CurrentAction), With<OnAir>>
)
{
    for (orders, mut movement, mut c_action ) in move_query.iter_mut(){
            let mut next_action = c_action.0.clone();

            if movement.vel.y > 0.0 && orders.keep_jump {
                movement.vel.y += GRAVITY * time.delta_seconds() / 2.0;
            }
            movement.vel.y = (movement.vel.y - GRAVITY * time.delta_seconds()).clamp(-movement.max_vel.y, movement.max_vel.y);

            if movement.vel.y > 0.0 { next_action = CharacterActions::Jumping}
            else if movement.vel.y < 0.0 {next_action = CharacterActions::Falling}

            //Acceleration on air is half the acceleration on ground
            movement.vel.x += (movement.forward_acc / 2.0) * time.delta_seconds() * orders.horiz_dir;

            if next_action != c_action.0{
                c_action.0 = next_action;
            }
    }
        
}

// System that controls the Player character from parses player input.
/// In reads keypresses and modifies the player Entity's Speed component.
fn update_ground_movement(
    time: Res<Time>,
    mut player_query: Query<(&InputOrders, &mut CurrentAction, &mut Movement, &mut TextureAtlasSprite), (With<Player>, With<OnGround>)>
) {
    if let Some((orders, mut c_action, mut movement, mut sprite)) = player_query.iter_mut().next() {
        
        let mut next_action = c_action.0.clone();

        if orders.do_jump {movement.vel.y = 160.0;}
        else{
            //movement.vel.y = -0.1;
            // Get the current velocity direction. If character is not moving we take the input direction.
            let vel_dir = 
            if movement.vel.x == 0.0 { orders.horiz_dir }
            else { movement.vel.x.signum()};

            if orders.horiz_dir == 0.0 && movement.vel.x.abs() < 1.0 { 
                movement.vel.x = 0.0;
                next_action = CharacterActions::Idle;
            }
            //If no input direction the character is gliding and we slowly reduce its velocity
            else if orders.horiz_dir == 0.0 {
                movement.vel.x -= movement.glide_acc * time.delta_seconds() * vel_dir;
                next_action = CharacterActions::Running;
            }//If both directions coincide we use the forward acceleration
            else if orders.horiz_dir == vel_dir {
                movement.vel.x += movement.forward_acc * time.delta_seconds() * orders.horiz_dir;
                next_action = CharacterActions::Running;
            //If directions are different we use the brake acceleration
            } else{
                movement.vel.x += movement.brake_acc * time.delta_seconds() * orders.horiz_dir;
                next_action = CharacterActions::Running;
            }
        }
        //Clamp velocity
        movement.vel.x = movement.vel.x.clamp(-movement.max_vel.x, movement.max_vel.x);
        
        //Change character action if needed
        if c_action.0 != next_action { c_action.0 = next_action; }

        if orders.horiz_dir == 1.0 {
            sprite.flip_x = false;
        }else if orders.horiz_dir == -1.0{
            sprite.flip_x = true;
        }
    }
}

fn apply_movement(
    time: Res<Time>,
    mut controller_query: Query<(&Movement, &mut KinematicCharacterController)>
){
    for (velocity, mut controller) in controller_query.iter_mut(){
        controller.translation = Some(velocity.vel * time.delta_seconds())
    }
}

fn stop_on_bump(
    mut move_query: Query<(&mut Movement, &KinematicCharacterControllerOutput)>
)
{
    for (mut movement, kin_output) in move_query.iter_mut(){
        if (kin_output.desired_translation.x - kin_output.effective_translation.x).abs() > 0.1{
            movement.vel.x = 0.0;
        }
        //Only check collisions while going up. There was a glitch if we use .abs() like we did in the x axis.
        //The character sometimes got stuck in a cliff upper corner
        if (kin_output.desired_translation.y - kin_output.effective_translation.y)> 0.1{
            movement.vel.y = -1.0;
            error!("v_bump");
        }
    }
}

fn check_ground(
    mut command: Commands,
    mut ground_query: Query<(Entity, &mut Movement, &KinematicCharacterControllerOutput, Option<&OnGround>, Option<&OnAir>)>
)
{
    for (char_id, mut movement, output, op_ground, op_air) in ground_query.iter_mut(){
        if let Some(_) = op_ground{
            if !output.grounded { 
                command.entity(char_id).insert(OnAir).remove::<OnGround>(); 
            }
        }
        if let Some(_) = op_air {
            if output.grounded && movement.vel.y < 0.0 {
                error!("Grounded"); 
                movement.vel.y = -1.0;
                command.entity(char_id).insert(OnGround).remove::<OnAir>(); 
            }
        }
    }
}

fn debug_collisions(
    mut gizmos: Gizmos,
    mut controller_outputs: Query<(&KinematicCharacterControllerOutput, &GlobalTransform)>
) {
    for (output, transform) in controller_outputs.iter_mut() {
        for collision in &output.collisions {
            // Collision in map space
            let contact_point = collision.toi.witness1;
            let normal = collision.toi.normal1;
            gizmos.line_2d(contact_point, contact_point + normal*10.0, Color::RED);
            // Collision in character space
            let contact_point = collision.toi.witness2 + transform.translation().truncate();
            let normal = collision.toi.normal2;
            gizmos.line_2d(contact_point, contact_point + normal*10.0, Color::GREEN);
        }
    }
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


#[derive(Bundle)]
pub struct CharacterBundle{
    ssheet: SpriteSheetBundle,
    body: RigidBody,
    collision: Collider,
    controller: KinematicCharacterController,
    movement: Movement,
    action: CurrentAction,
    animations: ActionAnimations,
    grounded: OnGround,
    orders: InputOrders
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
        animations.map.insert(CharacterActions::Jumping, CompAnimation::new_segment(8, 8, 1.0, 0));
        animations.map.insert(CharacterActions::Falling, CompAnimation::new_segment(9, 9, 1.0, 0));
        

        Self { 
            ssheet,
            body: RigidBody::KinematicPositionBased, 
            //collision: Collider::capsule(Vec2::NEG_Y, Vec2::Y, 3.0),
            collision: Collider::cuboid(3.0, 4.0), 
            controller: KinematicCharacterController{offset:CharacterLength::Absolute(0.3), ..default()},
            movement: PLAYER_MOV,
            action: CurrentAction(CharacterActions::Idle),
            animations,
            grounded: OnGround,
            orders: InputOrders::default()
        }
    }
}