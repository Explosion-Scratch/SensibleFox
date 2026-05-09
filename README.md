# SensibleFox

SensibleFox is an opinionated, zero config Firefox build for MacOS. It's philosophy is to remove as much as possible whilst keeping all core features intact. It prioritizes speed and usability over privacy paranoia, with a focus on a clean, beautiful, and intentionally Firefox experience. All features that are removed should always be able to be turned back on. Nothing is gone or broken, but the defaults are far more usable.

## Removes:

- Telemetry & crash reports
- Studies, experiments (normandy, shield, etc)
- Mozilla promotions, sponsored content
- Cookie banners
- Tracking
- Search engines (in enterprise ver.)
- Pocket, VPN, other bloat
- New tab bloat
- Context menu bloat
- Form autofill
- Onboarding + config requirements
- AI junk

## Adds:

- uBlock Origin with all default filters turned on + ClearURLs
- Private DNS by default ('Increased' protection) - Quad9 over DoH
- 800 ish prefs from Betterfox upstream
- Global CSS for: Context menu, tab bar, etc

## Install

Download the latest `.pkg` from the [releases page](https://github.com/sensiblefox/firebuilder/releases) and install it like any other macOS application. Note: Gatekeeper may block the installation as Firefox can't be correctly notarized by me.

## CLI options

| Flag                   | Description                                 |
| ---------------------- | ------------------------------------------- |
| `--profile-only`       | Build the profile without launching Firefox |
| `--profile-path <dir>` | Custom output path for the profile          |
| `--update-upstream`    | Re-fetch Betterfox/arkenfox prefs           |
| `--clean`              | Pick which profiles to delete               |
