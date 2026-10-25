<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";

  let voiceState = $state("idle");
  let level = $state(0);

  onMount(() => {
    const stop = listen<{ state: string; level?: number }>(
      "voice-state",
      (event) => {
        voiceState = event.payload.state;
        if (typeof event.payload.level === "number") level = event.payload.level;
      },
    );
    return () => {
      stop.then((fn) => fn());
    };
  });
</script>

<div
  class="hud"
  class:processing={voiceState === "processing"}
  class:done={voiceState === "done"}
>
  <div class="track">
    <span
      class="fill"
      style={voiceState === "recording"
        ? `width:${Math.min(100, Math.max(4, Math.round(level * 900)))}%`
        : undefined}
    ></span>
  </div>
</div>

<style>
  .hud {
    padding: 6px 8px;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
  }

  .track {
    height: 4px;
    border-radius: 999px;
    background: rgba(18, 19, 23, 0.62);
    box-shadow:
      0 1px 8px rgb(0 0 0 / 0.3),
      inset 0 0 0 1px rgb(255 255 255 / 0.12);
    overflow: hidden;
  }

  .fill {
    display: block;
    height: 100%;
    width: 4%;
    min-width: 4px;
    border-radius: inherit;
    background: linear-gradient(90deg, #ff6b61, #ff9f0a);
    transition: width 0.12s linear;
  }

  .processing .fill {
    width: 35%;
    background: linear-gradient(90deg, #5b94ff, #8fb6ff);
    animation: slide 1.1s ease-in-out infinite;
  }

  .done .fill {
    width: 100%;
    background: #6ddba5;
  }

  @keyframes slide {
    0% {
      transform: translateX(-120%);
    }
    100% {
      transform: translateX(320%);
    }
  }
</style>
