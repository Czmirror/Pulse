//! WASM ビルド用オーディオバックエンド。
//!
//! Bevy の bevy_audio / rodio には依存せず、Web Audio API を web_sys で
//! 直接呼び出す。AudioBuffer を Startup 時に手続き生成し、イベントを受け取るたびに
//! AudioBufferSourceNode を生成して再生する（同一音の重ね再生も可能）。
//!
//! # 各段階のログ
//! - `[WASM Audio] Setup` … AudioContext 生成 / バッファ作成
//! - `[WASM Audio] Unlock` … resume() 呼び出し
//! - `[WASM Audio] Play` … 再生開始 / エラー
//! - `[WASM Audio] Volume` … 音量変更

use bevy::prelude::*;
use std::f32::consts::TAU;
use wasm_bindgen::JsValue;
use web_sys::{AudioBuffer, AudioBufferSourceNode, AudioContext, GainNode};

use super::{AudioConfig, PlaySoundEffect, SetSfxVolume, SoundEffect};

const SAMPLE_RATE: f32 = 44100.0;

// ── 内部構造体 ────────────────────────────────────────────────────────────────

struct WasmAudioInner {
    ctx:         AudioContext,
    master_gain: GainNode,
    hit_perfect: AudioBuffer,
    hit_good:    AudioBuffer,
    miss:        AudioBuffer,
    combo:       AudioBuffer,
}

// SAFETY: WASM は常にシングルスレッド。JsValue は !Send だが実際に
// 並行アクセスは発生しない。
unsafe impl Send for WasmAudioInner {}
unsafe impl Sync for WasmAudioInner {}

// ── Resource ──────────────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct WasmAudio(Option<WasmAudioInner>);

// ── PCM サンプル生成（float32 mono）────────────────────────────────────────

fn gen_samples(dur: f32, f: impl Fn(f32, f32) -> f32) -> Vec<f32> {
    let n = (SAMPLE_RATE * dur) as usize;
    (0..n)
        .map(|i| f(i as f32 / SAMPLE_RATE, dur).clamp(-1.0, 1.0))
        .collect()
}

fn samples_hit_perfect() -> Vec<f32> {
    gen_samples(0.10, |t, dur| {
        let env = if t < 0.003 {
            t / 0.003
        } else {
            ((dur - t) / (dur - 0.003)).powf(1.5).max(0.0)
        };
        (0.55 * (TAU * 880.0 * t).sin() + 0.45 * (TAU * 1320.0 * t).sin()) * env * 0.9
    })
}

fn samples_hit_good() -> Vec<f32> {
    gen_samples(0.09, |t, dur| {
        ((dur - t) / dur).powf(1.2) * (TAU * 660.0 * t).sin() * 0.75
    })
}

fn samples_miss() -> Vec<f32> {
    gen_samples(0.14, |t, _| {
        (-t / 0.04).exp() * (TAU * 180.0 * t).sin() * 0.65
    })
}

fn samples_combo() -> Vec<f32> {
    gen_samples(0.18, |t, dur| {
        let env = (t / 0.008).min(1.0) * ((dur - t) / dur).powf(0.8).max(0.0);
        ((TAU * 523.25 * t).sin() + (TAU * 659.25 * t).sin() + (TAU * 784.0 * t).sin())
            / 3.0
            * env
            * 0.9
    })
}

// ── AudioBuffer ヘルパー ───────────────────────────────────────────────────────

/// Vec<f32> から AudioBuffer を生成する（同期・コピー不要）。
fn make_buffer(
    ctx: &AudioContext,
    samples: &[f32],
    label: &str,
) -> Result<AudioBuffer, JsValue> {
    let n = samples.len() as u32;
    let buf = ctx.create_buffer(1, n, SAMPLE_RATE)?;

    // web-sys の copy_to_channel は &[f32] を直接受け取る
    buf.copy_to_channel(samples, 0)
        .map_err(|e| {
            error!(
                "[WASM Audio] Setup: copy_to_channel '{}' failed: {:?}",
                label, e
            );
            e
        })?;

    info!(
        "[WASM Audio] Setup: buffer '{}' created ({} samples, {:.0} ms)",
        label,
        n,
        n as f32 / SAMPLE_RATE * 1000.0
    );
    Ok(buf)
}

