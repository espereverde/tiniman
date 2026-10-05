//#![cfg_attr(debug_assertions, allow(dead_code, unused_imports))]

use std::hash::Hash;

use bevy::ecs::query;
use bevy::transform::commands;
use bevy::{prelude::*, utils::HashMap, transform::TransformSystem};
use bevy_rapier2d::prelude::*;

use crate::asset_manager::TextureAssets;
use crate::animation::FrameAnimation;
use crate::weapons::{Weapon, BulletBundle};

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
            .register_type::<MoveStyle>()

            .configure_sets(Update, (CharacterSet::PrepareMovement, CharacterSet::ApplyMovement).chain())
            .configure_sets(PostUpdate, (CharacterSet::UpdateMode,CharacterSet::SyncSprite)
                .chain()
                .after(PhysicsSet::Writeback)
                .before(TransformSystem::TransformPropagate)
            )

            .add_systems(PreUpdate, (process_player_input, update_action_elapsed, update_shoot_elapsed,))
            .add_systems(Update, (
                update_player_air_movement,
                update_player_ground_movement,
            ).in_set(CharacterSet::PrepareMovement))
            .add_systems( Update, (apply_movement, try_to_shoot).chain().in_set(CharacterSet::ApplyMovement))

            .add_systems(PostUpdate, (stop_on_bump, check_ground,).in_set(CharacterSet::UpdateMode))
            .add_systems(PostUpdate, (sync_action_animation, sync_sprite_face_dir, sync_gun_gesture).in_set(CharacterSet::SyncSprite))
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
    RunStart,
    Running,
    RunEnd,
    Sliding,
    Jumping,
    Falling
}

#[derive(Debug, Component, Default)]
pub struct ShootGesture{
    active:bool
}

#[derive(Debug, Component, Default)]
struct ShootElapsed(f32);

#[derive(Debug, Component, Default)]
struct ActionElapsed(f32);

#[derive(Debug, Component)]
pub struct ActionAnimations{
    map: HashMap<CharacterAction, FrameAnimation>
}

#[derive(Debug, Component, Reflect, Default)]
struct InputOrders{
    horiz_dir: f32,
    do_slide: bool,
    do_jump: bool,
    keep_jump: bool,
    do_shoot: bool
}

#[derive(Debug, Component, Default, Clone)]
pub struct CharVelocity{
    pub x: f32,
    y: f32
}

#[derive(Debug, Component, Reflect)]
pub struct MoveStyle{
    walk_speed: f32,
    start_delay: f32,
    brake_delay: f32,
    brake_speed: f32,
    slide_speed: f32,
    slide_duration: f32,
    jump_impulse: f32,
    jump_h_speed: f32,
    gravity_acc: f32,
    terminal_vel: f32   
}

const TINYMAN_MOV: MoveStyle = MoveStyle{
    walk_speed: 41.3,
    start_delay: 0.1,
    brake_delay: 0.1,
    brake_speed: 0.0,
    slide_speed: 82.6,
    slide_duration: 0.333,
    jump_impulse: 146.0,
    jump_h_speed: 41.3,
    gravity_acc: 450.0,
    terminal_vel: 210.0
};

fn process_player_input(
    keyboard: Res<Input<KeyCode>>,
    mut input_query: Query<&mut InputOrders, With<Player>>
){
    //error!("Frame");
    if let Ok(mut player_input) = input_query.get_single_mut(){
        //Calculate desired input direction +1 for right, -1 for left. 
        //If both keys are pressed they cancel out.
        player_input.horiz_dir = 0.0;
        if keyboard.pressed(KeyCode::Right) { player_input.horiz_dir += 1.0; }
        if keyboard.pressed(KeyCode::Left) { player_input.horiz_dir -= 1.0; }
        //Store jump key press state
        player_input.keep_jump = keyboard.pressed(KeyCode::Z);
        if keyboard.just_pressed(KeyCode::Z){
            if keyboard.pressed(KeyCode::Down){
                player_input.do_slide = true;
                player_input.do_jump = false;
            }
            else{
                player_input.do_jump = true;
                player_input.do_slide = false;
            }
        }else{
            player_input.do_slide = false;
            player_input.do_jump = false;
        }
        player_input.do_shoot = keyboard.just_pressed(KeyCode::X);  
    }
}


