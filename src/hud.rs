use bevy::prelude::*;

use crate::GameStates;
use crate::characters::Player;
use crate::combat::Health;
use crate::room::{Badges, BestBadges, Exit, RoomPhase, RoomRun};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_hud)
            .add_systems(Update, (update_status_line, update_banner).run_if(in_state(GameStates::Running)));
    }
}

#[derive(Component)]
struct StatusLine;

#[derive(Component)]
struct Banner;

/// The badge spans of the banner: 0 fast, 1 ghost, 2 clean.
#[derive(Component)]
struct BadgeSpan(usize);

#[derive(Component)]
struct BannerHint;

const EARNED: Color = Color::srgb(1.0, 0.85, 0.2);
const MISSED: Color = Color::srgb(0.35, 0.35, 0.4);
const LOCKED: Color = Color::srgb(1.0, 0.25, 0.3);

fn setup_hud(mut commands: Commands) {
    let small = TextFont { font_size: FontSize::Px(9.0), ..default() };
    commands.spawn((
        StatusLine,
        Text::new("Loading..."),
        small.clone(),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
    ));
    commands.spawn((
        Text::new("Arrows move   Z jump   Down+Z slide   X shoot   R restart   F1 debug"),
        small.clone(),
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(0.0),
            width: Val::Percent(100.0),
            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
    ));

    let big = TextFont { font_size: FontSize::Px(22.0), ..default() };
    commands
        .spawn((
            Banner,
            Text::new(""),
            big.clone(),
            TextLayout::justify(Justify::Center),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(35.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                padding: UiRect::vertical(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
            Visibility::Hidden,
        ))
        .with_children(|banner| {
            for i in 0..3 {
                banner.spawn((BadgeSpan(i), TextSpan::new(""), big.clone(), TextColor(MISSED)));
            }
            banner.spawn((BannerHint, TextSpan::new(""), small.clone(), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.7))));
        });
}

fn badge_marks(badges: Badges) -> String {
    let mark = |earned: bool, letter: char| if earned { letter } else { '-' };
    format!("{}{}{}", mark(badges.fast, 'F'), mark(badges.ghost, 'G'), mark(badges.clean, 'C'))
}

fn update_status_line(
    run: Res<RoomRun>,
    best: Res<BestBadges>,
    player: Query<&Health, With<Player>>,
    exits: Query<&Exit>,
    mut line: Single<(&mut Text, &mut TextColor), With<StatusLine>>,
) {
    let (hp, max) = player.single().map_or((0, 0), |health| (health.hp.max(0), health.max));
    let locked = exits.iter().any(|exit| exit.locked);
    let best = best.0.get(&run.name).copied().unwrap_or_default();
    let text = format!(
        "ROOM {}/{} {}   TIME {:.1}/{:.0}   ALERTS {}   HP {}{}   BEST {}{}",
        run.room + 1,
        run.room_count,
        run.name,
        run.time,
        run.par,
        run.alerts,
        "|".repeat(hp as usize),
        ".".repeat((max - hp).max(0) as usize),
        badge_marks(best),
        if locked { "   EXIT LOCKED" } else { "" },
    );
    let (text_line, color) = &mut *line;
    if text_line.0 != text {
        text_line.0 = text;
    }
    let wanted = if locked { LOCKED } else { Color::WHITE };
    if color.0 != wanted {
        color.0 = wanted;
    }
}

fn update_banner(
    run: Res<RoomRun>,
    mut banner: Single<(&mut Text, &mut Visibility), With<Banner>>,
    mut badges: Query<(&BadgeSpan, &mut TextSpan, &mut TextColor), Without<BannerHint>>,
    mut hint: Single<&mut TextSpan, (With<BannerHint>, Without<BadgeSpan>)>,
) {
    if !run.is_changed() {
        return;
    }
    let (title, earned, hint_text) = match run.phase {
        RoomPhase::Cleared { result, .. } => {
            let last = run.room + 1 == run.room_count;
            let title = format!("{}\n{:.2}s\n", if last { "FORTRESS CLEAR" } else { "ROOM CLEAR" }, result.time);
            (title, Some(result.badges), "\n\nZ/X next room    R retry")
        }
        RoomPhase::Failed { .. } => ("TRY AGAIN".to_string(), None, ""),
        _ => (String::new(), None, ""),
    };
    let (text, visibility) = &mut *banner;
    let shown = if title.is_empty() { Visibility::Hidden } else { Visibility::Inherited };
    visibility.set_if_neq(shown);
    if text.0 != title {
        text.0 = title;
    }
    if hint.0 != hint_text {
        hint.0 = hint_text.to_string();
    }
    for (badge, mut span, mut color) in &mut badges {
        let (label, got) = match (badge.0, earned) {
            (0, Some(b)) => (format!("FAST {:.0}s ", run.par), b.fast),
            (1, Some(b)) => ("GHOST ".to_string(), b.ghost),
            (2, Some(b)) => ("CLEAN".to_string(), b.clean),
            _ => (String::new(), false),
        };
        if span.0 != label {
            span.0 = label;
        }
        color.0 = if got { EARNED } else { MISSED };
    }
}