// ── セットアップ ──────────────────────────────────────────────────────────────

fn create_inner(volume: f32) -> Result<WasmAudioInner, JsValue> {
    let ctx = AudioContext::new().map_err(|e| {
        error!("[WASM Audio] Setup: AudioContext::new() failed: {:?}", e);
        e
    })?;
    info!("[WASM Audio] Setup: AudioContext created (state={})", ctx_state_str(&ctx));

    let master_gain: GainNode = ctx.create_gain().map_err(|e| {
        error!("[WASM Audio] Setup: create_gain() failed: {:?}", e);
        e
    })?;
    master_gain.gain().set_value(volume);

    let dest = ctx.destination();
    master_gain
        .connect_with_audio_node(dest.as_ref())
        .map_err(|e| {
            error!("[WASM Audio] Setup: GainNode→destination connect failed: {:?}", e);
            e
        })?;
    info!(
        "[WASM Audio] Setup: master GainNode connected, volume={:.2}",
        volume
    );

    let hp_samples = samples_hit_perfect();
    let hg_samples = samples_hit_good();
    let ms_samples = samples_miss();
    let co_samples = samples_combo();

    let hit_perfect = make_buffer(&ctx, &hp_samples, "hit_perfect")?;
    let hit_good    = make_buffer(&ctx, &hg_samples, "hit_good")?;
    let miss        = make_buffer(&ctx, &ms_samples, "miss")?;
    let combo       = make_buffer(&ctx, &co_samples, "combo")?;

    info!("[WASM Audio] Setup: all 4 buffers ready ✓");
    Ok(WasmAudioInner {
        ctx,
        master_gain,
        hit_perfect,
        hit_good,
        miss,
        combo,
    })
}

fn setup_audio_wasm(mut commands: Commands, config: Res<AudioConfig>) {
    match create_inner(config.volume) {
        Ok(inner) => {
            commands.insert_resource(WasmAudio(Some(inner)));
        }
        Err(e) => {
            warn!(
                "[WASM Audio] Setup FAILED ({:?}). Game will run silently.",
                e
            );
            commands.insert_resource(WasmAudio(None));
        }
    }
}

// ── AudioContext アンロック（初回入力時）─────────────────────────────────────

/// ブラウザの Autoplay Policy でコンテキストが suspended になる場合があるため、
/// 任意の入力を検知したら resume() を呼ぶ。
fn unlock_audio_wasm(
    audio: Option<Res<WasmAudio>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touch: Res<Touches>,
    keys:  Res<ButtonInput<KeyCode>>,
) {
    let Some(audio) = audio else { return };
    let Some(inner) = &audio.0 else { return };

    let any_input = mouse.get_just_pressed().next().is_some()
        || touch.any_just_pressed()
        || keys.get_just_pressed().next().is_some();

    if !any_input {
        return;
    }

    let state_before = ctx_state_str(&inner.ctx);
    match inner.ctx.resume() {
        Ok(_promise) => {
            info!(
                "[WASM Audio] Unlock: resume() called (was {})",
                state_before
            );
        }
        Err(e) => {
            warn!("[WASM Audio] Unlock: resume() error: {:?}", e);
        }
    }
}

// ── 再生 ──────────────────────────────────────────────────────────────────────

fn play_buffer(
    ctx: &AudioContext,
    gain: &GainNode,
    buf: &AudioBuffer,
    pitch: f32,
    label: &str,
) -> Result<(), JsValue> {
    // コンテキストが suspended の場合は resume を試みてから再生
    let state = ctx_state_str(ctx);
    if state == "Suspended" {
        info!(
            "[WASM Audio] Play: context Suspended before '{}', calling resume()",
            label
        );
        let _ = ctx.resume();
        // resume は非同期。今フレームは再生をスキップせず続行する
        // （Chrome など多くのブラウザでは resume 後すぐ鳴る）
    }

    let src: AudioBufferSourceNode = ctx.create_buffer_source().map_err(|e| {
        error!("[WASM Audio] Play: create_buffer_source() failed: {:?}", e);
        e
    })?;

    src.set_buffer(Some(buf));
    src.playback_rate().set_value(pitch);

    src.connect_with_audio_node(gain.as_ref()).map_err(|e| {
        error!(
            "[WASM Audio] Play: src→gain connect failed for '{}': {:?}",
            label, e
        );
        e
    })?;

    src.start().map_err(|e| {
        error!("[WASM Audio] Play: start() failed for '{}': {:?}", label, e);
        e
    })?;

    // src は JS GC が管理するので Rust 側で drop しても再生は続く
    Ok(())
}

