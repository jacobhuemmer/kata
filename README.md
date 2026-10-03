# Kata

A script library for your terminal and AI agents. Keep reusable scripts in Git,
run them with typed arguments, and expose them through `kata mcp serve`.

Each **kata** is a script with a small comment header describing its arguments,
required secrets, risk level, and timeout. A **catalog** is a directory of those
scripts. Kata supplies a picker, validation, an encrypted vault, and run history.

## Install

### macOS and Linux (Homebrew)

```sh
brew tap masonhuemmer/tap
brew install kata
kata --version
```

Update with `brew update && brew upgrade kata`. To build the latest `main`:

```sh
brew install --HEAD masonhuemmer/tap/kata
```

### macOS and Linux (standalone)

Download a binary and `SHA256SUMS` from [Releases](https://github.com/masonhuemmer/kata/releases),
or use the installer, which verifies the archive before installing to `~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/masonhuemmer/kata/main/install.sh | sh
```

Ensure `~/.local/bin` is on your `PATH`. Set `KATA_VERSION=0.1.3` to select a version
or `KATA_INSTALL_DIR` to change the destination. Linux binaries require glibc 2.39+
(Ubuntu 24.04 or newer); build from source on older distributions.

### Windows (Scoop)

```powershell
scoop bucket add extras
scoop bucket add masonhuemmer https://github.com/masonhuemmer/scoop-bucket
scoop install kata
```

### Windows (WinGet)

The first submission must be accepted into the WinGet community repository before
this command is available:

```powershell
winget install --id JacobHuemmer.Kata --exact
```

Until then, use Scoop or the Windows x64 zip from
[Releases](https://github.com/masonhuemmer/kata/releases).

**Run Kata from Git Bash on Windows.** Scoop and WinGet install Git and the Visual C++ runtime as dependencies.
For a standalone Windows zip, install both first: `winget install Git.Git` and
`winget install --id Microsoft.VCRedist.2015+.x64 --exact`.
Kata scripts use POSIX interpreters and tools; those tools must be on `PATH`.
Native Windows cancellation currently stops the direct child only. Use WSL and the
Linux build when you need Unix process-group cancellation. PowerShell scripts and
native Windows parity are not claimed.

### From source

Install Rust 1.88 or newer and Git:

```sh
git clone https://github.com/masonhuemmer/kata.git
cd kata
cargo install --locked --path crates/kata-cli
```

The executable is `kata`; the Rust package is `kata-cli`. The unrelated crates.io
package named `kata` is not this project.

## Get started

```sh
kata                         # Show the library; starter scripts appear on first run
kata list
kata show starter/hello
kata run starter/hello
kata check
```

Install a Git-backed catalog (authenticate with the host first for private repositories):

```sh
kata get https://github.com/OWNER/SCRIPTS.git --as personal
kata list personal
kata update personal
```

Configuration lives under `~/.config/kata`; scripts are in
`~/.config/kata/catalog/`. `KATA_HOME` provides an isolated home for testing.
Keep scripts in their own repository and secrets in the vault, never in Git.

## Agents (MCP)

```sh
kata mcp serve
```

This starts a stdio MCP server with `list_kata`, `describe_kata`, `run_kata`, and
`propose_kata`. Configure an MCP client to run command `kata` with arguments
`["mcp", "serve"]`. Agents see low-risk scripts by default; secrets are supplied
through declared vault needs and redacted from results and logs.

## Development and releases

See [CONTRIBUTING](docs/CONTRIBUTING.md) and [the design docs](docs/design/README.md).
CI checks formatting, Clippy, tests, Rust 1.88 compatibility, dependency advisories
and licenses, and mutation-test coverage. Windows has a Git Bash smoke test.

A `v<version>` tag matching `Cargo.toml` builds macOS (Intel/Apple Silicon), Linux
(x64/ARM64), and Windows x64 archives. Each archive includes licenses. The release
workflow publishes binaries and `SHA256SUMS`, updates the Homebrew tap and Scoop
bucket, and opens a WinGet community pull request.

Repository secret `PACKAGE_TOKEN` must have write access to
`masonhuemmer/homebrew-tap`, `masonhuemmer/scoop-bucket`, and the
`masonhuemmer/winget-pkgs` fork, plus permission to open the upstream pull request.
The built-in Actions token publishes the Kata release itself.

## License

MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
Third-party license notices are included in release archives.
