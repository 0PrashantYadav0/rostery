<script lang="ts">
  import type { Source } from '../shared/types';

  let { source, color }: { source: Source; color: string } = $props();
</script>

<!-- A small picture of what this Source looks like in the sky: banded for Spotify, ringed for YouTube. -->
<span class="glyph {source}" style:--tint={color} aria-hidden="true"></span>

<style>
  .glyph {
    position: relative;
    display: inline-block;
    flex: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--tint);
  }

  .spotify {
    background: repeating-linear-gradient(170deg, var(--tint) 0 2px, color-mix(in srgb, var(--tint) 45%, var(--night)) 2px 4px);
  }

  .youtube::after {
    content: '';
    position: absolute;
    inset: 3px -4px;
    border: 1px solid var(--ivory);
    border-radius: 50%;
    transform: rotate(-22deg);
  }
</style>
