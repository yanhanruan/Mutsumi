# Sleep animation frames

Drop the sleeping animation here as a zero-padded WebP frame sequence, matching
the other animations in `public/assets/`:

```
frame_001.webp
frame_002.webp
...
frame_NNN.webp
```

Then update the `sleep` entry in
[`src/composables/useAnimator.ts`](../../../src/composables/useAnimator.ts):

```ts
sleep: { dir: 'sleep', count: NNN, fps: 12, loop: true },
```

- `count` must equal the number of frames on disk.
- `loop: true` keeps the rest state playing indefinitely.

Until the frames are added the app degrades gracefully: entering sleep simply
holds the last visible pose (the animator skips frames that fail to load), so
no broken-image glyph ever appears.
