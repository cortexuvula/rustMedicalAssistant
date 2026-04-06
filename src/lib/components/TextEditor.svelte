<script lang="ts">
  export let value: string = '';
  export let placeholder: string = '';
  export let readonly: boolean = false;
  export let onChange: (v: string) => void = () => {};
</script>

<div class="text-editor">
  <textarea
    {value}
    {placeholder}
    {readonly}
    class="editor-area"
    on:input={(e) => onChange((e.target as HTMLTextAreaElement).value)}
  ></textarea>
</div>

<style>
  .text-editor {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .editor-area {
    flex: 1;
    width: 100%;
    height: 100%;
    resize: none;
    border: none;
    border-radius: 0;
    background-color: var(--bg-primary);
    color: var(--text-primary);
    font-size: 14px;
    line-height: 1.6;
    padding: 16px;
    outline: none;
    box-shadow: none;
    min-height: 0;
  }

  .editor-area:focus {
    border-color: transparent;
    box-shadow: none;
  }

  .editor-area[readonly] {
    cursor: default;
    color: var(--text-secondary);
  }
</style>
