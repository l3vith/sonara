# Design System — Sonora Widget

## Surface

Sonora is a fixed 440×640 desktop listening widget. The window is frameless, non-resizable, fully rounded, and contains every state without page scrolling. Settings and source selection appear as compact glass sheets inside the same frame.

## Visual World

Album artwork is the visual source of truth. Native now-playing artwork fills the background as a blurred color field while a sampled palette supplies the base, deep, and glow colors. When artwork is unavailable, track metadata deterministically creates a music-specific palette and a restrained Sonora artwork tile.

Liquid glass is functional: it separates controls from the artwork-driven field. Glass uses translucent white, backdrop blur, a fine internal highlight, and a soft downward shadow. It is reserved for the action dock, live signal panel, room controls, dialogs, and compact circular actions.

## Color

- `--music-base`: average artwork color or metadata-derived fallback
- `--music-deep`: darkened artwork color for legibility and the lower field
- `--music-glow`: lifted artwork color for focus, selection, and the live meter
- Primary text: white
- Secondary text: hue-neutral translucent white at 56–78%
- Live state: pale mint with a visible text label
- Destructive hover: muted red, used only for window close and leaving a room

## Typography

- UI and track metadata: IBM Plex Sans 400/500/600
- Room codes and room-code input: IBM Plex Mono 600
- Track title: 26px on home, 24px in a room, maximum tracking −0.025em
- Controls: 10–14px, concise and action-led

## Layout

- Outer inset: 14px
- Window radius: 30px
- Primary artwork: 226×226px, 24px radius
- Active-room artwork: 122×122px, 19px radius
- Action dock stays pinned to the bottom; the artwork region absorbs remaining height
- No nested navigation, sidebar, or mobile tab bar

## Interaction

- Custom window controls and the brand bar provide drag, minimize, settings, and close
- Home exposes exactly two primary tasks: start a room or join with a code
- Host setup preserves source selection and Auto/High/Balanced/Saver quality
- Active rooms preserve live/path state, lossless format, level meter, room-code copy, listener count, 0–200% listener volume, presence notices, and leave
- Artwork and palette update every three seconds from the native media session
- Motion uses 180–700ms exponential ease-out transitions; reduced motion collapses them

## Accessibility

- Every icon button has an accessible name and uses authored SVG icons
- Focus rings use the artwork glow mixed toward white
- Status is label plus color; controls remain keyboard operable
- Text and controls remain inside the fixed frame with no page overflow
