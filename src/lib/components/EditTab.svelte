<script lang="ts">
	import Preview from './Preview.svelte';
	import { api, fileName, type Config, type JobStatus, type LightroomState, type Prediction, type StyleSummary } from '$lib/api';
	import { previewParams } from '$lib/blend';

	type Tab = 'edit' | 'looks' | 'retouch' | 'styles' | 'connect';
	let {
		config,
		styles,
		lr,
		go
	}: { config: Config; styles: StyleSummary[]; lr: LightroomState; go: (t: Tab) => void } = $props();

	const RETOUCH_LABELS: Record<string, string> = {
		subject: 'Brighten subject',
		background: 'Darken background',
		skin: 'Smooth skin',
		teeth: 'Whiten teeth',
		eyes: 'Brighten eyes',
		sky: 'Deepen sky'
	};

	let job = $state<JobStatus | null>(null);
	let busy = $state(false);
	let error = $state<string | null>(null);

	let previewPath = $derived(lr.selected[0] ?? null);
	let previewUrl = $state<string | null>(null);
	let prediction = $state<Prediction | null>(null);
	let lookParams = $state<Record<string, unknown> | null>(null);

	let style = $derived(styles.find((s) => s.id === config.style) ?? styles[0]);
	let steps = $derived([
		{ done: styles.length > 0, label: 'Learn your style from an edited catalog', tab: 'styles' as Tab },
		{ done: lr.plugin_installed, label: 'Install the Lightroom plugin', tab: 'connect' as Tab },
		{ done: lr.connected, label: 'Open Lightroom Classic', tab: 'connect' as Tab }
	]);

	$effect(() => {
		const path = previewPath;
		previewUrl = null;
		prediction = null;
		if (!path || !style) return;
		api.loadPreview(path, 1600).then((p) => path === previewPath && (previewUrl = p.url)).catch(() => {});
		api.predictOne(path, style.id).then((p) => path === previewPath && (prediction = p)).catch(() => {});
	});

	$effect(() => {
		const look = config.look;
		lookParams = null;
		if (look) api.lookParams(look.path).then((p) => (lookParams = p)).catch(() => {});
	});

	async function edit() {
		error = null;
		busy = true;
		try {
			job = await api.editSelection();
			for (let i = 0; i < 600 && job.state === 'waiting'; i++) {
				await new Promise((r) => setTimeout(r, 700));
				job = await api.jobStatus(job.id);
			}
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}
</script>

<div class="page">
	<header>
		<h2>Edit photos</h2>
		<p class="muted">
			Select photos in Lightroom, then send them here for a first pass. Every change stays editable in Lightroom.
		</p>
	</header>

	{#if steps.some((s) => !s.done)}
		<div class="card setup">
			<h3>Getting started</h3>
			<ol>
				{#each steps as s}
					<li class:done={s.done}>
						<span class="check">{s.done ? '✓' : ''}</span>
						<span>{s.label}</span>
						{#if !s.done}<button class="btn ghost small" onclick={() => go(s.tab)}>Set up</button>{/if}
					</li>
				{/each}
			</ol>
		</div>
	{/if}

	<div class="grid">
		<div class="card recipe">
			<div class="row">
				<div>
					<div class="label">Style</div>
					<div class="value">{style?.name ?? 'None yet'}</div>
				</div>
				<button class="btn ghost small" onclick={() => go('styles')}>Change</button>
			</div>
			<div class="row">
				<div>
					<div class="label">Look</div>
					<div class="value">
						{config.look ? `${config.look.name.trim()} · ${Math.round(config.look.amount * 100)}%` : 'None'}
					</div>
				</div>
				<button class="btn ghost small" onclick={() => go('looks')}>Change</button>
			</div>
			<div class="row">
				<div>
					<div class="label">Retouch</div>
					<div class="value">
						{config.retouch.length ? config.retouch.map((r) => RETOUCH_LABELS[r] ?? r).join(', ') : 'None'}
					</div>
				</div>
				<button class="btn ghost small" onclick={() => go('retouch')}>Change</button>
			</div>

			<button class="btn primary big go" disabled={!lr.connected || !lr.selected.length || !style || busy} onclick={edit}>
				{#if busy}
					Editing…
				{:else if !lr.connected}
					Open Lightroom to edit
				{:else if !lr.selected.length}
					Select photos in Lightroom
				{:else}
					Edit {lr.selected.length} selected photo{lr.selected.length === 1 ? '' : 's'}
				{/if}
			</button>

			{#if job}
				<div class="job" class:ok={job.state === 'done'}>
					{#if job.state === 'done'}
						Done. Lightroom edited {job.applied} photo{job.applied === 1 ? '' : 's'}.
						{#if job.message}<div class="muted small">{job.message}</div>{/if}
					{:else if job.state === 'waiting'}
						Waiting for Lightroom to apply the edits…
					{:else}
						This job is no longer queued.
					{/if}
				</div>
			{/if}
			{#if error}<div class="job err">{error}</div>{/if}
		</div>

		<div class="card preview">
			{#if previewPath && style}
				<div class="pv">
					<Preview url={previewUrl} params={previewParams(prediction, lookParams, config.look?.amount ?? 1)} compare />
				</div>
				<p class="muted small cap">
					{fileName(previewPath)}: preview of the first selected photo. Drag to compare. Approximate; Lightroom renders the
					final result.
				</p>
			{:else}
				<div class="empty muted">
					{lr.connected ? 'Select a photo in Lightroom to preview it here.' : 'A preview appears here when Lightroom is open.'}
				</div>
			{/if}
		</div>
	</div>
</div>

<style>
	.page {
		padding: 28px 32px;
		display: flex;
		flex-direction: column;
		gap: 20px;
		height: 100%;
	}
	header h2 {
		font-size: 26px;
		margin-bottom: 4px;
	}
	.setup ol {
		list-style: none;
		padding: 0;
		margin: 12px 0 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.setup li {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.setup li.done {
		color: var(--muted);
	}
	.check {
		width: 22px;
		height: 22px;
		border-radius: 50%;
		border: 1.5px solid var(--line);
		display: grid;
		place-items: center;
		font-size: 12px;
		color: #1a1208;
	}
	li.done .check {
		background: var(--leaf);
		border-color: var(--leaf);
	}
	.grid {
		display: grid;
		grid-template-columns: 340px 1fr;
		gap: 20px;
		flex: 1;
		min-height: 420px;
	}
	.recipe {
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	.row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 8px;
		padding-bottom: 12px;
		border-bottom: 1px solid var(--line);
	}
	.label {
		font-size: 12px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--muted);
	}
	.value {
		font-weight: 550;
	}
	.go {
		margin-top: auto;
		justify-content: center;
	}
	.job {
		padding: 12px;
		border-radius: 10px;
		background: var(--card);
		font-size: 14px;
	}
	.job.ok {
		border: 1px solid var(--leaf);
	}
	.job.err {
		border: 1px solid var(--rose);
	}
	.preview {
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-height: 0;
	}
	.pv {
		flex: 1;
		min-height: 0;
	}
	.cap {
		text-align: center;
	}
	.empty {
		flex: 1;
		display: grid;
		place-items: center;
		text-align: center;
	}
	.small {
		font-size: 13px;
	}
</style>
