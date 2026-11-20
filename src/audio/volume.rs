//! 音量の保存値と変更イベントに共通の数値検証を適用する。

/// 有限の音量を0〜1に収める。NaNと無限大は受け付けない。
pub fn validated_volume(value: f32) -> Option<f32> {
    value.is_finite().then(|| value.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::validated_volume;

    #[test]
    fn accepts_existing_decimal_settings() {
        for text in ["0", "0.0000", "0.6000", "1.0000"] {
            let value: f32 = text.parse().unwrap();
            assert_eq!(validated_volume(value), Some(value));
        }
    }

    #[test]
    fn clamps_finite_values_to_existing_limits() {
        assert_eq!(validated_volume(-0.1), Some(0.0));
        assert_eq!(validated_volume(1.1), Some(1.0));
    }

    #[test]
    fn rejects_non_finite_values_from_storage_and_events() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(validated_volume(value), None);
        }
        assert_eq!(validated_volume("NaN".parse().unwrap()), None);
    }
}
