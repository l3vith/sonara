# Design System — Sonora

## Theme
Late-night listening bar: near-black room, cool steel-blue instrument marks, a single tube-filament amber for the act of going live. Product register, restrained strategy. Mood lives in primary + accent, not in a tinted background.

## Colors
| Token | Value | Role |
|---|---|---|
| `--bg` | `oklch(0.10 0 0)` | App canvas |
| `--surface` | `oklch(0.145 0 0)` | Panels, fields, lists |
| `--surface-2` | `oklch(0.18 0 0)` | Raised controls, selected rows |
| `--ink` | `oklch(0.95 0.012 210)` | Body and titles |
| `--muted` | `oklch(0.68 0.015 210)` | Secondary labels |
| `--line` | `oklch(0.28 0.01 210)` | Hairline borders |
| `--primary` | `oklch(0.72 0.09 210)` | Live/path, selection, meters |
| `--accent` | `oklch(0.78 0.14 72)` | Host / join CTAs |
| `--danger` | `oklch(0.68 0.18 25)` | Leave, errors |
| `--on-accent` | `oklch(0.99 0 0)` | Text on accent fills |
| `--on-primary` | `oklch(0.12 0 0)` | Text on pale primary fills |

## Typography
- UI: **IBM Plex Sans** (400/500/600), 14px body, 1.2 scale: 12 / 14 / 16 / 20 / 28
- Room codes: **IBM Plex Mono** 600, tracked slightly open
- Wordmark: IBM Plex Sans 500, 20px, no display face in controls
- Line length for any prose ≤ 65ch

## Layout
- Desktop window ~1080×720, min 840×560
- Home: centered column, max 28rem
- Host/listen: header + one primary column, no nested cards
- 8px spacing unit; section gaps 32–48px, not even 24s throughout

## Components
- Buttons: 36px height, 8px radius, accent fill (white label) or ghost on surface
- Fields: surface fill, 1px `--line`, focus ring `--primary` 2px
- Source rows: full-width list, selected = `--surface-2` + primary text, not a card grid
- Meter: single horizontal energy bar, not a 32-band graphic EQ
- Toasts/errors: inline under the action, not a modal

## Motion
- 180ms `cubic-bezier(0.16, 1, 0.3, 1)` on hover/focus/state
- Meter width is the only continuous motion; freeze under reduced motion
- No page-load choreography
