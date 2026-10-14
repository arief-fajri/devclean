<script lang="ts">
  /**
   * The DevClean mark, inlined rather than loaded from `icons/`.
   *
   * The same artwork backs the tray icon (`src-tauri/icons/tray.png`) and
   * the window icon, so the header tile and the menu bar are visibly the
   * same product. Inlining avoids a runtime fetch and keeps the SVG crisp
   * in dark mode.
   */
  interface Props {
    size?: number;
    class?: string;
  }

  let { size = 36, class: klass = "" }: Props = $props();

  let gradientId = `devclean-tile-${Math.random().toString(36).slice(2, 9)}`;
</script>

<span
  aria-hidden="true"
  class="flex shrink-0 items-center justify-center overflow-hidden rounded-xl {klass}"
  style="width: {size}px; height: {size}px"
>
  <svg viewBox="0 0 512 512" width={size} height={size} role="presentation">
    <defs>
      <linearGradient id={gradientId} x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#10b981"></stop>
        <stop offset="1" stop-color="#047857"></stop>
      </linearGradient>
    </defs>
    <rect x="24" y="24" width="464" height="464" rx="112" fill="url(#{gradientId})"></rect>
    <path d="M256 404c0-96 52-176 152-204-8 108-64 176-152 204z" fill="#ecfdf5" opacity="0.95"
    ></path>
    <path
      d="M256 404C256 296 212 208 120 180c8 120 60 196 136 224z"
      fill="#a7f3d0"
      opacity="0.9"
    ></path>
    <rect x="246" y="264" width="20" height="176" rx="10" fill="#ecfdf5"></rect>
  </svg>
</span>
