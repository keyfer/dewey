# Dewey

A TUI task manager that pulls tasks from multiple sources into one view. Single Rust binary. Works on Linux and macOS.

- Aggregate tasks from local files and [Linear](https://linear.app)
- Quick-add with natural language — priorities, due dates, tags, backend routing
- Scrollable task detail view with full metadata and descriptions
- Live config reload — toggle backends, switch themes, no restart needed
- Optional [Waybar](https://github.com/Alexays/Waybar/) integration (Linux) with smart badge and tooltip
- Dark, light, and dynamic [Omarchy](https://github.com/basecamp/omarchy) theme support

---

### TUI

<img src="assets/hero.gif" width="600" alt="dewey TUI demo">

Navigate, quick-add, edit, and complete tasks without leaving the terminal. Tasks from all backends sorted by urgency.

### Waybar (Linux)

<img src="assets/waybar.gif" width="600" alt="waybar tooltip">

Smart badge shows the most urgent count. Tooltip groups tasks by date with source icons.

### Backend aggregation

<img src="assets/backends.gif" width="600" alt="toggling backends">

Toggle backends on and off in the config — tasks appear and disappear live.

### Live theme switching

<img src="assets/theme.gif" width="600" alt="live theme switching">

Themes reload instantly when config changes.

---

## Install

### Linux

Pre-built binary:

```bash
curl -fsSL https://github.com/keyfer/dewey/raw/main/install.sh | bash
```

### macOS

Build from source using [Rust](https://rustup.rs/):

```bash
# Install Rust if you don't have it
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and install
git clone https://github.com/keyfer/dewey.git
cd dewey
cargo install --path .
```

The binary is installed to `~/.cargo/bin/dewey`. Make sure `~/.cargo/bin` is in your `PATH` (the Rust installer usually sets this up).

### From source (any platform)

```bash
cargo install --path .
```

## Quick start

```bash
mkdir -p ~/.config/dewey
curl -fsSL https://github.com/keyfer/dewey/raw/main/config.example.toml \
  -o ~/.config/dewey/config.toml
dewey add "Review PR today (p1)"
dewey add "Buy groceries tomorrow #errands"
dewey tui
```

## Backends

### Local file

Reads and writes `~/.dewey/todo.txt` by default.

```toml
[backends.local]
enabled = true
# path = "~/.dewey/todo.txt"
```

### Linear

Syncs issues from your [Linear](https://linear.app) workspace. Issues show up alongside your other tasks, and you can complete, edit, or create them directly from the TUI.

**Setup via the TUI wizard (recommended):**

1. Add the backend to your config (`~/.config/dewey/config.toml`):

   ```toml
   [backends.linear]
   enabled = true
   ```

2. Run `dewey tui` and press `L` to launch the setup wizard. It will walk you through:
   - Pasting your [Linear API key](https://linear.app/settings/api) (personal key starting with `lin_api_`)
   - Selecting your team
   - Choosing which team member's issues to show (or "me" for your own)
   - Picking which workflow statuses to display (e.g. Todo, In Progress, Backlog)

The wizard writes the full config for you. The result looks like:

```toml
[backends.linear]
enabled = true
api_key = "lin_api_..."
team_id = "..."
team_name = "Engineering"
assignee = "me"
user_id = "..."
filter_status = ["In Progress", "Todo", "Backlog"]
```

**Multiple Linear workspaces:** You can connect more than one team by running the wizard again (`L`). Each gets its own named section:

```toml
[backends.linear.work]
enabled = true
api_key = "lin_api_..."
team_id = "..."

[backends.linear.personal]
enabled = true
api_key = "lin_api_..."
team_id = "..."
```

## Waybar (Linux)

Dewey can output [Waybar](https://github.com/Alexays/Waybar/)-compatible JSON for use as a status bar module. When run outside a terminal (e.g. from Waybar), it outputs JSON by default.

```jsonc
"custom/tasks": {
    "exec": "dewey",
    "return-type": "json",
    "format": "{}",
    "on-click": "<your-terminal> -e dewey tui",
    "interval": 30,
    "tooltip": true
}
```

CSS classes: `has-overdue`, `has-tasks`, `all-done`, `backend-error`

## Configuration

`~/.config/dewey/config.toml` — changes are hot-reloaded. Press `c` in the TUI to edit. See [`config.example.toml`](config.example.toml) for all options.

## CLI

```
dewey              # TUI in terminal, Waybar JSON otherwise
dewey tui          # Force TUI mode
dewey add "..."    # Quick-add a task (supports natural language)
dewey list         # List today's tasks
dewey list all     # List all tasks
dewey list --format json  # JSON output for scripting
dewey config       # Print resolved config
dewey setup linear # Instructions for Linear setup
dewey agent status # Show running background agents
```

## TUI keybindings

### Task list

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate up / down |
| `Enter` | View task detail |
| `a` | Quick-add task |
| `e` | Edit task |
| `x` | Toggle complete |
| `d` | Delete task |
| `o` | Open in source app / `$EDITOR` |
| `/` | Search |
| `Tab` / `S-Tab` | Jump between groups |
| `Space` | Collapse / expand group |
| `C` | Collapse / expand all groups |
| `r` | Refresh tasks |
| `c` | Open config in `$EDITOR` |
| `L` | Linear setup wizard |
| `A` | Launch AI agent |
| `S` | Agent status |
| `?` | Help |
| `q` | Quit |

### Task detail (`Enter`)

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll down / up |
| `e` | Edit task |
| `x` | Toggle complete |
| `o` | Open in source app |
| `Esc` / `q` | Close detail |

## License

MIT
