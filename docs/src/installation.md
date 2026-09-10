# Installation

## Prebuilt binaries (recommended)

Download a prebuilt binary for your platform from the [latest release](https://github.com/varuns2903/datapipe/releases/latest), or install with the one-line script:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/varuns2903/datapipe/releases/latest/download/datapipe-cli-installer.sh | sh
```

## Homebrew (macOS/Linux)

```bash
brew install varuns2903/tap/dp
```

## From crates.io

Requires a Rust toolchain.

```bash
cargo install datapipe-cli
```

*(This will install the `dp` binary to your `~/.cargo/bin` directory)*

## From source

```bash
git clone https://github.com/varuns2903/datapipe.git
cd datapipe
cargo install --path .
```

## Shell completions

`dp` can generate completion scripts for bash, zsh, fish, PowerShell, and elvish via `dp completions <shell>`:

```bash
# bash (persist across sessions by adding this to your ~/.bashrc)
source <(dp completions bash)

# zsh (persist by saving to a directory in your $fpath)
dp completions zsh > "${fpath[1]}/_dp"

# fish
dp completions fish | source

# PowerShell (add to your $PROFILE to persist)
dp completions powershell | Out-String | Invoke-Expression
```

## Man page

`dp man` prints a troff-formatted man page to stdout:

```bash
dp man | gzip > dp.1.gz
sudo mv dp.1.gz /usr/local/share/man/man1/
```
