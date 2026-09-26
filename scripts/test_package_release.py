"""Release archives must accept Cargo's normalized pre-1980 license timestamps."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile

SCRIPT = Path(__file__).with_name('package-release.py').resolve()

class ReleaseArchiveTests(unittest.TestCase):
    def test_windows_zip_preserves_license_with_old_timestamp(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'Cargo.toml').write_text('[workspace.package]\nversion="0.1.0"\n')
            (root / 'Cargo.lock').write_text('[[package]]\nname="legacy"\nversion="1.0"\nsource="registry+https://github.com/rust-lang/crates.io-index"\n')
            for name in ['README.md', 'LICENSE-MIT', 'LICENSE-APACHE']:
                (root / name).write_text(name)
            binary = root / 'target/x86_64-pc-windows-msvc/release/kata.exe'
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b'fixture binary')
            cargo = root / 'cargo'
            license_file = cargo / 'registry/src/example/legacy-1.0/LICENSE'
            license_file.parent.mkdir(parents=True)
            license_file.write_text('Fixture license notice')
            os.utime(license_file, (0, 0))
            result = subprocess.run([sys.executable, str(SCRIPT), '--target', 'x86_64-pc-windows-msvc', '--os', 'windows', '--arch', 'x86_64'],
                                    cwd=root, env={**os.environ, 'CARGO_HOME': str(cargo)}, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            with zipfile.ZipFile(root / 'dist/kata-0.1.0-windows-x86_64.zip') as archive:
                self.assertEqual(archive.read('THIRD-PARTY-LICENSES/legacy-1.0/LICENSE'), b'Fixture license notice')
                self.assertEqual(archive.read('kata.exe'), b'fixture binary')

if __name__ == '__main__':
    unittest.main()
