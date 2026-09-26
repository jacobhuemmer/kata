#!/usr/bin/env python3
"""Package a built binary and license notices with deterministic asset names."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import tarfile
import tempfile
import tomllib
import zipfile

p = argparse.ArgumentParser()
p.add_argument('--target', required=True)
p.add_argument('--os', required=True)
p.add_argument('--arch', required=True)
args = p.parse_args()
version = tomllib.loads(Path('Cargo.toml').read_text())['workspace']['package']['version']
binary = 'kata.exe' if args.os == 'windows' else 'kata'
out = Path('dist')
out.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory() as temporary:
    stage = Path(temporary)
    shutil.copy2(Path('target') / args.target / 'release' / binary, stage / binary)
    for name in ['LICENSE-MIT', 'LICENSE-APACHE', 'README.md']:
        shutil.copy2(name, stage / name)
    registry = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo')) / 'registry' / 'src'
    packages = tomllib.loads(Path('Cargo.lock').read_text())['package']
    for package in packages:
        if not package.get('source', '').startswith('registry+'):
            continue
        name = f"{package['name']}-{package['version']}"
        for source in registry.glob('*/' + name):
            for license_file in source.rglob('*'):
                if license_file.is_file() and license_file.name.upper().startswith(('LICENSE', 'COPYING', 'NOTICE', 'COPYRIGHT')):
                    dest = stage / 'THIRD-PARTY-LICENSES' / name / license_file.relative_to(source)
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(license_file, dest)
    name = f'kata-{version}-{args.os}-{args.arch}'
    if args.os == 'windows':
        asset = out / (name + '.zip')
        with zipfile.ZipFile(asset, 'w', zipfile.ZIP_DEFLATED) as archive:
            for file in stage.rglob('*'):
                if file.is_file():
                    archive.write(file, file.relative_to(stage))
    else:
        asset = out / (name + '.tar.gz')
        with tarfile.open(asset, 'w:gz') as archive:
            for file in stage.iterdir():
                archive.add(file, arcname=file.name)
    digest = hashlib.sha256(asset.read_bytes()).hexdigest()
    (out / (asset.name + '.sha256')).write_text(f'{digest}  {asset.name}\n')
    print(asset)
