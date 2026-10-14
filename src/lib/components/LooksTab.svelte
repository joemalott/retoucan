<script lang="ts">
	import { onMount } from 'svelte';
	import Preview from './Preview.svelte';
	import { api, fileName, spread, type Config, type LightroomState, type LookSummary, type Prediction } from '$lib/api';
	import { previewParams } from '$lib/blend';

	let {
		config,
		save,
		chosen = [],
		lr
	}: {
		config: Config;
		save: (c: Config) => Promise<void>;
		/** Photos chosen on the Photos screen, in capture order. */
		chosen?: string[];
		lr: LightroomState;
	} = $props();

	let looks = $state<LookSummary[]>([]);
	let query = $state('');
	let samples = $state<string[]>([]);
	let thumbs = $state<Record<string, string>>({});
	let current = $state<string | null>(null);
	let url = $state<string | null>(null);
	let predictions: Record<string, Prediction> = {};
	let prediction = $state<Prediction | null>(null);

	// The look being previewed; starts as the saved one.
	let picked = $state<LookSummary | null>(null);
	let amount = $state(1);
	let params = $state<Record<string, unknown> | null>(null);
	let applyStyle = $state(true);
	let compare = $state(false);
	let note = $state<string | null>(null);
	let folder = $state<string | null>(null);

	let matches = $derived.by(() => {
		const q = query.trim().toLowerCase();
		return looks.filter((l) => !q || l.name.toLowerCase().includes(q) || l.group.toLowerCase().includes(q));
	});
	let builtin = $derived(matches.filter((l) => l.builtin));
	// The photographer's own presets, by Lightroom group, alphabetically.
	let groups = $derived.by(() => {
		const m = new Map<string, LookSummary[]>();
		for (const l of matches) {
			if (l.builtin) continue;
			const g = l.group || 'Other';
			if (!m.has(g)) m.set(g, []);
			m.get(g)!.push(l);
		}
		return [...m.entries()].sort(([a], [b]) => a.localeCompare(b));
	});
	let ownCount = $derived(looks.filter((l) => !l.builtin).length);

	let dirty = $derived(
		(picked?.uuid ?? null) !== (config.look?.uuid ?? null) ||
			(picked != null && Math.abs(amount - (config.look?.amount ?? 1)) > 0.005)
	);

	onMount(async () => {
		looks = await api.listLooks();
		api.presetsFolder().then((f) => (folder = f));
		if (config.look) {
			picked = looks.find((l) => l.uuid === config.look!.uuid) ?? null;
			amount = config.look.amount;
		}
		// Spread across the whole shoot so the look can be judged as the light
		// changes through the day. Prefer the Photos screen's choice, then a big
		// Lightroom selection, then the style's training catalog.
		const source = chosen.length ? chosen : lr.selected.length > 1 ? lr.selected : [];
		if (source.length) {
			samples = spread(source);
		} else {
			try {
				samples = await api.samplePhotos(config.style, 10);
			} catch {
				note = 'Learn a style first to preview looks on your own photos.';
			}
		}
		if (samples[0]) show(samples[0]);
		for (const s of samples) {
			api.loadPreview(s, 220).then((p) => (thumbs[s] = p.url)).catch(() => {});
		}
	});

	async function show(path: string) {
		current = path;
		url = null;
		prediction = predictions[path] ?? null;
		const [p, pred] = await Promise.all([
			api.loadPreview(path, 1600),
			predictions[path] ? Promise.resolve(predictions[path]) : api.predictOne(path, config.style).catch(() => null)
		]);
		if (current !== path) return;
		url = p.url;
		if (pred) predictions[path] = pred;
		prediction = pred;
	}

	$effect(() => {
		const l = picked;
		params = null;
		if (l) api.lookParams(l.path).then((p) => picked === l && (params = p));
	});

	function pick(l: LookSummary | null) {
		picked = l;
	}

	async function use() {
		await save({
			...config,
			look: picked ? { uuid: picked.uuid, name: picked.name, path: picked.path, amount } : null
		});
	}
</script>

