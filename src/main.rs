//! Pulseの画面、リズム判定、スコアと画面遷移を管理する。

mod audio;

use audio::{AudioConfig, AudioPlugin, PlaySoundEffect, SetSfxVolume, SoundEffect};
use bevy::{
    prelude::*,
    render::{camera::ScalingMode, mesh::Indices},
    ui::{FocusPolicy, RelativeCursorPosition},
    window::WindowResolution,
};

// ── Constants ────────────────────────────────────────────────────────────────

const GAME_DURATION: f32 = 30.0;

const TARGET_RADIUS: f32 = 60.0;
const TARGET_THICKNESS: f32 = 4.0;
const PULSE_START_RADIUS: f32 = 260.0;
const PULSE_SPEED: f32 = 220.0;
const PULSE_INTERVAL: f32 = 1.2;

const PERFECT_WINDOW: f32 = 10.0;
const GOOD_WINDOW: f32 = 26.0;
const MAX_COMBO_MULTIPLIER: u32 = 4;
const COMBO_MILESTONE: u32 = 5;
const COMBO_PULSE_DURATION: f32 = 0.24;
const COMBO_BREAK_DURATION: f32 = 0.32;

#[cfg(target_arch = "wasm32")]
const GAME_URL: &str = "https://czmirror.github.io/Pulse/";

// リングの座標系は480pxの正方形を基準にする。
const DESIGN_SIZE: f32 = 480.0;
const RING_SEGMENTS: u32 = 96;
// ターゲットとパルスを同じ量だけ広げ、リングが重なる判定位置は変えない。
const RING_DISPLAY_OFFSET: f32 = 28.0;
const SPACE_SM: f32 = 8.0;
const SPACE_MD: f32 = 16.0;
const SPACE_LG: f32 = 24.0;
const LABEL_SIZE: f32 = 16.0;
const BODY_SIZE: f32 = 18.0;
const DISPLAY_SIZE: f32 = 64.0;

const BG_COLOR: Color = Color::srgb(0.014, 0.020, 0.064);
const SURFACE: Color = Color::srgb(0.051, 0.064, 0.136);
const SURFACE_HOVER: Color = Color::srgb(0.090, 0.115, 0.232);
const BORDER: Color = Color::srgb(0.38, 0.42, 0.80);
const INK: Color = Color::srgb(235.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0);
const MUTED: Color = Color::srgb(0.70, 0.75, 0.93);
const ACCENT: Color = Color::srgb(0.21, 0.91, 1.0);
const CYAN_GLOW: Color = Color::srgba(0.04, 0.79, 1.0, 0.33);
const TARGET_COLOR: Color = INK;
const PULSE_COLOR: Color = ACCENT;
const TARGET_GLOW: Color = Color::srgba(0.32, 0.37, 1.0, 0.38);
const PERFECT_COLOR: Color = Color::srgb(1.0, 0.73, 0.49);
const GOOD_COLOR: Color = Color::srgb(0.3, 1.0, 0.5);
const MISS_COLOR: Color = Color::srgb(1.0, 0.3, 0.3);

const RETRY_NORMAL: Color = Color::srgb(0.024, 0.16, 0.24);
const RETRY_HOVER: Color = Color::srgb(0.035, 0.24, 0.34);
const RETRY_PRESS: Color = Color::srgb(0.015, 0.10, 0.18);

#[cfg(target_arch = "wasm32")]
const SHARE_NORMAL: Color = SURFACE;
#[cfg(target_arch = "wasm32")]
const SHARE_HOVER: Color = SURFACE_HOVER;
#[cfg(target_arch = "wasm32")]
const SHARE_PRESS: Color = BG_COLOR;

const VOL_SLIDER_TRACK: Color = SURFACE;
const VOL_SLIDER_HOVER: Color = SURFACE_HOVER;
const VOL_SLIDER_PRESS: Color = BG_COLOR;
const VOL_SLIDER_FILL: Color = ACCENT;
const VOL_SLIDER_KNOB: Color = INK;

// ── States ───────────────────────────────────────────────────────────────────

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
enum AppState {
    #[default]
    Title,
    Playing,
    GameOver,
}

// ── Resources ────────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct GameData {
    score:       u32,
    combo:       u32,
    best_combo:  u32,
    time_left:   f32,
    pulse_timer: f32,
    high_score:  u32,
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

#[derive(Resource, Default)]
struct ComboDisplayFx {
    last_combo:  u32,
    pulse_timer: f32,
    break_timer: f32,
    broken_from: u32,
}

