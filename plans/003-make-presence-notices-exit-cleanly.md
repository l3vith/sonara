# 003 — Make presence notices enter and exit cleanly

- **Status**: DONE
- **Commit**: 68d3e43
- **Severity**: MEDIUM
- **Category**: Interruptibility
- **Estimated scope**: 2 files, moderate React and CSS change

## Problem

`src/App.tsx:24` removes a notice directly after four seconds, while
`src/styles.css:96` uses a one-way keyframe only for entry. The exit teleports.

```tsx
window.setTimeout(() => setNotices(current => current.filter(item => item.id !== notice.id)), 4_000);
```

```css
.presence-notices p { animation: notice-in 220ms cubic-bezier(.16, 1, .3, 1); }
```

## Target

Use a CSS transition, not a keyframe. Extend `Notice` with `leaving?: boolean`.
At 3,800ms set the notice to leaving; at 4,000ms remove it. The normal state is
`opacity: 1; transform: translateY(0)`. The leaving state is `opacity: 0;
transform: translateY(-4px)`. Use `transform 200ms var(--ease-out), opacity
200ms var(--ease-out)`.

## Repo conventions to follow

- Notice state is owned by `App` in `src/App.tsx`.
- Visual state classes belong in `src/styles.css`.
- Do not introduce a toast library for this small, inline presence UI.

## Steps

1. Add `leaving?: boolean` to `Notice`.
2. On listener presence, schedule the leaving state at 3,800ms and removal at
   4,000ms; preserve list capping and cleanup behavior.
3. Add a `leaving` class to the notice paragraph.
4. Replace the `notice-in` keyframe with transition-based entry and exit.
5. Under reduced motion, retain the opacity transition and disable translate.

## Boundaries

- Do not change listener notification copy or timeout duration.
- Do not animate layout properties.

## Verification

- **Mechanical**: `npm run build` passes.
- **Feel check**: trigger several join/leave events. Each notice should appear
  promptly, fade and lift out after four seconds, and never restart another
  notice’s animation. In slow motion, entry and exit follow the same path.
- **Done when**: no `@keyframes notice-in` rule remains.
