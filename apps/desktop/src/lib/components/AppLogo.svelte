<script lang="ts">
  /**
   * The DevClean mark, inlined rather than loaded from `icons/`.
   *
   * The leaf outline is the same artwork as the app icon, the favicon and the
   * tray icon (`src-tauri/icons/tray.png`), so the header and the dock are
   * visibly the same product. Inlining avoids a runtime fetch and keeps the
   * SVG crisp at any size.
   *
   * The mark is drawn without the icon tile: in the sidebar it stands alone
   * next to the wordmark, sized to the wordmark + tagline block. `size` is the
   * leaf's height; the width follows the leaf's natural aspect ratio.
   */
  interface Props {
    size?: number;
    class?: string;
  }

  let { size = 36, class: klass = "" }: Props = $props();

  // Stroke-bbox of the leaf outline (assets/devclean-leaf.svg), padded by 2px
  // so the rounded caps are never clipped.
  const VIEW = { x: 97, y: 163, w: 325, h: 258 };
  let width = $derived(Math.round((size * VIEW.w) / VIEW.h));
</script>

<span
  aria-hidden="true"
  class="flex shrink-0 items-center justify-center {klass}"
  style="width: {width}px; height: {size}px"
>
  <svg viewBox="{VIEW.x} {VIEW.y} {VIEW.w} {VIEW.h}" width={width} height={size} role="presentation">
    <g fill="none" stroke="#0B936B" stroke-width="18" stroke-linecap="round" stroke-linejoin="round">
      <path
        d="M256 410 C224 398 184 372 155 334 C121 290 108 235 108 174 C162 190 207 221 234 266 C250 292 256 326 256 359"
      ></path>
      <path
        d="M256 410 C288 395 330 367 360 327 C392 284 407 230 411 194 C360 208 316 237 285 278 C265 305 256 337 256 370"
      ></path>
      <path d="M256 282 L256 410"></path>
    </g>
  </svg>
</span>