// ── Components ───────────────────────────────────────────────────────────────

#[derive(Component)]
struct Pulse { radius: f32, missed: bool }

#[derive(Component)]
struct JudgmentText { timer: f32 }

#[derive(Component)] struct ScoreText;
#[derive(Component)] struct ComboText;
#[derive(Component)] struct ComboSubText;
#[derive(Component)] struct TimerText;
#[derive(Component)] struct TitleScreen;
#[derive(Component)] struct GameOverScreen;
#[derive(Component)] struct HudRoot;
#[derive(Component)] struct TargetRing;
#[derive(Component)] struct RetryButton;
#[derive(Component)] struct StartButton;
#[derive(Component)] struct MissFlashOverlay;

// タイトル画面の SFX 音量スライダー
#[derive(Component)] struct VolumeDisplay;
#[derive(Component)] struct VolumeSlider;
#[derive(Component)] struct VolumeSliderFill;
#[derive(Component)] struct VolumeSliderKnob;

#[cfg(target_arch = "wasm32")]
#[derive(Component)]
struct ShareButton;

// ── App ──────────────────────────────────────────────────────────────────────

#[derive(Resource)]
struct DisplayFont(Handle<Font>);

impl DisplayFont {
    fn at(&self, size: f32) -> TextFont {
        TextFont { font: self.0.clone(), font_size: size, ..default() }
    }
}

fn main() {
    let mut app = App::new();

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: "Pulse".into(),
            resolution: WindowResolution::new(DESIGN_SIZE, DESIGN_SIZE),
            canvas: Some("#bevy-canvas".into()),
            fit_canvas_to_parent: true,
            prevent_default_event_handling: true,
            ..default()
        }),
        ..default()
    };

    // WASM では native.rs の AudioPlayer が一切生成されないため
    // bevy_audio は実質 no-op。disable() は型不一致でパニックする恐れが
    // あるため使わず、両プラットフォームで同一の DefaultPlugins を使う。
    app.add_plugins(DefaultPlugins.set(window_plugin));
    let display_font = Font::try_from_bytes(include_bytes!("../assets/fonts/Inter-Regular.ttf").to_vec())
        .expect("bundled Inter font must be valid");
    let display_font = app.world_mut().resource_mut::<Assets<Font>>().add(display_font);
    app.insert_resource(DisplayFont(display_font));

    app.add_plugins(AudioPlugin)
        .insert_resource(ClearColor(BG_COLOR))
        .init_resource::<GameData>()
        .init_resource::<ComboDisplayFx>()
        .init_state::<AppState>()
        // Title
        .add_systems(Startup, setup_camera)
        .add_systems(OnEnter(AppState::Title), setup_title)
        .add_systems(OnExit(AppState::Title), despawn_with::<TitleScreen>)
        .add_systems(
            Update,
            (
                title_input,
                title_volume_slider,
                update_volume_slider_display,
                volume_slider_visual,
                primary_button_visual::<StartButton>,
            )
                .run_if(in_state(AppState::Title)),
        )
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
                animate_combo_display,
                update_judgment_texts,
            )
                .chain()
                .run_if(in_state(AppState::Playing)),
        )
        // GameOver
        .add_systems(OnEnter(AppState::GameOver), setup_game_over)
        .add_systems(OnExit(AppState::GameOver), despawn_with::<GameOverScreen>)
        .add_systems(
            Update,
            (game_over_input, primary_button_visual::<RetryButton>).run_if(in_state(AppState::GameOver)),
        );

    #[cfg(target_arch = "wasm32")]
    app.add_systems(
        Update,
        share_button_system.run_if(in_state(AppState::GameOver)),
    );

    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, sync_canvas_ui_scale).add_systems(
        PreUpdate,
        sync_canvas_cursor.before(bevy::ui::UiSystem::Focus),
    );

    app.run();
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn despawn_with<T: Component>(mut commands: Commands, q: Query<Entity, With<T>>) {
    for e in &q { commands.entity(e).despawn_recursive(); }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        OrthographicProjection {
            scaling_mode: ScalingMode::FixedHorizontal { viewport_width: DESIGN_SIZE },
            ..OrthographicProjection::default_2d()
        },
    ));
}

#[cfg(target_arch = "wasm32")]
fn browser_canvas(window: &web_sys::Window) -> Option<web_sys::Element> {
    window.document()?.get_element_by_id("bevy-canvas")
}

