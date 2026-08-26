# 004 — Consolidate the motion vocabulary

- **Status**: DONE
- **Commit**: 68d3e43
- **Severity**: LOW
- **Category**: Cohesion and tokens
- **Estimated scope**: 1 file, small CSS change

## Problem

`src/styles.css` repeats raw durations and the non-token curve
`cubic-bezier(.16, 1, .3, 1)` across buttons, meter, switch, and notices.
The app has no shared motion vocabulary.

## Target

Add these tokens to `:root`:

```css
--ease-out: cubic-bezier(0.23, 1, 0.32, 1);
--duration-press: 160ms;
--duration-ui: 180ms;
--duration-notice: 200ms;
```

Use `--duration-press` for press feedback, `--duration-ui` for ordinary control
and meter feedback, and `--duration-notice` for notice entry/exit. Keep color
hover transitions on CSS `ease` because they are hover feedback.

## Repo conventions to follow

- Tokens belong in the `:root` block in `src/styles.css`.
- Do not add a second token file or any JavaScript animation framework.

## Steps

1. Add the exact tokens above to `:root`.
2. Replace raw transform curves for buttons, switches, meter, and notice with
   `var(--ease-out)`.
3. Replace matching raw durations with the relevant duration token.

## Boundaries

- Do not change the visual duration of state feedback beyond the values above.
- Do not replace hover color easing with the entrance curve.

## Verification

- **Mechanical**: `npm run build` passes and `rg 'cubic-bezier' src/styles.css`
  shows only the `--ease-out` token.
- **Feel check**: buttons, toggle thumb, meter, and notices settle with one
  coherent, fast ease-out character.
- **Done when**: raw motion curves and duplicate durations are removed.
