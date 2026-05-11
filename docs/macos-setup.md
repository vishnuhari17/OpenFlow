# macOS setup and test flow

## What is implemented

- Accessibility-based focused element reading
- Visible text or selected text extraction when the app exposes it
- Clipboard paste plus synthetic `Cmd+V`
- Global hold-key monitoring through an event tap

## Recommended trigger key

The default trigger is `F18`.

Why:

- it uses normal `keyDown` and `keyUp` events
- it is more predictable than modifier-only keys
- it is easier to suppress without side effects

`right_command` is also supported, but modifier-only keys use `flagsChanged` events and can be trickier around combined modifier presses.

## First-time setup

From the `rust-assistant` directory:

```bash
cargo run -- permissions
```

Then in macOS:

1. Open `System Settings > Privacy & Security > Accessibility`
2. Enable your terminal app for Accessibility
3. Open `System Settings > Privacy & Security > Input Monitoring`
4. Enable your terminal app for Input Monitoring
5. If macOS asks about synthetic events or automation, allow that too

After granting permissions, quit and reopen the terminal app before testing again.

## Test the three core pieces

### 1. Read focused screen context

If you run the command directly from Terminal, Terminal becomes frontmost. So for real testing, use the delayed form:

```bash
cargo run -- focus-after 3
```

You should see:

- focused app name
- focused role
- window title
- selected text, visible text, or current value preview

### 2. Paste into the current app

Use the delayed form so you have time to switch to the target app:

```bash
cargo run -- paste-after 3 "test from rust"
```

This writes to the pasteboard and then injects `Cmd+V`.

### 3. Monitor the global trigger

```bash
cargo run -- monitor-hotkey
```

That uses the default `F18`.

Or:

```bash
cargo run -- monitor-hotkey right_command
```

You should see `Pressed` and `Released` printed globally. The trigger is currently suppressed in the event tap so it does not keep flowing through the app beneath it.

## What to do next

1. Confirm `focus` works in the apps you care about most.
2. Confirm `paste` works in those same apps.
3. Pick your real trigger key:
   `F18` for reliability, or `right_command` if the ergonomics matter more.
4. Put your Groq API key in `.env` or the shell environment:

```bash
GROQ_API_KEY=your_key_here
```

5. Run the live dictation loop:

```bash
cargo run -- live right_command
```

6. Hold the trigger to record, release to transcribe, and the app will paste the result into the focused field.
