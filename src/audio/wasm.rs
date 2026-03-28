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

// ── 内部構造体 ────────────────────────────────────────────────────────────────

/// `WasmAudioInner` は web_sys のオーディオハンドルを保持する。
/// web_sys 型は !Send なので NonSend リソースとして管理する。
struct WasmAudioInner {
    ctx:         AudioContext,
    master_gain: GainNode,
    hit_perfect: AudioBuffer,
    hit_good:    AudioBuffer,
    miss:        AudioBuffer,
    combo:       AudioBuffer,
}

// ── NonSend リソース ──────────────────────────────────────────────────────────

/// NonSend リソース。derive(Resource) は付けず、
/// world.insert_non_send_resource で登録する。
pub struct WasmAudio(Option<WasmAudioInner>);

// ── PCM サンプル生成（float32 mono）────────────────────────────────────────

fn gen_samples(dur: f32, sample_rate: f32, f: impl Fn(f32, f32) -> f32) -> Vec<f32> {
    let n = (sample_rate * dur) as usize;
    (0..n)
        .map(|i| f(i as f32 / sample_rate, dur).clamp(-1.0, 1.0))
        .collect()
}

fn samples_hit_perfect(sr: f32) -> Vec<f32> {
    gen_samples(0.10, sr, |t, dur| {
        let env = if t < 0.003 {
            t / 0.003
        } else {
            ((dur - t) / (dur - 0.003)).powf(1.5).max(0.0)
        };
        (0.55 * (TAU * 880.0 * t).sin() + 0.45 * (TAU * 1320.0 * t).sin()) * env * 0.9
    })
}

fn samples_hit_good(sr: f32) -> Vec<f32> {
    gen_samples(0.09, sr, |t, dur| {
        ((dur - t) / dur).powf(1.2) * (TAU * 660.0 * t).sin() * 0.75
    })
}

fn samples_miss(sr: f32) -> Vec<f32> {
    gen_samples(0.14, sr, |t, _| {
        (-t / 0.04).exp() * (TAU * 180.0 * t).sin() * 0.65
    })
}

fn samples_combo(sr: f32) -> Vec<f32> {
    gen_samples(0.18, sr, |t, dur| {
        let env = (t / 0.008).min(1.0) * ((dur - t) / dur).powf(0.8).max(0.0);
        ((TAU * 523.25 * t).sin() + (TAU * 659.25 * t).sin() + (TAU * 784.0 * t).sin())
            / 3.0
            * env
            * 0.9
    })
}

// ── AudioBuffer ヘルパー ───────────────────────────────────────────────────────

fn make_buffer(
    ctx: &AudioContext,
    samples: &[f32],
    sample_rate: f32,
    label: &str,
) -> Result<AudioBuffer, JsValue> {
    let n = samples.len() as u32;
    let buf = ctx.create_buffer(1, n, sample_rate)?;

    buf.copy_to_channel(samples, 0).map_err(|e| {
        error!("[WASM Audio] Setup: copy_to_channel '{}' failed: {:?}", label, e);
        e
    })?;

    info!(
        "[WASM Audio] Setup: buffer '{}' created ({} samples, {:.0} ms @ {:.0} Hz)",
        label,
        n,
        n as f32 / sample_rate * 1000.0,
        sample_rate,
    );
    Ok(buf)
}

// ── セットアップ ──────────────────────────────────────────────────────────────

fn create_inner(volume: f32) -> Result<WasmAudioInner, JsValue> {
    let ctx = AudioContext::new().map_err(|e| {
        error!("[WASM Audio] Setup: AudioContext::new() failed: {:?}", e);
        e
    })?;

    // ブラウザの実際のサンプルレートを使用してリサンプリングを回避する
    let sample_rate = ctx.sample_rate();
    info!(
        "[WASM Audio] Setup: AudioContext created (state={}, sample_rate={} Hz)",
        ctx_state_str(&ctx),
        sample_rate,
    );

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
    info!("[WASM Audio] Setup: master GainNode connected, volume={:.2}", volume);

    let hit_perfect = make_buffer(&ctx, &samples_hit_perfect(sample_rate), sample_rate, "hit_perfect")?;
    let hit_good    = make_buffer(&ctx, &samples_hit_good(sample_rate),    sample_rate, "hit_good")?;
    let miss        = make_buffer(&ctx, &samples_miss(sample_rate),        sample_rate, "miss")?;
    let combo       = make_buffer(&ctx, &samples_combo(sample_rate),       sample_rate, "combo")?;

    info!("[WASM Audio] Setup: all 4 buffers ready ✓");
    Ok(WasmAudioInner { ctx, master_gain, hit_perfect, hit_good, miss, combo })
}