fn update_player_air_movement(
    time: Res<Time>,
    mut move_query: Query<(&InputOrders, &MoveStyle, &mut CharVelocity, &mut CharacterAction, &mut Facing), With<OnAir>>
)
{
    for (orders, movement, mut velocity, mut c_action , mut face_dir) in move_query.iter_mut(){
            
            let mut next_action = c_action.clone();

            if velocity.y > 30.0 && !orders.keep_jump { velocity.y = 30.0}
            else{
                velocity.y -= movement.gravity_acc*time.delta_seconds();
                velocity.y = velocity.y.clamp(-movement.terminal_vel, movement.terminal_vel);
            }

            if velocity.y > 0.0 { next_action = CharacterAction::Jumping}
            else if velocity.y < 0.0 {next_action = CharacterAction::Jumping}

            velocity.x = orders.horiz_dir * movement.walk_speed;
            if orders.horiz_dir == 1.0 {
                face_dir.set_dir(Facing::Right);   
            }
            else if orders.horiz_dir == -1.0{
                face_dir.set_dir(Facing::Left);
            }

            //Change character action if needed
            if *c_action != next_action { *c_action = next_action; }
    }
        
}

// System that controls the Player character from parses player input.
/// In reads keypresses and modifies the player Entity's Speed component.
fn update_player_ground_movement(
    mut player_query: Query<(&InputOrders, &mut CharacterAction, &ActionElapsed, &MoveStyle, &mut CharVelocity, &mut Facing), (With<Player>, With<OnGround>)>
){
    if let Some((orders, mut c_action, a_elapsed, movement, mut velocity, mut face_dir)) = player_query.iter_mut().next() {
        
        let mut next_action = c_action.clone();

        let vel_dir = 
                if velocity.x == 0.0 { 
                    match *face_dir {
                        Facing::Left => -1.0,
                        Facing::Right => 1.0
                    }
                 }
                else { velocity.x.signum()};

        if orders.do_jump{
            *c_action = CharacterAction::Jumping;
            velocity.x = movement.jump_h_speed * orders.horiz_dir;
            velocity.y = movement.jump_impulse;
            if orders.horiz_dir == 1.0 {
                face_dir.set_dir(Facing::Right);   
            }
            else if orders.horiz_dir == -1.0{
                face_dir.set_dir(Facing::Left);
            }
            return;
        }

        velocity.y = -1.0;
        match *c_action {
            CharacterAction::Jumping =>{
                if orders.horiz_dir == 1.0 {
                    face_dir.set_dir(Facing::Right);
                    velocity.x = movement.walk_speed * orders.horiz_dir;
                    next_action = CharacterAction::Running;
                }
                else if orders.horiz_dir == -1.0{
                    face_dir.set_dir(Facing::Left);
                    velocity.x = movement.walk_speed * orders.horiz_dir;
                    next_action = CharacterAction::Running;
                }
                else{
                    velocity.x = 0.0;
                    next_action = CharacterAction::Idle;

                }
            }
            CharacterAction::Idle =>{
                if orders.do_slide{
                    velocity.x = movement.slide_speed * vel_dir;
                    next_action = CharacterAction::Sliding;
                }
                else if orders.horiz_dir == 1.0 {
                    face_dir.set_dir(Facing::Right);
                    velocity.x = 60.0 * orders.horiz_dir;
                    next_action = CharacterAction::RunStart;
                }
                else if orders.horiz_dir == -1.0{
                    face_dir.set_dir(Facing::Left);
                    velocity.x = 60.0 * orders.horiz_dir;
                    next_action = CharacterAction::RunStart;
                }
            },
            CharacterAction::RunStart =>{
                if orders.do_slide{
                    velocity.x = movement.slide_speed * vel_dir;
                    next_action = CharacterAction::Sliding;
                }
                else if orders.horiz_dir == 0.0{
                    velocity.x = 0.0;
                    next_action = CharacterAction::Idle;
                }
                else {
                    if a_elapsed.0 <= movement.start_delay{
                        velocity.x = 0.0;
                        error!("asdf");
                    }
                    else{
                        velocity.x = movement.walk_speed * orders.horiz_dir;
                        if orders.horiz_dir == 1.0 {
                            face_dir.set_dir(Facing::Right);
                        }
                        else{ face_dir.set_dir(Facing::Left); }
                        next_action = CharacterAction::Running
                    }
                }
            },
            CharacterAction::Running =>{
                if orders.do_slide{
                    velocity.x = movement.slide_speed * vel_dir;
                    next_action = CharacterAction::Sliding;
                }
                else if orders.horiz_dir == vel_dir{
                    velocity.x = movement.walk_speed * orders.horiz_dir;
                }else{
                    velocity.x = 0.0;
                    next_action = CharacterAction::RunEnd;
                }
            },
            CharacterAction::RunEnd =>{
                if orders.do_slide{
                    velocity.x = movement.slide_speed * vel_dir;
                    next_action = CharacterAction::Sliding;
                }
                else if orders.horiz_dir == 0.0{
                    if a_elapsed.0 <= movement.brake_delay{ 
                        velocity.x = 0.0;
                    }else{
                        velocity.x = 0.0;
                        next_action = CharacterAction::Idle;
                    }
                }else if orders.horiz_dir == vel_dir{
                    velocity.x = 60.0 * orders.horiz_dir;
                    next_action = CharacterAction::RunStart;
                }else{
                    if a_elapsed.0 <= movement.brake_delay{ 
                        velocity.x = 0.0;
                    }
                    else{
                        velocity.x = movement.walk_speed * orders.horiz_dir;
                        if orders.horiz_dir == 1.0 {
                            face_dir.set_dir(Facing::Right);
                        }
                        else{ face_dir.set_dir(Facing::Left); }
                        next_action = CharacterAction::Running;
                    }
                }
            },
            CharacterAction::Sliding => {
                if a_elapsed.0 > movement.slide_duration{
                    if orders.horiz_dir == 1.0 {
                        face_dir.set_dir(Facing::Right);
                        velocity.x = movement.walk_speed * orders.horiz_dir;
                        next_action = CharacterAction::Running;
                    }
                    else if orders.horiz_dir == -1.0{
                        face_dir.set_dir(Facing::Left);
                        velocity.x = movement.walk_speed * orders.horiz_dir;
                        next_action = CharacterAction::Running;
                    }
                    else{
                        velocity.x = 0.0;
                        next_action = CharacterAction::Idle;
                    }
                }
            },
            _ => {}

        }

        //Change character action if needed
        if *c_action != next_action { *c_action = next_action; }
    }
}

