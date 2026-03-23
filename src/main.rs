use bevy::{audio::{PlaybackMode, Volume}, prelude::*, window::WindowResolution};
use std::f32::consts::TAU;

// ── Constants ────────────────────────────────────────────────────────────────

const GAME_DURATION: f32 = 30.0;

/// Radius of the fixed target ring at the center
const TARGET_RADIUS: f32 = 60.0;
const TARGET_THICKNESS: f32 = 4.0;

/// Pulses start at this radius and shrink toward TARGET_RADIUS
const PULSE_START_RADIUS: f32 = 260.0;

/// Shrink speed (px / sec)
const PULSE_SPEED: f32 = 220.0;

/// Spawn interval (sec)
const PULSE_INTERVAL: f32 = 1.2;

// Judgment windows (distance between pulse radius and TARGET_RADIUS)
const PERFECT_WINDOW: f32 = 10.0;
const GOOD_WINDOW: f32 = 26.0;

const MAX_COMBO_MULTIPLIER: u32 = 4;

/// Milestone combo count that triggers a ComboEvent (every N hits)
const COMBO_MILESTONE: u32 = 5;

// Game URL used in the share tweet (referenced only in WASM builds)
#[cfg(target_arch = "wasm32")]
const GAME_URL: &str = "https://czmirror.github.io/Pulse/";

// Colors
const BG_COLOR: Color = Color::srgb(0.04, 0.04, 0.08);
const TARGET_COLOR: Color = Color::srgb(0.35, 0.35, 0.55);
const PULSE_COLOR: Color = Color::srgb(0.45, 0.75, 1.0);
const PERFECT_COLOR: Color = Color::srgb(1.0, 0.92, 0.2);
const GOOD_COLOR: Color = Color::srgb(0.3, 1.0, 0.5);
const MISS_COLOR: Color = Color::srgb(1.0, 0.3, 0.3);

// Retry button colors
const RETRY_NORMAL: Color = Color::srgb(0.12, 0.12, 0.20);
const RETRY_HOVER: Color = Color::srgb(0.22, 0.22, 0.38);
const RETRY_PRESS: Color = Color::srgb(0.06, 0.06, 0.12);

// Share button colors (WASM only)
#[cfg(target_arch = "wasm32")]
const SHARE_NORMAL: Color = Color::srgb(0.05, 0.10, 0.22);
#[cfg(target_arch = "wasm32")]
const SHARE_HOVER: Color = Color::srgb(0.10, 0.22, 0.45);
#[cfg(target_arch = "wasm32")]
const SHARE_PRESS: Color = Color::srgb(0.02, 0.05, 0.12);

// Audio
const SAMPLE_RATE: u32 = 44100;

// ── States ───────────────────────────────────────────────────────────────────

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
enum AppState {
    #[default]
    Title,
    Playing,
    GameOver,
}

// ── Events ───────────────────────────────────────────────────────────────────

/// Fired on a successful hit (PERFECT or GOOD).
#[derive(Event)]
struct HitEvent {
    perfect: bool,
    /// Current combo count after this hit (used for pitch scaling)
    combo: u32,
}

/// Fired on a MISS (input too late, or pulse escaped without input).
#[derive(Event)]
struct MissEvent;

/// Fired when combo reaches a multiple of COMBO_MILESTONE.
#[derive(Event)]
struct ComboEvent {
    combo: u32,
}

// ── Resources ────────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct GameData {
    score: u32,
    combo: u32,
    best_combo: u32,
    time_left: f32,
    pulse_timer: f32,
    high_score: u32,
}

impl GameData {
    fn combo_multiplier(&self) -> u32 {
        (1 + self.combo / 5).min(MAX_COMBO_MULTIPLIER)
    }

    fn reset(&mut self) {
        let hs = self.high_score;
        *self = GameData {
            time_left: GAME_DURATION,
            high_score: hs,
            ..default()
        };
    }
}

/// Handles to the procedurally generated AudioSource assets.
#[derive(Resource)]
struct AudioHandles {
    hit_perfect: Handle<AudioSource>,
    hit_good: Handle<AudioSource>,
    miss: Handle<AudioSource>,
    combo: Handle<AudioSource>,
}

/// Global audio volume config (0.0 – 1.0).
#[derive(Resource)]
struct AudioConfig {
    volume: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self { volume: 0.6 }
    }
}

