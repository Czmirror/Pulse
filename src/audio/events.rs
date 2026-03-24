use bevy::prelude::*;

/// 再生する効果音の種類。
#[derive(Debug, Clone, Copy)]
pub enum SoundEffect {
    HitPerfect { combo: u32 },
    HitGood    { combo: u32 },
    Miss,
    Combo      { combo: u32 },
    /// タイトル画面ボタン操作音
    UiClick,
}

/// この Event を送ると音が鳴る。
#[derive(Event)]
pub struct PlaySoundEffect(pub SoundEffect);

/// SE 音量を 0.0–1.0 で変更する Event。
#[derive(Event)]
pub struct SetSfxVolume(pub f32);
