<script lang="ts">
  import SettingsContent from '../components/SettingsContent.svelte';
</script>

<div class="settings-page">
  <div class="settings-header">
    <h2>Settings</h2>
  </div>
  <SettingsContent />
</div>

<style>
  .settings-page {
    flex: 1;
    display: flex;
    flex-direction: column;
    background-color: var(--bg-primary);
    overflow: hidden;
  }

  .settings-header {
    padding: 16px 24px 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .settings-header h2 {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
  }
</style>