/// モバイルの autoplay 制約に合わせて、AudioContext 自体も
/// 最初のユーザー操作時まで遅延初期化する。
fn setup_audio_wasm(world: &mut World) {
    world.insert_non_send_resource(WasmAudio(None));
}

// ── AudioContext アンロック（初回入力時）─────────────────────────────────────

fn unlock_audio_wasm(
    audio: Option<NonSendMut<WasmAudio>>,
    config: Res<AudioConfig>,
    mouse: Res<ButtonInput<MouseButton>>,
    touch: Res<Touches>,
    keys:  Res<ButtonInput<KeyCode>>,
) {
    let any_input = mouse.get_just_pressed().next().is_some()
        || touch.any_just_pressed()
        || keys.get_just_pressed().next().is_some();

    if !any_input { return; }

    let Some(mut audio) = audio else { return };

    if audio.0.is_none() {
        match create_inner(config.volume) {
            Ok(inner) => {
                info!("[WASM Audio] Unlock: initialized AudioContext on first user gesture");
                audio.0 = Some(inner);
            }
            Err(e) => {
                warn!("[WASM Audio] Unlock: setup failed ({:?}). Game will run silently.", e);
                return;
            }
        }
    }

    let Some(inner) = &audio.0 else { return };

    let state_before = ctx_state_str(&inner.ctx);
    match inner.ctx.resume() {
        Ok(_) => info!("[WASM Audio] Unlock: resume() called (was {})", state_before),
        Err(e) => warn!("[WASM Audio] Unlock: resume() error: {:?}", e),
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
    if ctx_state_str(ctx) == "Suspended" {
        info!("[WASM Audio] Play: context Suspended before '{}', calling resume()", label);
        let _ = ctx.resume();
    }

    let src: AudioBufferSourceNode = ctx.create_buffer_source().map_err(|e| {
        error!("[WASM Audio] Play: create_buffer_source() failed: {:?}", e);
        e
    })?;

    src.set_buffer(Some(buf));
    src.playback_rate().set_value(pitch);
    src.connect_with_audio_node(gain.as_ref()).map_err(|e| {
        error!("[WASM Audio] Play: src→gain connect failed for '{}': {:?}", label, e);
        e
    })?;
    src.start().map_err(|e| {
        error!("[WASM Audio] Play: start() failed for '{}': {:?}", label, e);
        e
    })?;

    Ok(())
}

fn play_sound_wasm(
    audio: Option<NonSend<WasmAudio>>,
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
    audio: Option<NonSend<WasmAudio>>,
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

fn save_volume_to_storage(volume: f32) {
    let Some(window) = web_sys::window() else { return };
    let Ok(Some(storage)) = window.local_storage() else { return };
    if let Err(e) = storage.set_item("pulse_sfx_volume", &format!("{:.4}", volume)) {
        warn!("[WASM Audio] localStorage: save failed: {:?}", e);
    }
}

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
    if s == web_sys::AudioContextState::Running        { "Running"   }
    else if s == web_sys::AudioContextState::Suspended { "Suspended" }
    else                                               { "Closed"    }
}

// ── プラグインエントリ ────────────────────────────────────────────────────────

pub fn build(app: &mut App) {
    app.add_systems(Startup, setup_audio_wasm)
        .add_systems(
            Update,
            // set_volume_wasm を play_sound_wasm より先に実行して
            // 同フレームのボリューム変更が即座に反映されるようにする
            (unlock_audio_wasm, set_volume_wasm, play_sound_wasm).chain(),
        );
}
