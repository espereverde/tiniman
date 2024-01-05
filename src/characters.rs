#![cfg_attr(debug_assertions, allow(dead_code, unused_imports))]

use bevy::{prelude::*, utils::HashMap, transform::TransformSystem};
use bevy_rapier2d::prelude::*;

use crate::asset_manager::PlayerAssets;
use crate::animation::CompAnimation;

const GRAVITY: f32 = 700.0;

#[derive(Debug, PartialEq, Eq, Hash, Clone, SystemSet)]
pub enum CharacterSet{
    PrepareMovement,
    ApplyMovement,
    UpdateMode,
    SyncSprite
}

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_type::<Movement>()

            .configure_sets(Update, (CharacterSet::PrepareMovement, CharacterSet::ApplyMovement).chain())
            .configure_sets(PostUpdate, (CharacterSet::UpdateMode,CharacterSet::SyncSprite)
                .chain()
                .after(PhysicsSet::Writeback)
                .before(TransformSystem::TransformPropagate)
            )

            .add_systems(PreUpdate, process_player_input)
            .add_systems(Update, (
                update_air_movement,
                update_ground_movement,
                update_wall_movement,
            ).in_set(CharacterSet::PrepareMovement))
            .add_systems( Update, apply_movement.in_set(CharacterSet::ApplyMovement))

            .add_systems(PostUpdate, (check_into_wall, check_outo_wall, check_ground, stop_on_bump).in_set(CharacterSet::UpdateMode))
            .add_systems(PostUpdate, (sync_action_animation, sync_sprite_face_dir, /* debug_collisions */).in_set(CharacterSet::SyncSprite))
        ;
    }
}

#[derive(Debug, Component)]
struct OnGround;

#[derive(Debug, Component)]
struct OnAir;

#[derive(Debug, Component)]
struct OnWall;

#[derive(Debug, Component)]
pub struct Player();

#[derive(Debug, PartialEq, Eq, Component)]
pub enum Facing{
    Left,
    Right
}

