# 001 — Preserve feedback under reduced motion

- **Status**: DONE
- **Commit**: 68d3e43
- **Severity**: MEDIUM
- **Category**: Accessibility
- **Estimated scope**: 1 file, small CSS change

## Problem

`src/styles.css:173` suppresses every transition and animation to `0.01ms`.
That removes useful state feedback from controls and notices instead of only
removing spatial movement.

```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: .01ms !important;
    transition-duration: .01ms !important;
  }
}
```

## Target

Keep color and opacity feedback. Keep the final visual transform state for the
meter and switch, but remove its temporal transition. The notice uses `opacity
200ms cubic-bezier(0.23, 1, 0.32, 1)` with no translate under reduced motion.

## Repo conventions to follow

- Edit only `src/styles.css`.
- Add shared motion tokens in the same `:root` block; see plan 004.

## Steps

1. Replace the universal duration override with targeted reduced-motion rules.
2. Preserve `background`, `color`, `border-color`, and `opacity` transitions.
3. Set transform changes on `.button`, `.settings-button`, `.icon-button`,
   `.switch span`, and `.meter span` to `none` in the reduced-motion query.
4. Make `.presence-notices p` use opacity-only transition in that query.

## Boundaries

- Do not remove visible focus indicators.
- Do not add a dependency or change component markup.

## Verification

- **Mechanical**: `npm run build` passes.
- **Feel check**: enable reduced motion in DevTools. Toggling diagnostics still
  visibly changes color; a presence notice fades but does not travel.
- **Done when**: no universal `0.01ms` transition override remains.
