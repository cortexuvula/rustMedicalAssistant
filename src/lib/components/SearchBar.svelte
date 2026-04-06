<script lang="ts">
  export let value: string = '';
  export let placeholder: string = 'Search…';
  export let onSearch: (query: string) => void = () => {};

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  function handleInput(e: Event) {
    const input = e.target as HTMLInputElement;
    value = input.value;
    if (debounceTimer !== null) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => onSearch(value), 300);
  }
</script>

<div class="search-bar">
  <input
    type="text"
    {value}
    {placeholder}
    on:input={handleInput}
    class="search-input"
  />
</div>

<style>
  .search-bar {
    padding: 10px 12px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--border);
    background-color: var(--bg-secondary);
  }

  .search-input {
    width: 100%;
    border-radius: var(--radius-md);
    background-color: var(--bg-input);
    font-size: 13px;
  }
</style>
