# Lofiji design

The real-world clock remains the centerpiece. Seven-row and compact five-row numeral faces share a layout that centers the time horizontally. With the text workspace enabled, the clock sits in the upper portion; hiding the workspace restores vertical centering. The date and timer sit below the time without shifting it. Eight clock styles and palette selection are independent of the background. The seven-segment face derives strokes from A–G masks with connected junctions; Outline aligns stem and junction anchors at either scale. Block, Slanted and Scanlines add square, italic and striped faces, with distinct compact renderings. Pomodoro appears beneath the time; opening its controls keeps clock pixels in place.

All scenes are raster assets. Rainy Window is a looping GIF; Moonlit City, Forest, Snow Mountains and Bonfire are PNGs. A custom PNG/GIF slot uses a user path. Former terminal-drawn scenes and their geometry were removed. Sources and licensing are recorded alongside the media.

Built-in images decode lazily and share cached immutable frames. GIF offsets, transparency and Keep/Background/Previous disposal are composited once; elapsed monotonic time selects the frame. Drawing caps at ten frames per second and skips frames after delays rather than slowing the loop. The PNG scenes use local sampling effects: city lights, forest clouds/foliage/fireflies, drifting snow, and bonfire flame/embers/reflections. Geometry and camera remain anchored. Effects repeat on a 24-second monotonic timeline. Scene motion off shows the initial frame and freezes image effects and weather. Pixel Art Animator's timing/hold guidance informed this behavior; no Aseprite runtime is needed.

Images preserve aspect ratio through centered cover cropping and adjustable dimming. Protected clock/date/header/timer cells receive averaged image background colors so half-block details do not interfere with glyphs. PNG/GIF rendering uses ordinary true-color half-blocks, without a terminal image protocol.

The fireplace is an independent offline recording placed beside Campfire in the mixer. A stable stored layer order keeps existing mixes intact when the new level is appended.

Five persistent palettes define hours/minutes, seconds, separator and date colors. Scene mode picks a suitable preset; explicit selection pins a palette. Editing a color modifies that preset, with validation before accepting changes. The settings list scrolls and color values serve as small swatches.

The ambient mixer is a separate optional overlay. Users select independent levels, then apply/save with Enter; Esc cancels. The saved mix starts muted on launch. Each active layer owns an mpv process group and loops a bundled local recording or synthesized noise. Changes restart only affected voices. YouTube has a separate player and volume, allowing music and ambient layers together. Both audio paths clean up children on stop/exit; no network or audio is started by snapshots.

Environmental recordings have crossfaded seams. Sound files are embedded, extracted on demand into a private temporary directory, and removed on orderly exit. Normal quit and handled termination signals restore the terminal and clean up owned processes. SIGKILL cannot run cleanup.

Timer deadlines remain independent of drawing, scene motion, audio, resizing and input. The default screen presents the clock, date, image and a compact row of discoverable controls. Footer keys use the active clock palette’s primary color; descriptions use its separator color. The title row retains one row of top padding in compact and large windows.

## Reminders, objectives and conversation

Goals/tasks are independent of appearance settings and timer state. Their file is derived from the selected config path and is only created on the first save. A schema version, stable entry IDs, strict validation, private mode 600, advisory writer locking, stale-state checks and atomic file replacement protect personal entries. Archived records remain available for immediate undo. Completing a goal does not cascade into its tasks.

The default screen reserves a weather-protected workspace below the raised clock/date/timer. A small h/g/c menu opens character reminders, tasks/objectives, or a text conversation. The overview places reminders beside prioritized tasks with linked objectives and an optional AI suggestion. Lists, forms and chat share this lower rectangle; the clock stays fixed between views. Tiny windows fall back to the compact overlays. Text and controls follow the clock palette and are clipped using terminal column widths. Chat history scrolls with PageUp/PageDown; keyboard hints follow the active input context.

Character reminders share the private planner store through an additional entry kind. They represent ongoing qualities/habits and have pause/resume, editing and archive/undo controls; they are not ranked as tasks or offered as goal parents. The existing file schema and saved task/goal entries continue to load. Explicit reminder:/character: commands are handled locally. Active reminders are included separately in bounded AI context; paused/archived reminders are omitted.

Local daily suggestions rank overdue tasks, today's deadlines, unscheduled tasks and future tasks, then priority/date/ID. Linked goals supply context. Optional AI adapters support OpenAI Responses, Anthropic Messages, Gemini generateContent, Ollama chat, OpenRouter and compatible Chat Completions APIs. Model IDs and base URLs are configurable; switching providers restores cached model/endpoint/key-variable references.

A cancellable worker owns each HTTP request and its curl process group. Credentials enter through private standard input and never appear in command arguments or response errors. The renderer and timer do not wait for the network. API-version and token settings are configurable; no SDK pins the app to a particular model release. Requests are non-streaming and one at a time, with timeouts, bounded response size and no automatic retries.

The conversation visibly distinguishes local commands from model replies. Explicit user commands save locally; model text cannot mutate records or execute actions. Goal/task context is bounded and excludes archived/completed records. AI daily suggestions are optional, scoped to the current task snapshot, provider settings and date, and rejected if they arrive after that context changes. Unavailable providers retain local guidance. AI is off by default and snapshots never create requests.

Login installation uses a stable binary copy and desktop launcher. The current i3 session has a missing dex autostart helper; a minimal managed include launches Kitty at session startup. The installer validates i3 syntax and can remove its own changes. It does not log out or restart the user's session.
