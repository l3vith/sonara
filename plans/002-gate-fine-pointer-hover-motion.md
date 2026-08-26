# 002 — Gate hover movement to fine pointers

- **Status**: DONE
- **Commit**: 68d3e43
- **Severity**: MEDIUM
- **Category**: Accessibility
- **Estimated scope**: 1 file, small CSS change

## Problem

`src/styles.css:73` moves every button on hover without checking pointer type.
Touch devices can retain a hover state after a tap, causing unnecessary motion.

```css
.button:hover:not(:disabled) { transform: translateY(-1px); }
```

## Target

Place transform hover rules inside:

```css
@media (hover: hover) and (pointer: fine) {
  .button:hover:not(:disabled) { transform: translateY(-1px); }
}
```

Keep color and background hover feedback available outside the media query.
Keep press feedback as `transform: scale(.975)` with a 160ms transition.

## Repo conventions to follow

- CSS lives in `src/styles.css`.
- Use the shared `--ease-out` token from plan 004 for transform transitions.

## Steps

1. Move transform-only hover rules for `.button` into the fine-pointer query.
2. Apply the same treatment to `.settings-button` and `.icon-button` if their
   hover behavior later gains a transform.
3. Leave `:active` feedback unchanged so direct manipulation is immediate.

## Boundaries

- Do not gate keyboard focus styling.
- Do not add touch-specific JavaScript.

## Verification

- **Mechanical**: `npm run build` passes.
- **Feel check**: use DevTools touch emulation; button color changes may remain,
  but the control must not lift. With a mouse, it lifts by 1px smoothly.
- **Done when**: all hover transforms are contained in a fine-pointer query.