#[cfg(target_arch = "wasm32")]
fn sync_canvas_cursor(
    mut moved: EventReader<bevy::window::CursorMoved>,
    mut windows: Query<(Entity, &mut Window), With<bevy::window::PrimaryWindow>>,
) {
    let Ok((entity, mut window)) = windows.get_single_mut() else { return };
    let Some(cursor) = moved.read().filter(|event| event.window == entity).last() else { return };
    let Some(browser) = web_sys::window() else { return };
    let Some(canvas) = browser_canvas(&browser) else { return };
    let css_width = f64::from(canvas.client_width());
    if css_width <= 0.0 || window.physical_cursor_position().is_none() {
        return;
    }

    // ブラウザの報告するDPRと描画バッファの倍率が違う場合は、
    // BevyのUIが使うバッファ座標にカーソル位置を合わせる。
    let buffer_ratio = f64::from(window.physical_width()) / css_width;
    let correction = buffer_ratio / browser.device_pixel_ratio();
    if correction.is_finite() && (correction - 1.0).abs() > 0.02 {
        let position = cursor.position.as_dvec2() * f64::from(window.scale_factor()) * correction;
        window.set_physical_cursor_position(Some(position));
    }
}

#[cfg(target_arch = "wasm32")]
fn sync_canvas_ui_scale(
    windows: Query<&Window, (With<bevy::window::PrimaryWindow>, Changed<Window>)>,
    mut ui_scale: ResMut<UiScale>,
) {
    let Ok(window) = windows.get_single() else { return };
    let Some(canvas) = web_sys::window().as_ref().and_then(browser_canvas)
    else {
        return;
    };
    let css_width = canvas.client_width() as f32;
    if css_width <= 0.0 {
        return;
    }

    // CSSで縮めたキャンバスに合わせ、UIの1単位を画面上の1 CSS pxにそろえる。
    let scale = window.width() / css_width;
    if scale > 0.0 && (ui_scale.0 - scale).abs() >= 0.01 {
        ui_scale.0 = scale;
    }
}

fn annulus_mesh(radius: f32, thickness: f32) -> Mesh {
    let radius = radius + RING_DISPLAY_OFFSET;
    let inner = (radius - thickness * 0.5).max(0.0);
    let outer = radius + thickness * 0.5;
    Annulus::new(inner, outer).mesh().resolution(RING_SEGMENTS).build()
}