fn try_to_shoot(
    mut command: Commands,
    query: Query<(Entity, &InputOrders, &Transform, &Facing,  &mut Weapon)>
){
    for (e_id, orders, transform,facing, mut weapon) in query.iter(){
        if orders.do_shoot{
            let face_dir = match facing {
                Facing::Left => -1.0,
                Facing::Right => 1.0
            };
            if weapon.try_shoot(&mut command, transform, face_dir){
                command.entity(e_id)
                    .insert(ShootGesture{active:true})
                    .insert(ShootElapsed(0.0));
            }
        }
    }
}

fn apply_movement(
    time: Res<Time>,
    mut controller_query: Query<(&CharVelocity, &mut KinematicCharacterController)>
){
    for (velocity, mut controller) in controller_query.iter_mut(){
        controller.translation = Some(Vec2::new(velocity.x, velocity.y) * time.delta_seconds());
    }
}

fn stop_on_bump(
    mut move_query: Query<(&mut CharVelocity, &KinematicCharacterControllerOutput)>
)
{
    for (mut velocity, kin_output) in move_query.iter_mut(){
        //Only check collisions while going up. There was a glitch if we use .abs() like we did in the x axis.
        //The character sometimes got stuck in a cliff upper corner
        if (kin_output.desired_translation.y - kin_output.effective_translation.y)> 0.1{
            velocity.y = -4.0;
            error!("v_bump");
        }
    }
}