// ── Components ───────────────────────────────────────────────────────────────

#[derive(Component)]
struct Pulse {
    radius: f32,
    missed: bool,
}

#[derive(Component)]
struct JudgmentText {
    timer: f32,
}

#[derive(Component)]
struct ScoreText;

#[derive(Component)]
struct ComboText;

#[derive(Component)]
struct TimerText;

/// Marker for the root title-screen UI node
#[derive(Component)]
struct TitleScreen;

/// Marker for the root game-over UI node
#[derive(Component)]
struct GameOverScreen;

/// Marker for the in-game HUD root UI node
#[derive(Component)]
struct HudRoot;

#[derive(Component)]
struct TargetRing;

/// Retry button on the game-over screen
#[derive(Component)]
struct RetryButton;

/// Share-to-X button on the game-over screen (spawned only on WASM)
#[cfg(target_arch = "wasm32")]
#[derive(Component)]
struct ShareButton;

// ── App ──────────────────────────────────────────────────────────────────────

fn main() {
    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Pulse".into(),
                resolution: WindowResolution::new(480.0, 480.0),
                canvas: Some("#bevy-canvas".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        }),
    )
    .insert_resource(ClearColor(BG_COLOR))
    .init_resource::<GameData>()
    .init_resource::<AudioConfig>()
    .init_state::<AppState>()
    // Events
    .add_event::<HitEvent>()
    .add_event::<MissEvent>()
    .add_event::<ComboEvent>()
    // One persistent camera + audio for all states
    .add_systems(Startup, (setup_camera, setup_audio))
    // Title
    .add_systems(OnEnter(AppState::Title), setup_title)
    .add_systems(OnExit(AppState::Title), despawn_with::<TitleScreen>)
    .add_systems(Update, title_input.run_if(in_state(AppState::Title)))
    // Playing
    .add_systems(OnEnter(AppState::Playing), setup_game)
    .add_systems(OnExit(AppState::Playing), cleanup_game)
    .add_systems(
        Update,
        (
            tick_timer,
            spawn_pulses,
            move_pulses,
            handle_input,
            miss_check,
            update_hud,
            update_judgment_texts,
            // Audio: reads events sent by handle_input / miss_check above
            play_hit_sound,
            play_miss_sound,
            play_combo_sound,
        )
            .chain()
            .run_if(in_state(AppState::Playing)),
    )
    // GameOver
    .add_systems(OnEnter(AppState::GameOver), setup_game_over)
    .add_systems(OnExit(AppState::GameOver), despawn_with::<GameOverScreen>)
    .add_systems(
        Update,
        (game_over_input, retry_button_system).run_if(in_state(AppState::GameOver)),
    );

    // ShareButton is never spawned on native, so register this system only on WASM
    #[cfg(target_arch = "wasm32")]
    app.add_systems(
        Update,
        share_button_system.run_if(in_state(AppState::GameOver)),
    );

    app.run();
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn despawn_with<T: Component>(mut commands: Commands, q: Query<Entity, With<T>>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Build a thin annulus (ring) mesh.
fn annulus_mesh(radius: f32, thickness: f32) -> Mesh {
    let inner = (radius - thickness * 0.5).max(0.0);
    let outer = radius + thickness * 0.5;
    Annulus::new(inner, outer).into()
}

fn spawn_judgment_text(commands: &mut Commands, label: &str, color: Color) {
    commands.spawn((
        Text2d::new(label),
        TextFont {
            font_size: 40.0,
            ..default()
        },
        TextColor(color),
        Transform::from_xyz(0.0, 20.0, 10.0),
        JudgmentText { timer: 0.8 },
    ));
}

/// Spawn a styled UI button with a text label.
fn spawn_button<M: Component>(
    parent: &mut ChildBuilder,
    label: &str,
    bg: Color,
    border: Color,
    marker: M,
) {
    parent
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(Val::Px(22.0), Val::Px(11.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor(border),
            BorderRadius::all(Val::Px(8.0)),
            marker,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

// ── Procedural audio ──────────────────────────────────────────────────────────

/// Build a mono 16-bit PCM WAV from a sample generator `f(t, duration) -> [-1, 1]`.
fn build_wav(duration_secs: f32, generator: impl Fn(f32, f32) -> f32) -> AudioSource {
    let num_samples = (SAMPLE_RATE as f32 * duration_secs) as u32;
    let data_size = num_samples * 2; // 16-bit = 2 bytes per sample

    let mut bytes: Vec<u8> = Vec::with_capacity(44 + data_size as usize);

    // ── RIFF / WAVE header ──
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    // fmt chunk
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    // data chunk
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());

    for i in 0..num_samples {
        let t = i as f32 / SAMPLE_RATE as f32;
        let value = generator(t, duration_secs).clamp(-1.0, 1.0);
        let sample = (value * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    AudioSource { bytes: bytes.into() }
}

/// PERFECT hit: bright two-tone ding (880 Hz + 1320 Hz), 100 ms.
fn sound_hit_perfect() -> AudioSource {
    build_wav(0.10, |t, dur| {
        let env = if t < 0.003 {
            t / 0.003
        } else {
            ((dur - t) / (dur - 0.003)).powf(1.5).max(0.0)
        };
        let wave = 0.55 * (TAU * 880.0 * t).sin() + 0.45 * (TAU * 1320.0 * t).sin();
        wave * env * 0.9
    })
}

/// GOOD hit: clean single tone (660 Hz), 90 ms.
fn sound_hit_good() -> AudioSource {
    build_wav(0.09, |t, dur| {
        let env = ((dur - t) / dur).powf(1.2);
        (TAU * 660.0 * t).sin() * env * 0.75
    })
}

/// MISS: low dull thud (180 Hz) with fast exponential decay, 140 ms.
fn sound_miss() -> AudioSource {
    build_wav(0.14, |t, _| {
        let env = (-t / 0.04).exp();
        (TAU * 180.0 * t).sin() * env * 0.65
    })
}

/// Combo milestone: C-major chord (C5-E5-G5), 180 ms.
fn sound_combo() -> AudioSource {
    build_wav(0.18, |t, dur| {
        let attack = (t / 0.008).min(1.0);
        let decay = ((dur - t) / dur).powf(0.8).max(0.0);
        let env = attack * decay;
        let wave = (TAU * 523.25 * t).sin()
            + (TAU * 659.25 * t).sin()
            + (TAU * 784.0 * t).sin();
        wave / 3.0 * env * 0.9
    })
}

// ── Audio setup ───────────────────────────────────────────────────────────────

fn setup_audio(mut commands: Commands, mut audio_assets: ResMut<Assets<AudioSource>>) {
    commands.insert_resource(AudioHandles {
        hit_perfect: audio_assets.add(sound_hit_perfect()),
        hit_good: audio_assets.add(sound_hit_good()),
        miss: audio_assets.add(sound_miss()),
        combo: audio_assets.add(sound_combo()),
    });
}

// ── Audio playback systems ────────────────────────────────────────────────────

fn play_hit_sound(
    mut commands: Commands,
    mut events: EventReader<HitEvent>,
    handles: Res<AudioHandles>,
    config: Res<AudioConfig>,
) {
    for ev in events.read() {
        let handle = if ev.perfect {
            handles.hit_perfect.clone()
        } else {
            handles.hit_good.clone()
        };
        // Raise pitch slightly as combo builds – rewards sustained accuracy
        let pitch = 1.0 + (ev.combo as f32 * 0.015).min(0.30);
        commands.spawn((
            AudioPlayer(handle),
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: Volume::new(config.volume),
                speed: pitch,
                ..default()
            },
        ));
    }
}

fn play_miss_sound(
    mut commands: Commands,
    mut events: EventReader<MissEvent>,
    handles: Res<AudioHandles>,
    config: Res<AudioConfig>,
) {
    for _ in events.read() {
        commands.spawn((
            AudioPlayer(handles.miss.clone()),
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: Volume::new(config.volume * 0.8),
                ..default()
            },
        ));
    }
}

fn play_combo_sound(
    mut commands: Commands,
    mut events: EventReader<ComboEvent>,
    handles: Res<AudioHandles>,
    config: Res<AudioConfig>,
) {
    for ev in events.read() {
        // Slightly higher pitch on larger combos
        let pitch = 1.0 + (ev.combo as f32 * 0.01).min(0.25);
        commands.spawn((
            AudioPlayer(handles.combo.clone()),
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: Volume::new(config.volume),
                speed: pitch,
                ..default()
            },
        ));
    }
}

// ── Title ────────────────────────────────────────────────────────────────────

fn setup_title(mut commands: Commands) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(20.0),
                ..default()
            },
            TitleScreen,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("PULSE"),
                TextFont {
                    font_size: 80.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.85, 1.0)),
            ));
            p.spawn((
                Text::new("Tap / Click to Start"),
                TextFont {
                    font_size: 28.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.9)),
            ));
            p.spawn((
                Text::new("Match the pulse to the ring!"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.5, 0.5, 0.7)),
            ));
        });
}

