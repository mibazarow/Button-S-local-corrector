<script lang="ts">
  let {
    checked = false,
    label = "",
    onchange,
  }: {
    checked: boolean;
    label?: string;
    onchange: (value: boolean) => void;
  } = $props();
</script>

<button
  class="switch"
  class:on={checked}
  role="switch"
  aria-label={label}
  aria-checked={checked}
  onclick={() => onchange(!checked)}
><span></span></button>
