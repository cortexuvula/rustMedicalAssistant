<script lang="ts">
  import Modal from '../components/Modal.svelte';
  import SettingsContent from '../components/SettingsContent.svelte';

  interface Props {
    open: boolean;
  }

  let { open = $bindable() }: Props = $props();
</script>

<Modal {open} title="Settings" onClose={() => (open = false)}>
  <SettingsContent />
</Modal>
