<script lang="ts">
	import type { Config, StyleSummary } from '$lib/api';

	let { config, styles, save }: { config: Config; styles: StyleSummary[]; save: (c: Config) => Promise<void> } =
		$props();

	const OPTIONS = [
		{ id: 'subject', label: 'Brighten subject', text: 'Lifts the people in the frame a little so they stand out.' },
		{ id: 'background', label: 'Darken background', text: 'Pulls the background down slightly to match.' },
		{ id: 'skin', label: 'Smooth skin', text: 'Softens skin texture on faces without blurring eyes or hair.' },
		{ id: 'teeth', label: 'Whiten teeth', text: 'Brightens teeth and takes out yellow.' },
		{ id: 'eyes', label: 'Brighten eyes', text: 'Adds clarity and light to the whites and irises.' },
		{ id: 'sky', label: 'Deepen sky', text: 'Brings back highlight detail and color in skies.' }
	];

	let style = $derived(styles.find((s) => s.id === config.style) ?? styles[0]);

	function toggle(id: string) {
		const on = config.retouch.includes(id);
		save({ ...config, retouch: on ? config.retouch.filter((r) => r !== id) : [...config.retouch, id] });
	}
</script>

<div class="page">
	<header>
		<h2>Retouch</h2>
		<p class="muted">
			Lightroom finds faces, teeth, eyes, skies and subjects with its own AI masks. Retoucan adds the masks; you can tweak
			or delete any of them in Lightroom's Masking panel.
		</p>
	</header>
	<div class="grid">
		{#each OPTIONS as o}
			{@const on = config.retouch.includes(o.id)}
			{@const learned = style?.retouch_learned.includes(o.id)}
			<button class="card opt" class:on onclick={() => toggle(o.id)}>
				<div class="top">
					<h3>{o.label}</h3>
					<span class="switch" class:on></span>
				</div>
				<p class="muted">{o.text}</p>
				{#if learned}
					<span class="pill learned">Uses the amounts from “{style.name}”</span>
				{/if}
			</button>
		{/each}
	</div>
	<p class="muted small">
		Changes to retouch amounts reach Lightroom the next time it starts.
	</p>
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
		max-width: 680px;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
		gap: 14px;
	}
	.opt {
		text-align: left;
		display: flex;
		flex-direction: column;
		gap: 8px;
		transition: border-color 0.15s;
	}
	.opt.on {
		border-color: var(--beak);
	}
	.top {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}
	.switch {
		width: 38px;
		height: 22px;
		border-radius: 99px;
		background: var(--line);
		position: relative;
		transition: background 0.15s;
		flex: none;
	}
	.switch::after {
		content: '';
		position: absolute;
		top: 3px;
		left: 3px;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #fff;
		transition: transform 0.15s;
	}
	.switch.on {
		background: var(--beak);
	}
	.switch.on::after {
		transform: translateX(16px);
	}
	.learned {
		align-self: flex-start;
		color: var(--beak-2);
	}
	.small {
		font-size: 13px;
	}
</style>
