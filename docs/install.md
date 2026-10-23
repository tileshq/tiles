# Installing Tiles on a Mac

Each release ships one installer package for every Mac, `tiles-<tag>-macos.pkg`, beside a tarball per architecture (Apple Silicon and Intel), `SHA256SUMS`, its signature `SHA256SUMS.sig` and `install.sh`. The release workflow (`.github/workflows/release.yml`) builds them all when a `v*` tag is pushed. Releases live on this repository's Releases page ([ADR-059](adr/ADR-059-public-cut.md)). `scripts/install.sh` and `tiles update` read [tileshq/tiles](https://github.com/tileshq/tiles) unless `TILES_RELEASES_REPO=<owner>/<repo>` names another.

`SHA256SUMS` holds the SHA-256 of the package, each tarball and `install.sh`. The release job signs it with the Tiles release key, whose private half only the release workflow can read, from the repository's `release` environment, so whoever can upload a file to a release still cannot replace it and `SHA256SUMS` together ([ADR-056](adr/ADR-056-signed-sums-and-weekly-advisories.md)). `install.sh` checks that signature before it trusts `SHA256SUMS`, and [Checking a download by hand](#checking-a-download-by-hand) shows how to check it yourself.

## The package: download, open, answer y

1. Open the latest release on the Releases page and download `tiles-<tag>-macos.pkg`.
2. Open it from Downloads. The package is not signed yet, so macOS stops it the first time:
   - macOS 15 or later: choose Done, open System Settings > Privacy & Security, choose Open Anyway beside the package's name, then open it again.
   - macOS 14 or earlier: Control-click the package, choose Open, then Open again.
3. Click through the installer and enter an administrator password. It puts `tiles` in `/usr/local/bin`, which is on every Mac's `PATH`.
4. Terminal opens and runs `tiles setup`. Answer `y` to set Tiles up for the harnesses it lists. If no window opened, open Terminal and run `tiles setup`.

What the package cannot do yet ([ADR-035](adr/ADR-035-macos-package-and-tiles-setup.md)):

- It needs the Open Anyway step once per downloaded package until releases are signed with a Developer ID and notarised.
- A managed laptop can refuse unsigned installers outright, or its endpoint security software can delete an unsigned binary as it is written. Then ask IT to allow Tiles, or use `install.sh` if the laptop allows that.
- It needs an administrator password. Without one, use `install.sh`, which installs into your home folder.

## `tiles setup`: every harness in one step

`tiles setup` finds each harness Tiles supports on your `PATH` (Claude Code, Codex, OpenCode and Pi), the terminal, and the data login it can see without reading a secret, an Omni URL in the Keychain or `TILES_OMNI_URL`. It prints one list and asks once; nothing changes unless you answer `y`:

```text
Tiles found:
  terminal: warp, nothing to change (tiles doctor shows how cards draw here)
  pi: /Users/<you>/.local/bin/pi, already set up
  opencode: not on PATH
  claude: /Users/<you>/.local/bin/claude, not set up
  codex: /usr/local/bin/codex, not set up
  omni: the Keychain names acme.omniapp.co but holds no API key for it
Tiles will change:
  claude: run claude mcp add --scope user tiles -e TILES_DRAW=text -- /usr/local/bin/tiles mcp
  codex: run codex mcp add tiles --env TILES_DRAW=text -- /usr/local/bin/tiles mcp
  codex: write /Users/<you>/.codex/skills/tiles-theme/SKILL.md
Set up Tiles for Claude Code and Codex? [y/N]
```

On `y` it runs, for each harness, the steps `tiles install <harness>` runs ([surfaces.md](surfaces.md)). Then it prints the commands that finish each data source it found, which need a key or a choice only you have: `tiles omni set-key https://<host>` ([omni.md](omni.md)). After those, run `tiles setup` again so each harness gets the data tools and asks before each Tiles tool call.

Running it again changes nothing that is already set up. `tiles setup --dry-run` prints the list and changes nothing; `tiles setup --yes` answers for a script. A harness installed later shows up in `tiles doctor` as `setup: Tiles is not set up, or is out of date, for <harness>; run tiles setup`; for Claude Code, doctor reads its registration from `~/.claude.json` and starts nothing. `tiles setup` leaves alone an OpenCode `config.json` or a Codex approval setting of your own, as `tiles install` does, and says so.

When Claude Code already has a user-scope `tiles` server that runs another `tiles`, such as `~/.local/bin/tiles` from an earlier `install.sh`, `tiles setup` lists `claude: <path>, set up with another tiles, /Users/<you>/.local/bin/tiles; Tiles will replace it`, and on `y` runs `claude mcp remove tiles --scope user` and then the `claude mcp add` above ([ADR-039](adr/ADR-039-setup-replaces-a-stale-claude-code-server.md)). If the remove fails it adds nothing and prints the two commands to run yourself. A `tiles` server in a project's or a folder's Claude Code config, or one that sets variables of your own, is named with the commands that replace it and left as it is.