// 96分割の円周から、6区画ごとに最後の2区画を外して破線を描く。
fn target_dash_mesh(thickness: f32) -> Mesh {
    let mut mesh = annulus_mesh(TARGET_RADIUS, thickness);
    let indices = (0..RING_SEGMENTS)
        .filter(|segment| segment % 6 < 4)
        .flat_map(|segment| {
            let inner = segment * 2;
            [inner, inner + 1, inner + 3, inner + 3, inner + 2, inner]
        })
        .collect();
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn glow(color: Color, blur: f32) -> BoxShadow {
    BoxShadow {
        color,
        x_offset: Val::ZERO,
        y_offset: Val::ZERO,
        blur_radius: Val::Px(blur),
        ..default()
    }
}

fn spawn_judgment_text(commands: &mut Commands, label: &str, color: Color) {
    commands.spawn((
        Text2d::new(label),
        TextFont { font_size: 40.0, ..default() },
        TextColor(color),
        Transform::from_xyz(0.0, -188.0, 10.0),
        JudgmentText { timer: 0.8 },
    ));
}

fn spawn_button<M: Component>(
    parent: &mut ChildBuilder,
    label: &str,
    font: &DisplayFont,
    primary: bool,
    marker: M,
) {
    let (bg, border) = if primary { (RETRY_NORMAL, ACCENT) } else { (SURFACE, BORDER) };
    let mut button = parent
        .spawn((
            Button,
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(48.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor(border),
            BorderRadius::all(Val::Px(8.0)),
            marker,
        ));
    if primary {
        button.insert(glow(CYAN_GLOW, 10.0));
    }
    // ラベル上の入力も親のボタンに届ける。
    button.with_child((Text::new(label), font.at(BODY_SIZE), TextColor(INK), FocusPolicy::Pass));
}

// ── Title ────────────────────────────────────────────────────────────────────

fn setup_title(mut commands: Commands, config: Res<AudioConfig>, font: Res<DisplayFont>) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Vh(1.8),
                ..default()
            },
            TitleScreen,
        ))
        .with_children(|p| {
            p.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Vh(26.0),
                min_height: Val::Px(90.0),
                max_height: Val::Px(224.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|brand| {
                for (height, color, halo) in [(100.0, ACCENT, CYAN_GLOW), (72.0, BORDER, TARGET_GLOW)] {
                    brand.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::ZERO,
                            height: Val::Percent(height),
                            aspect_ratio: Some(1.0),
                            border: UiRect::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(BG_COLOR),
                        BorderColor(color),
                        BorderRadius::MAX,
                        glow(halo, 12.0),
                    ));
                }
                brand.spawn((
                    Text::new("PULSE"),
                    font.at(64.0),
                    TextColor(INK),
                    BackgroundColor(BG_COLOR),
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::ZERO,
                        padding: UiRect::horizontal(Val::Px(SPACE_SM)),
                        ..default()
                    },
                ));
            });
            p.spawn((
                Text::new("Match the pulse to the ring"),
                font.at(BODY_SIZE),
                TextColor(MUTED),
                TextLayout::new_with_justify(JustifyText::Center),
            ));
            p.spawn(Node {
                width: Val::Percent(82.0),
                max_width: Val::Px(340.0),
                margin: UiRect::top(Val::Px(SPACE_SM)),
                ..default()
            })
            .with_children(|row| spawn_button(row, "START", &font, true, StartButton));

            // ── SFX 音量スライダー行 ──────────────────────────────────────
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(SPACE_SM),
                margin: UiRect::top(Val::Vh(7.0)),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new("SFX"),
                    font.at(LABEL_SIZE),
                    TextColor(MUTED),
                ));

                row
                    .spawn((
                        Button,
                        Node {
                            width: Val::Px(180.0),
                            height: Val::Px(22.0),
                            border: UiRect::all(Val::Px(2.0)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        RelativeCursorPosition::default(),
                        BackgroundColor(VOL_SLIDER_TRACK),
                        BorderColor(BORDER),
                        BorderRadius::all(Val::Px(999.0)),
                        VolumeSlider,
                    ))
                    .with_children(|slider| {
                        slider.spawn((
                            Node {
                                width: Val::Percent(config.volume * 100.0),
                                height: Val::Percent(100.0),
                                ..default()
                            },
                            BackgroundColor(VOL_SLIDER_FILL),
                            BorderRadius::all(Val::Px(999.0)),
                            VolumeSliderFill,
                        ));

                        slider.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Percent((config.volume * 100.0 - 5.0).clamp(0.0, 90.0)),
                                width: Val::Px(18.0),
                                height: Val::Px(18.0),
                                ..default()
                            },
                            BackgroundColor(VOL_SLIDER_KNOB),
                            BorderRadius::all(Val::Px(999.0)),
                            VolumeSliderKnob,
                        ));
                    });

                row.spawn((
                    Text::new(format!("{:.0}%", config.volume * 100.0)),
                    font.at(LABEL_SIZE),
                    TextColor(INK),
                    Node {
                        min_width: Val::Px(44.0),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    VolumeDisplay,
                ));
            });
        });
}

fn title_input(
    mouse: Res<ButtonInput<MouseButton>>,
    touch: Res<Touches>,
    keys:  Res<ButtonInput<KeyCode>>,
    // スライダーが押されているときはゲーム開始しない
    vol_slider: Query<&Interaction, With<VolumeSlider>>,
    mut next: ResMut<NextState<AppState>>,
    mut data: ResMut<GameData>,
) {
    if vol_slider.iter().any(|i| *i == Interaction::Pressed) {
        return;
    }

    if mouse.just_pressed(MouseButton::Left)
        || touch.any_just_pressed()
        || keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Enter)
    {
        data.reset();
        next.set(AppState::Playing);
    }
}

/// タイトル画面のスライダー操作で SE 音量を変更。
fn title_volume_slider(
    slider_q: Query<(&Interaction, &RelativeCursorPosition), With<VolumeSlider>>,
    config: Res<AudioConfig>,
    mut ev_vol: EventWriter<SetSfxVolume>,
) {
    for (interaction, cursor) in &slider_q {
        if *interaction == Interaction::Pressed {
            if let Some(pos) = cursor.normalized {
                let v = pos.x.clamp(0.0, 1.0);
                if (v - config.volume).abs() >= 0.005 {
                    ev_vol.send(SetSfxVolume(v));
                }
            }
        }
    }
}

