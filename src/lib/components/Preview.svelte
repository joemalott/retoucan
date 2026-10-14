<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { createRenderer, type DevelopParams, type Renderer } from '$lib/render/renderer';

	let { url, params, compare = false }: { url: string | null; params: DevelopParams; compare?: boolean } =
		$props();

	let canvas: HTMLCanvasElement;
	let renderer: Renderer | null = null;
	let ready = $state(false);
	let split = $state(0.5);
	let dragging = false;
	let aspect = $state(1.5);

	onMount(() => {
		renderer = createRenderer(canvas);
	});
	onDestroy(() => renderer?.destroy());

	$effect(() => {
		if (!url) return;
		ready = false;
		const img = new Image();
		img.onload = () => {
			canvas.width = img.naturalWidth;
			canvas.height = img.naturalHeight;
			aspect = img.naturalWidth / img.naturalHeight;
			renderer?.setImage(img);
			ready = true;
		};
		img.src = url;
	});

	$effect(() => {
		if (ready && renderer) renderer.render(params, { split: compare ? split : undefined });
	});

	function move(e: PointerEvent) {
		if (!compare || !dragging) return;
		const r = canvas.getBoundingClientRect();
		split = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
	}
</script>

<div class="frame">
	<div
		class="stage"
		style:--a={aspect}
		role="presentation"
		onpointerdown={(e) => {
			dragging = true;
			(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
			move(e);
		}}
		onpointermove={move}
		onpointerup={() => (dragging = false)}
	>
		<canvas bind:this={canvas} class:hidden={!ready}></canvas>
		{#if compare && ready}
			<div class="divider" style:left="{split * 100}%"><span>Before · After</span></div>
		{/if}
		{#if !ready}
			<div class="loading">{url ? 'Loading photo…' : 'No photo yet'}</div>
		{/if}
	</div>
</div>

<style>
	.frame {
		width: 100%;
		height: 100%;
		display: grid;
		place-items: center;
		min-height: 0;
		container-type: size;
	}
	.stage {
		position: relative;
		width: min(100cqw, calc(100cqh * var(--a)));
		aspect-ratio: var(--a);
		border-radius: 10px;
		overflow: hidden;
		background: var(--well);
		cursor: ew-resize;
		touch-action: none;
	}
	canvas {
		display: block;
		width: 100%;
		height: 100%;
	}
	canvas.hidden {
		visibility: hidden;
	}
	.divider {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 2px;
		background: rgba(255, 255, 255, 0.85);
		box-shadow: 0 0 8px rgba(0, 0, 0, 0.5);
		pointer-events: none;
	}
	.divider span {
		position: absolute;
		bottom: 12px;
		left: 50%;
		transform: translateX(-50%);
		white-space: nowrap;
		font-size: 11px;
		padding: 3px 8px;
		border-radius: 99px;
		background: rgba(0, 0, 0, 0.6);
		color: #fff;
	}
	.loading {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		color: var(--muted);
		font-size: 14px;
	}
</style>
