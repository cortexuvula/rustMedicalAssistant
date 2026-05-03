<script lang="ts">
  import QRCode from 'qrcode';
  export let payload: string;
  let canvas: HTMLCanvasElement;
  $: if (canvas && payload) QRCode.toCanvas(canvas, payload, { width: 240 });
</script>
<canvas bind:this={canvas}></canvas>
<code class="payload">{payload}</code>
<style>.payload { font-size: 0.75rem; word-break: break-all; }</style>