/// AudioConfig が変わったらスライダーと表示を更新。
fn update_volume_slider_display(
    config: Res<AudioConfig>,
    mut display_q: Query<&mut Text, With<VolumeDisplay>>,
    mut fill_q: Query<&mut Node, (With<VolumeSliderFill>, Without<VolumeSliderKnob>, Without<VolumeDisplay>)>,
    mut knob_q: Query<&mut Node, (With<VolumeSliderKnob>, Without<VolumeSliderFill>, Without<VolumeDisplay>)>,
) {
    if config.is_changed() {
        for mut t in &mut display_q {
            **t = format!("{:.0}%", config.volume * 100.0);
        }

        for mut node in &mut fill_q {
            node.width = Val::Percent(config.volume * 100.0);
        }

        for mut node in &mut knob_q {
            node.left = Val::Percent((config.volume * 100.0 - 5.0).clamp(0.0, 90.0));
        }
    }
}

/// 音量スライダーのホバー/プレス色。
fn volume_slider_visual(
    mut slider_q: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<VolumeSlider>),
    >,
) {
    for (i, mut bg) in &mut slider_q {
        bg.0 = match i {
            Interaction::Pressed  => VOL_SLIDER_PRESS,
            Interaction::Hovered  => VOL_SLIDER_HOVER,
            Interaction::None     => VOL_SLIDER_TRACK,
        };
    }
}

// ── Game setup / cleanup ─────────────────────────────────────────────────────

fn setup_game(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut combo_fx: ResMut<ComboDisplayFx>,
    font: Res<DisplayFont>,
) {
    combo_fx.last_combo = 0;
    combo_fx.pulse_timer = 0.0;
    combo_fx.break_timer = 0.0;
    combo_fx.broken_from = 0;

    for (thickness, color, depth) in [
        (14.0, Color::srgba(0.25, 0.28, 1.0, 0.14), -2.0),
        (8.0, TARGET_GLOW, -1.0),
        (TARGET_THICKNESS, TARGET_COLOR, 0.0),
    ] {
        commands.spawn((
            Mesh2d(meshes.add(target_dash_mesh(thickness))),
            MeshMaterial2d(materials.add(color)),
            Transform::from_xyz(0.0, 0.0, depth),
            TargetRing,
        ));
    }

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
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 0.2, 0.2, 0.0)),
                ZIndex(1),
                MissFlashOverlay,
            ));
            p.spawn((
                Text::new("TIME LEFT"),
                font.at(LABEL_SIZE),
                TextColor(MUTED),
                BackgroundColor(BG_COLOR),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(SPACE_MD),
                    right: Val::Px(SPACE_LG),
                    ..default()
                },
            ));
            p.spawn((
                Text::new("30.0"),
                font.at(40.0),
                TextColor(ACCENT),
                BackgroundColor(BG_COLOR),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(32.0),
                    right: Val::Px(SPACE_LG),
                    ..default()
                },
                TimerText,
            ));
            p.spawn((
                Text::new("SCORE"),
                font.at(LABEL_SIZE),
                TextColor(MUTED),
                BackgroundColor(BG_COLOR),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(SPACE_MD),
                    left: Val::Px(SPACE_LG),
                    ..default()
                },
            ));
            p.spawn((
                Text::new("0"),
                font.at(40.0),
                TextColor(INK),
                BackgroundColor(BG_COLOR),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(32.0),
                    left: Val::Px(SPACE_LG),
                    ..default()
                },
                ScoreText,
            ));
            p.spawn((
                Text::new(""),
                font.at(32.0),
                TextColor(Color::srgba(0.8, 0.9, 1.0, 0.0)),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(80.0),
                    left: Val::Px(SPACE_LG),
                    ..default()
                },
                TextLayout::new_with_justify(JustifyText::Center),
                ComboText,
            ));
            p.spawn((
                Text::new(""),
                font.at(LABEL_SIZE),
                TextColor(Color::srgba(0.7, 0.82, 1.0, 0.0)),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(80.0),
                    right: Val::Px(SPACE_LG),
                    ..default()
                },
                TextLayout::new_with_justify(JustifyText::Center),
                ComboSubText,
            ));
        });
}

