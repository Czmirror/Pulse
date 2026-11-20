"""Webビルドのツール選択と失敗時の停止を、外部通信せず検証する。"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class WebBuildTest(unittest.TestCase):
    def run_build(self, installed=None, install_fails=False, lock_version="0.2.114"):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tools = root / "bin"
            tools.mkdir()
            script = Path(__file__).resolve().parents[1] / "build_web.sh"
            shutil.copy(script, root)
            (root / "Cargo.lock").write_text(
                '[[package]]\nname = "wasm-bindgen"\nversion = "' + lock_version + '"\n'
                '[[package]]\nname = "wasm-bindgen-futures"\nversion = "0.4.64"\n'
            )
            # PATHを隔離して、ホストのCLIやネットワークを使わない。
            for command in ["bash", "awk"]:
                (tools / command).symlink_to(shutil.which(command))
            bindgen = '#!/bin/bash\nif [[ "$1" == "--version" ]]; then\n'
            bindgen += '  echo "wasm-bindgen 0.2.114"\nelse\n'
            bindgen += '  echo "bindgen $*" >> "$BUILD_LOG"\nfi\n'
            cargo = '#!/bin/bash\necho "cargo $*" >> "$BUILD_LOG"\n'
            cargo += 'if [[ "$1" == "install" ]]; then\n'
            cargo += '  if [[ "$INSTALL_FAILS" == "1" ]]; then exit 1; fi\n'
            cargo += '  /bin/cp "$BINDGEN_STUB" "$TOOLS_DIR/wasm-bindgen"\nfi\n'
            (root / "bindgen-stub").write_text(bindgen)
            (root / "bindgen-stub").chmod(0o755)
            for name, content in [
                ("rustup", '#!/bin/bash\necho "rustup $*" >> "$BUILD_LOG"\n'),
                ("cargo", cargo),
            ]:
                (tools / name).write_text(content)
                (tools / name).chmod(0o755)
            if installed is not None:
                (tools / "wasm-bindgen").write_text(
                    bindgen.replace("0.2.114", installed)
                )
                (tools / "wasm-bindgen").chmod(0o755)
            env = dict(os.environ, PATH=str(tools), BUILD_LOG=str(root / "log"),
                       TOOLS_DIR=str(tools), BINDGEN_STUB=str(root / "bindgen-stub"),
                       INSTALL_FAILS="1" if install_fails else "0")
            result = subprocess.run([str(tools / "bash"), "build_web.sh"],
                                    cwd=root, env=env, capture_output=True, text=True)
            log = (root / "log").read_text() if (root / "log").exists() else ""
            return result, log

    def test_matching_cli_is_reused(self):
        result, log = self.run_build("0.2.114")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("cargo install", log)
        self.assertIn("cargo build --locked --release", log)
        self.assertIn("bindgen --out-dir web", log)

    def test_missing_or_mismatched_cli_installs_locked_version(self):
        for version in [None, "0.2.100"]:
            with self.subTest(version=version):
                result, log = self.run_build(version)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("cargo install wasm-bindgen-cli --version =0.2.114 --locked", log)

    def test_install_failure_stops_build(self):
        result, log = self.run_build("0.2.100", install_fails=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("cargo build", log)
        self.assertNotIn("bindgen --out-dir", log)

    def test_invalid_lock_version_stops_before_install(self):
        result, log = self.run_build(lock_version="invalid")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(log, "")