impl Facing{
    fn set_dir(&mut self, new_dir: Facing){
        if *self != new_dir{
            *self = new_dir;
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Component)]
pub enum CharacterAction{
    Idle,
    Running,
    Jumping,
    Falling,
    Sliding
}


/* #[derive(Debug, Component)]
pub struct CurrentAction(CharacterAction); */

#[derive(Debug, Component)]
pub struct ActionAnimations{
    map: HashMap<CharacterAction, CompAnimation>
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
    max_vel: Vec2 { x: 80., y: 200. },
    forward_acc: 320.0,
    glide_acc: 250.0,
    brake_acc: 350.0
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
    mut move_query: Query<(&InputOrders, &mut Movement, &mut CharacterAction), With<OnAir>>
)
{
    for (orders, mut movement, mut c_action ) in move_query.iter_mut(){
            let mut next_action = c_action.clone();

            if movement.vel.y > 0.0 && !orders.keep_jump {
                movement.vel.y -= GRAVITY * time.delta_seconds();
            }
            else{
                movement.vel.y = (movement.vel.y - GRAVITY * time.delta_seconds()/2.0).clamp(-movement.max_vel.y, movement.max_vel.y);
            }

            if movement.vel.y > 0.0 { next_action = CharacterAction::Jumping}
            else if movement.vel.y < 0.0 {next_action = CharacterAction::Falling}

            //Acceleration on air is half the acceleration on ground
            movement.vel.x += (movement.forward_acc / 2.0) * time.delta_seconds() * orders.horiz_dir;
            //Clamp velocity
            movement.vel.x = movement.vel.x.clamp(-movement.max_vel.x, movement.max_vel.x);

            //Change character action if needed
            if *c_action != next_action { *c_action = next_action; }
    }
        
}

fn update_wall_movement(
    mut commands: Commands,
    time: Res<Time>,
    mut player_query: Query<(Entity, &InputOrders, &Facing, &mut CharacterAction, &mut Movement), (With<Player>, With<OnWall>)>
){
    for (char_id, orders, face_dir, mut action, mut movement) in player_query.iter_mut(){
        if orders.do_jump{
            if *face_dir == Facing::Right{
                movement.vel.x = 40.0;}
            else{
                movement.vel.x = -40.0;
            }
            movement.vel.y = 120.0;
            commands.entity(char_id).remove::<OnWall>().insert(OnAir);
        }
        else{
            movement.vel.x = 0.0;
            if movement.vel.y < 0.0{
                movement.vel.y = (movement.vel.y - GRAVITY * time.delta_seconds()/4.0).clamp(-movement.max_vel.y/4.0, movement.max_vel.y);
            }
            else{
                if orders.keep_jump{ movement.vel.y = (movement.vel.y - GRAVITY * time.delta_seconds()/2.0).clamp(-movement.max_vel.y/4.0, movement.max_vel.y); }
                else{ movement.vel.y = (movement.vel.y - GRAVITY * time.delta_seconds()).clamp(-movement.max_vel.y/4.0, movement.max_vel.y); }
            }

            //Change character action if needed
            if *action != CharacterAction::Sliding { *action = CharacterAction::Sliding; }
        }
    }
}

// System that controls the Player character from parses player input.
/// In reads keypresses and modifies the player Entity's Speed component.
fn update_ground_movement(
    time: Res<Time>,
    mut player_query: Query<(&InputOrders, &mut CharacterAction, &mut Movement, &mut Facing), (With<Player>, With<OnGround>)>
) {
    if let Some((orders, mut c_action, mut movement, mut face_dir)) = player_query.iter_mut().next() {
        
        let mut next_action = c_action.clone();

        if orders.do_jump {movement.vel.y = 160.0;}
        else{
            //movement.vel.y = -0.1;
            // Get the current velocity direction. If character is not moving we take the input direction.
            let vel_dir = 
                if movement.vel.x == 0.0 { orders.horiz_dir }
                else { movement.vel.x.signum()};

            if orders.horiz_dir == 0.0 && movement.vel.x.abs() < 1.0 { 
                movement.vel.x = 0.0;
                next_action = CharacterAction::Idle;
            }
            //If no input direction the character is gliding and we slowly reduce its velocity
            else if orders.horiz_dir == 0.0 {
                let target_velx = movement.vel.x - movement.glide_acc * time.delta_seconds() * vel_dir;
                // If move direction after gliding is different than the oribinal move direction we stop
                if movement.vel.x * target_velx <= 0.0{
                    movement.vel.x = 0.0;
                    next_action = CharacterAction::Idle
                }
                else{
                    movement.vel.x = target_velx;
                    next_action = CharacterAction::Running;
                }
            }//If both directions coincide we use the forward acceleration
            else if orders.horiz_dir == vel_dir {
                movement.vel.x += movement.forward_acc * time.delta_seconds() * orders.horiz_dir;
                next_action = CharacterAction::Running;
            //If directions are different we use the brake acceleration
            } else{
                movement.vel.x += movement.brake_acc * time.delta_seconds() * orders.horiz_dir;
                next_action = CharacterAction::Running;
            }
        }
        //Clamp velocity
        movement.vel.x = movement.vel.x.clamp(-movement.max_vel.x, movement.max_vel.x);
        
        //Change character action if needed
        if *c_action != next_action { *c_action = next_action; }

        if orders.horiz_dir == 1.0 {
            face_dir.set_dir(Facing::Right)
        }else if orders.horiz_dir == -1.0{
            face_dir.set_dir(Facing::Left)
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
            movement.vel.x = 0.1 * kin_output.desired_translation.x.signum();
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

fn check_into_wall(
    mut commands: Commands, 
    mut wall_query: Query<(Entity, &Movement, &KinematicCharacterControllerOutput, &mut Facing, &mut CharacterAction),
        With<OnAir>>
){
    for (char_id, movement, output,
         mut face_dir, mut action
    ) in wall_query.iter_mut(){
        for collision in output.collisions.iter(){
            if output.grounded && movement.vel.y < 0.0{ 
                commands.entity(char_id).insert(OnGround).remove::<OnAir>().remove::<OnWall>(); 
            }
            // Collision on the right side
            if collision.toi.normal2.x > 0.8 && movement.vel.x > 0.0{
                commands.entity(char_id).insert(OnWall).remove::<OnAir>();
                face_dir.set_dir(Facing::Left);
                *action = CharacterAction::Sliding;
                error!("start slide right");
            // Collision on the left side 
            }else if collision.toi.normal2.x < -0.8 && movement.vel.x < 0.0{
                commands.entity(char_id).insert(OnWall).remove::<OnAir>(); 
                face_dir.set_dir(Facing::Right);
                *action = CharacterAction::Sliding;
                error!("start slide");
            }
        }
    }
}

fn check_outo_wall(
    mut commands: Commands, 
    rapier_context: Res<RapierContext>,
    mut wall_query: Query<(Entity, &Movement, &GlobalTransform, &KinematicCharacterControllerOutput, &Facing, &mut CharacterAction),
        With<OnWall>>
){
    for (char_id, movement, g_transform, output,
         mut face_dir, mut action
    ) in wall_query.iter_mut(){
        
        let char_pos = g_transform.translation().truncate();
        if output.grounded && movement.vel.y < 0.0{ 
            commands.entity(char_id).insert(OnGround).remove::<OnWall>(); 
        }

        let ray_dir = match face_dir{
            Facing::Right => Vec2::NEG_X,
            Facing::Left => Vec2::X
        };
                
        if let None = rapier_context.cast_ray(char_pos, ray_dir, 3.5, true, QueryFilter::default().exclude_rigid_body(char_id)){
            commands.entity(char_id).insert(OnAir).remove::<OnWall>();
            //*action = CharacterAction::Sliding;
            error!("end slide");
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
    anim_query: Query<(Entity, &CharacterAction, &ActionAnimations), Changed<CharacterAction>>
)
{
    for (entity_id, action, animation_map) in anim_query.iter(){
        error!("{:?}", action);
        if let Some(anim) = animation_map.map.get(action){
            commands.entity(entity_id).insert(anim.clone());
        }
    }
}

fn sync_sprite_face_dir(
    mut query: Query<(&Facing, &mut TextureAtlasSprite), Changed<Facing>>
)
{
    for (face_dir, mut sprite) in query.iter_mut(){
        sprite.flip_x = *face_dir == Facing::Left;
    }
}


#[derive(Bundle)]
pub struct CharacterBundle{
    ssheet: SpriteSheetBundle,
    body: RigidBody,
    collision: Collider,
    controller: KinematicCharacterController,
    movement: Movement,
    action: CharacterAction,
    facing: Facing,
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
        animations.map.insert(CharacterAction::Idle, CompAnimation::new_segment(0, 2, 2.0, 0));
        animations.map.insert(CharacterAction::Running, CompAnimation::new_segment(4, 7, 8.0, 0));
        animations.map.insert(CharacterAction::Jumping, CompAnimation::new_segment(8, 8, 1.0, 0));
        animations.map.insert(CharacterAction::Falling, CompAnimation::new_segment(9, 9, 1.0, 0));
        animations.map.insert(CharacterAction::Sliding, CompAnimation::new_segment( 16, 16, 1.0, 0));
        

        Self { 
            ssheet,
            body: RigidBody::KinematicPositionBased, 
            //collision: Collider::capsule(Vec2::NEG_Y, Vec2::Y, 3.0),
            collision: Collider::cuboid(3.0, 4.0), 
            controller: KinematicCharacterController{
                offset:CharacterLength::Absolute(0.4),
                
                 ..default()},
            movement: PLAYER_MOV,
            facing: Facing::Right,
            action: CharacterAction::Idle,
            animations,
            grounded: OnGround,
            orders: InputOrders::default()
        }
    }
}