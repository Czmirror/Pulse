//! ネイティブビルド用オーディオバックエンド。
//! bevy_audio + 手続き生成 WAV で効果音を鳴らす。

use bevy::{
    audio::{PlaybackMode, Volume},
    prelude::*,
};
use std::f32::consts::TAU;

use super::{AudioConfig, PlaySoundEffect, SetSfxVolume, SoundEffect};

const SAMPLE_RATE: u32 = 44100;

// ── PCM / WAV 生成 ────────────────────────────────────────────────────────────

fn build_wav(duration_secs: f32, generator: impl Fn(f32, f32) -> f32) -> AudioSource {
    let num_samples = (SAMPLE_RATE as f32 * duration_secs) as u32;
    let data_size = num_samples * 2; // 16-bit mono

    let mut bytes: Vec<u8> = Vec::with_capacity(44 + data_size as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
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

fn sound_hit_good() -> AudioSource {
    build_wav(0.09, |t, dur| {
        ((dur - t) / dur).powf(1.2) * (TAU * 660.0 * t).sin() * 0.75
    })
}

fn sound_miss() -> AudioSource {
    build_wav(0.14, |t, _| {
        (-t / 0.04).exp() * (TAU * 180.0 * t).sin() * 0.65
    })
}

fn sound_combo() -> AudioSource {
    build_wav(0.18, |t, dur| {
        let env = (t / 0.008).min(1.0) * ((dur - t) / dur).powf(0.8).max(0.0);
        let wave = (TAU * 523.25 * t).sin()
            + (TAU * 659.25 * t).sin()
            + (TAU * 784.0 * t).sin();
        wave / 3.0 * env * 0.9
    })
}

// ── Resource ──────────────────────────────────────────────────────────────────

#[derive(Resource)]
struct AudioHandles {
    hit_perfect: Handle<AudioSource>,
    hit_good:    Handle<AudioSource>,
    miss:        Handle<AudioSource>,
    combo:       Handle<AudioSource>,
}

// ── Systems ───────────────────────────────────────────────────────────────────

fn setup_audio(mut commands: Commands, mut audio_assets: ResMut<Assets<AudioSource>>) {
    commands.insert_resource(AudioHandles {
        hit_perfect: audio_assets.add(sound_hit_perfect()),
        hit_good:    audio_assets.add(sound_hit_good()),
        miss:        audio_assets.add(sound_miss()),
        combo:       audio_assets.add(sound_combo()),
    });
    info!("[Native Audio] 4 procedural buffers ready.");
}

fn play_sound(
    mut commands: Commands,
    mut events: EventReader<PlaySoundEffect>,
    handles: Res<AudioHandles>,
    config: Res<AudioConfig>,
) {
    for PlaySoundEffect(effect) in events.read() {
        match effect {
            SoundEffect::HitPerfect { combo } => {
                let pitch = 1.0 + (*combo as f32 * 0.015).min(0.30);
                commands.spawn((
                    AudioPlayer(handles.hit_perfect.clone()),
                    PlaybackSettings {
                        mode: PlaybackMode::Despawn,
                        volume: Volume::new(config.volume),
                        speed: pitch,
                        ..default()
                    },
                ));
            }
            SoundEffect::HitGood { combo } => {
                let pitch = 1.0 + (*combo as f32 * 0.015).min(0.30);
                commands.spawn((
                    AudioPlayer(handles.hit_good.clone()),
                    PlaybackSettings {
                        mode: PlaybackMode::Despawn,
                        volume: Volume::new(config.volume),
                        speed: pitch,
                        ..default()
                    },
                ));
            }
            SoundEffect::Miss | SoundEffect::UiClick => {
                commands.spawn((
                    AudioPlayer(handles.miss.clone()),
                    PlaybackSettings {
                        mode: PlaybackMode::Despawn,
                        volume: Volume::new(config.volume * 0.8),
                        ..default()
                    },
                ));
            }
            SoundEffect::Combo { combo } => {
                let pitch = 1.0 + (*combo as f32 * 0.01).min(0.25);
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
    }
}

fn set_volume(mut config: ResMut<AudioConfig>, mut events: EventReader<SetSfxVolume>) {
    for SetSfxVolume(v) in events.read() {
        config.volume = v.clamp(0.0, 1.0);
        info!("[Native Audio] Volume → {:.2}", config.volume);
    }
}

pub fn build(app: &mut App) {
    app.add_systems(Startup, setup_audio)
        // set_volume を play_sound より先に実行して同フレームの
        // 音量変更が即座に反映されるようにする
        .add_systems(Update, (set_volume, play_sound).chain());
}
