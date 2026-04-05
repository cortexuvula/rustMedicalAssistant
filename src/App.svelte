<script lang="ts">
  let name = $state('Medical Assistant');
</script>

<main>
  <h1>{name}</h1>
  <p>Rust Medical Assistant — Tauri + Svelte</p>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100vh;
    font-family: sans-serif;
  }
</style>
