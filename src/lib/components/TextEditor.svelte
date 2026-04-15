<script lang="ts">
  interface Props {
    value?: string;
    placeholder?: string;
    readonly?: boolean;
    onChange?: (v: string) => void;
  }

  let { value = '', placeholder = '', readonly = false, onChange = () => {} }: Props = $props();
</script>

<div class="text-editor">
  <textarea
    {value}
    {placeholder}
    readonly={readonly}
    class="editor-area"
    class:readonly
    oninput={(e) => onChange((e.target as HTMLTextAreaElement).value)}
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

  .editor-area.readonly {
    cursor: default;
    color: var(--text-secondary);
    user-select: text;
  }
</style>