<div class="page">
	<section class="viewer">
		<div class="pv">
			<Preview {url} params={previewParams(applyStyle ? prediction : null, params, amount)} {compare} />
		</div>
		<div class="strip">
			{#each samples as s}
				<button class="thumb" class:on={s === current} onclick={() => show(s)} title={fileName(s)}>
					{#if thumbs[s]}<img src={thumbs[s]} alt="" />{/if}
				</button>
			{/each}
		</div>
		{#if note}<p class="muted small">{note}</p>{/if}
	</section>

	<section class="side">
		<div class="card controls">
			<h3>{picked ? picked.name : 'No look'}</h3>
			{#if picked}
				<p class="muted small source">
					{picked.builtin ? 'Built into Retoucan' : `From your Lightroom presets · ${picked.group || 'Other'}`}
				</p>
			{/if}
			{#if picked?.has_table}
				<p class="muted small">
					This look includes a color table (LUT). The preview shows its sliders; Lightroom adds the color table.
				</p>
			{/if}
			<label class="slider">
				<span>Strength <b>{Math.round(amount * 100)}%</b></span>
				<input type="range" min="0" max="1" step="0.01" bind:value={amount} disabled={!picked} />
			</label>
			<label class="check"><input type="checkbox" bind:checked={applyStyle} /> Include my style's corrections</label>
			<label class="check"><input type="checkbox" bind:checked={compare} /> Before / after</label>
			<button class="btn primary" disabled={!dirty} onclick={use}>
				{dirty ? 'Use this look' : 'In use'}
			</button>
		</div>

		<input type="search" placeholder="Search looks" bind:value={query} />

		<div class="list">
			<button class="look" class:on={!picked} onclick={() => pick(null)}>No look</button>
			{#if builtin.length}
				<div class="section">Built into Retoucan</div>
				{#each builtin as l}
					<button class="look" class:on={picked?.uuid === l.uuid} onclick={() => pick(l)}>
						<span>{l.name}</span>
						{#if l.has_table}<span class="tag">LUT</span>{/if}
					</button>
				{/each}
			{/if}
			{#if groups.length}
				<div class="section">From your Lightroom presets</div>
				{#each groups as [group, items]}
					<div class="group">{group}</div>
					{#each items as l}
						<button class="look" class:on={picked?.uuid === l.uuid} onclick={() => pick(l)}>
							<span>{l.name}</span>
							{#if l.has_table}<span class="tag">LUT</span>{/if}
						</button>
					{/each}
				{/each}
			{/if}
			{#if !looks.length}
				<p class="muted small pad">
					No presets found. Install the Lightroom plugin (Connect tab) to add Retoucan's starter looks.
				</p>
			{/if}
		</div>

		<details class="about">
			<summary>Where do these looks come from?</summary>
			<p>
				Retoucan doesn't take uploads. It scans the presets and profiles Lightroom Classic already has installed{ownCount
					? ` (${ownCount} found)`
					: ''}, plus the ten looks built into Retoucan.
			</p>
			<p>
				To add your own, import them in Lightroom: Develop module, Presets panel, <b>+</b>, Import Presets. Then
				reopen this tab. A LUT (.cube file) has to be made into a Lightroom profile or preset first; Retoucan can't
				read a bare .cube file.
			</p>
			{#if folder}<p class="path">{folder}</p>{/if}
		</details>
	</section>
</div>

<style>
	.page {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 320px;
		height: 100%;
	}
	.viewer {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 14px;
		padding: 24px;
		min-height: 0;
	}
	.pv {
		flex: 1;
		min-height: 0;
	}
	.strip {
		display: flex;
		gap: 8px;
		overflow-x: auto;
		padding-bottom: 4px;
	}
	.thumb {
		flex: none;
		width: 92px;
		height: 62px;
		border-radius: 8px;
		overflow: hidden;
		background: var(--well);
		border: 2px solid transparent;
		padding: 0;
	}
	.thumb.on {
		border-color: var(--beak);
	}
	.thumb img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.side {
		border-left: 1px solid var(--line);
		background: var(--panel);
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 18px;
		min-height: 0;
	}
	.controls {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px;
		background: var(--card);
	}
	.slider span {
		display: flex;
		justify-content: space-between;
		font-size: 14px;
	}
	.check {
		display: flex;
		gap: 8px;
		align-items: center;
		font-size: 14px;
	}
	.check input {
		accent-color: var(--beak);
	}
	.list {
		overflow-y: auto;
		flex: 1;
		min-height: 0;
		margin: 0 -6px;
	}
	.source {
		margin-top: -6px;
	}
	.section {
		font-size: 12.5px;
		font-weight: 600;
		color: var(--beak-2);
		padding: 16px 10px 2px;
		border-top: 1px solid var(--line);
		margin-top: 8px;
	}
	.about {
		font-size: 12.5px;
		color: var(--muted);
	}
	.about summary {
		cursor: pointer;
	}
	.about p {
		margin: 8px 0 0;
		line-height: 1.45;
	}
	.about .path {
		font-family: ui-monospace, monospace;
		font-size: 11px;
		word-break: break-all;
		user-select: text;
	}
	.group {
		font-size: 11.5px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--muted);
		padding: 14px 10px 4px;
	}
	.look {
		width: 100%;
		text-align: left;
		padding: 7px 10px;
		border-radius: 8px;
		display: flex;
		justify-content: space-between;
		align-items: center;
		font-size: 14px;
	}
	.look:hover {
		background: var(--card);
	}
	.look.on {
		background: rgba(255, 138, 31, 0.16);
		color: var(--beak-2);
	}
	.tag {
		font-size: 10.5px;
		padding: 1px 6px;
		border-radius: 6px;
		border: 1px solid var(--line);
		color: var(--muted);
	}
	.pad {
		padding: 10px;
	}
</style>