fn cleanup_game(
    mut commands: Commands,
    pulses:    Query<Entity, With<Pulse>>,
    rings:     Query<Entity, With<TargetRing>>,
    judgments: Query<Entity, With<JudgmentText>>,
    hud:       Query<Entity, With<HudRoot>>,
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
            Pulse { radius: PULSE_START_RADIUS, missed: false },
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
    keys:  Res<ButtonInput<KeyCode>>,
    mut pulses: Query<(Entity, &mut Pulse)>,
    mut data:   ResMut<GameData>,
    mut commands: Commands,
    mut ev_sfx: EventWriter<PlaySoundEffect>,
) {
    let pressed = mouse.just_pressed(MouseButton::Left)
        || touch.any_just_pressed()
        || keys.just_pressed(KeyCode::Space);

    if !pressed { return; }

    // 最もターゲットに近いパルスを探す
    let mut best: Option<(Entity, f32)> = None;
    for (entity, pulse) in &pulses {
        if pulse.missed { continue; }
        let dist = (pulse.radius - TARGET_RADIUS).abs();
        if best.map_or(true, |(_, d)| dist < d) {
            best = Some((entity, dist));
        }
    }

    let (judgment, score_delta) = match best {
        Some((entity, dist)) if dist <= PERFECT_WINDOW => {
            if let Ok((_, mut p)) = pulses.get_mut(entity) { p.missed = true; }
            commands.entity(entity).despawn_recursive();
            ("PERFECT", 100u32)
        }
        Some((entity, dist)) if dist <= GOOD_WINDOW => {
            if let Ok((_, mut p)) = pulses.get_mut(entity) { p.missed = true; }
            commands.entity(entity).despawn_recursive();
            ("GOOD", 50u32)
        }
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
        "GOOD"    => GOOD_COLOR,
        _         => MISS_COLOR,
    };

    if score_delta > 0 {
        data.combo += 1;
        if data.combo > data.best_combo { data.best_combo = data.combo; }
        data.score += score_delta * data.combo_multiplier();

        if judgment == "PERFECT" {
            ev_sfx.send(PlaySoundEffect(SoundEffect::HitPerfect { combo: data.combo }));
        } else {
            ev_sfx.send(PlaySoundEffect(SoundEffect::HitGood { combo: data.combo }));
        }

        if data.combo % COMBO_MILESTONE == 0 {
            ev_sfx.send(PlaySoundEffect(SoundEffect::Combo { combo: data.combo }));
        }
    } else {
        data.combo = 0;
        ev_sfx.send(PlaySoundEffect(SoundEffect::Miss));
    }

    spawn_judgment_text(&mut commands, judgment, color);
}

fn miss_check(
    mut pulses: Query<&mut Pulse>,
    mut data:   ResMut<GameData>,
    mut commands: Commands,
    mut ev_sfx: EventWriter<PlaySoundEffect>,
) {
    for mut pulse in &mut pulses {
        if !pulse.missed && pulse.radius < TARGET_RADIUS - GOOD_WINDOW {
            pulse.missed = true;
            data.combo = 0;
            ev_sfx.send(PlaySoundEffect(SoundEffect::Miss));
            spawn_judgment_text(&mut commands, "MISS", MISS_COLOR);
        }
    }
}

