# Pulse

ブラウザで動くシンプルなリズムゲーム。
Bevy 0.15 + WebAssembly 製。PC・スマートフォン両対応。

## ゲームルール

- 画面中央の**ターゲットリング**に向かって外側から**パルス（円形の波）**が迫ってくる
- パルスがターゲットリングと重なった瞬間に**タップ / クリック**する
- 精度に応じてスコアが加算される：
  - **PERFECT** : +100 × コンボ倍率
  - **GOOD**    : +50 × コンボ倍率
  - **MISS**    : コンボリセット
- コンボが続くと倍率アップ（最大 ×4）
- 30 秒間のスコアを競う

## ビルド方法

```bash
# 依存ツールを含めて一括ビルド
./build_web.sh

# ローカルサーバで動作確認
python3 -m http.server 8080 --directory web
# ブラウザで http://localhost:8080 を開く
```

### 必要なもの

- Rust (stable, 1.85+)
- `wasm32-unknown-unknown` target（スクリプトが自動インストール）
- `wasm-bindgen-cli`（Cargo.lockと一致するバージョンをスクリプトがインストール）

> **ネイティブ Linux ビルドについて**
> `bevy_audio` は Linux では ALSA を使用するため、ネイティブビルド時は
> `libasound2-dev` が必要です。
> ```bash
> sudo apt-get install libasound2-dev
> ```
> ブラウザ向け WASM ビルド（`./build_web.sh`）および GitHub Actions CI は
> WASM ターゲットのみでビルドするため ALSA は不要です。

## 技術スタック

| ツール | バージョン |
|---|---|
| Bevy | 0.15 |
| wasm-bindgen | 0.2 |
| Rust edition | 2021 |

## フォント

画面表示に Inter Regular を使用しています。配布元は cosmic-text 0.12.1 に同梱された Inter で、ライセンスは [SIL Open Font License 1.1](assets/fonts/Inter-LICENSE) です。

## 起動処理の検証

Node.js 22以上で、配信HTML内の起動処理を検証します。

    node --experimental-vm-modules --test tests/startup.test.cjs

ブラウザAPIとゲーム初期化をテスト用に置き換え、読み込み失敗時の案内と再読み込みを確認します。画面の描画とタッチ操作は別途確認が必要です。

Webビルドスクリプトの検証は次のコマンドで実行できます。外部のツール導入や通信をテスト用の処理に置き換えて確認します。

    python3 -m unittest discover -s tests -p 'build_web_test.py'