fn title_input(
    mouse: Res<ButtonInput<MouseButton>>,
    touch: Res<Touches>,
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<AppState>>,
    mut data: ResMut<GameData>,
) {
    if mouse.just_pressed(MouseButton::Left)
        || touch.any_just_pressed()
        || keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Enter)
    {
        data.reset();
        next.set(AppState::Playing);
    }
}

// ── Game setup / cleanup ─────────────────────────────────────────────────────

fn setup_game(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // Target ring (stays fixed at center)
    commands.spawn((
        Mesh2d(meshes.add(annulus_mesh(TARGET_RADIUS, TARGET_THICKNESS))),
        MeshMaterial2d(materials.add(TARGET_COLOR)),
        Transform::from_xyz(0.0, 0.0, 0.0),
        TargetRing,
    ));

    // HUD overlay
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(16.0)),
                position_type: PositionType::Absolute,
                ..default()
            },
            HudRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("30.0"),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                TimerText,
            ));
            p.spawn((
                Text::new("0"),
                TextFont {
                    font_size: 48.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                ScoreText,
            ));
            p.spawn((
                Text::new(""),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.9, 1.0)),
                ComboText,
            ));
        });
}

fn cleanup_game(
    mut commands: Commands,
    pulses: Query<Entity, With<Pulse>>,
    rings: Query<Entity, With<TargetRing>>,
    judgments: Query<Entity, With<JudgmentText>>,
    hud: Query<Entity, With<HudRoot>>,
) {
    for e in pulses.iter().chain(rings.iter()).chain(judgments.iter()) {
        commands.entity(e).despawn_recursive();
    }
    for e in &hud {
        commands.entity(e).despawn_recursive();
    }
}