fn animate_combo_display(
    time: Res<Time>,
    data: Res<GameData>,
    mut combo_fx: ResMut<ComboDisplayFx>,
    mut combo_q: Query<
        (&mut TextFont, &mut TextColor),
        (With<ComboText>, Without<ComboSubText>),
    >,
    mut combo_sub_q: Query<
        (&mut TextFont, &mut TextColor),
        (With<ComboSubText>, Without<ComboText>),
    >,
    mut miss_flash_q: Query<&mut BackgroundColor, With<MissFlashOverlay>>,
) {
    if combo_fx.last_combo != data.combo {
        if data.combo > combo_fx.last_combo {
            combo_fx.pulse_timer = COMBO_PULSE_DURATION;
        } else if data.combo < combo_fx.last_combo {
            combo_fx.pulse_timer = 0.0;
            if combo_fx.last_combo >= 2 {
                combo_fx.break_timer = COMBO_BREAK_DURATION;
                combo_fx.broken_from = combo_fx.last_combo;
            }
        }
        combo_fx.last_combo = data.combo;
    }

    combo_fx.pulse_timer = (combo_fx.pulse_timer - time.delta_secs()).max(0.0);
    combo_fx.break_timer = (combo_fx.break_timer - time.delta_secs()).max(0.0);
    let pulse = if COMBO_PULSE_DURATION > 0.0 {
        combo_fx.pulse_timer / COMBO_PULSE_DURATION
    } else {
        0.0
    };
    let break_pulse = if COMBO_BREAK_DURATION > 0.0 {
        combo_fx.break_timer / COMBO_BREAK_DURATION
    } else {
        0.0
    };
    let combo_visible = data.combo >= 2;
    let combo_stage = if data.combo >= 20 {
        2
    } else if data.combo >= 10 {
        1
    } else {
        0
    };
    let (base_font_size, base_color, sub_color) = match combo_stage {
        2 => (
            40.0,
            Color::srgba(1.0, 0.93, 0.38, 0.96),
            Color::srgba(1.0, 0.84, 0.30, 0.88),
        ),
        1 => (
            36.0,
            Color::srgba(0.88, 0.94, 1.0, 0.92),
            Color::srgba(0.62, 0.84, 1.0, 0.82),
        ),
        _ => (
            32.0,
            Color::srgba(0.78, 0.90, 1.0, 0.82),
            Color::srgba(0.74, 0.84, 1.0, 0.76),
        ),
    };
    let pulse_boost = match combo_stage {
        2 => 8.0,
        1 => 6.0,
        _ => 4.0,
    };
    let alpha = if combo_visible { base_color.to_srgba().alpha + pulse * 0.08 } else { 0.0 };
    let font_size = if combo_visible { base_font_size + pulse * pulse_boost } else { base_font_size };
    let sub_font_size = if combo_visible { 16.0 + combo_stage as f32 * 2.0 + pulse * 4.0 } else { 16.0 };
    let break_visible = combo_fx.break_timer > 0.0;

    for (mut font, mut color) in &mut combo_q {
        if break_visible {
            font.font_size = 36.0 + break_pulse * 4.0;
            color.0 = Color::srgba(1.0, 0.34 + break_pulse * 0.20, 0.34 + break_pulse * 0.10, 0.78 * break_pulse);
        } else if combo_visible {
            font.font_size = font_size;
            let c = base_color.to_srgba();
            color.0 = Color::srgba(c.red, c.green, c.blue, alpha);
        } else {
            font.font_size = base_font_size;
            color.0 = Color::srgba(0.8, 0.9, 1.0, 0.0);
        }
    }

    for (mut font, mut color) in &mut combo_sub_q {
        if break_visible {
            font.font_size = 18.0;
            color.0 = Color::srgba(1.0, 0.72, 0.72, 0.68 * break_pulse);
        } else if combo_visible {
            font.font_size = sub_font_size;
            let c = sub_color.to_srgba();
            color.0 = Color::srgba(c.red, c.green, c.blue, (c.alpha + pulse * 0.06).clamp(0.0, 1.0));
        } else {
            font.font_size = 20.0;
            color.0 = Color::srgba(0.7, 0.82, 1.0, 0.0);
        }
    }

    for mut flash in &mut miss_flash_q {
        flash.0 = if break_visible {
            Color::srgba(1.0, 0.10, 0.12, 0.16 * break_pulse)
        } else {
            Color::srgba(1.0, 0.2, 0.2, 0.0)
        };
    }
}

fn update_hud(
    data: Res<GameData>,
    mut score_q: Query<&mut Text, (With<ScoreText>, Without<ComboText>, Without<TimerText>)>,
    mut combo_q: Query<&mut Text, (With<ComboText>, Without<ScoreText>, Without<TimerText>, Without<ComboSubText>)>,
    mut combo_sub_q: Query<&mut Text, (With<ComboSubText>, Without<ComboText>, Without<ScoreText>, Without<TimerText>)>,
    mut timer_q: Query<&mut Text, (With<TimerText>, Without<ScoreText>, Without<ComboText>)>,
    combo_fx: Res<ComboDisplayFx>,
) {
    for mut t in &mut score_q { **t = format!("{}", data.score); }
    for mut t in &mut combo_q {
        if data.combo >= 2 {
            **t = format!("{} COMBO", data.combo);
        } else if combo_fx.break_timer > 0.0 {
            **t = "COMBO BREAK".to_string();
        } else {
            **t = String::new();
        }
    }
    for mut t in &mut combo_sub_q {
        if data.combo >= 2 {
            **t = format!("x{}", data.combo_multiplier());
        } else if combo_fx.break_timer > 0.0 {
            **t = format!("lost at {}", combo_fx.broken_from);
        } else {
            **t = String::new();
        }
    }
    for mut t in &mut timer_q { **t = format!("{:.1}", data.time_left.max(0.0)); }
}

fn update_judgment_texts(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut JudgmentText, &mut Transform, &mut TextColor, &mut TextFont, &Text2d)>,
) {
    for (entity, mut jt, mut transform, mut color, mut font, text) in &mut q {
        let is_perfect = text.0 == "PERFECT";
        let rise_speed = if is_perfect { 82.0 } else { 60.0 };
        let base_size = if is_perfect { 48.0 } else { 40.0 };

        jt.timer -= time.delta_secs();
        transform.translation.y += rise_speed * time.delta_secs();
        font.font_size = base_size + (jt.timer / 0.8).clamp(0.0, 1.0) * if is_perfect { 8.0 } else { 4.0 };
        let alpha = (jt.timer / 0.8).clamp(0.0, 1.0);
        let c = color.0.to_srgba();
        color.0 = Color::srgba(c.red, c.green, c.blue, alpha);
        if jt.timer <= 0.0 { commands.entity(entity).despawn_recursive(); }
    }
}

