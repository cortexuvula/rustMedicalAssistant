<script lang="ts">
  import { rsvp } from '../stores/rsvp';
  import { settings } from '../stores/settings';
  import {
    tokenize,
    orpIndex,
    type Token,
  } from '../rsvp/engine';

  let tokens = $state<Token[]>([]);
  let index = $state(0);

  $effect(() => {
    if (!$rsvp.reader.open) return;
    tokens = tokenize($rsvp.reader.text);
    index = 0;
  });

  // Current word (or chunk) to display.
  function currentWords(): Token[] {
    const chunk = $settings.rsvp_chunk_size || 1;
    return tokens.slice(index, index + chunk);
  }

  function splitOrp(word: string): { pre: string; orp: string; post: string } {
    const i = orpIndex(word);
    return {
      pre: word.slice(0, i),
      orp: word[i] ?? '',
      post: word.slice(i + 1),
    };
  }

  function close(): void {
    rsvp.closeAll();
  }

  // Font size for the stage; chunks shrink to fit.
  let stageFont = $derived(
    ($settings.rsvp_font_size ?? 48) - (($settings.rsvp_chunk_size ?? 1) > 1 ? 8 : 0),
  );
</script>

{#if $rsvp.reader.open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="backdrop"
    class:dark={$settings.rsvp_dark_theme}
    onclick={close}
  >
    <div class="dialog" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()}>
      <div class="header">
        <h2>Speed Read</h2>
        <button class="btn-close" aria-label="Close" onclick={close}>&times;</button>
      </div>

      <div class="stage" style="font-size: {stageFont}px;">
        {#each currentWords() as tok, i}
          {#if i === Math.floor(currentWords().length / 2)}
            {@const parts = splitOrp(tok.word)}
            <span class="word orp-word">
              <span class="pre">{parts.pre}</span><span class="orp">{parts.orp}</span><span class="post">{parts.post}</span>
            </span>
          {:else}
            <span class="word">{tok.word}</span>
          {/if}
        {/each}
      </div>

      <div class="progress">
        {index} / {tokens.length}
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.8);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1600;
  }
  .dialog {
    background: var(--bg-primary);
    color: var(--text-primary);
    border-radius: var(--radius-lg);
    padding: 24px 32px;
    width: min(900px, 95vw);
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .backdrop.dark .dialog {
    background: #111;
    color: #eee;
  }
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .btn-close {
    background: none;
    border: none;
    color: inherit;
    font-size: 1.4rem;
    line-height: 1;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: 4px;
  }
  .btn-close:hover { background: rgba(255,255,255,0.1); }

  .stage {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 24px;
    min-height: 180px;
    font-family: 'SF Mono', Menlo, Consolas, monospace;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .word { display: inline-flex; }
  .orp-word .pre, .orp-word .post { opacity: 0.95; }
  .orp-word .orp { color: #ef4444; }

  .progress {
    text-align: center;
    color: var(--text-secondary);
    font-size: 0.9rem;
  }
</style>
