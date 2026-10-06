use bevy::{platform::collections::HashMap, prelude::*, transform::TransformSystems};

use std::time::Duration;

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AnimationSet>()
            .add_message::<AnimationEnded>()
            .add_systems(
                PostUpdate,
                (play_current_clip, advance_animation)
                    .chain()
                    .after(TransformSystems::Propagate),
            );
    }
}

/// An animation as written in a blueprint. Turned into a [`FrameAnimation`] when played.
#[derive(Reflect, Clone, Debug, Default)]
#[reflect(Default)]
pub struct AnimClip {
    pub start: usize,
    pub frames: usize,
    pub fps: f32,
    /// 0 loops forever.
    pub repeats: usize,
    /// Variants of the clip starting at another frame (e.g. "gun" while shooting).
    pub alternates: HashMap<String, usize>,
}

impl AnimClip {
    pub fn to_animation(&self) -> FrameAnimation {
        let mut animation = FrameAnimation::new(self.start, self.frames, self.fps, self.repeats);
        for (name, start) in &self.alternates {
            animation = animation.with_alternate(name, *start);
        }
        animation
    }
}

/// Every clip an entity can play, by name.
#[derive(Component, Reflect, Clone, Debug, Default)]
#[reflect(Component, Default)]
#[require(CurrentClip)]
pub struct AnimationSet {
    pub clips: HashMap<String, AnimClip>,
}

/// The clip an entity is playing. Change it (with `set_if_neq`) to switch animations.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct CurrentClip {
    pub name: &'static str,
    pub alternate: Option<&'static str>,
}

impl Default for CurrentClip {
    fn default() -> Self {
        Self { name: "idle", alternate: None }
    }
}

impl CurrentClip {
    pub fn new(name: &'static str) -> Self {
        Self { name, alternate: None }
    }
}

fn play_current_clip(
    mut commands: Commands,
    query: Query<(Entity, &AnimationSet, &CurrentClip), Or<(Changed<CurrentClip>, Changed<AnimationSet>)>>,
) {
    for (entity, set, current) in &query {
        let Some(clip) = set.clips.get(current.name) else {
            warn!("No animation clip named '{}'", current.name);
            continue;
        };
        let mut animation = clip.to_animation();
        if let Some(alternate) = current.alternate {
            animation.set_alternate(alternate);
        }
        commands.entity(entity).insert(animation);
    }
}

#[derive(Component, Clone, Debug)]
pub struct FrameAnimation {
    start: usize,
    frame_count: usize,
    timer: Timer,
    repeats: usize,
    alternates: HashMap<String, usize>,
    index: usize,
}

impl FrameAnimation {
    pub fn new(start: usize, frame_count: usize, fps: f32, repeats: usize) -> Self {
        let mut alternates = HashMap::new();
        alternates.insert("default".to_string(), start);
        Self {
            start,
            frame_count,
            timer: Timer::from_seconds(1.0 / fps, TimerMode::Repeating),
            repeats,
            alternates,
            index: 0,
        }
    }

    pub fn with_alternate(mut self, name: &str, alt_start: usize) -> Self {
        self.alternates.insert(name.to_string(), alt_start);
        self
    }

    pub fn set_fps(&mut self, new_fps: f32) {
        self.timer.set_duration(Duration::from_secs_f32(1.0 / new_fps));
    }

    pub fn default_alter(&mut self) {
        self.set_alternate("default");
    }

    pub fn set_alternate(&mut self, alt_name: &str) {
        match self.alternates.get(alt_name) {
            Some(&start) => self.start = start,
            None => error!("Could not find alternate anim '{:}'", alt_name),
        }
    }

    fn advance_frame(&mut self, atlas: &mut TextureAtlas, dt: Duration) -> bool {
        let mut finished = false;
        self.timer.tick(dt);

        if self.timer.just_finished() {
            self.index += 1;
            // We have reached the end of the animation
            if self.index >= self.frame_count {
                finished = update_loop_count(&mut self.repeats);
                self.index = 0;
            }
        }
        atlas.index = self.start + self.index;
        finished
    }
}

fn update_loop_count(repeats: &mut usize) -> bool {
    // We just did our last animation loop
    if *repeats == 1 {
        true
    }
    // We still have some more loops to do
    else if *repeats > 1 {
        *repeats -= 1;
        false
    }
    // This animation repeats forever
    else {
        false
    }
}

#[derive(Component, Clone, Debug)]
pub enum CompAnimation {
    Segment { start: usize, end: usize, timer: Timer, repeats: usize },
    Sequence { components: Vec<CompAnimation>, cindex: usize, repeats: usize },
}

impl CompAnimation {
    pub fn new_segment(start: usize, end: usize, fps: f32, repeats: usize) -> Self {
        Self::Segment { start, end, repeats, timer: Timer::from_seconds(1.0 / fps, TimerMode::Repeating) }
    }

    pub fn from_sequence(sequence: Vec<CompAnimation>, repeats: usize) -> Self {
        Self::Sequence { components: sequence, cindex: 0, repeats }
    }

    pub fn set_fps(&mut self, new_fps: f32) {
        match self {
            Self::Segment { timer, .. } => timer.set_duration(Duration::from_secs_f32(1.0 / new_fps)),
            Self::Sequence { components, .. } => components.iter_mut().for_each(|comp| comp.set_fps(new_fps)),
        }
    }

    pub fn get_start(&self) -> usize {
        match self {
            Self::Segment { start, .. } => *start,
            Self::Sequence { components, .. } => components[0].get_start(),
        }
    }

    pub fn get_end(&self) -> usize {
        match self {
            Self::Segment { end, .. } => *end,
            Self::Sequence { components, .. } => components[0].get_end(),
        }
    }

    fn advance_frame(&mut self, atlas: &mut TextureAtlas, dt: Duration) -> bool {
        match self {
            Self::Segment { start, end, timer, repeats } => {
                if atlas.index < *start || atlas.index > *end {
                    atlas.index = *start;
                }
                let mut finished = false;
                timer.tick(dt);
                if timer.just_finished() {
                    let current_frame = atlas.index + 1;
                    // We have reached the end of the animation
                    if current_frame > *end {
                        finished = update_loop_count(repeats);
                        // If we are looping we set the index back at the start
                        if !finished {
                            atlas.index = *start;
                        }
                    } else {
                        atlas.index = current_frame
                    }
                }
                finished
            }
            Self::Sequence { components, cindex, repeats } => {
                let mut finished = false;
                let inner_finished = components[*cindex].advance_frame(atlas, dt);
                if inner_finished {
                    *cindex += 1;
                    if *cindex >= components.len() {
                        finished = update_loop_count(repeats);
                        // If we are looping we set the index back at the start
                        if !finished {
                            *cindex = 0;
                            atlas.index = components[0].get_start();
                        };
                    }
                }
                finished
            }
        }
    }
}

#[derive(Message)]
pub struct AnimationEnded(pub Entity);

fn advance_animation(
    time: Res<Time>,
    mut ev_ended: MessageWriter<AnimationEnded>,
    mut q_animated: Query<(Entity, &mut FrameAnimation, &mut Sprite)>,
) {
    for (id, mut animation, mut sprite) in q_animated.iter_mut() {
        let Some(atlas) = sprite.texture_atlas.as_mut() else { continue };
        if animation.advance_frame(atlas, time.delta()) {
            ev_ended.write(AnimationEnded(id));
        }
    }
}