fn check_ground(
    mut command: Commands,
    mut ground_query: Query<(Entity, &mut CharVelocity, &KinematicCharacterControllerOutput, Option<&OnGround>, Option<&OnAir>)>
)
{
    for (char_id, mut velocity, output, op_ground, op_air) in ground_query.iter_mut(){
        if let Some(_) = op_ground{
            if !output.grounded || velocity.y > 0.0{ 
                error!("OnAir");
                command.entity(char_id).insert(OnAir).remove::<OnGround>(); 
            }
        }
        if let Some(_) = op_air {
            if output.grounded && velocity.y < 0.0 {
                error!("Grounded"); 
                velocity.y = -1.0;
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
            if let Some(col_details) = collision.toi.details{ 
                // Collision in map space
                let contact_point = col_details.witness1;
                let normal = col_details.normal1;
                gizmos.line_2d(contact_point, contact_point + normal*10.0, Color::RED);
                // Collision in character space
                let contact_point = col_details.witness2 + transform.translation().truncate();
                let normal = col_details.normal2;
                gizmos.line_2d(contact_point, contact_point + normal*10.0, Color::GREEN);
                }
        }
    }
}

fn update_action_elapsed(
    time: Res<Time>,
    mut el_query: Query<&mut ActionElapsed>
){
    for mut elapsed in el_query.iter_mut(){
        elapsed.0 += time.delta_seconds();
    }
}

fn update_shoot_elapsed(
    time: Res<Time>,
    mut el_query: Query<(&mut ShootGesture, &mut ShootElapsed)>
){
    for (mut gesture, mut elapsed) in el_query.iter_mut(){
        if gesture.active{
            elapsed.0 += time.delta_seconds();
            if elapsed.0 > 0.25{
                elapsed.0 = 0.0;
                gesture.active = false;
            }
        }
    }
}

fn sync_action_animation(
    mut commands: Commands,
    mut anim_query: Query<(Entity, &CharacterAction, &ActionAnimations, &mut ActionElapsed, Option<&ShootGesture>), Changed<CharacterAction>>
)
{
    for (entity_id, action, animation_map, mut elapsed, opt_shooted) in anim_query.iter_mut(){
        error!("sync {:?}", action);
        elapsed.0 = 0.0;
        let has_shooted = match opt_shooted{
            Some(v) => v.active,
            None => false
        };
        if let Some(anim) = animation_map.map.get(action){
            let mut anim = anim.clone();
            if has_shooted{ anim.set_alternate("gun") }
            commands.entity(entity_id).insert(anim);
        }
    }
}

fn sync_gun_gesture(
    mut anim_query: Query<(&mut FrameAnimation, &ShootGesture), Changed<ShootGesture>>
)
{
    for (mut animation, gesture) in anim_query.iter_mut(){
        error!("Gesture {:?}", gesture.active);
        if gesture.active{ animation.set_alternate("gun");}
        else{ animation.default_alter(); }
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
pub struct TinyCharBundle{
    ssheet: SpriteSheetBundle,
    body: RigidBody,
    collision: Collider,
    controller: KinematicCharacterController,
    movement: MoveStyle,
    velocity: CharVelocity,
    action: CharacterAction,
    a_elapsed: ActionElapsed,
    facing: Facing,
    animations: ActionAnimations,
    grounded: OnGround,
    orders: InputOrders,
    weapon: Weapon
}

impl TinyCharBundle{
    pub fn new(name: &str, assets: &Res<TextureAssets>) -> Self{
        let atlas_field_name  = format!("{name}");
        let atlas_field = assets.field(&atlas_field_name).unwrap();
        let atlas_handle = atlas_field.downcast_ref::<Handle<TextureAtlas>>().unwrap();
        
        let ssheet = SpriteSheetBundle{
            texture_atlas: atlas_handle.clone(),
            //sprite: TextureAtlasSprite{anchor:bevy::sprite::Anchor::BottomCenter, ..default()},
            ..default()
        };

        let mut animations = ActionAnimations{ map: HashMap::new() };
        animations.map.insert(
            CharacterAction::Idle, 
            FrameAnimation::new(0, 2, 1.0, 0)
                .with_alternate("gun", 20)
        );
        animations.map.insert(
            CharacterAction::RunStart,
            FrameAnimation::new(4, 1, 1.0, 0)
                .with_alternate("gun", 20)
        );
        animations.map.insert(
            CharacterAction::RunEnd,
            FrameAnimation::new(4, 1, 1.0, 0)
                .with_alternate("gun", 20)
        );
        animations.map.insert(
            CharacterAction::Running,
            FrameAnimation::new(8, 4, 8.0, 0)
                .with_alternate("gun", 24)
        );
        animations.map.insert(
            CharacterAction::Sliding,
            FrameAnimation::new( 12, 1, 1.0, 0)
                .with_alternate("gun", 12)
        );
        animations.map.insert(
            CharacterAction::Jumping,
            FrameAnimation::new(16, 1, 1.0, 0)
                .with_alternate("gun", 28)
        );
        animations.map.insert(
            CharacterAction::Falling,
            FrameAnimation::new(8, 1, 1.0, 0)
                .with_alternate("gun", 28)
        );
        
        Self { 
            ssheet,
            body: RigidBody::KinematicPositionBased, 
            //collision: Collider::capsule(Vec2::NEG_Y, Vec2::Y, 3.0),
            collision: Collider::cuboid(4.0, 6.0), 
            controller: KinematicCharacterController{
                offset:CharacterLength::Absolute(0.2),
                
                 ..default()},
            movement: TINYMAN_MOV,
            velocity: CharVelocity::default(),
            facing: Facing::Right,
            action: CharacterAction::Idle,
            a_elapsed: ActionElapsed::default(),
            animations,
            grounded: OnGround,
            orders: InputOrders::default(),
            weapon: Weapon{
                    offset: Vec2 { x: 8.0, y: 1.0 },
                    bullets: BulletBundle::new("bullet", assets)
                }
        }
    }
}