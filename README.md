# sensiblefox

Opinionated, zero-config Firefox for macOS. Single binary — run it and go.

```
sensiblefox
```

That's it. It downloads Firefox if needed, builds a clean profile, installs uBlock Origin, and launches.

## What it does

- **No telemetry** — all Mozilla data collection killed
- **No password manager** — disabled (use Bitwarden, 1Password, etc.)
- **No studies or experiments** — Normandy, Shield, all off
- **No crash reports** — disabled
- **No sponsored content** — no pocket stories, no sponsored shortcuts, no VPN promos
- **No Mozilla promotions** — no "What's New", no recommendations, no discovery
- **Private DNS** — Quad9 DoH (mode 2)
- **uBlock Origin** — auto-installed with clean SVG icon
- **Background blur** — macOS native titlebar translucency
- **Sliding bookmarks** — auto-hide bookmarks bar, slides down on hover
- **DevTools on the right** — docked right by default
- **Cookie banners** — auto-rejected
- **Strict tracking protection** — fingerprinting, cryptomining, social tracking blocked
- **Performance** — GPU acceleration, WebRender, JPEG XL
- **Clean new tab** — no shortcuts, no pocket, no weather, no highlights
- **808 prefs** — Betterfox upstream (Fastfox, Peskyfox, Securefox, Smoothfox) + sensiblefox overrides

## Install

### .pkg installer — complete Firefox install
```bash
./scripts/generate-prefs.sh   # pull fresh Betterfox + arkenfox prefs
./scripts/build-pkg.sh        # downloads Firefox, bakes in defaults
open dist/sensiblefox.pkg
```
The pkg installs a fully-configured `Firefox.app` to `/Applications`. Launch
Firefox normally from Spotlight/Dock — every default profile picks up
sensiblefox prefs, policies, and CSS without any extra setup.

The `.pkg` does **not** install the `sensiblefox` CLI. It is a self-contained
Firefox installer.

Customize the install location or locale:
```bash
INSTALL_LOCATION=/opt/sensiblefox ./scripts/build-pkg.sh
FIREFOX_LANG=de ./scripts/build-pkg.sh
```

### Developer CLI — iterate on prefs/CSS without rebuilding the .pkg
```bash
./scripts/generate-prefs.sh   # pull fresh Betterfox + arkenfox prefs
cargo build --release
./target/release/sensiblefox
```
Builds a named `~/Library/Application Support/sensiblefox/profile` and launches
Firefox with `--profile`. Useful while iterating on prefs/CSS before re-baking
the installer. Reads its Firefox path from `SENSIBLEFOX_FIREFOX_PATH` at build
time (defaults to `/Applications/Firefox.app`).

## CLI usage

```bash
sensiblefox                          # build profile + launch Firefox
sensiblefox --profile-only           # build without launching
sensiblefox --profile-path ~/my-ff   # custom output path
sensiblefox --update-upstream        # re-fetch Betterfox/arkenfox
sensiblefox --clean                  # delete sensiblefox profiles
```

Re-running with an existing profile just relaunches it — no rebuild.

## Updating upstream prefs

```bash
./scripts/generate-prefs.sh   # fetches latest from Betterfox + arkenfox
./scripts/build-pkg.sh        # rebuild .pkg with new defaults
cargo build --release         # rebuild CLI (optional)
```

## Notes

The `.pkg` modifies `Firefox.app` (drops in `policies.json`, `defaults/pref/`,
and an `autoconfig` script) which invalidates Mozilla's notarization signature.
For local installs Gatekeeper warns once and then it's fine. For wider
distribution the `.pkg` and the modified `Firefox.app` would need to be
re-signed and notarized with your own Developer ID.

## Architecture

Two distinct artifacts, two distinct audiences:

### `dist/sensiblefox.pkg` — for end users
A complete Firefox install. Bundles `Firefox.app` patched with three layered
customization mechanisms, all bundle-resident (no per-profile setup):

| Layer | Location inside `Firefox.app` | Purpose |
|---|---|---|
| Enterprise policies | `Contents/Resources/distribution/policies.json` | uBlock auto-install, telemetry/Pocket/studies off, DoH, suppress first-run UI |
| Autoconfig sentinel | `Contents/Resources/defaults/pref/autoconfig.js` | Three-line bootstrap pointing Firefox at `sensiblefox.cfg` |
| Autoconfig script | `Contents/Resources/sensiblefox.cfg` | All ~800 prefs via `defaultPref()` calls + global CSS injection. This is the canonical Mozilla pattern (modern Firefox ignores arbitrary `.js` files in `defaults/pref/`) |
| Bundled CSS | `Contents/Resources/sensiblefox/userChrome.css` | Loaded as `AGENT_SHEET` by the autoconfig script — applies to every profile, no per-profile `chrome/` folder needed |

### The `sensiblefox` CLI — for developers iterating on the config
```
src/
├── main.rs          CLI (clap)
├── firefox.rs       Detect or download Firefox.app (path baked at compile time)
├── profile.rs       Profile directory creation
├── prefs.rs         Embeds generated/user.js at compile time
├── css.rs           Embeds CSS assets at compile time
├── extensions.rs    Downloads uBlock Origin from AMO
└── upstream.rs      Runtime fetcher for Betterfox/arkenfox

build.rs                       Bakes SENSIBLEFOX_FIREFOX_PATH into the binary
assets/                        CSS files + policies.json + autoconfig.js + sensiblefox.cfg
generated/user.js              Opinionated prefs (user_pref form, used by CLI)
generated/sensiblefox-defaults.js  Same prefs in defaultPref form (baked into .pkg)
scripts/
├── generate-prefs.sh   Pull + merge upstream prefs (writes both variants)
└── build-pkg.sh        Build the macOS Firefox .pkg installer
```
