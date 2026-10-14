<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type LightroomState } from '$lib/api';

	let { lr }: { lr: LightroomState } = $props();

	let exe = $state('');
	let installing = $state(false);
	let installed = $state<string | null>(null);
	let error = $state<string | null>(null);
	let copied = $state<string | null>(null);

	onMount(async () => (exe = await api.mcpCommand()));

	let claudeCode = $derived(`claude mcp add retoucan -- "${exe}" mcp`);
	let desktopJson = $derived(
		JSON.stringify({ mcpServers: { retoucan: { command: exe, args: ['mcp'] } } }, null, 2)
	);

	async function install() {
		installing = true;
		error = null;
		try {
			installed = await api.installPlugin();
		} catch (e) {
			error = String(e);
		} finally {
			installing = false;
		}
	}

	async function copy(text: string, key: string) {
		await navigator.clipboard.writeText(text);
		copied = key;
		setTimeout(() => copied === key && (copied = null), 1500);
	}
</script>

<div class="page">
	<header>
		<h2>Connect</h2>
	</header>

	<div class="card">
		<div class="head">
			<div>
				<h3>Lightroom Classic</h3>
				<p class="muted">
					The Retoucan plugin applies edits inside Lightroom, so your catalog stays safe and every slider stays editable.
					It also adds Retoucan's starter looks and retouch presets.
				</p>
			</div>
			<span class="pill">
				<span class="dot" class:on={lr.connected} class:warn={!lr.connected && lr.plugin_installed}></span>
				{lr.connected ? 'Connected' : lr.plugin_installed ? 'Installed, Lightroom closed' : 'Not installed'}
			</span>
		</div>
		<div class="row">
			<button class="btn primary" onclick={install} disabled={installing}>
				{lr.plugin_installed ? 'Reinstall / update plugin' : 'Install plugin'}
			</button>
			{#if installed}
				<span class="ok">Installed. Restart Lightroom Classic to load it.</span>
			{/if}
		</div>
		{#if error}<p class="err">{error}</p>{/if}
		{#if lr.connected && lr.catalog}
			<p class="muted small">Catalog: <span class="mono">{lr.catalog}</span></p>
		{/if}
		<p class="muted small">
			In Lightroom you'll also find <b>Library › Plug-in Extras › Edit Selected Photos with Retoucan</b>.
		</p>
	</div>

	<div class="card">
		<h3>AI assistants (MCP)</h3>
		<p class="muted">
			Let Claude or any MCP-compatible tool run Retoucan for you: "edit the photos I've selected with the Golden Film look
			at 40%", "learn a style from my Smith wedding catalog", and so on.
		</p>
		<div class="snippet">
			<div class="label">Claude Code</div>
			<pre class="mono">{claudeCode}</pre>
			<button class="btn small" onclick={() => copy(claudeCode, 'code')}>{copied === 'code' ? 'Copied' : 'Copy'}</button>
		</div>
		<div class="snippet">
			<div class="label">Claude Desktop and other apps (add to the MCP config file)</div>
			<pre class="mono">{desktopJson}</pre>
			<button class="btn small" onclick={() => copy(desktopJson, 'json')}>{copied === 'json' ? 'Copied' : 'Copy'}</button>
		</div>
	</div>

	<p class="muted small">
		Retoucan runs entirely on this computer. No photos, catalogs or settings are uploaded anywhere.
	</p>
</div>

<style>
	.page {
		padding: 28px 32px;
		display: flex;
		flex-direction: column;
		gap: 20px;
		max-width: 900px;
	}
	header h2 {
		font-size: 26px;
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	.head {
		display: flex;
		justify-content: space-between;
		gap: 16px;
		align-items: flex-start;
	}
	.row {
		display: flex;
		gap: 14px;
		align-items: center;
	}
	.ok {
		color: var(--leaf);
	}
	.err {
		color: var(--rose);
	}
	.snippet {
		position: relative;
		background: var(--well);
		border: 1px solid var(--line);
		border-radius: 10px;
		padding: 12px 14px;
	}
	.snippet pre {
		margin: 6px 0 0;
		white-space: pre-wrap;
		word-break: break-all;
	}
	.snippet .btn {
		position: absolute;
		top: 8px;
		right: 8px;
		padding: 5px 10px;
		font-size: 12.5px;
	}
	.label {
		font-size: 12px;
		color: var(--muted);
	}
	.small {
		font-size: 13px;
	}
</style>
