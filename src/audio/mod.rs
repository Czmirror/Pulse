pub mod events;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use events::*;
use bevy::prelude::*;

pub const DEFAULT_VOLUME: f32 = 0.6;

// ── AudioConfig ───────────────────────────────────────────────────────────────

/// SE 音量設定。全プラットフォーム共通の Resource。
#[derive(Resource)]
pub struct AudioConfig {
    pub volume: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        // WASM では localStorage から復元を試みる
        #[cfg(target_arch = "wasm32")]
        let volume = wasm::load_volume_from_storage().unwrap_or(DEFAULT_VOLUME);
        #[cfg(not(target_arch = "wasm32"))]
        let volume = DEFAULT_VOLUME;
        Self { volume }
    }
}

// ── AudioPlugin ───────────────────────────────────────────────────────────────

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<PlaySoundEffect>()
            .add_event::<SetSfxVolume>()
            .init_resource::<AudioConfig>();

        #[cfg(not(target_arch = "wasm32"))]
        native::build(app);

        #[cfg(target_arch = "wasm32")]
        wasm::build(app);
    }
}
