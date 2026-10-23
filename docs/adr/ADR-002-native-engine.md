# ADR-002: The engine is native Rust drawing with Skia through skia-safe; Chrome only makes golden references

- Status: Accepted; its "system fonts are never used" and "the same pixels on every machine" are amended by [ADR-050](ADR-050-macos-system-fonts-behind-inter.md), which sets the characters Inter lacks with three macOS system fonts, so a card holding them is the same on one Mac but can differ between Macs
- Date: 2026-10-17

## Context

The prototype draws each card as HTML and CSS in warm headless Chrome. Measured in the prototype rounds and the research review on this Mac (2017 dual-core i5-7360U, 8 GB):

| | Headless Chrome (prototype) | Skia, measured through skia-python |
|---|---|---|
| First card in a session | 4.4 to 6.4 s while Chrome starts | no start-up cost |
| Warm card | 0.20 to 0.37 s | 4.3 ms to draw a 1280x720 card, plus about 70 ms for PNG encoding |
| Entrance frames | 0.3 to 0.7 s for 15 to 22 frames | about 90 ms for 22 frames |
| Memory | 344 MB across 10 processes | one process |
| Install | system Chrome or a 108 to 125 MB download | one binary |
| Failure seen | the browser died while the render server kept answering `/ping` | none of that kind |

Other options from the research: vl-convert (Vega in embedded V8) was no faster (310 ms SVG plus 231 ms PNG); Vello's CPU renderer lacks complete blur filters; Blitz is beta; Avenger is an early prototype that needs a GPU; Servo is too early to depend on.

## Decision

- **Skia through skia-safe 0.153.3**, CPU raster. Skia is Chrome's own rasterizer, so shadows, blur, anti-aliasing and text match the approved HTML card closely. GPU backends stay off until a surface needs them.
- **Prebuilt Skia binaries.** skia-safe's build downloads a prebuilt Skia from rust-skia's GitHub releases. Prebuilt sets exist only for fixed feature combinations; the one that exists for macOS arm64, macOS x86_64 and Linux x86_64 is the default set (jpeg, pdf, embedded ICU data) plus `textlayout`, so that is the feature set in the workspace `Cargo.toml`. Adding a feature means checking first that the release has that combination for all three targets; otherwise skia-bindings falls back to building Skia from source, which needs Python, network access and about an hour on this Mac.
- **Pipeline:** intent (ADR-003) to a scene of positioned marks and text, to pixels. The scene stays in memory, so frames, hit-testing for hover and partial images (ADR-004) are cheap later.
- **Determinism:** fonts are bundled in the binary (Inter, SIL Open Font Licence) and system fonts are never used, colours are sRGB, and the scale factor is explicit. The same intent gives the same pixels on every machine, which is what makes golden tests possible (ADR-006).
- **Rust holds the stable parts; forms and styling are data.** The compiled core is the scene, layout, raster, frames and hit map. Chart forms and design tokens (the playbook's numbers and colours) are data files the engine loads, validated against a schema: embedded in the release binary, and reloadable from a directory named by an environment variable in a dev mode, so a new form or a taste change is tried without a recompile (a cold build here takes over two minutes, measured below). The cost is that a form can only use the mark types and layout rules the core provides; a form needing a new kind of mark is Rust work. Validation errors also move from compile time to load time, so a test loads and renders every shipped form file, and the release binary never reads form files from disk.
- **No unsafe code in our crates** (`unsafe_code = "forbid"` in the workspace lints). skia-safe holds the unsafe boundary.
- **Headless Chrome stays in the repository only as a developer tool** that renders the prototype's HTML cards into golden reference images. It is never shipped, never a dependency of any crate, and never runs at product run time.
- **Vega-Lite** is later work: arbitrary specs would compile in a sandboxed embedded engine and their scene would be drawn by our Skia renderer (ADR-003).

### Measured on this Mac

On 2026-10-17, skia-safe 0.153.3 with prebuilt binaries built on this Mac (Intel macOS 13, x86_64, Rust 1.99.1), and the engine's test drew and encoded a PNG through it.

| Step | Time |
|---|---|
| Cold `cargo test --workspace` after `cargo clean` (downloads the 16 MB Skia archive, compiles every crate, runs the tests) | 132 s wall, 164 s user CPU |
| Warm rebuild after a manifest change | 22 s |

The first attempt asked for `textlayout` without the default features. No prebuilt archive exists for that combination, so skia-bindings fell back to fetching Skia's source and failed on a certificate error in Skia's own Python fetch script. The feature rule above prevents that. Build one thing at a time here; the Mac is shared.

## Consequences

- We give up Vega's free grammar for the native forms and author each form ourselves (ADR-003). That is also where the chart sense lives.
- The build needs network access once, to fetch the Skia binary, so CI caches it with the cargo cache (ADR-007).
- skia-safe releases often; an upgrade is a PR that reruns the golden tests.
- The binary is about 15 to 40 MB with fonts embedded (an estimate until round 2 measures it).

Sources: https://github.com/rust-skia/rust-skia ; https://github.com/rust-skia/skia-binaries/releases/tag/0.153.3 (asset list fetched 2026-10-17); the prototype's round reports and the startup architecture review of 2026-10-17, sections 1 and 4 (private research notes).
