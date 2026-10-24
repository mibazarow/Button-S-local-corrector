<script lang="ts">
  let { name, size = 16 }: { name: string; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === "main"}
    <path d="M2 4.5h12M2 8h12M2 11.5h12" />
    <circle cx="5.5" cy="4.5" r="1.6" fill="currentColor" stroke="none" />
    <circle cx="10.5" cy="8" r="1.6" fill="currentColor" stroke="none" />
    <circle cx="6.5" cy="11.5" r="1.6" fill="currentColor" stroke="none" />
  {:else if name === "voice"}
    <rect x="6" y="1.5" width="4" height="8" rx="2" />
    <path d="M3.5 7.5a4.5 4.5 0 0 0 9 0M8 12v2.5M5.5 14.5h5" />
  {:else if name === "dictionary"}
    <path d="M2.5 3c2-1.2 3.7-1.2 5.5 0 1.8-1.2 3.5-1.2 5.5 0v10c-2-1.2-3.7-1.2-5.5 0-1.8-1.2-3.5-1.2-5.5 0V3Z" />
    <path d="M8 3v10" />
  {:else if name === "rules"}
    <path d="M2 5h10.5M9.5 2l3 3-3 3M14 11H3.5M6.5 8l-3 3 3 3" />
  {:else if name === "apps"}
    <rect x="2" y="2" width="5" height="5" rx="1.3" />
    <rect x="9" y="2" width="5" height="5" rx="1.3" />
    <rect x="2" y="9" width="5" height="5" rx="1.3" />
    <rect x="9" y="9" width="5" height="5" rx="1.3" />
  {:else if name === "history"}
    <circle cx="8" cy="8" r="6" />
    <path d="M8 4.5V8l2.5 1.8" />
  {:else if name === "ai"}
    <path d="M8 1.5 9.6 6 14 7.5 9.6 9 8 13.5 6.4 9 2 7.5 6.4 6 8 1.5Z" />
  {:else if name === "hotkeys"}
    <rect x="1.5" y="4" width="13" height="8.5" rx="1.6" />
    <path d="M4 7h.01M6.5 7h.01M9 7h.01M11.5 7h.01M4 9.8h8" />
  {:else if name === "data"}
    <ellipse cx="8" cy="3.5" rx="5.5" ry="2" />
    <path d="M2.5 3.5v9c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2v-9M2.5 8c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2" />
  {:else if name === "info"}
    <circle cx="8" cy="8" r="6" />
    <path d="M8 7.2v4M8 4.7h.01" />
  {:else if name === "check"}
    <path d="M3 8.5 6.5 12 13 4" />
  {:else if name === "plus"}
    <path d="M8 3v10M3 8h10" />
  {:else if name === "trash"}
    <path d="M2.5 4h11M6.5 4V2.5h3V4M4 4l.7 9.5h6.6L12 4M6.7 7v4M9.3 7v4" />
  {:else if name === "pencil"}
    <path d="M11.3 2.1 13.9 4.7 5.6 13H3v-2.6l8.3-8.3Z" />
  {:else if name === "link"}
    <path d="M6.5 9.5 9.5 6.5M5 7 3.5 8.5a2.7 2.7 0 0 0 3.8 3.8L8.8 10.8M11 6.8l1.5-1.5a2.7 2.7 0 0 0-3.8-3.8L7.2 3.2" />
  {:else if name === "dot"}
    <circle cx="8" cy="8" r="3" fill="currentColor" stroke="none" />
  {/if}
</svg>