// ── Game Over ────────────────────────────────────────────────────────────────

fn setup_game_over(mut commands: Commands, data: Res<GameData>, font: Res<DisplayFont>) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceEvenly,
                row_gap: Val::Px(SPACE_SM),
                ..default()
            },
            GameOverScreen,
        ))
        .with_children(|p| {
            p.spawn((Text::new("ROUND COMPLETE"), font.at(20.0), TextColor(ACCENT)));
            p.spawn((
                Node {
                    height: Val::Vh(24.0),
                    max_height: Val::Px(208.0),
                    aspect_ratio: Some(1.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(BG_COLOR),
                BorderColor(BORDER),
                BorderRadius::MAX,
                glow(TARGET_GLOW, 14.0),
            ))
            .with_child((Text::new(data.score.to_string()), font.at(DISPLAY_SIZE), TextColor(INK)));

            p.spawn(Node {
                width: Val::Percent(82.0),
                max_width: Val::Px(340.0),
                ..default()
            })
            .with_children(|row| {
                for (label, value) in [("Best combo", data.best_combo), ("High score", data.high_score)] {
                    row.spawn(Node {
                        width: Val::Percent(50.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|stat| {
                        stat.spawn((Text::new(label), font.at(LABEL_SIZE), TextColor(MUTED)));
                        stat.spawn((Text::new(value.to_string()), font.at(28.0), TextColor(INK)));
                    });
                }
            });

            p.spawn(Node {
                width: Val::Percent(82.0),
                max_width: Val::Px(340.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(SPACE_SM),
                margin: UiRect::top(Val::Px(SPACE_SM)),
                ..default()
            })
            .with_children(|row| {
                spawn_button(row, "RETRY", &font, true, RetryButton);

                #[cfg(target_arch = "wasm32")]
                spawn_button(row, "SHARE ON X", &font, false, ShareButton);
            });

            p.spawn((
                Text::new("Space / Enter to retry"),
                font.at(LABEL_SIZE),
                TextColor(MUTED),
            ));
        });
}

fn game_over_input(
    keys:    Res<ButtonInput<KeyCode>>,
    retry_q: Query<&Interaction, (Changed<Interaction>, With<RetryButton>)>,
    mut next: ResMut<NextState<AppState>>,
    mut data: ResMut<GameData>,
    mut ev_sfx: EventWriter<PlaySoundEffect>,
) {
    let retry = keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Enter)
        || retry_q.iter().any(|i| *i == Interaction::Pressed);

    if retry {
        ev_sfx.send(PlaySoundEffect(SoundEffect::UiClick));
        data.reset();
        next.set(AppState::Playing);
    }
}

fn primary_button_visual<M: Component>(
    mut q: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<M>)>,
) {
    for (i, mut bg) in &mut q {
        bg.0 = match i {
            Interaction::Pressed => RETRY_PRESS,
            Interaction::Hovered => RETRY_HOVER,
            Interaction::None    => RETRY_NORMAL,
        };
    }
}

#[cfg(target_arch = "wasm32")]
fn share_button_system(
    mut q: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<ShareButton>)>,
    data: Res<GameData>,
) {
    for (interaction, mut bg) in &mut q {
        bg.0 = match interaction {
            Interaction::Pressed => { open_tweet(data.score, data.best_combo); SHARE_PRESS }
            Interaction::Hovered => SHARE_HOVER,
            Interaction::None    => SHARE_NORMAL,
        };
    }
}

// ── X (Twitter) share ─────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
fn open_tweet(score: u32, best_combo: u32) {
    let text = format!(
        "🎵 PULSE - Score: {score} / Max Combo: {best_combo}\nシンプルなリズムゲームに挑戦！\n{GAME_URL}\n#PULSE #ゲーム制作 #bevy"
    );
    let url = format!(
        "https://twitter.com/intent/tweet?text={}",
        percent_encode(&text)
    );
    if let Some(window) = web_sys::window() {
        let _ = window.open_with_url_and_target_and_features(&url, "_blank", "noopener,noreferrer");
    }
}

#[cfg(target_arch = "wasm32")]
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