// ── Game systems ─────────────────────────────────────────────────────────────

fn tick_timer(
    time: Res<Time>,
    mut data: ResMut<GameData>,
    mut next: ResMut<NextState<AppState>>,
) {
    data.time_left -= time.delta_secs();
    if data.time_left <= 0.0 {
        data.time_left = 0.0;
        if data.score > data.high_score {
            data.high_score = data.score;
        }
        next.set(AppState::GameOver);
    }
}

fn spawn_pulses(
    time: Res<Time>,
    mut data: ResMut<GameData>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    data.pulse_timer -= time.delta_secs();
    if data.pulse_timer <= 0.0 {
        data.pulse_timer = PULSE_INTERVAL;

        commands.spawn((
            Mesh2d(meshes.add(annulus_mesh(PULSE_START_RADIUS, 3.5))),
            MeshMaterial2d(materials.add(PULSE_COLOR)),
            Transform::from_xyz(0.0, 0.0, 1.0),
            Pulse {
                radius: PULSE_START_RADIUS,
                missed: false,
            },
        ));
    }
}

fn move_pulses(
    time: Res<Time>,
    mut commands: Commands,
    mut pulses: Query<(Entity, &mut Pulse, &Mesh2d)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.delta_secs();
    for (entity, mut pulse, mesh2d) in &mut pulses {
        pulse.radius -= PULSE_SPEED * dt;

        if let Some(mesh) = meshes.get_mut(mesh2d.0.id()) {
            *mesh = annulus_mesh(pulse.radius.max(0.0), 3.5);
        }

        if pulse.radius < TARGET_RADIUS - GOOD_WINDOW - 4.0 {
            commands.entity(entity).despawn_recursive();
        }
    }
}

