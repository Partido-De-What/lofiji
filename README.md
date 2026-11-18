# Lofiji

**A small pocket of quiet for your terminal.**

Lofiji is a Linux desk clock with lofi pixel-art scenes, a Pomodoro timer, and a small space for deciding what to do next. It’s built for people who spend much of their day in a terminal and want the time, their next task, and a little atmosphere in the same window.

**Work in progress.** The core clock and workspace are usable. Expect the interface and setup to evolve.

![Lofiji showing the clock, character reminders, and a short task list against a pixel-art bonfire](docs/plan.png)

## What you can do with it

| If you’re… | Try this |
| --- | --- |
| **Studying or writing** | Open Pomodoro with **p**, start a focus session with **Space**, and mix a little rain or café noise with **a**. |
| **Juggling a project and a deadline** | Capture a task with **+** and manage goals and due dates with **g**. Lofiji puts overdue and due-today work near the top. |
| **Building a habit** | Use **h** to keep a short character reminder in sight, such as “Listen before responding.” Pause it when it no longer needs your attention. |
| **Looking for a next step** | Open **Talk** with **c** for local guidance or optional AI chat. With a provider connected, **d** asks for one concrete first action on your top pending task and may suggest a relevant app shortcut. |
| **Keeping a clock on a second screen** | Press **v** for clock-only view. Cycle pixel-art scenes with **b** and clock styles with **t**. You can also use your own PNG or GIF. |

The clock, bundled scenes, timer, and planner work offline. Ambient loops work offline with **mpv**. YouTube playback is optional and requires **mpv**, **yt-dlp**, and **Node.js**.

![Rainy terminal clock with a running Pomodoro focus timer](docs/pomodoro.png)

[Scene gallery](docs/scenes.png) · [Rain Room](docs/rain-room.png) · [Compact view](docs/plan-compact.png) · [Sound mixer](docs/sounds.png) · [Talk view](docs/conversation.png)

## Start here

You’ll need Linux, Rust **1.74+**, and a true-color terminal such as Kitty.

```sh
git clone https://github.com/Partido-De-What/lofiji.git
cd lofiji
cargo run --release --locked
```

Press **?** for shortcuts and **s** for settings. The [user guide](docs/usage.md) covers customization, audio, and optional launch at desktop login.

## Optional AI

Lofiji supports **ChatGPT plan sign-in**, **OpenAI API**, **Anthropic Claude**, **Google Gemini**, **Ollama**, **OpenRouter**, and **OpenAI-compatible endpoints**. Choose one under **Settings → AI**. For Claude or Gemini, enter a model ID available to your account and supply the API key through an environment variable. [AI setup and provider requirements](docs/ai.md)

AI starts off. Goals, tasks, and reminders are saved locally. When you use a cloud model, Lofiji sends relevant planner entries and recent chat as context. Model replies can suggest actions, and you decide what to save.

## Development

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Issues and pull requests are welcome while Lofiji takes shape. Please include your terminal and window size when reporting a display issue. [Design notes](docs/design.md) · [Verification](docs/verification.md)

[MIT license](LICENSE) · [Artwork credits](assets/wallpapers/LICENSE.md) · [Audio credits](assets/sounds/LICENSE.md)
