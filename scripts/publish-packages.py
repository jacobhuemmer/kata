#!/usr/bin/env python3
"""Generate package manifests from release checksums; optionally publish them via gh."""
import argparse
import base64
import json
from pathlib import Path
import subprocess
import tomllib

OWNER = 'jacobhuemmer'
REPO = f'{OWNER}/kata'
version = tomllib.loads(Path('Cargo.toml').read_text())['workspace']['package']['version']
base = f'https://github.com/{REPO}/releases/download/v{version}'
sums = {line.split()[1]: line.split()[0] for line in Path('dist/SHA256SUMS').read_text().splitlines()}
output = Path('dist/packages')
output.mkdir(parents=True, exist_ok=True)

def asset(os_name, arch):
    ext = 'zip' if os_name == 'windows' else 'tar.gz'
    name = f'kata-{version}-{os_name}-{arch}.{ext}'
    return f'{base}/{name}', sums[name]

formula = f'''class Kata < Formula
  desc "Script library and MCP server for reusable automation"
  homepage "https://github.com/{REPO}"
  version "{version}"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/{REPO}.git", branch: "main"

  depends_on "rust" => :build if build.head?
  depends_on "git"

'''
for os_name, architectures in [('darwin', [('arm', 'arm64'), ('intel', 'x86_64')]), ('linux', [('arm', 'aarch64'), ('intel', 'x86_64')])]:
    formula += '  on_' + ('macos' if os_name == 'darwin' else 'linux') + ' do\n'
    for cpu, arch in architectures:
        url, sha = asset(os_name, arch)
        formula += f'    on_{cpu} do\n      url "{url}"\n      sha256 "{sha}"\n    end\n'
    formula += '  end\n\n'
formula += '''  def install
    if build.head?
      system "cargo", "install", *std_cargo_args(path: "crates/kata-cli")
    else
      bin.install "kata"
      pkgshare.install "THIRD-PARTY-LICENSES" if File.directory?("THIRD-PARTY-LICENSES")
    end
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/kata --version")
    ENV["KATA_HOME"] = testpath/"kata-home"
    assert_match "hello", shell_output("#{bin}/kata --plain run starter/hello").downcase
  end
end
'''
(output / 'kata.rb').write_text(formula)
url, sha = asset('windows', 'x86_64')
scoop = {'version': version, 'description': 'Script library and MCP server for reusable automation',
         'homepage': f'https://github.com/{REPO}', 'license': 'MIT|Apache-2.0', 'depends': ['git', 'extras/vcredist2022'],
         'architecture': {'64bit': {'url': url, 'hash': sha}}, 'bin': 'kata.exe',
         'notes': 'Run Kata from Git Bash. POSIX scripts require sh and their tools on PATH.'}
(output / 'kata.json').write_text(json.dumps(scoop, indent=2) + '\n')
identifier = 'JacobHuemmer.Kata'
winget = output / 'winget'
winget.mkdir(exist_ok=True)
common = f'PackageIdentifier: {identifier}\nPackageVersion: {version}\n'
(winget / f'{identifier}.yaml').write_text(common + 'DefaultLocale: en-US\nManifestType: version\nManifestVersion: 1.6.0\n')
(winget / f'{identifier}.locale.en-US.yaml').write_text(common + f'''PackageLocale: en-US
Publisher: Jacob Huemmer
PublisherUrl: https://github.com/{OWNER}
PackageName: Kata
PackageUrl: https://github.com/{REPO}
License: MIT OR Apache-2.0
LicenseUrl: https://github.com/{REPO}/blob/main/LICENSE-MIT
ShortDescription: Script library and MCP server for reusable automation
Description: Run reusable POSIX scripts from the CLI or an MCP client. On Windows, run Kata from Git Bash.
ReleaseNotesUrl: https://github.com/{REPO}/releases/tag/v{version}
ManifestType: defaultLocale
ManifestVersion: 1.6.0
''')
(winget / f'{identifier}.installer.yaml').write_text(common + f'''InstallerType: zip
NestedInstallerType: portable
NestedInstallerFiles:
  - RelativeFilePath: kata.exe
    PortableCommandAlias: kata
Dependencies:
  PackageDependencies:
    - PackageIdentifier: Git.Git
    - PackageIdentifier: Microsoft.VCRedist.2015+.x64
Installers:
  - Architecture: x64
    InstallerUrl: {url}
    InstallerSha256: "{sha.upper()}"
ManifestType: installer
ManifestVersion: 1.6.0
''')

