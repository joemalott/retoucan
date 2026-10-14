<script lang="ts">
	import { onDestroy } from 'svelte';
	import { open } from '@tauri-apps/plugin-dialog';
	import { listen, type UnlistenFn } from '@tauri-apps/api/event';
	import { api, fileName, type Config, type Progress, type StyleSummary } from '$lib/api';

	let {
		config,
		styles,
		save,
		refresh
	}: { config: Config; styles: StyleSummary[]; save: (c: Config) => Promise<void>; refresh: () => Promise<void> } =
		$props();

	let catalog = $state<string | null>(null);
	let name = $state('');
	let training = $state(false);
	let progress = $state<Progress | null>(null);
	let error = $state<string | null>(null);
	let unlisten: UnlistenFn | null = null;
	onDestroy(() => unlisten?.());

	async function choose() {
		const file = await open({
			title: 'Choose an edited Lightroom catalog',
			filters: [{ name: 'Lightroom catalog', extensions: ['lrcat'] }]
		});
		if (typeof file === 'string') {
			catalog = file;
			if (!name) name = fileName(file).replace(/\.lrcat$/i, '').replace(/[-_]+/g, ' ');
		}
	}

	async function train() {
		if (!catalog || !name.trim()) return;
		error = null;
		training = true;
		progress = { stage: 'Opening catalog', done: 0, total: 0 };
		unlisten = await listen<Progress>('train-progress', (e) => (progress = e.payload));
		try {
			const s = await api.trainStyle(catalog, name.trim());
			await refresh();
			if (!config.style) await save({ ...config, style: s.id });
			catalog = null;
			name = '';
		} catch (e) {
			error = String(e);
		} finally {
			training = false;
			unlisten?.();
			unlisten = null;
		}
	}

	async function remove(s: StyleSummary) {
		await api.deleteStyle(s.id);
		if (config.style === s.id) await save({ ...config, style: null });
		await refresh();
	}

	// Plain-language accuracy for the two sliders people care most about.
	function describe(s: StyleSummary) {
		const out: { label: string; text: string; gain: number }[] = [];
		for (const [label, base, model] of s.accuracy) {
			if (base <= 0.001) continue;
			const gain = Math.max(0, 1 - model / base);
			let text: string;
			if (label === 'Exposure') text = `usually within ±${model.toFixed(2)} stops`;
			else if (label === 'Temperature') {
				const k = Math.round((5500 - 1e6 / (1e6 / 5500 + model)) / 10) * 10;
				text = `usually within ±${k}K`;
			} else text = `usually within ±${model.toFixed(0)}`;
			out.push({ label, text, gain });
		}
		return out.filter((r) => ['Exposure', 'Temperature', 'Tint', 'Shadows', 'Whites', 'Highlights'].includes(r.label));
	}
</script>

<div class="page">
	<header>
		<h2>Styles</h2>
		<p class="muted">
			A style is what Retoucan learns from photos you've already edited: how you set exposure, white balance and tone for
			different kinds of light. Point it at any edited Lightroom catalog. It only reads; nothing in the catalog changes.
		</p>
	</header>

	<div class="card learn">
		<h3>Learn a new style</h3>
		<div class="form">
			<button class="btn" onclick={choose} disabled={training}>
				{catalog ? fileName(catalog) : 'Choose catalog…'}
			</button>
			<input type="text" placeholder="Name, e.g. Weddings" bind:value={name} disabled={training} />
			<button class="btn primary" onclick={train} disabled={!catalog || !name.trim() || training}>
				{training ? 'Learning…' : 'Learn style'}
			</button>
		</div>
		{#if training && progress}
			<div class="progress">
				<div class="bar"><div style:width="{progress.total ? (progress.done / progress.total) * 100 : 5}%"></div></div>
				<span class="muted small">{progress.stage}{progress.total ? ` · ${progress.done} of ${progress.total}` : ''}</span>
			</div>
		{/if}
		{#if error}<p class="err">{error}</p>{/if}
		<p class="muted small">
			Tip: a few hundred edited photos from similar shoots works well. The original photo files need to be reachable, so
			plug in any external drive first.
		</p>
	</div>

	<div class="list">
		{#each styles as s}
			{@const active = config.style === s.id || (!config.style && styles[0]?.id === s.id)}
			<div class="card style" class:active>
				<div class="head">
					<div>
						<h3>{s.name}</h3>
						<p class="muted small">
							Learned from {s.photo_count} photos{s.created ? ` · ${s.created.slice(0, 10)}` : ''}
						</p>
					</div>
					<div class="actions">
						{#if active}
							<span class="pill">In use</span>
						{:else}
							<button class="btn small" onclick={() => save({ ...config, style: s.id })}>Use</button>
						{/if}
						<button class="btn ghost small" onclick={() => remove(s)}>Delete</button>
					</div>
				</div>
				<div class="acc">
					{#each describe(s) as r}
						<div class="arow">
							<span>{r.label}</span>
							<div class="meter" title="{Math.round(r.gain * 100)}% closer than using your average setting">
								<div style:width="{Math.max(4, r.gain * 100)}%"></div>
							</div>
							<span class="muted small">{r.text}</span>
						</div>
					{/each}
				</div>
			</div>
		{/each}
	</div>
</div>

<style>
	.page {
		padding: 28px 32px;
		display: flex;
		flex-direction: column;
		gap: 20px;
	}
	header h2 {
		font-size: 26px;
		margin-bottom: 4px;
	}
	header p {
		max-width: 720px;
	}
	.learn {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.form {
		display: grid;
		grid-template-columns: auto 1fr auto;
		gap: 10px;
	}
	.progress {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.bar {
		height: 8px;
		border-radius: 99px;
		background: var(--well);
		overflow: hidden;
	}
	.bar div {
		height: 100%;
		background: linear-gradient(90deg, var(--beak), var(--beak-2));
		transition: width 0.3s;
	}
	.err {
		color: var(--rose);
	}
	.list {
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	.style.active {
		border-color: var(--beak);
	}
	.head {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: 12px;
	}
	.actions {
		display: flex;
		gap: 6px;
		align-items: center;
	}
	.acc {
		margin-top: 14px;
		display: grid;
		gap: 8px;
	}
	.arow {
		display: grid;
		grid-template-columns: 110px 160px 1fr;
		gap: 12px;
		align-items: center;
		font-size: 14px;
	}
	.meter {
		height: 6px;
		border-radius: 99px;
		background: var(--well);
		overflow: hidden;
	}
	.meter div {
		height: 100%;
		background: var(--leaf);
	}
	.small {
		font-size: 13px;
	}
	.btn.small {
		padding: 6px 12px;
		font-size: 13px;
	}
</style>