fn handle_input(
    mouse: Res<ButtonInput<MouseButton>>,
    touch: Res<Touches>,
    keys: Res<ButtonInput<KeyCode>>,
    mut pulses: Query<(Entity, &mut Pulse)>,
    mut data: ResMut<GameData>,
    mut commands: Commands,
    mut ev_hit: EventWriter<HitEvent>,
    mut ev_miss: EventWriter<MissEvent>,
    mut ev_combo: EventWriter<ComboEvent>,
) {
    let pressed = mouse.just_pressed(MouseButton::Left)
        || touch.any_just_pressed()
        || keys.just_pressed(KeyCode::Space);

    if !pressed {
        return;
    }

    // Find the pulse closest to the target radius
    let mut best: Option<(Entity, f32)> = None;
    for (entity, pulse) in &pulses {
        if pulse.missed {
            continue;
        }
        let dist = (pulse.radius - TARGET_RADIUS).abs();
        if best.map_or(true, |(_, d)| dist < d) {
            best = Some((entity, dist));
        }
    }

    let (judgment, score_delta) = match best {
        Some((entity, dist)) if dist <= PERFECT_WINDOW => {
            if let Ok((_, mut p)) = pulses.get_mut(entity) {
                p.missed = true;
            }
            commands.entity(entity).despawn_recursive();
            ("PERFECT", 100u32)
        }
        Some((entity, dist)) if dist <= GOOD_WINDOW => {
            if let Ok((_, mut p)) = pulses.get_mut(entity) {
                p.missed = true;
            }
            commands.entity(entity).despawn_recursive();
            ("GOOD", 50u32)
        }
        // Pulse exists but outside any judgment window.
        // If it already escaped past the GOOD window, mark it as missed now so
        // miss_check does NOT fire for the same pulse later in this frame
        // (which would produce a second MissEvent and doubled miss SFX).
        Some((entity, _)) => {
            if let Ok((_, mut p)) = pulses.get_mut(entity) {
                if p.radius < TARGET_RADIUS - GOOD_WINDOW {
                    p.missed = true;
                }
            }
            ("MISS", 0u32)
        }
        None => ("MISS", 0u32),
    };

    let color = match judgment {
        "PERFECT" => PERFECT_COLOR,
        "GOOD" => GOOD_COLOR,
        _ => MISS_COLOR,
    };

    if score_delta > 0 {
        data.combo += 1;
        if data.combo > data.best_combo {
            data.best_combo = data.combo;
        }
        data.score += score_delta * data.combo_multiplier();

        ev_hit.send(HitEvent {
            perfect: judgment == "PERFECT",
            combo: data.combo,
        });

        if data.combo % COMBO_MILESTONE == 0 {
            ev_combo.send(ComboEvent { combo: data.combo });
        }
    } else {
        data.combo = 0;
        ev_miss.send(MissEvent);
    }

    spawn_judgment_text(&mut commands, judgment, color);
}

/// Pulses that passed the window without input → MISS
fn miss_check(
    mut pulses: Query<&mut Pulse>,
    mut data: ResMut<GameData>,
    mut commands: Commands,
    mut ev_miss: EventWriter<MissEvent>,
) {
    for mut pulse in &mut pulses {
        if !pulse.missed && pulse.radius < TARGET_RADIUS - GOOD_WINDOW {
            pulse.missed = true;
            data.combo = 0;
            ev_miss.send(MissEvent);
            spawn_judgment_text(&mut commands, "MISS", MISS_COLOR);
        }
    }
}

fn update_hud(
    data: Res<GameData>,
    mut score_q: Query<&mut Text, (With<ScoreText>, Without<ComboText>, Without<TimerText>)>,
    mut combo_q: Query<&mut Text, (With<ComboText>, Without<ScoreText>, Without<TimerText>)>,
    mut timer_q: Query<&mut Text, (With<TimerText>, Without<ScoreText>, Without<ComboText>)>,
) {
    for mut t in &mut score_q {
        **t = format!("{}", data.score);
    }
    for mut t in &mut combo_q {
        if data.combo >= 2 {
            let mult = data.combo_multiplier();
            **t = format!("{}  ×{}", data.combo, mult);
        } else {
            **t = String::new();
        }
    }
    for mut t in &mut timer_q {
        **t = format!("{:.1}", data.time_left.max(0.0));
    }
}

fn update_judgment_texts(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut JudgmentText, &mut Transform, &mut TextColor)>,
) {
    for (entity, mut jt, mut transform, mut color) in &mut q {
        jt.timer -= time.delta_secs();
        transform.translation.y += 60.0 * time.delta_secs();
        let alpha = (jt.timer / 0.8).clamp(0.0, 1.0);
        let c = color.0.to_srgba();
        color.0 = Color::srgba(c.red, c.green, c.blue, alpha);
        if jt.timer <= 0.0 {
            commands.entity(entity).despawn_recursive();
        }
    }
}

