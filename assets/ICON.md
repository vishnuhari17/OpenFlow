# Icon assets

Place your menubar icon here as `icon.png` (32×32 px, RGBA).

After adding it, run `cargo build` — the build script detects the file and
the tray will use it automatically. The colored-circle fallback is used until
then.

## Design spec

- **Canvas:** 32×32 px (Figma frame)
- **Shape:** Microphone — capsule body + stand arc
- **Color:** White (#FFFFFF) on transparent background
- **Style:** Template image — macOS will tint it for light/dark mode automatically
- **Weight:** ~2px stroke, filled capsule body

See the root README for the full Figma → `iconutil` workflow.
