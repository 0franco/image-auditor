# 🖼 AI Image Auditor Tool

**Find and fix image performance problems in seconds.**

This tool is a **blazing-fast Rust TUI** that scans your codebase
for image delivery issues that damage **Core Web Vitals**, **Lighthouse
scores**, and **SEO performance**.

It detects the exact problems that slow down modern sites --- **missing
lazy loading, wrong formats, layout shifts, and oversized images** ---
directly inside your HTML, templates, and frontend code.

Perfect for **frontend developers, performance engineers, and ecommerce
teams** who care about **LCP, CLS, and page speed**.

## ⚡ Key Features

-   **Extremely fast Rust scanner** for large codebases
-   **Interactive terminal UI (TUI)** for easy navigation
-   Detects **real Lighthouse / Core Web Vitals problems**
-   Works across **HTML, template engines, and modern JS frameworks**
-   Instant filtering by **severity**
-   Export results to **JSON reports**
-   Copy file paths directly from the UI
-   **AI‑powered automatic code fix suggestions** (OpenAI, Anthropic, or local Ollama)


## 🔎 Detected Issues

| Issue | Severity |
|---|---|
| Wrong format (PNG/JPG instead of WebP/AVIF) | ⚠ Warning |
| Missing `width` / `height` attributes (causes CLS) | ✖ Error |
| Missing `loading="lazy"` | ⚠ Warning |
| Oversized image file (>200 KiB, local images) | ✖ Error |
| Missing `srcset` / responsive images | ℹ Info |
| JSX `<Image>` missing `alt` attribute | ⚠ Warning |

## 📁 Supported File Types

`html`, `phtml`, `htm`, `jsx`, `tsx`, `js`, `ts`, `vue`, `svelte`, `hbs`, `ejs`, `njk`, `php`

Automatically skips: `node_modules`, `.git`, `dist`, `build`, `.next`

## 🎬 Video Demo
![demo.gif](docs/images/demo.gif)

# ⚡ Install

```bash
cargo install --path .
```

### macOS
```bash
brew tap 0franco/ai-image-auditor
brew install image-auditor
```

# 🧪 Usage

```bash
# Launch interactive TUI (menu to pick path)
image-auditor

# Scan a specific directory directly
image-auditor ./my-project
image-auditor /var/www/html
```

## 🖥 TUI Controls

| Key | Action |
|---|---|
| `Enter` | Edit path / confirm / view detail |
| `↑ ↓` or `j k` | Navigate |
| `Tab / Shift+Tab` | Filter by issue category |
| `1` | Show all severities |
| `2` | Errors only |
| `3` | Warnings only |
| `4` | Info only |
| `s` | Save report to `image-audit-report.json` |
| `c` | Copy current row file path to clipboard |
| `q / Esc` | Back / quit |
| `a` (Detail view) | Ask AI for an automatic code fix suggestion |
| `p` (Detail view) | Preview & apply the AI‑proposed patch (with confirmation) |

## 🤖 AI‑Powered Automatic Code Fix Suggestions

Turn Image Auditor into your **AI image‑performance co‑pilot**.

When you open the **Detail** view for any issue, you can:

-   Press **`a`** to **ask the configured LLM** (OpenAI, Anthropic, or local Ollama) for:
    - A **natural‑language explanation** of how to fix the problem.
    - A **concrete code patch** targeting the exact snippet that triggered the issue.
-   If a patch is available, you’ll see:
    - A clear banner: **“Patch available (press `p` to preview & apply)”**.
    - Press **`p`** to open a **side‑by‑side diff‑style preview** (Before / After).
    - Confirm with **`y`** to write the change back to disk, or **`n` / `Esc`** to cancel.

This gives you **instant, context‑aware fixes** for things like:

- Converting heavy JPG/PNG assets into modern formats.
- Adding `width`/`height` to kill CLS.
- Wiring in `loading="lazy"` and `srcset` correctly.
- Cleaning up templates and JSX/TSX image components.

### 🔧 Configuring the AI engine

The AI helper is **fully optional** and controlled through environment variables.
Use the provided `.env.example` as a starting point:

```bash
cp .env.example .env
```

Then edit `.env` and pick your provider:

```bash
# Possible values: openai, anthropic, ollama
ACTIVE_LLM_PROVIDER=openai
OPENAI_API_KEY=sk-...

# Optional: skip confirmation prompt in the TUI
LLM_SKIP_CONFIRM=true
```

#### OpenAI

```bash
OPENAI_API_KEY=your-openai-api-key
# Optional:
# OPENAI_BASE_URL=https://api.openai.com
# OPENAI_MODEL=gpt-5.2
```

#### Anthropic

```bash
ANTHROPIC_API_KEY=your-anthropic-api-key
# Optional:
# ANTHROPIC_BASE_URL=https://api.anthropic.com
# ANTHROPIC_MODEL=claude-3-5-sonnet-latest
```

#### Ollama (local)

```bash
ACTIVE_LLM_PROVIDER=ollama
OLLAMA_BASE_URL=http://localhost:11434
OLLAMA_MODEL=llama3.2
```

Once your environment is set, launch `image-auditor`, open an issue detail, and hit **`a`** to let the AI propose a fix — then **`p` → `y`** to apply it in seconds.

### 🔊 Controlling AI verbosity

By default, Image Auditor tells the AI to **return code only**, with no extra prose, so the Detail view stays clean and patch‑focused.

You can control this with the `AI_VERBOSE` flag in `.env`:

```bash
# Default (unset or false): code‑only output
AI_VERBOSE=false

# Verbose mode: allow explanations + code
AI_VERBOSE=true
```

- When `AI_VERBOSE=false` (or unset), the prompt instructs the LLM to output **only the structured patch block** (no explanations, no markdown).
- When `AI_VERBOSE=true`, the AI is allowed to return a **short explanation plus code**, which is rendered under “LLM Suggestion” in the Detail view.

## 🏗 Build

```bash
cargo build --release
./target/release/image-auditor
```

## Star History

<a href="https://www.star-history.com/?repos=0franco%2Fimage-auditor&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/image?repos=0franco/image-auditor&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/image?repos=0franco/image-auditor&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/image?repos=0franco/image-auditor&type=date&legend=top-left" />
 </picture>
</a>

## 🤝 Contributing

Contribute! Please open an issue or submit a pull request.

<a href="https://www.buymeacoffee.com/travelingcode" target="_blank">
  <img src="https://cdn.buymeacoffee.com/buttons/default-red.png" alt="Buy Me A Coffee" height="41" width="174" style="border-radius:10px">
</a>