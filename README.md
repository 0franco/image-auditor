# 🖼 Image Auditor Tool

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