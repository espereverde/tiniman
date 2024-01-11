#![cfg_attr(debug_assertions, allow(dead_code, unused_imports))]

use bevy::{
    prelude::*, 
    sprite::Anchor,
    transform::{TransformSystem, components}, utils::HashMap
};


use std::time::Duration;

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin{
    fn build(&self, app: &mut App){
        app
            .add_event::<AnimationEnded>()
            .add_systems(PostUpdate, (advance_animation).after(TransformSystem::TransformPropagate));
    }

}

#[derive(Component, Clone, Debug)]
pub struct FrameAnimation{
    start: usize,
    frame_count: usize,
    timer: Timer,
    repeats: usize,
    alternates:HashMap<String, usize>,
    index: usize
}

impl FrameAnimation{
    pub fn new(start: usize, frame_count: usize, fps: f32, repeats: usize)-> Self{
        let mut alternates = HashMap::new();
        alternates.insert("default".to_string(), start);
        Self { start,
            frame_count: frame_count,
            timer: Timer::from_seconds(1.0/fps, TimerMode::Repeating),
            repeats: repeats, 
            alternates,
            index: 0}
    }

    pub fn with_alternate(mut self, name: &str, alt_start: usize) -> Self{
        self.alternates.insert(name.to_string(), alt_start);
        return self
    }

    pub fn set_fps(&mut self, new_fps: f32){
        self.timer.set_duration(Duration::from_secs_f32(1.0/new_fps));
    }

    pub fn default_alter(&mut self){
        self.set_alternate("default");
    }

    pub fn set_alternate(&mut self, alt_name: &str){
        self.alternates.get(alt_name)
            .and_then(|&v| {self.start = v; Some(v)})
            .or_else(|| {error!("Could not find alternate anim '{:}'", alt_name); None});
    }

    fn advance_frame(&mut self, spritesheet: &mut TextureAtlasSprite, dt: Duration)-> bool{
 
        let mut finished = false;
        self.timer.tick(dt);
        
        if self.timer.just_finished(){
            self.index += 1;
            // We have reached the end of the animation
            if self.index >= self.frame_count{
                finished = Self::update_loop_count(&mut self.repeats);
                self.index = 0;
            }
            // If the animation is not finished
            // update the sprite index
            //if !finished {spritesheet.index = self.start + self.index;}
        }
        spritesheet.index = self.start + self.index;
        finished
            
    }

    fn update_loop_count(repeats: &mut usize) -> bool{
        
        // We just did our last animation loop
        if *repeats == 1{
            true
        }
        // We still have some more loops to do
        else if *repeats > 1{
            *repeats -=1;
            false
        }
        // This animation repeats forever
        else { // if repeats == 0{
            false
        }
    }
}


#[derive(Component, Clone, Debug)]
pub enum CompAnimation{
    Segment{
        start: usize,
        end: usize,
        timer: Timer,
        repeats: usize
    },
    Sequence{
        components: Vec<CompAnimation>,
        cindex: usize,
        repeats: usize
    }
}

impl CompAnimation{

    pub fn new_segment(start: usize, end: usize, fps: f32, repeats: usize) -> Self{
        Self::Segment{ start, end, repeats, timer: Timer::from_seconds(1.0/fps, TimerMode::Repeating) }
    }

    pub fn from_sequence(sequence: Vec<CompAnimation>, repeats: usize) -> Self{
        Self::Sequence { components: sequence, cindex: 0, repeats: repeats }
    }

    pub fn set_fps(&mut self, new_fps: f32){
        match self{
            Self::Segment { start:_, end:_, timer, repeats:_ } => 
                timer.set_duration(Duration::from_secs_f32(1.0/new_fps)),
            Self::Sequence { components, cindex:_, repeats:_ } =>
                {components.iter_mut().for_each(|comp| comp.set_fps(new_fps))},
        }
    }

    pub fn get_start(&self) -> usize{
        match self{
            Self::Segment { start, end:_, timer:_, repeats:_ } => *start,
            Self::Sequence { components, cindex:_, repeats:_} => components[0].get_start()
        }
    }

    pub fn get_end(&self) -> usize{
        match self{
            Self::Segment { start:_, end, timer:_, repeats:_ } => *end,
            Self::Sequence { components, cindex:_, repeats:_} => components[0].get_end()
        }
    }

    fn advance_frame(&mut self, spritesheet: &mut TextureAtlasSprite, dt: Duration)-> bool{
        match self{
            Self::Segment { start, end, timer, repeats } => {
                if spritesheet.index < *start || spritesheet.index > *end{
                    spritesheet.index = *start;
                } 
                let mut finished = false;
                timer.tick(dt);
                if timer.just_finished(){
                    let current_frame = spritesheet.index +1;
                    // We have reached the end of the animation
                    if current_frame > *end{
                        finished = Self::update_loop_count(repeats);
                        // If we are looping we set the index back at the start
                        if !finished {spritesheet.index=*start;}
                    }else{ spritesheet.index=current_frame }
                }
                finished
            },
            Self::Sequence { components, cindex, repeats }=>{
                let mut finished = false;
                let inner_finished = components[*cindex].advance_frame(spritesheet, dt);
                if inner_finished{
                    
                    *cindex +=1;
                    if *cindex >= components.len(){
                        finished = Self::update_loop_count(repeats);
                        // If we are looping we set the index back at the start
                        if !finished {
                            *cindex=0; 
                            spritesheet.index=self.get_start();};
                    }
                    
                }
                finished
            }
        }
    }


    fn update_loop_count(repeats: &mut usize) -> bool{
        
        // We just did our last animation loop
        if *repeats == 1{
            true
        }
        // We still have some more loops to do
        else if *repeats > 1{
            *repeats -=1;
            false
        }
        // This animation repeats forever
        else { // if repeats == 0{
            false
        }
    }
}

#[derive(Bundle, Clone)]
pub struct AnimatedSprite{
    spritesheet: SpriteSheetBundle,
    animation: CompAnimation
}

impl AnimatedSprite{
    pub fn new(sheet_handle: Handle<TextureAtlas>, animation: CompAnimation) -> Self {
        Self { 
            spritesheet: SpriteSheetBundle { 
                sprite: TextureAtlasSprite { index: animation.get_start(), ..Default::default()},
                texture_atlas: sheet_handle.clone(),
                ..Default::default()
            },
            animation
        }
    }

    pub fn with_frame(mut self, start_frame: usize) -> Self{
        self.spritesheet.sprite.index=start_frame;
        self
    }

    pub fn with_anchor(mut self, anchor: Anchor) -> Self{
        self.spritesheet.sprite.anchor = anchor;
        self
    }
}

#[derive(Event)]
pub struct AnimationEnded( pub Entity);

fn advance_animation(time: Res<Time>, mut ev_ended: EventWriter<AnimationEnded>, mut q_animated: Query<(Entity, &mut FrameAnimation, &mut TextureAtlasSprite)>){
    for (id, mut animation, mut sheet) in &mut q_animated.iter_mut() {
        if animation.advance_frame(&mut sheet, time.delta()){
            ev_ended.send(AnimationEnded(id));
        }
    }
}

/* fn setup_animation(
    mut query: Query<(&CompAnimation, &mut TextureAtlasSprite), Changed<CompAnimation>>
){
    for (animation, mut sprite) in query.iter_mut(){
        error!("asdf");
        if sprite.index < animation.get_start() || sprite.index > animation.get_end(){
            sprite.index = animation.get_start();
        }
    }
} */