def api(endpoint, method='GET', payload=None):
    command = ['gh', 'api', endpoint, '--method', method]
    if payload is not None:
        command += ['--input', '-']
    result = subprocess.run(command, input=json.dumps(payload) if payload is not None else None,
                            capture_output=True, text=True, check=True)
    return json.loads(result.stdout) if result.stdout else None

def publish_file(repository, path, local):
    existing = subprocess.run(['gh', 'api', f'repos/{OWNER}/{repository}/contents/{path}'], capture_output=True, text=True)
    payload = {'message': f'chore: update kata to {version}', 'content': base64.b64encode(local.read_bytes()).decode()}
    if existing.returncode == 0:
        prior = json.loads(existing.stdout)
        if base64.b64decode(prior['content']) == local.read_bytes():
            return
        payload['sha'] = prior['sha']
    api(f'repos/{OWNER}/{repository}/contents/{path}', 'PUT', payload)
    print(f'Updated {repository}/{path}')

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--publish', action='store_true')
if parser.parse_args().publish:
    publish_file('homebrew-tap', 'Formula/kata.rb', output / 'kata.rb')
    publish_file('scoop-bucket', 'kata.json', output / 'kata.json')
    fork = f'{OWNER}/winget-pkgs'
    branch = f'kata-{version}'
    existing_branch = subprocess.run(['gh', 'api', f'repos/{fork}/git/ref/heads/{branch}'], capture_output=True, text=True)
    parent = (json.loads(existing_branch.stdout)['object']['sha'] if existing_branch.returncode == 0
              else api('repos/microsoft/winget-pkgs/git/ref/heads/master')['object']['sha'])
    prefix = f'manifests/j/JacobHuemmer/Kata/{version}'
    base_tree = api(f'repos/{fork}/git/commits/{parent}')['tree']['sha']
    tree = api(f'repos/{fork}/git/trees', 'POST', {'base_tree': base_tree, 'tree': [
        {'path': f'{prefix}/{file.name}', 'mode': '100644', 'type': 'blob', 'content': file.read_text()}
        for file in sorted(winget.iterdir())]})
    commit = api(f'repos/{fork}/git/commits', 'POST', {'message': f'New version: {identifier} version {version}', 'tree': tree['sha'], 'parents': [parent]})
    if existing_branch.returncode == 0:
        api(f'repos/{fork}/git/refs/heads/{branch}', 'PATCH', {'sha': commit['sha']})
    else:
        api(f'repos/{fork}/git/refs', 'POST', {'ref': f'refs/heads/{branch}', 'sha': commit['sha']})
    body = output / 'winget-pr.md'
    body.write_text(f'Adds {identifier} {version}, a portable CLI for reusable POSIX scripts.\n\nWindows requires Git Bash and the Visual C++ runtime, supplied by the Git.Git and Microsoft.VCRedist.2015+.x64 dependencies. Release binaries are built by GitHub Actions; SHA-256 hashes come from the release checksums.\n')
    existing_pr = api(f'repos/microsoft/winget-pkgs/pulls?head={OWNER}:{branch}&state=open')
    if existing_pr:
        print(existing_pr[0]['html_url'])
        raise SystemExit(0)
    subprocess.run(['gh', 'pr', 'create', '--repo', 'microsoft/winget-pkgs', '--base', 'master', '--head', f'{OWNER}:{branch}',
                    '--title', f'New package: {identifier} version {version}', '--body-file', str(body)], check=True)
else:
    print(output)