// ── Game Over ────────────────────────────────────────────────────────────────

fn setup_game_over(mut commands: Commands, data: Res<GameData>) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(16.0),
                ..default()
            },
            GameOverScreen,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("GAME OVER"),
                TextFont {
                    font_size: 64.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.4, 0.4)),
            ));
            p.spawn((
                Text::new(format!("Score: {}", data.score)),
                TextFont {
                    font_size: 40.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            p.spawn((
                Text::new(format!("Best Combo: {}", data.best_combo)),
                TextFont {
                    font_size: 26.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.9, 1.0)),
            ));
            p.spawn((
                Text::new(format!("High Score: {}", data.high_score)),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.9, 0.3)),
            ));

            // Button row
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                margin: UiRect::top(Val::Px(8.0)),
                ..default()
            })
            .with_children(|row| {
                spawn_button(
                    row,
                    "Retry",
                    RETRY_NORMAL,
                    Color::srgb(0.4, 0.4, 0.6),
                    RetryButton,
                );

                #[cfg(target_arch = "wasm32")]
                spawn_button(
                    row,
                    "𝕏  Share",
                    SHARE_NORMAL,
                    Color::srgb(0.2, 0.4, 0.9),
                    ShareButton,
                );
            });

            p.spawn((
                Text::new("Tap Retry  •  Space / Enter"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.4, 0.4, 0.6)),
            ));
        });
}

/// Restart the game via keyboard or RetryButton click / tap.
/// Global touch / mouse are intentionally excluded: on touch devices
/// touch.any_just_pressed() would also fire when tapping the Share button,
/// causing an immediate restart before the tweet opens.
fn game_over_input(
    keys: Res<ButtonInput<KeyCode>>,
    retry_q: Query<&Interaction, (Changed<Interaction>, With<RetryButton>)>,
    mut next: ResMut<NextState<AppState>>,
    mut data: ResMut<GameData>,
) {
    let retry = keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Enter)
        || retry_q.iter().any(|i| *i == Interaction::Pressed);

    if retry {
        data.reset();
        next.set(AppState::Playing);
    }
}

/// Animate the Retry button on hover / press.
fn retry_button_system(
    mut q: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<RetryButton>)>,
) {
    for (interaction, mut bg) in &mut q {
        bg.0 = match interaction {
            Interaction::Pressed => RETRY_PRESS,
            Interaction::Hovered => RETRY_HOVER,
            Interaction::None => RETRY_NORMAL,
        };
    }
}

/// Animate the Share button and open the tweet URL on press (WASM only).
#[cfg(target_arch = "wasm32")]
fn share_button_system(
    mut q: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<ShareButton>)>,
    data: Res<GameData>,
) {
    for (interaction, mut bg) in &mut q {
        bg.0 = match interaction {
            Interaction::Pressed => {
                open_tweet(data.score);
                SHARE_PRESS
            }
            Interaction::Hovered => SHARE_HOVER,
            Interaction::None => SHARE_NORMAL,
        };
    }
}

// ── X (Twitter) share ─────────────────────────────────────────────────────────

/// Open the X / Twitter intent URL in a new tab.
/// Compiled only for WASM; on native this is an empty stub.
#[cfg(target_arch = "wasm32")]
fn open_tweet(score: u32) {
    let text = format!(
        "🎵 PULSE - Score: {score}\nシンプルなリズムゲームに挑戦！\n{GAME_URL}\n#PULSE #ゲーム制作 #bevy"
    );
    let url = format!(
        "https://twitter.com/intent/tweet?text={}",
        percent_encode(&text)
    );
    if let Some(window) = web_sys::window() {
        // "noopener,noreferrer" prevents the opened page from accessing
        // window.opener (reverse-tabnabbing mitigation)
        let _ = window.open_with_url_and_target_and_features(&url, "_blank", "noopener,noreferrer");
    }
}

/// Percent-encode a string for use in a URL query parameter.
/// Encodes everything except unreserved characters (RFC 3986).
#[cfg(target_arch = "wasm32")]
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}
