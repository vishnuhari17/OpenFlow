# Rust macOS assistant architecture

## Core principle

Paste the first usable transcript fast, then improve it only if the refined result is materially better. Waiting for every stage before pasting will cost more than it helps.

## Recommended pipeline

1. Global hold-to-talk key goes down through a macOS event tap.
2. Start microphone capture immediately on key down.
3. Snapshot focused element metadata through the Accessibility API during recording, not after release.
4. On key up, stop capture and send the full buffered audio plus trimmed screen context to the speech model.
5. Paste the first transcript into the currently focused element as soon as it returns.
6. Run a very small refinement pass only if confidence is low or the text looks malformed.
7. If the refined text is meaningfully different, replace only the just-inserted range.

## macOS hotspots

### 1. Accessibility screen context

The lowest-latency path is not full-screen OCR or a full AX tree dump. Start with:

- system-wide focused application
- focused UI element
- focused element role
- focused element value or nearby text
- window title

Only expand the search when the focused element is empty. Put strict caps on traversal depth, node count, and extracted characters.

### 2. Global key capture

For a single hold key on macOS, the best shape is a background app with:

- event tap for key down and key up
- Accessibility permission for UI reading
- Input Monitoring permission if your chosen hotkey strategy needs it
- optional menu bar presence instead of a dock-first app

Avoid a normal focused window for trigger capture. It adds friction and loses the "Siri-like" feel.

For the first reliable build, prefer a dedicated non-modifier key such as `F18`. Modifier-only keys like `right_command` work, but they arrive as `flagsChanged` events and are harder to reason about when other modifiers are pressed at the same time.

### 3. Fast paste

There are two reliable strategies:

- set the pasteboard, then synthesize `Cmd+V`
- inject text directly into the focused accessibility element when supported

Pasteboard plus synthetic paste is usually the most universal first implementation. Direct AX insertion can be cleaner later for richer editing.

## Rust module boundaries

- `AudioCapture`: microphone capture with press and release lifecycle
- `FocusResolver`: focused app, window, and text context from macOS AX
- `TranscriptionEngine`: low-latency ASR with screen context as prompt
- `TranscriptRefiner`: tiny cleanup pass, ideally sub-150ms
- `TextPaster`: immediate insert, then optional replace of the inserted range

## What to build next

1. Replace the demo services with real macOS adapters.
2. Keep transcription one-shot for now and optimize startup, upload, and response latency before adding complexity.
3. Track the inserted text range so refinement can patch safely.
4. Add a lightweight non-activating overlay panel for feedback while recording.