An earlier `install.sh` leaves its own copy in `~/.local/bin`, and when that folder comes first on your `PATH` a terminal keeps running the old `tiles`. `tiles setup` names every other `tiles` on your `PATH` with its version, and offers to move one that comes before it aside ([ADR-035](adr/ADR-035-macos-package-and-tiles-setup.md)):

```text
Tiles found:
  terminal: warp, nothing to change (tiles doctor shows how cards draw here)
  tiles: /Users/<you>/.local/bin/tiles, version 0.1.2, comes first on PATH, so a terminal runs it, not this one (/usr/local/bin/tiles, version 0.1.9)
  ...
Tiles will change:
  claude: run claude mcp add --scope user tiles -e TILES_DRAW=text -- /usr/local/bin/tiles mcp
  tiles: move /Users/<you>/.local/bin/tiles aside, to tiles-0.1.2-old beside it
Set up Tiles for Claude Code? [y/N] y
installed Tiles for claude; ...
Move /Users/<you>/.local/bin/tiles aside, to /Users/<you>/.local/bin/tiles-0.1.2-old? [y/N] y
Moved /Users/<you>/.local/bin/tiles to /Users/<you>/.local/bin/tiles-0.1.2-old. To run /usr/local/bin/tiles as tiles, open a new terminal tab, or run hash -r (rehash in zsh).
```

It renames the old copy and never deletes it; delete `tiles-0.1.2-old` yourself once the new one works. It offers only for a copy older than itself, or one whose version it cannot read; a newer copy that comes first is named as newer and left. It asks only in a terminal and only for a file you own: without a terminal, with `--yes` or `--dry-run`, or when you answer `n`, it prints the `mv -n` command to run yourself, and a copy owned by another user is named with `sudo mv -n`. `tiles doctor` prints the same command on one line while the old copy comes first.

## `install.sh`: no administrator, or a script

`install.sh` installs `tiles` to `~/.local/bin` and tells you if that folder is not on your `PATH`. It needs only `curl` and the `ssh-keygen` every Mac ships: it finds the latest release, downloads the tarball for this Mac's architecture, `SHA256SUMS` and `SHA256SUMS.sig` over HTTPS, checks the signature against the release key it pins, and then checks the tarball against `SHA256SUMS`. Download `install.sh` from the latest release, check it as [below](#checking-a-download-by-hand), then:

```sh
sh install.sh
```

Or pipe it from the latest release, which skips checking the script itself; it still checks the signature and the tarball, and `tiles setup` reads its question from the terminal:

```sh
curl -fsSL https://github.com/tileshq/tiles/releases/latest/download/install.sh | sh
```

Pass a tag to pin one: `sh install.sh v0.1.0`. `TILES_RELEASES_REPO=<owner>/<repo>` reads another GitHub repository's releases. To install from files you downloaded, download `tiles-<tag>-aarch64-apple-darwin.tar.gz` (Apple Silicon) or `tiles-<tag>-x86_64-apple-darwin.tar.gz` (Intel), `SHA256SUMS`, `SHA256SUMS.sig` and `install.sh` from the release page in a browser, then:

```sh
TILES_ARCHIVE=~/Downloads/tiles-v0.1.0-aarch64-apple-darwin.tar.gz \
TILES_SUMS=~/Downloads/SHA256SUMS sh ~/Downloads/install.sh
```

The script reads the signature from `SHA256SUMS.sig` beside `SHA256SUMS`. Whenever the signature does not check out, it prints one line and installs nothing. With no `SHA256SUMS.sig` beside `SHA256SUMS` it prints `tiles: no signature <path> for <sums>; download SHA256SUMS.sig from the same release beside it; not installed`; when a release has no `SHA256SUMS.sig` to download, `tiles: cannot download <url>/SHA256SUMS.sig, the signature of SHA256SUMS, so its checksums cannot be trusted; not installed`; and for a signature the release key did not make, one for another namespace, or a `SHA256SUMS` changed after signing, `tiles: SHA256SUMS is not signed by the Tiles release key; not installed`. It has no switch to skip the check. Releases published before signing began have no `SHA256SUMS.sig`; install one of those with the `install.sh` from its own release. The script refuses an archive whose checksum does not match. It removes the quarantine flag a browser download carries from the binary it installs, and says `installed` only when `tiles --version` runs and prints a version. When macOS stops the binary anyway, it says so, names Gatekeeper and endpoint security software, and points to System Settings > Privacy & Security. Run in Terminal, it then runs `tiles setup`, which asks before it changes anything; run from a script or with its output redirected, it prints `next: tiles setup` instead.

## Checking a download by hand

The release key's public half, also pinned in `install.sh`:

```text
ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB1zTnuDfOn8LcGgjw4yMeYLz9yGe0nvyY+KYWy1ksnB tiles-release
```

Its fingerprint is `SHA256:PoJflLMy3GoOM5xsIUXgQFQUn8hCPW4GKA0DTpUNeDQ`. With `SHA256SUMS`, `SHA256SUMS.sig` and the files you downloaded in one folder, run, with nothing to install first:

```sh
cd ~/Downloads
printf 'tiles-release namespaces="tiles-release" %s\n' \
  'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB1zTnuDfOn8LcGgjw4yMeYLz9yGe0nvyY+KYWy1ksnB tiles-release' > tiles-release.allowed
ssh-keygen -Y verify -f tiles-release.allowed -I tiles-release -n tiles-release -s SHA256SUMS.sig < SHA256SUMS
shasum -a 256 -c --ignore-missing SHA256SUMS
```

`ssh-keygen` prints one line that starts `Good "tiles-release" signature for tiles-release` and ends with the fingerprint above; anything else means `SHA256SUMS` did not come from the release workflow, so delete the downloads. `shasum` prints `<file>: OK` for each file you have; a `FAILED` line means that file changed after the release was made. This covers `tiles-<tag>-macos.pkg` too, which `install.sh` never reads.

`install.sh` runs before anything has checked it, so check it first. The `install.sh` this page describes has this SHA-256, which `shasum -a 256 install.sh` prints:

```text
e4ba5f1928d81ae42857279f36cad97b215aa530e44c9d7e43dc655e120c1196
```

The signed `SHA256SUMS` lists it as well, so the commands above check it the same way for any release.

## Then

```sh
tiles --version     # tiles 0.1.1 (intent schema 1): the release tag, without its v
tiles check --tag v0.1.1   # one pass or fail line per check of how cards draw here
tiles doctor        # terminal, route, harnesses and their versions, Omni setup, the update check
tiles run pi        # or opencode, claude, codex
```

The release job stamps the tag into the binary and stops when `tiles --version` does not print it ([ADR-025](adr/ADR-025-warp-cell-size-rows-and-laptop-check.md)); a build made outside the release job prints 0.0.0.

## Updating

A `tiles` the package installed updates by the next package: download `tiles-<tag>-macos.pkg` from the latest release and open it. `tiles update` run from `/usr/local/bin/tiles` says so instead of writing a second copy to `~/.local/bin`; `tiles update --check` and the daily notice name the package, as in `Tiles v0.1.5 is available; download tiles-v0.1.5-macos.pkg from its release and open it`.

A `tiles` that `install.sh` installed updates in place:

```sh
tiles update --check   # Tiles v0.1.5 is available; run tiles update
tiles update           # updated tiles from 0.1.4 to 0.1.5 at /Users/<you>/.local/bin/tiles
tiles update v0.1.4 --allow-older   # one tag, older than the installed one
```

`tiles update` downloads through `gh` rather than curl. With `gh` signed in, it downloads the latest release, or the tag you name, from the Releases page. Without it, it prints the steps for a download by hand: open the release page in a browser signed in to GitHub, download the archive for this Mac and `SHA256SUMS`, then run

```sh
TILES_ARCHIVE=~/Downloads/tiles-v0.1.5-aarch64-apple-darwin.tar.gz \
TILES_SUMS=~/Downloads/SHA256SUMS tiles update
```

Either way it checks the archive against `SHA256SUMS` and refuses a mismatch with nothing replaced. It does not check `SHA256SUMS.sig` yet; to be sure of a release before `tiles update`, check its `SHA256SUMS` as [above](#checking-a-download-by-hand). It removes the quarantine flag a browser download carries, so no `xattr` step is needed. It writes the new binary beside `~/.local/bin/tiles` (or `$TILES_BIN_DIR/tiles`), runs its `--version`, and renames it over in one step. It refuses an older release unless you pass `--allow-older`, and tells you when `~/.local/bin` is not on your `PATH`. `TILES_RELEASES_REPO=<owner>/<repo>` reads releases from another repository, for `tiles update`, the daily check and `install.sh`.

At most once a day, `tiles run` and `tiles doctor` ask GitHub whether a newer release exists and print one line before the harness starts and again after it exits: `Tiles v0.1.5 is available; run tiles update`. The check is one anonymous request to `api.github.com` carrying only `User-Agent: tiles/<version>` and `Accept: application/json` ([ADR-032](adr/ADR-032-update-command-and-harness-versions.md)). `TILES_UPDATE_CHECK=off` (or `0`, `false`, `no`) turns the check off, and `tiles doctor` shows the value it read and how long ago the check last ran. Tiles never updates itself; only `tiles update` or a newer package replaces the binary.

`tiles run` also names a harness version Tiles was not proved on, before the harness starts and again after it exits: `Tiles was proved on Claude Code 2.1.308, and this is 2.2.0; if a card misdraws, run tiles check`. `tiles check` fails that harness's `version:` line, and `tiles doctor` prints each harness's version beside the newest one Tiles was proved on.

## Removing it

For a package install: `sudo rm /usr/local/bin/tiles && sudo rm -r /usr/local/share/tiles && sudo pkgutil --forget io.github.tileshq.tiles`. For an `install.sh` install: `rm ~/.local/bin/tiles`. Then remove Tiles from each harness: `claude mcp remove tiles --scope user`, `codex mcp remove tiles`, and the files [surfaces.md](surfaces.md) lists for Pi and OpenCode.
