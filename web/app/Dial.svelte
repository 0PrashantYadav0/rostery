<script lang="ts">
  /** The instrument on Sources and in a Verdict. `share` is how much of the ring is lit, out of 100. */
  let { share, value, unit }: { share: number; value?: string; unit?: string } = $props();
</script>

<div class="dial">
  <svg viewBox="0 0 200 200" aria-hidden="true">
    {#each { length: 60 } as _, i}
      <line
        x1="100"
        y1={i % 5 === 0 ? 6 : 10}
        x2="100"
        y2="15"
        transform="rotate({i * 6} 100 100)"
        class:major={i % 5 === 0}
      />
    {/each}
    <circle class="track" cx="100" cy="100" r="76" />
    <circle
      class="arc"
      cx="100"
      cy="100"
      r="76"
      pathLength="100"
      stroke-dasharray="{share} 100"
      transform="rotate(-90 100 100)"
    />
  </svg>
  {#if value !== undefined}
    <p class="reading">
      <span class="num">{value}</span>
      <span>{unit}</span>
    </p>
  {/if}
</div>

<style>
  .dial {
    position: relative;
    width: var(--dial-size, 220px);
    margin: 0 auto;
  }

  svg {
    display: block;
    width: 100%;
  }

  line {
    stroke: var(--rule);
    stroke-width: 1;
  }

  line.major {
    stroke: var(--faint);
    stroke-width: 1.5;
  }

  circle {
    fill: none;
    stroke-width: 7;
  }

  .track {
    stroke: var(--rule);
  }

  .arc {
    stroke: var(--ink);
    transition: stroke-dasharray 0.9s cubic-bezier(0.2, 0.7, 0.2, 1);
  }

  .reading {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    margin: 0;
    color: var(--dim);
    font-size: 13px;
    line-height: 1.2;
  }

  .num {
    color: var(--ivory);
    font: 500 40px/1.1 var(--serif);
    font-variant-numeric: tabular-nums;
  }

  @media (prefers-reduced-motion: reduce) {
    .arc {
      transition: none;
    }
  }
</style>
