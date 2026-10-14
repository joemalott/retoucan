<script lang="ts">
	import Preview from './Preview.svelte';
	import { api, type Prediction } from '$lib/api';
	import { previewParams } from '$lib/blend';

	// One photo rendered with the style's predicted correction and the look.
	let {
		path,
		styleId,
		lookParams,
		amount,
		edge = 700,
		compare = false
	}: {
		path: string;
		styleId: string;
		lookParams: Record<string, unknown> | null;
		amount: number;
		edge?: number;
		compare?: boolean;
	} = $props();

	let url = $state<string | null>(null);
	let prediction = $state<Prediction | null>(null);
	let error = $state(false);

	$effect(() => {
		const p = path;
		url = null;
		prediction = null;
		error = false;
		api.loadPreview(p, edge).then((r) => p === path && (url = r.url)).catch(() => (error = true));
		api.predictOne(p, styleId).then((r) => p === path && (prediction = r)).catch(() => {});
	});
</script>

{#if error}
	<div class="err muted small">Couldn't read this photo.</div>
{:else}
	<Preview {url} params={previewParams(prediction, lookParams, amount)} {compare} />
{/if}

<style>
	.err {
		height: 100%;
		display: grid;
		place-items: center;
	}
</style>