fn play_sound_wasm(
    audio: Option<Res<WasmAudio>>,
    mut events: EventReader<PlaySoundEffect>,
) {
    let Some(audio) = audio else { return };
    let Some(inner) = &audio.0 else { return };

    for PlaySoundEffect(effect) in events.read() {
        let result = match effect {
            SoundEffect::HitPerfect { combo } => {
                let pitch = 1.0 + (*combo as f32 * 0.015).min(0.30);
                play_buffer(&inner.ctx, &inner.master_gain, &inner.hit_perfect, pitch, "hit_perfect")
            }
            SoundEffect::HitGood { combo } => {
                let pitch = 1.0 + (*combo as f32 * 0.015).min(0.30);
                play_buffer(&inner.ctx, &inner.master_gain, &inner.hit_good, pitch, "hit_good")
            }
            SoundEffect::Miss | SoundEffect::UiClick => {
                play_buffer(&inner.ctx, &inner.master_gain, &inner.miss, 1.0, "miss/ui_click")
            }
            SoundEffect::Combo { combo } => {
                let pitch = 1.0 + (*combo as f32 * 0.01).min(0.25);
                play_buffer(&inner.ctx, &inner.master_gain, &inner.combo, pitch, "combo")
            }
        };

        if let Err(e) = result {
            warn!("[WASM Audio] Play: error for {:?}: {:?}", effect, e);
        }
    }
}

// ── 音量変更 ──────────────────────────────────────────────────────────────────

fn set_volume_wasm(
    audio: Option<Res<WasmAudio>>,
    mut config: ResMut<AudioConfig>,
    mut events: EventReader<SetSfxVolume>,
) {
    for SetSfxVolume(v) in events.read() {
        let v = v.clamp(0.0, 1.0);
        config.volume = v;

        if let Some(audio) = &audio {
            if let Some(inner) = &audio.0 {
                inner.master_gain.gain().set_value(v);
            }
        }

        info!("[WASM Audio] Volume: → {:.2} ({:.0}%)", v, v * 100.0);
        save_volume_to_storage(v);
    }
}

// ── localStorage ─────────────────────────────────────────────────────────────

/// SE 音量を localStorage に保存する。
fn save_volume_to_storage(volume: f32) {
    let Some(window) = web_sys::window() else { return };
    let Ok(Some(storage)) = window.local_storage() else { return };
    if let Err(e) = storage.set_item("pulse_sfx_volume", &format!("{:.4}", volume)) {
        warn!("[WASM Audio] localStorage: save failed: {:?}", e);
    }
}

/// localStorage から SE 音量を復元する。
pub fn load_volume_from_storage() -> Option<f32> {
    let window = web_sys::window()?;
    let storage = window.local_storage().ok()??;
    let s = storage.get_item("pulse_sfx_volume").ok()??;
    let v: f32 = s.parse().ok()?;
    info!("[WASM Audio] localStorage: loaded volume={:.4}", v);
    Some(v.clamp(0.0, 1.0))
}

// ── ユーティリティ ──────────────────────────────────────────────────────────

fn ctx_state_str(ctx: &AudioContext) -> &'static str {
    let s = ctx.state();
    if s == web_sys::AudioContextState::Running   { "Running"   }
    else if s == web_sys::AudioContextState::Suspended { "Suspended" }
    else                                               { "Closed"    }
}

// ── プラグインエントリ ────────────────────────────────────────────────────────

pub fn build(app: &mut App) {
    app.add_systems(Startup, setup_audio_wasm)
        .add_systems(
            Update,
            (unlock_audio_wasm, play_sound_wasm, set_volume_wasm),
        );
}
