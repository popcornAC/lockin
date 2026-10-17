<!-- An icon with a graceful placeholder while loading, offline or on error. -->
<script lang="ts">
  let {
    src,
    alt,
    size = 32,
    shape = "square",
    label = "",
    initials = false,
    class: cls = "",
  }: {
    src: string | null | undefined;
    alt: string;
    size?: number;
    shape?: "square" | "round" | "rounded" | "bare";
    /** Placeholder text. */
    label?: string;
    /** Show the alt text's first letters as the placeholder (champions). */
    initials?: boolean;
    class?: string;
  } = $props();

  // Keyed by URL so a new src starts fresh.
  let failedSrc = $state<string | null>(null);
  let loadedSrc = $state<string | null>(null);
  let failed = $derived(!!src && failedSrc === src);
  let loaded = $derived(!!src && loadedSrc === src);
  let text = $derived(label || (initials ? alt.replace(/[^\p{L}\p{N}]/gu, "").slice(0, size >= 28 ? 2 : 1) : ""));
</script>

<span
  class="img {shape} {cls}"
  class:ph={!src || failed || !loaded}
  style:--s="{size}px"
  title={alt}
>
  {#if src && !failed}
    <img {src} {alt} loading="lazy" decoding="async" draggable="false" onload={() => (loadedSrc = src ?? null)} onerror={() => (failedSrc = src ?? null)} />
  {/if}
  {#if !src || failed || !loaded}
    <span class="ph-text" aria-hidden="true">{text}</span>
  {/if}
</span>

<style>
  .img {
    position: relative;
    display: inline-grid;
    place-items: center;
    flex: none;
    width: var(--s);
    height: var(--s);
    overflow: hidden;
    vertical-align: middle;
  }
  .square {
    border-radius: 3px;
  }
  .rounded {
    border-radius: 6px;
  }
  .round {
    border-radius: 50%;
  }
  .ph:not(.bare) {
    background: linear-gradient(135deg, var(--surface-3), var(--surface-2));
    box-shadow: inset 0 0 0 1px var(--line);
  }
  img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    animation: fade-in 0.18s ease-out;
  }
  .bare img {
    object-fit: contain;
  }
  .ph-text {
    font-size: calc(var(--s) * 0.36);
    font-weight: 600;
    color: var(--faint);
    text-transform: uppercase;
    letter-spacing: 0.02em;
    line-height: 1;
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }
</style>
